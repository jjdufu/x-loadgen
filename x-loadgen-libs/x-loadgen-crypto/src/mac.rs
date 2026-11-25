//! HMAC 原语：`hmac-sha256` / `hmac-sha384` / `hmac-sha512` / `hmac-sm3`。

use hmac::{Hmac, KeyInit as HmacKeyInit, Mac as HmacMac};
use sha2::{Sha256, Sha384, Sha512};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::id::AlgorithmId;
use crate::material::KeyRef;
use crate::traits::{CryptoError, Mac};

type HmacSha256 = Hmac<Sha256>;
type HmacSha384 = Hmac<Sha384>;
type HmacSha512 = Hmac<Sha512>;
type HmacSm3 = Hmac<sm3::Sm3>;

/// RFC 5869 `HKDF-SHA256` 派生函数（不引入额外依赖，基于 `Hmac<Sha256>` 实现）。
///
/// # Errors
///
/// 当 `okm.len() > 255 * 32` 时返回 [`CryptoError::InvalidKey`]。
pub fn hkdf_sha256(
    salt: Option<&[u8]>,
    ikm: &[u8],
    info: &[u8],
    okm: &mut [u8],
) -> Result<(), CryptoError> {
    if okm.len() > 255 * 32 {
        return Err(CryptoError::InvalidKey {
            reason: "hkdf-sha256 output length exceeds 255 * 32".to_owned(),
        });
    }
    let zero_salt = [0u8; 32];
    let effective_salt = match salt {
        Some(s) if !s.is_empty() => s,
        _ => &zero_salt,
    };
    let mut prk_mac =
        <HmacSha256 as HmacKeyInit>::new_from_slice(effective_salt).map_err(|_| {
            CryptoError::InvalidKey {
                reason: "hkdf-sha256 extract key init failed".to_owned(),
            }
        })?;
    prk_mac.update(ikm);
    let prk = Zeroizing::new(prk_mac.finalize().into_bytes());

    let mut prev = Zeroizing::new([0u8; 32]);
    let mut has_prev = false;
    let mut written = 0usize;
    let mut counter = 1u8;
    while written < okm.len() {
        let mut mac = <HmacSha256 as HmacKeyInit>::new_from_slice(&prk).map_err(|_| {
            CryptoError::InvalidKey {
                reason: "hkdf-sha256 expand key init failed".to_owned(),
            }
        })?;
        if has_prev {
            mac.update(prev.as_slice());
        }
        mac.update(info);
        mac.update(&[counter]);
        let block = mac.finalize().into_bytes();
        prev.copy_from_slice(&block);
        has_prev = true;
        let to_copy = (okm.len() - written).min(32);
        okm[written..written + to_copy].copy_from_slice(&prev[..to_copy]);
        written += to_copy;
        counter = counter.saturating_add(1);
    }
    Ok(())
}

/// 本步已实现的 MAC 算法句柄。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacAlgorithm {
    /// `hmac-sha256`
    HmacSha256,
    /// `hmac-sha384`
    HmacSha384,
    /// `hmac-sha512`
    HmacSha512,
    /// `hmac-sm3`
    HmacSm3,
}

impl MacAlgorithm {
    /// 由闭集 id 解析。
    ///
    /// # Errors
    ///
    /// [`CryptoError::Unsupported`]：非本步 HMAC。
    pub fn from_id(id: AlgorithmId) -> Result<Self, CryptoError> {
        match id {
            AlgorithmId::HmacSha256 => Ok(Self::HmacSha256),
            AlgorithmId::HmacSha384 => Ok(Self::HmacSha384),
            AlgorithmId::HmacSha512 => Ok(Self::HmacSha512),
            AlgorithmId::HmacSm3 => Ok(Self::HmacSm3),
            other => Err(CryptoError::Unsupported {
                reason: format!("not an implemented MAC: {other}"),
            }),
        }
    }

    /// 对应闭集 id。
    #[must_use]
    pub const fn id(self) -> AlgorithmId {
        match self {
            Self::HmacSha256 => AlgorithmId::HmacSha256,
            Self::HmacSha384 => AlgorithmId::HmacSha384,
            Self::HmacSha512 => AlgorithmId::HmacSha512,
            Self::HmacSm3 => AlgorithmId::HmacSm3,
        }
    }

    /// 标签字节长度。
    #[must_use]
    pub const fn tag_len(self) -> usize {
        match self {
            Self::HmacSha256 | Self::HmacSm3 => 32,
            Self::HmacSha384 => 48,
            Self::HmacSha512 => 64,
        }
    }
}

impl Mac for MacAlgorithm {
    fn sign(&self, key: &KeyRef, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let key_bytes = key.raw_bytes()?;
        match self {
            Self::HmacSha256 => {
                let mut mac =
                    <HmacSha256 as HmacKeyInit>::new_from_slice(key_bytes).map_err(|_| {
                        CryptoError::InvalidKey {
                            reason: "hmac-sha256 key init failed".to_owned(),
                        }
                    })?;
                mac.update(message);
                Ok(mac.finalize().into_bytes().to_vec())
            }
            Self::HmacSha384 => {
                let mut mac =
                    <HmacSha384 as HmacKeyInit>::new_from_slice(key_bytes).map_err(|_| {
                        CryptoError::InvalidKey {
                            reason: "hmac-sha384 key init failed".to_owned(),
                        }
                    })?;
                mac.update(message);
                Ok(mac.finalize().into_bytes().to_vec())
            }
            Self::HmacSha512 => {
                let mut mac =
                    <HmacSha512 as HmacKeyInit>::new_from_slice(key_bytes).map_err(|_| {
                        CryptoError::InvalidKey {
                            reason: "hmac-sha512 key init failed".to_owned(),
                        }
                    })?;
                mac.update(message);
                Ok(mac.finalize().into_bytes().to_vec())
            }
            Self::HmacSm3 => {
                let mut mac =
                    <HmacSm3 as HmacKeyInit>::new_from_slice(key_bytes).map_err(|_| {
                        CryptoError::InvalidKey {
                            reason: "hmac-sm3 key init failed".to_owned(),
                        }
                    })?;
                mac.update(message);
                Ok(mac.finalize().into_bytes().to_vec())
            }
        }
    }

    fn verify(&self, key: &KeyRef, message: &[u8], tag: &[u8]) -> Result<(), CryptoError> {
        let expected = self.sign(key, message)?;
        if expected.len() != tag.len() || !bool::from(expected.as_slice().ct_eq(tag)) {
            return Err(CryptoError::AuthenticationFailed);
        }
        Ok(())
    }
}
