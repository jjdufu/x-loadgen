//! 国密原语：SM4（CBC·CTR·ECB）与 SM2 签名；HMAC-SM3 见 [`crate::mac`]。
//!
//! # SM4-GCM
//!
//! **不**注册 `sm4-gcm`。国密 AEAD 替代请使用命名套件
//! [`crate::suite::SUITE_GM_SM4_HMAC_SM3`]（SM4-CBC + HMAC-SM3）。
//!
//! # SM9
//!
//! 不实现；请求 `sm9-*` 经 [`crate::registry::parse_algorithm_id`] 返回
//! [`CryptoError::Unsupported`]。

use cbc::{Decryptor as CbcDecryptor, Encryptor as CbcEncryptor};
use cipher::{
    BlockDecryptMut, BlockEncryptMut, KeyInit, KeyIvInit, StreamCipher, block_padding::Pkcs7,
};
use ctr::Ctr128BE;
use libsm::sm2::signature::{SigCtx, Signature as Sm2Signature};
use sm4::Sm4;

use crate::id::AlgorithmId;
use crate::legacy::assert_not_legacy_or_allowed;
use crate::material::KeyRef;
use crate::traits::{AsymSign, BlockCipher, CryptoError};

/// SM4 分组 / IV 长度（字节）。
pub const SM4_BLOCK_LEN: usize = 16;
/// SM4 密钥长度（字节）。
pub const SM4_KEY_LEN: usize = 16;
/// SM3 摘要长度（字节）。
pub const SM3_OUTPUT_LEN: usize = 32;
/// SM2 默认 `user_id`（GM/T 0009 常见缺省）。
pub const SM2_DEFAULT_USER_ID: &str = "1234567812345678";

/// 国密 SM4 对称算法（非 legacy ECB；ECB 见 [`crate::legacy_algs`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sm4Algorithm {
    /// `sm4-cbc-pkcs7`
    Sm4CbcPkcs7,
    /// `sm4-ctr`
    Sm4Ctr,
}

impl Sm4Algorithm {
    /// 由闭集 id 解析（不含 ECB legacy）。
    ///
    /// # Errors
    ///
    /// [`CryptoError::Unsupported`]：非本模块 SM4 模式。
    pub fn from_id(id: AlgorithmId) -> Result<Self, CryptoError> {
        match id {
            AlgorithmId::Sm4CbcPkcs7 => Ok(Self::Sm4CbcPkcs7),
            AlgorithmId::Sm4Ctr => Ok(Self::Sm4Ctr),
            other => Err(CryptoError::Unsupported {
                reason: format!("not an implemented SM4 mode: {other}"),
            }),
        }
    }

    /// 对应闭集 id。
    #[must_use]
    pub const fn id(self) -> AlgorithmId {
        match self {
            Self::Sm4CbcPkcs7 => AlgorithmId::Sm4CbcPkcs7,
            Self::Sm4Ctr => AlgorithmId::Sm4Ctr,
        }
    }
}

fn expect_sm4_key(key: &KeyRef) -> Result<&[u8], CryptoError> {
    let bytes = key.raw_bytes()?;
    if bytes.len() != SM4_KEY_LEN {
        return Err(CryptoError::InvalidKey {
            reason: format!("SM4 key must be {SM4_KEY_LEN} bytes, got {}", bytes.len()),
        });
    }
    Ok(bytes)
}

fn expect_sm4_iv(iv: &[u8]) -> Result<&[u8], CryptoError> {
    if iv.len() != SM4_BLOCK_LEN {
        return Err(CryptoError::InvalidNonce {
            reason: format!(
                "SM4 IV/nonce must be {SM4_BLOCK_LEN} bytes, got {}",
                iv.len()
            ),
        });
    }
    Ok(iv)
}

fn encrypt_sm4_cbc(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let encryptor =
        CbcEncryptor::<Sm4>::new_from_slices(key, iv).map_err(|_| CryptoError::InvalidKey {
            reason: "sm4-cbc key/iv init failed".to_owned(),
        })?;
    Ok(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

fn decrypt_sm4_cbc(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let decryptor =
        CbcDecryptor::<Sm4>::new_from_slices(key, iv).map_err(|_| CryptoError::InvalidKey {
            reason: "sm4-cbc key/iv init failed".to_owned(),
        })?;
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| CryptoError::InvalidInput {
            reason: "sm4-cbc PKCS7 unpad failed".to_owned(),
        })
}

fn apply_sm4_ctr(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut out = data.to_vec();
    let mut cipher =
        Ctr128BE::<Sm4>::new_from_slices(key, iv).map_err(|_| CryptoError::InvalidKey {
            reason: "sm4-ctr key/iv init failed".to_owned(),
        })?;
    cipher.apply_keystream(&mut out);
    Ok(out)
}

impl BlockCipher for Sm4Algorithm {
    fn encrypt(&self, key: &KeyRef, iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let key_bytes = expect_sm4_key(key)?;
        let iv = expect_sm4_iv(iv)?;
        match self {
            Self::Sm4CbcPkcs7 => encrypt_sm4_cbc(key_bytes, iv, plaintext),
            Self::Sm4Ctr => apply_sm4_ctr(key_bytes, iv, plaintext),
        }
    }

    fn decrypt(&self, key: &KeyRef, iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let key_bytes = expect_sm4_key(key)?;
        let iv = expect_sm4_iv(iv)?;
        match self {
            Self::Sm4CbcPkcs7 => decrypt_sm4_cbc(key_bytes, iv, ciphertext),
            Self::Sm4Ctr => apply_sm4_ctr(key_bytes, iv, ciphertext),
        }
    }
}

/// `id-ecPublicKey`（RFC 5480）。
const EC_PUBLIC_KEY_OID: der::asn1::ObjectIdentifier =
    der::asn1::ObjectIdentifier::new_unwrap("1.2.840.10045.2.1");
/// SM2 曲线 OID（GB/T 33560 / `sm2p256v1`）。
const SM2_CURVE_OID: der::asn1::ObjectIdentifier =
    der::asn1::ObjectIdentifier::new_unwrap("1.2.156.10197.1.301");
/// SM2 私钥标量长度（字节）。
const SM2_PRIVATE_LEN: usize = 32;

fn sm2_pem_error(what: &str) -> CryptoError {
    CryptoError::InvalidKey {
        reason: format!("invalid SM2 {what} PEM"),
    }
}

fn ensure_sm2_algorithm(
    oid: der::asn1::ObjectIdentifier,
    parameters: Option<der::asn1::ObjectIdentifier>,
    what: &str,
) -> Result<(), CryptoError> {
    if oid == EC_PUBLIC_KEY_OID && parameters == Some(SM2_CURVE_OID) {
        Ok(())
    } else {
        Err(CryptoError::InvalidKey {
            reason: format!("{what} PEM is not an SM2 (id-ecPublicKey + sm2p256v1) key"),
        })
    }
}

/// 解析 SM2 SPKI PEM（`PUBLIC KEY`，id-ecPublicKey + sm2p256v1），返回 SEC1 未压缩点。
fn sm2_public_point_from_pem(pem: &str) -> Result<Vec<u8>, CryptoError> {
    use der::DecodePem;
    let info = spki::SubjectPublicKeyInfoOwned::from_pem(pem.trim().as_bytes())
        .map_err(|_| sm2_pem_error("public"))?;
    let params = info
        .algorithm
        .parameters
        .as_ref()
        .map(der::Any::decode_as::<der::asn1::ObjectIdentifier>)
        .transpose()
        .map_err(|_| sm2_pem_error("public"))?;
    ensure_sm2_algorithm(info.algorithm.oid, params, "public")?;
    info.subject_public_key
        .as_bytes()
        .map(<[u8]>::to_vec)
        .ok_or_else(|| sm2_pem_error("public"))
}

/// 解析 SM2 PKCS#8 PEM（`PRIVATE KEY`），返回 32 字节私钥标量。
fn sm2_private_scalar_from_pem(pem: &str) -> Result<zeroize::Zeroizing<Vec<u8>>, CryptoError> {
    use der::Decode;
    let (label, doc) =
        der::pem::decode_vec(pem.trim().as_bytes()).map_err(|_| sm2_pem_error("private"))?;
    let doc = zeroize::Zeroizing::new(doc);
    if label != "PRIVATE KEY" {
        return Err(sm2_pem_error("private"));
    }
    let info = pkcs8::PrivateKeyInfo::from_der(&doc).map_err(|_| sm2_pem_error("private"))?;
    let params = info
        .algorithm
        .parameters
        .map(der::AnyRef::decode_as::<der::asn1::ObjectIdentifier>)
        .transpose()
        .map_err(|_| sm2_pem_error("private"))?;
    let ec =
        sec1::EcPrivateKey::from_der(info.private_key).map_err(|_| sm2_pem_error("private"))?;
    let curve = params.or_else(|| ec.parameters.and_then(sec1::EcParameters::named_curve));
    ensure_sm2_algorithm(info.algorithm.oid, curve, "private")?;
    if ec.private_key.len() != SM2_PRIVATE_LEN {
        return Err(sm2_pem_error("private"));
    }
    Ok(zeroize::Zeroizing::new(ec.private_key.to_vec()))
}

/// SM2 公钥（SEC1 未压缩点 65 字节）编码为 SPKI PEM（与 `OpenSSL` 3 `genpkey -algorithm SM2` 一致）。
///
/// # Errors
///
/// 点格式非法或 DER 编码失败时返回错误。
pub fn sm2_public_key_to_pem(point: &[u8]) -> Result<String, CryptoError> {
    use der::EncodePem;
    SigCtx::new()
        .load_pubkey(point)
        .map_err(|e| CryptoError::InvalidKey {
            reason: format!("invalid SM2 public key: {e}"),
        })?;
    let info = spki::SubjectPublicKeyInfoOwned {
        algorithm: spki::AlgorithmIdentifierOwned {
            oid: EC_PUBLIC_KEY_OID,
            parameters: Some(
                der::Any::encode_from(&SM2_CURVE_OID).map_err(|_| sm2_pem_error("public"))?,
            ),
        },
        subject_public_key: der::asn1::BitString::from_bytes(point)
            .map_err(|_| sm2_pem_error("public"))?,
    };
    info.to_pem(der::pem::LineEnding::LF)
        .map_err(|_| sm2_pem_error("public"))
}

/// SM2 私钥（32 字节标量）编码为 PKCS#8 PEM（内含 SEC1 `ECPrivateKey` 与公钥点）。
///
/// # Errors
///
/// 标量非法或 DER 编码失败时返回错误。
pub fn sm2_private_key_to_pem(scalar: &[u8]) -> Result<zeroize::Zeroizing<String>, CryptoError> {
    use der::Encode;
    let ctx = SigCtx::new();
    let sk = ctx
        .load_seckey(scalar)
        .map_err(|e| CryptoError::InvalidKey {
            reason: format!("invalid SM2 private key: {e}"),
        })?;
    let pk = ctx.pk_from_sk(&sk).map_err(|e| CryptoError::InvalidKey {
        reason: format!("derive SM2 public from private: {e}"),
    })?;
    let point = ctx
        .serialize_pubkey(&pk, false)
        .map_err(|e| CryptoError::InvalidKey {
            reason: format!("serialize SM2 public key: {e}"),
        })?;
    let ec = sec1::EcPrivateKey {
        private_key: scalar,
        parameters: None,
        public_key: Some(&point),
    };
    let ec_der = zeroize::Zeroizing::new(ec.to_der().map_err(|_| sm2_pem_error("private"))?);
    let params = der::AnyRef::from(&SM2_CURVE_OID);
    let info = pkcs8::PrivateKeyInfo {
        algorithm: spki::AlgorithmIdentifierRef {
            oid: EC_PUBLIC_KEY_OID,
            parameters: Some(params),
        },
        private_key: &ec_der,
        public_key: None,
    };
    let doc = zeroize::Zeroizing::new(info.to_der().map_err(|_| sm2_pem_error("private"))?);
    der::pem::encode_string("PRIVATE KEY", der::pem::LineEnding::LF, &doc)
        .map(zeroize::Zeroizing::new)
        .map_err(|_| sm2_pem_error("private"))
}

/// 解析 SM2 公钥：`KeyRef::Raw`（SEC1 点）或 `KeyRef::Pem`（SPKI）。
pub(crate) fn parse_sm2_public(key: &KeyRef) -> Result<libsm::sm2::signature::Pubkey, CryptoError> {
    let point = match key {
        KeyRef::Pem { pem, .. } => sm2_public_point_from_pem(pem)?,
        KeyRef::Raw { bytes, .. } => bytes.as_slice().to_vec(),
    };
    let ctx = SigCtx::new();
    ctx.load_pubkey(&point)
        .map_err(|e| CryptoError::InvalidKey {
            reason: format!("invalid SM2 public key: {e}"),
        })
}

/// 解析 SM2 私钥：`KeyRef::Raw`（32 字节标量）或 `KeyRef::Pem`（PKCS#8）。
pub(crate) fn parse_sm2_private(
    key: &KeyRef,
) -> Result<libsm::sm2::signature::Seckey, CryptoError> {
    let ctx = SigCtx::new();
    let load = |bytes: &[u8]| {
        ctx.load_seckey(bytes).map_err(|e| CryptoError::InvalidKey {
            reason: format!("invalid SM2 private key: {e}"),
        })
    };
    match key {
        KeyRef::Pem { pem, .. } => load(&sm2_private_scalar_from_pem(pem)?),
        KeyRef::Raw { bytes, .. } => load(bytes.as_slice()),
    }
}

/// SM2 签名句柄（可配置 `user_id`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sm2SignAlgorithm {
    /// GM/T 用户标识；默认 [`SM2_DEFAULT_USER_ID`]。
    pub user_id: String,
}

impl Default for Sm2SignAlgorithm {
    fn default() -> Self {
        Self {
            user_id: SM2_DEFAULT_USER_ID.to_owned(),
        }
    }
}

impl Sm2SignAlgorithm {
    /// 指定 `user_id`。
    #[must_use]
    pub fn with_user_id(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
        }
    }

    /// 由闭集 id 解析（默认 `user_id`）。
    ///
    /// # Errors
    ///
    /// [`CryptoError::Unsupported`]：非 `sm2-sign`。
    pub fn from_id(id: AlgorithmId) -> Result<Self, CryptoError> {
        match id {
            AlgorithmId::Sm2Sign => Ok(Self::default()),
            other => Err(CryptoError::Unsupported {
                reason: format!("not sm2-sign: {other}"),
            }),
        }
    }

    /// 对应闭集 id。
    #[must_use]
    pub const fn id(&self) -> AlgorithmId {
        AlgorithmId::Sm2Sign
    }
}

impl AsymSign for Sm2SignAlgorithm {
    fn sign(&self, signer: &KeyRef, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let sk = parse_sm2_private(signer)?;
        let ctx = SigCtx::new();
        let pk = ctx.pk_from_sk(&sk).map_err(|e| CryptoError::InvalidKey {
            reason: format!("derive SM2 public from private: {e}"),
        })?;
        let digest =
            ctx.hash(&self.user_id, &pk, message)
                .map_err(|e| CryptoError::OperationFailed {
                    reason: format!("sm2 hash: {e}"),
                })?;
        let sig = ctx
            .sign_raw(&digest, &sk)
            .map_err(|e| CryptoError::OperationFailed {
                reason: format!("sm2 sign: {e}"),
            })?;
        Ok(sig.der_encode())
    }

    fn verify(
        &self,
        verifier: &KeyRef,
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), CryptoError> {
        let pk = parse_sm2_public(verifier)?;
        let ctx = SigCtx::new();
        let sig = Sm2Signature::der_decode(signature).map_err(|e| CryptoError::InvalidInput {
            reason: format!("invalid SM2 DER signature: {e}"),
        })?;
        let digest =
            ctx.hash(&self.user_id, &pk, message)
                .map_err(|e| CryptoError::OperationFailed {
                    reason: format!("sm2 hash: {e}"),
                })?;
        let ok = ctx
            .verify_raw(&digest, &pk, &sig)
            .map_err(|e| CryptoError::OperationFailed {
                reason: format!("sm2 verify: {e}"),
            })?;
        if ok {
            Ok(())
        } else {
            Err(CryptoError::AuthenticationFailed)
        }
    }
}

/// SM4-ECB-PKCS7（legacy）：必须显式 `allow_insecure`。
///
/// # Errors
///
/// - [`CryptoError::LegacyNotAllowed`]
/// - 密钥/填充非法
pub fn sm4_ecb_encrypt(
    allow_insecure: bool,
    key: &KeyRef,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::Sm4EcbPkcs7, allow_insecure)?;
    let key_bytes = expect_sm4_key(key)?;
    let key_arr: [u8; SM4_KEY_LEN] = key_bytes.try_into().map_err(|_| CryptoError::InvalidKey {
        reason: "sm4-ecb key length".to_owned(),
    })?;
    let encryptor = ecb::Encryptor::<Sm4>::new(&key_arr.into());
    Ok(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

/// SM4-ECB-PKCS7 解密（legacy）。
///
/// # Errors
///
/// - [`CryptoError::LegacyNotAllowed`]
/// - 密钥/填充非法
pub fn sm4_ecb_decrypt(
    allow_insecure: bool,
    key: &KeyRef,
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::Sm4EcbPkcs7, allow_insecure)?;
    let key_bytes = expect_sm4_key(key)?;
    let key_arr: [u8; SM4_KEY_LEN] = key_bytes.try_into().map_err(|_| CryptoError::InvalidKey {
        reason: "sm4-ecb key length".to_owned(),
    })?;
    let decryptor = ecb::Decryptor::<Sm4>::new(&key_arr.into());
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| CryptoError::InvalidInput {
            reason: "sm4-ecb PKCS7 unpad failed".to_owned(),
        })
}
