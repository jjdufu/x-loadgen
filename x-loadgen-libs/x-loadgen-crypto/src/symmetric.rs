//! 对称分组/流式模式：AES-CBC-PKCS7、AES-CTR、AES-CFB。
//!
//! # 安全说明
//!
//! CBC / CTR / CFB **不提供静默无 MAC 的安全默认**。画像层（step-04/09）
//! 必须为这些算法配置 `Mac`；可用 [`crate::registry::require_mac_for`] 查询元数据。
//!
//! 本步 **不** 实现 ECB（见 step-03 legacy）。

use aes::{Aes128, Aes256};
use cbc::{Decryptor as CbcDecryptor, Encryptor as CbcEncryptor};
use cipher::{
    AsyncStreamCipher, BlockDecryptMut, BlockEncryptMut, KeyIvInit, StreamCipher,
    block_padding::Pkcs7,
};
use ctr::Ctr128BE;

use crate::id::AlgorithmId;
use crate::material::KeyRef;
use crate::traits::{BlockCipher, CryptoError};

/// AES 分组长度 / CBC·CFB·CTR 的 IV 长度（字节）。
pub const AES_BLOCK_LEN: usize = 16;

/// AES-CFB 反馈段长度参数。
///
/// 本步实现：`128`（整块 CFB）与 `8`（CFB-8）；其它值返回
/// [`CryptoError::Unsupported`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CfbParams {
    /// 反馈段位数（常见 8 或 128）。
    pub segment_bits: u16,
}

impl CfbParams {
    /// 整块反馈（AES 上即 CFB-128）。
    #[must_use]
    pub const fn full_block() -> Self {
        Self { segment_bits: 128 }
    }

    /// 8-bit 反馈（CFB-8）。
    #[must_use]
    pub const fn eight_bit() -> Self {
        Self { segment_bits: 8 }
    }
}

impl Default for CfbParams {
    fn default() -> Self {
        Self::full_block()
    }
}

/// 本步已实现的对称（非 AEAD）算法句柄。
///
/// CFB 经 [`BlockCipher`] 调用时默认 `segment_bits=128`；其它段长请用
/// [`encrypt_cfb`] / [`decrypt_cfb`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymmetricAlgorithm {
    /// `aes-128-cbc`
    Aes128Cbc,
    /// `aes-256-cbc`
    Aes256Cbc,
    /// `aes-128-ctr`
    Aes128Ctr,
    /// `aes-256-ctr`
    Aes256Ctr,
    /// `aes-128-cfb`（默认 CFB-128）
    Aes128Cfb,
    /// `aes-256-cfb`（默认 CFB-128）
    Aes256Cfb,
}

impl SymmetricAlgorithm {
    /// 由闭集 id 解析。
    ///
    /// # Errors
    ///
    /// [`CryptoError::Unsupported`]：非本步对称算法。
    pub fn from_id(id: AlgorithmId) -> Result<Self, CryptoError> {
        match id {
            AlgorithmId::Aes128Cbc => Ok(Self::Aes128Cbc),
            AlgorithmId::Aes256Cbc => Ok(Self::Aes256Cbc),
            AlgorithmId::Aes128Ctr => Ok(Self::Aes128Ctr),
            AlgorithmId::Aes256Ctr => Ok(Self::Aes256Ctr),
            AlgorithmId::Aes128Cfb => Ok(Self::Aes128Cfb),
            AlgorithmId::Aes256Cfb => Ok(Self::Aes256Cfb),
            other => Err(CryptoError::Unsupported {
                reason: format!("not an implemented AES mode: {other}"),
            }),
        }
    }

    /// 对应闭集 id。
    #[must_use]
    pub const fn id(self) -> AlgorithmId {
        match self {
            Self::Aes128Cbc => AlgorithmId::Aes128Cbc,
            Self::Aes256Cbc => AlgorithmId::Aes256Cbc,
            Self::Aes128Ctr => AlgorithmId::Aes128Ctr,
            Self::Aes256Ctr => AlgorithmId::Aes256Ctr,
            Self::Aes128Cfb => AlgorithmId::Aes128Cfb,
            Self::Aes256Cfb => AlgorithmId::Aes256Cfb,
        }
    }

    const fn key_len(self) -> usize {
        match self {
            Self::Aes128Cbc | Self::Aes128Ctr | Self::Aes128Cfb => 16,
            Self::Aes256Cbc | Self::Aes256Ctr | Self::Aes256Cfb => 32,
        }
    }

    const fn is_cfb(self) -> bool {
        matches!(self, Self::Aes128Cfb | Self::Aes256Cfb)
    }
}

fn expect_raw_key(key: &KeyRef, expected_len: usize) -> Result<&[u8], CryptoError> {
    let bytes = key.raw_bytes()?;
    if bytes.len() != expected_len {
        return Err(CryptoError::InvalidKey {
            reason: format!("expected key length {expected_len}, got {}", bytes.len()),
        });
    }
    Ok(bytes)
}

fn expect_iv(iv: &[u8]) -> Result<&[u8], CryptoError> {
    if iv.len() != AES_BLOCK_LEN {
        return Err(CryptoError::InvalidNonce {
            reason: format!(
                "AES IV/nonce must be {AES_BLOCK_LEN} bytes, got {}",
                iv.len()
            ),
        });
    }
    Ok(iv)
}

fn encrypt_cbc_128(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let encryptor =
        CbcEncryptor::<Aes128>::new_from_slices(key, iv).map_err(|_| CryptoError::InvalidKey {
            reason: "aes-128-cbc key/iv init failed".to_owned(),
        })?;
    Ok(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

fn decrypt_cbc_128(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let decryptor =
        CbcDecryptor::<Aes128>::new_from_slices(key, iv).map_err(|_| CryptoError::InvalidKey {
            reason: "aes-128-cbc key/iv init failed".to_owned(),
        })?;
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| CryptoError::InvalidInput {
            reason: "aes-128-cbc PKCS7 unpad failed".to_owned(),
        })
}

fn encrypt_cbc_256(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let encryptor =
        CbcEncryptor::<Aes256>::new_from_slices(key, iv).map_err(|_| CryptoError::InvalidKey {
            reason: "aes-256-cbc key/iv init failed".to_owned(),
        })?;
    Ok(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

fn decrypt_cbc_256(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let decryptor =
        CbcDecryptor::<Aes256>::new_from_slices(key, iv).map_err(|_| CryptoError::InvalidKey {
            reason: "aes-256-cbc key/iv init failed".to_owned(),
        })?;
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| CryptoError::InvalidInput {
            reason: "aes-256-cbc PKCS7 unpad failed".to_owned(),
        })
}

fn apply_ctr_128(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut out = data.to_vec();
    let mut cipher =
        Ctr128BE::<Aes128>::new_from_slices(key, iv).map_err(|_| CryptoError::InvalidKey {
            reason: "aes-128-ctr key/iv init failed".to_owned(),
        })?;
    cipher.apply_keystream(&mut out);
    Ok(out)
}

fn apply_ctr_256(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut out = data.to_vec();
    let mut cipher =
        Ctr128BE::<Aes256>::new_from_slices(key, iv).map_err(|_| CryptoError::InvalidKey {
            reason: "aes-256-ctr key/iv init failed".to_owned(),
        })?;
    cipher.apply_keystream(&mut out);
    Ok(out)
}

fn encrypt_cfb128_128(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut out = plaintext.to_vec();
    cfb_mode::Encryptor::<Aes128>::new_from_slices(key, iv)
        .map_err(|_| CryptoError::InvalidKey {
            reason: "aes-128-cfb key/iv init failed".to_owned(),
        })?
        .encrypt(&mut out);
    Ok(out)
}

fn decrypt_cfb128_128(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut out = ciphertext.to_vec();
    cfb_mode::Decryptor::<Aes128>::new_from_slices(key, iv)
        .map_err(|_| CryptoError::InvalidKey {
            reason: "aes-128-cfb key/iv init failed".to_owned(),
        })?
        .decrypt(&mut out);
    Ok(out)
}

fn encrypt_cfb128_256(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut out = plaintext.to_vec();
    cfb_mode::Encryptor::<Aes256>::new_from_slices(key, iv)
        .map_err(|_| CryptoError::InvalidKey {
            reason: "aes-256-cfb key/iv init failed".to_owned(),
        })?
        .encrypt(&mut out);
    Ok(out)
}

fn decrypt_cfb128_256(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut out = ciphertext.to_vec();
    cfb_mode::Decryptor::<Aes256>::new_from_slices(key, iv)
        .map_err(|_| CryptoError::InvalidKey {
            reason: "aes-256-cfb key/iv init failed".to_owned(),
        })?
        .decrypt(&mut out);
    Ok(out)
}

fn encrypt_cfb8_128(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut out = plaintext.to_vec();
    cfb8::Encryptor::<Aes128>::new_from_slices(key, iv)
        .map_err(|_| CryptoError::InvalidKey {
            reason: "aes-128-cfb8 key/iv init failed".to_owned(),
        })?
        .encrypt(&mut out);
    Ok(out)
}

fn decrypt_cfb8_128(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut out = ciphertext.to_vec();
    cfb8::Decryptor::<Aes128>::new_from_slices(key, iv)
        .map_err(|_| CryptoError::InvalidKey {
            reason: "aes-128-cfb8 key/iv init failed".to_owned(),
        })?
        .decrypt(&mut out);
    Ok(out)
}

fn encrypt_cfb8_256(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut out = plaintext.to_vec();
    cfb8::Encryptor::<Aes256>::new_from_slices(key, iv)
        .map_err(|_| CryptoError::InvalidKey {
            reason: "aes-256-cfb8 key/iv init failed".to_owned(),
        })?
        .encrypt(&mut out);
    Ok(out)
}

fn decrypt_cfb8_256(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut out = ciphertext.to_vec();
    cfb8::Decryptor::<Aes256>::new_from_slices(key, iv)
        .map_err(|_| CryptoError::InvalidKey {
            reason: "aes-256-cfb8 key/iv init failed".to_owned(),
        })?
        .decrypt(&mut out);
    Ok(out)
}

/// AES-CFB 加密（显式 `segment_bits`）。
///
/// # Errors
///
/// 密钥/IV 非法、段长不支持或运算失败时返回 [`CryptoError`]。
pub fn encrypt_cfb(
    alg: SymmetricAlgorithm,
    key: &KeyRef,
    iv: &[u8],
    plaintext: &[u8],
    params: CfbParams,
) -> Result<Vec<u8>, CryptoError> {
    if !alg.is_cfb() {
        return Err(CryptoError::Unsupported {
            reason: format!("{} is not a CFB algorithm", alg.id()),
        });
    }
    let key_bytes = expect_raw_key(key, alg.key_len())?;
    let iv = expect_iv(iv)?;
    match (alg, params.segment_bits) {
        (SymmetricAlgorithm::Aes128Cfb, 128) => encrypt_cfb128_128(key_bytes, iv, plaintext),
        (SymmetricAlgorithm::Aes256Cfb, 128) => encrypt_cfb128_256(key_bytes, iv, plaintext),
        (SymmetricAlgorithm::Aes128Cfb, 8) => encrypt_cfb8_128(key_bytes, iv, plaintext),
        (SymmetricAlgorithm::Aes256Cfb, 8) => encrypt_cfb8_256(key_bytes, iv, plaintext),
        (_, bits) => Err(CryptoError::Unsupported {
            reason: format!("CFB segment_bits={bits} not implemented (supported: 8, 128)"),
        }),
    }
}

/// AES-CFB 解密（显式 `segment_bits`）。
///
/// # Errors
///
/// 密钥/IV 非法、段长不支持或运算失败时返回 [`CryptoError`]。
pub fn decrypt_cfb(
    alg: SymmetricAlgorithm,
    key: &KeyRef,
    iv: &[u8],
    ciphertext: &[u8],
    params: CfbParams,
) -> Result<Vec<u8>, CryptoError> {
    if !alg.is_cfb() {
        return Err(CryptoError::Unsupported {
            reason: format!("{} is not a CFB algorithm", alg.id()),
        });
    }
    let key_bytes = expect_raw_key(key, alg.key_len())?;
    let iv = expect_iv(iv)?;
    match (alg, params.segment_bits) {
        (SymmetricAlgorithm::Aes128Cfb, 128) => decrypt_cfb128_128(key_bytes, iv, ciphertext),
        (SymmetricAlgorithm::Aes256Cfb, 128) => decrypt_cfb128_256(key_bytes, iv, ciphertext),
        (SymmetricAlgorithm::Aes128Cfb, 8) => decrypt_cfb8_128(key_bytes, iv, ciphertext),
        (SymmetricAlgorithm::Aes256Cfb, 8) => decrypt_cfb8_256(key_bytes, iv, ciphertext),
        (_, bits) => Err(CryptoError::Unsupported {
            reason: format!("CFB segment_bits={bits} not implemented (supported: 8, 128)"),
        }),
    }
}

impl BlockCipher for SymmetricAlgorithm {
    fn encrypt(&self, key: &KeyRef, iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let key_bytes = expect_raw_key(key, self.key_len())?;
        let iv = expect_iv(iv)?;
        match self {
            Self::Aes128Cbc => encrypt_cbc_128(key_bytes, iv, plaintext),
            Self::Aes256Cbc => encrypt_cbc_256(key_bytes, iv, plaintext),
            Self::Aes128Ctr => apply_ctr_128(key_bytes, iv, plaintext),
            Self::Aes256Ctr => apply_ctr_256(key_bytes, iv, plaintext),
            Self::Aes128Cfb => encrypt_cfb128_128(key_bytes, iv, plaintext),
            Self::Aes256Cfb => encrypt_cfb128_256(key_bytes, iv, plaintext),
        }
    }

    fn decrypt(&self, key: &KeyRef, iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let key_bytes = expect_raw_key(key, self.key_len())?;
        let iv = expect_iv(iv)?;
        match self {
            Self::Aes128Cbc => decrypt_cbc_128(key_bytes, iv, ciphertext),
            Self::Aes256Cbc => decrypt_cbc_256(key_bytes, iv, ciphertext),
            Self::Aes128Ctr => apply_ctr_128(key_bytes, iv, ciphertext),
            Self::Aes256Ctr => apply_ctr_256(key_bytes, iv, ciphertext),
            Self::Aes128Cfb => decrypt_cfb128_128(key_bytes, iv, ciphertext),
            Self::Aes256Cfb => decrypt_cfb128_256(key_bytes, iv, ciphertext),
        }
    }
}
