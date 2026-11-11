//! HMAC 原语：`hmac-sha256` / `hmac-sha384` / `hmac-sha512` / `hmac-sm3`。

use hmac::{Hmac, Mac as HmacMac};
use sha2::{Sha256, Sha384, Sha512};
use subtle::ConstantTimeEq;

use crate::id::AlgorithmId;
use crate::material::KeyRef;
use crate::traits::{CryptoError, Mac};

type HmacSha256 = Hmac<Sha256>;
type HmacSha384 = Hmac<Sha384>;
type HmacSha512 = Hmac<Sha512>;
type HmacSm3 = Hmac<sm3::Sm3>;

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
                let mut mac = <HmacSha256 as HmacMac>::new_from_slice(key_bytes).map_err(|_| {
                    CryptoError::InvalidKey {
                        reason: "hmac-sha256 key init failed".to_owned(),
                    }
                })?;
                mac.update(message);
                Ok(mac.finalize().into_bytes().to_vec())
            }
            Self::HmacSha384 => {
                let mut mac = <HmacSha384 as HmacMac>::new_from_slice(key_bytes).map_err(|_| {
                    CryptoError::InvalidKey {
                        reason: "hmac-sha384 key init failed".to_owned(),
                    }
                })?;
                mac.update(message);
                Ok(mac.finalize().into_bytes().to_vec())
            }
            Self::HmacSha512 => {
                let mut mac = <HmacSha512 as HmacMac>::new_from_slice(key_bytes).map_err(|_| {
                    CryptoError::InvalidKey {
                        reason: "hmac-sha512 key init failed".to_owned(),
                    }
                })?;
                mac.update(message);
                Ok(mac.finalize().into_bytes().to_vec())
            }
            Self::HmacSm3 => {
                let mut mac = <HmacSm3 as HmacMac>::new_from_slice(key_bytes).map_err(|_| {
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
