//! Legacy 算法实现入口：默认拒绝，须显式 `allow_insecure`。
//!
//! 每个公开函数**开头**调用 [`crate::legacy::assert_not_legacy_or_allowed`]。

use aes::Aes128;
use aes::Aes256;
use cbc::{Decryptor as CbcDecryptor, Encryptor as CbcEncryptor};
use cipher::{BlockDecryptMut, BlockEncryptMut, KeyInit, KeyIvInit, block_padding::Pkcs7};
use des::Des;
use hmac::{Hmac, Mac as HmacMac};
use md5::Md5;
use subtle::ConstantTimeEq;

use crate::id::AlgorithmId;
use crate::legacy::assert_not_legacy_or_allowed;
use crate::material::KeyRef;
use crate::traits::CryptoError;

const DES_BLOCK: usize = 8;
const DES_KEY_LEN: usize = 8;

type HmacMd5 = Hmac<Md5>;

fn expect_len<'a>(label: &str, bytes: &'a [u8], expected: usize) -> Result<&'a [u8], CryptoError> {
    if bytes.len() != expected {
        return Err(CryptoError::InvalidKey {
            reason: format!("{label} must be {expected} bytes, got {}", bytes.len()),
        });
    }
    Ok(bytes)
}

/// AES-128-ECB-PKCS7 加密（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或密钥/填充错误。
pub fn aes128_ecb_encrypt(
    allow_insecure: bool,
    key: &KeyRef,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::Aes128Ecb, allow_insecure)?;
    let key_bytes = expect_len("aes-128-ecb key", key.raw_bytes()?, 16)?;
    let arr: [u8; 16] = key_bytes.try_into().map_err(|_| CryptoError::InvalidKey {
        reason: "aes-128-ecb key".to_owned(),
    })?;
    let encryptor = ecb::Encryptor::<Aes128>::new(&arr.into());
    Ok(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

/// AES-128-ECB-PKCS7 解密（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或密钥/填充错误。
pub fn aes128_ecb_decrypt(
    allow_insecure: bool,
    key: &KeyRef,
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::Aes128Ecb, allow_insecure)?;
    let key_bytes = expect_len("aes-128-ecb key", key.raw_bytes()?, 16)?;
    let arr: [u8; 16] = key_bytes.try_into().map_err(|_| CryptoError::InvalidKey {
        reason: "aes-128-ecb key".to_owned(),
    })?;
    let decryptor = ecb::Decryptor::<Aes128>::new(&arr.into());
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| CryptoError::InvalidInput {
            reason: "aes-128-ecb PKCS7 unpad failed".to_owned(),
        })
}

/// AES-256-ECB-PKCS7 加密（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或密钥/填充错误。
pub fn aes256_ecb_encrypt(
    allow_insecure: bool,
    key: &KeyRef,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::Aes256Ecb, allow_insecure)?;
    let key_bytes = expect_len("aes-256-ecb key", key.raw_bytes()?, 32)?;
    let arr: [u8; 32] = key_bytes.try_into().map_err(|_| CryptoError::InvalidKey {
        reason: "aes-256-ecb key".to_owned(),
    })?;
    let encryptor = ecb::Encryptor::<Aes256>::new(&arr.into());
    Ok(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

/// AES-256-ECB-PKCS7 解密（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或密钥/填充错误。
pub fn aes256_ecb_decrypt(
    allow_insecure: bool,
    key: &KeyRef,
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::Aes256Ecb, allow_insecure)?;
    let key_bytes = expect_len("aes-256-ecb key", key.raw_bytes()?, 32)?;
    let arr: [u8; 32] = key_bytes.try_into().map_err(|_| CryptoError::InvalidKey {
        reason: "aes-256-ecb key".to_owned(),
    })?;
    let decryptor = ecb::Decryptor::<Aes256>::new(&arr.into());
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| CryptoError::InvalidInput {
            reason: "aes-256-ecb PKCS7 unpad failed".to_owned(),
        })
}

/// DES-CBC-PKCS5（≡ PKCS7 for 8-byte blocks）加密（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或密钥/IV/填充错误。
pub fn des_cbc_encrypt(
    allow_insecure: bool,
    key: &KeyRef,
    iv: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::DesCbcPkcs5, allow_insecure)?;
    let key_bytes = expect_len("des key", key.raw_bytes()?, DES_KEY_LEN)?;
    if iv.len() != DES_BLOCK {
        return Err(CryptoError::InvalidNonce {
            reason: format!("des-cbc IV must be {DES_BLOCK} bytes, got {}", iv.len()),
        });
    }
    let encryptor = CbcEncryptor::<Des>::new_from_slices(key_bytes, iv).map_err(|_| {
        CryptoError::InvalidKey {
            reason: "des-cbc key/iv init failed".to_owned(),
        }
    })?;
    Ok(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

/// DES-CBC-PKCS5 解密（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或密钥/IV/填充错误。
pub fn des_cbc_decrypt(
    allow_insecure: bool,
    key: &KeyRef,
    iv: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::DesCbcPkcs5, allow_insecure)?;
    let key_bytes = expect_len("des key", key.raw_bytes()?, DES_KEY_LEN)?;
    if iv.len() != DES_BLOCK {
        return Err(CryptoError::InvalidNonce {
            reason: format!("des-cbc IV must be {DES_BLOCK} bytes, got {}", iv.len()),
        });
    }
    let decryptor = CbcDecryptor::<Des>::new_from_slices(key_bytes, iv).map_err(|_| {
        CryptoError::InvalidKey {
            reason: "des-cbc key/iv init failed".to_owned(),
        }
    })?;
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| CryptoError::InvalidInput {
            reason: "des-cbc PKCS7 unpad failed".to_owned(),
        })
}

/// HMAC-MD5（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或密钥初始化失败。
pub fn hmac_md5_sign(
    allow_insecure: bool,
    key: &KeyRef,
    message: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::HmacMd5, allow_insecure)?;
    let key_bytes = key.raw_bytes()?;
    let mut mac =
        <HmacMd5 as HmacMac>::new_from_slice(key_bytes).map_err(|_| CryptoError::InvalidKey {
            reason: "hmac-md5 key init failed".to_owned(),
        })?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().to_vec())
}

/// HMAC-MD5 校验（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] / [`CryptoError::AuthenticationFailed`]。
pub fn hmac_md5_verify(
    allow_insecure: bool,
    key: &KeyRef,
    message: &[u8],
    tag: &[u8],
) -> Result<(), CryptoError> {
    let expected = hmac_md5_sign(allow_insecure, key, message)?;
    if expected.len() != tag.len() || !bool::from(expected.as_slice().ct_eq(tag)) {
        return Err(CryptoError::AuthenticationFailed);
    }
    Ok(())
}
