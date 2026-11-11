//! AEAD 原语：AES-GCM（128/192/256）与 ChaCha20-Poly1305。
//!
//! `seal`/`open` 密文编码为 **ciphertext ‖ tag**（nonce 由调用方单独传入）。

use aes::Aes192;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes128Gcm, Aes256Gcm, AesGcm, Nonce as AesGcmNonce};
use chacha20poly1305::{ChaCha20Poly1305, Nonce as ChaNonce};

use crate::id::AlgorithmId;
use crate::material::KeyRef;
use crate::traits::{AeadSeal, CryptoError};

/// AES-GCM / ChaCha20-Poly1305 的 nonce 长度（字节）。
pub const AEAD_NONCE_LEN: usize = 12;
/// 认证标签长度（字节）。
pub const AEAD_TAG_LEN: usize = 16;

type Aes192Gcm = AesGcm<Aes192, aes_gcm::aead::consts::U12>;

/// 本步已实现的 AEAD 算法句柄（可经注册表取得）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AeadAlgorithm {
    /// `aes-128-gcm`
    Aes128Gcm,
    /// `aes-192-gcm`
    Aes192Gcm,
    /// `aes-256-gcm`
    Aes256Gcm,
    /// `chacha20-poly1305`
    Chacha20Poly1305,
}

impl AeadAlgorithm {
    /// 由闭集 id 解析；非本步 AEAD 则报错。
    ///
    /// # Errors
    ///
    /// [`CryptoError::Unsupported`]：id 不是已实现的 AEAD。
    pub fn from_id(id: AlgorithmId) -> Result<Self, CryptoError> {
        match id {
            AlgorithmId::Aes128Gcm => Ok(Self::Aes128Gcm),
            AlgorithmId::Aes192Gcm => Ok(Self::Aes192Gcm),
            AlgorithmId::Aes256Gcm => Ok(Self::Aes256Gcm),
            AlgorithmId::Chacha20Poly1305 => Ok(Self::Chacha20Poly1305),
            other => Err(CryptoError::Unsupported {
                reason: format!("not an implemented AEAD: {other}"),
            }),
        }
    }

    /// 对应闭集 id。
    #[must_use]
    pub const fn id(self) -> AlgorithmId {
        match self {
            Self::Aes128Gcm => AlgorithmId::Aes128Gcm,
            Self::Aes192Gcm => AlgorithmId::Aes192Gcm,
            Self::Aes256Gcm => AlgorithmId::Aes256Gcm,
            Self::Chacha20Poly1305 => AlgorithmId::Chacha20Poly1305,
        }
    }

    const fn key_len(self) -> usize {
        match self {
            Self::Aes128Gcm => 16,
            Self::Aes192Gcm => 24,
            Self::Aes256Gcm | Self::Chacha20Poly1305 => 32,
        }
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

fn expect_nonce(nonce: &[u8]) -> Result<&[u8], CryptoError> {
    if nonce.len() != AEAD_NONCE_LEN {
        return Err(CryptoError::InvalidNonce {
            reason: format!(
                "AEAD nonce must be {AEAD_NONCE_LEN} bytes, got {}",
                nonce.len()
            ),
        });
    }
    Ok(nonce)
}

fn map_aead_encrypt(err: aes_gcm::Error) -> CryptoError {
    let _ = err;
    CryptoError::OperationFailed {
        reason: "AEAD encrypt failed".to_owned(),
    }
}

fn map_aead_decrypt(err: aes_gcm::Error) -> CryptoError {
    let _ = err;
    CryptoError::AuthenticationFailed
}

impl AeadSeal for AeadAlgorithm {
    fn seal(
        &self,
        key: &KeyRef,
        nonce: &[u8],
        aad: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let key_bytes = expect_raw_key(key, self.key_len())?;
        let nonce = expect_nonce(nonce)?;
        let payload = Payload {
            msg: plaintext,
            aad,
        };
        match self {
            Self::Aes128Gcm => {
                let cipher =
                    Aes128Gcm::new_from_slice(key_bytes).map_err(|_| CryptoError::InvalidKey {
                        reason: "aes-128-gcm key init failed".to_owned(),
                    })?;
                cipher
                    .encrypt(AesGcmNonce::from_slice(nonce), payload)
                    .map_err(map_aead_encrypt)
            }
            Self::Aes192Gcm => {
                let cipher =
                    Aes192Gcm::new_from_slice(key_bytes).map_err(|_| CryptoError::InvalidKey {
                        reason: "aes-192-gcm key init failed".to_owned(),
                    })?;
                cipher
                    .encrypt(AesGcmNonce::from_slice(nonce), payload)
                    .map_err(map_aead_encrypt)
            }
            Self::Aes256Gcm => {
                let cipher =
                    Aes256Gcm::new_from_slice(key_bytes).map_err(|_| CryptoError::InvalidKey {
                        reason: "aes-256-gcm key init failed".to_owned(),
                    })?;
                cipher
                    .encrypt(AesGcmNonce::from_slice(nonce), payload)
                    .map_err(map_aead_encrypt)
            }
            Self::Chacha20Poly1305 => {
                let cipher = ChaCha20Poly1305::new_from_slice(key_bytes).map_err(|_| {
                    CryptoError::InvalidKey {
                        reason: "chacha20-poly1305 key init failed".to_owned(),
                    }
                })?;
                cipher
                    .encrypt(ChaNonce::from_slice(nonce), payload)
                    .map_err(|e| {
                        let _ = e;
                        CryptoError::OperationFailed {
                            reason: "AEAD encrypt failed".to_owned(),
                        }
                    })
            }
        }
    }

    fn open(
        &self,
        key: &KeyRef,
        nonce: &[u8],
        aad: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        if ciphertext.len() < AEAD_TAG_LEN {
            return Err(CryptoError::InvalidInput {
                reason: format!("ciphertext shorter than tag ({AEAD_TAG_LEN} bytes)"),
            });
        }
        let key_bytes = expect_raw_key(key, self.key_len())?;
        let nonce = expect_nonce(nonce)?;
        let payload = Payload {
            msg: ciphertext,
            aad,
        };
        match self {
            Self::Aes128Gcm => {
                let cipher =
                    Aes128Gcm::new_from_slice(key_bytes).map_err(|_| CryptoError::InvalidKey {
                        reason: "aes-128-gcm key init failed".to_owned(),
                    })?;
                cipher
                    .decrypt(AesGcmNonce::from_slice(nonce), payload)
                    .map_err(map_aead_decrypt)
            }
            Self::Aes192Gcm => {
                let cipher =
                    Aes192Gcm::new_from_slice(key_bytes).map_err(|_| CryptoError::InvalidKey {
                        reason: "aes-192-gcm key init failed".to_owned(),
                    })?;
                cipher
                    .decrypt(AesGcmNonce::from_slice(nonce), payload)
                    .map_err(map_aead_decrypt)
            }
            Self::Aes256Gcm => {
                let cipher =
                    Aes256Gcm::new_from_slice(key_bytes).map_err(|_| CryptoError::InvalidKey {
                        reason: "aes-256-gcm key init failed".to_owned(),
                    })?;
                cipher
                    .decrypt(AesGcmNonce::from_slice(nonce), payload)
                    .map_err(map_aead_decrypt)
            }
            Self::Chacha20Poly1305 => {
                let cipher = ChaCha20Poly1305::new_from_slice(key_bytes).map_err(|_| {
                    CryptoError::InvalidKey {
                        reason: "chacha20-poly1305 key init failed".to_owned(),
                    }
                })?;
                cipher
                    .decrypt(ChaNonce::from_slice(nonce), payload)
                    .map_err(|e| {
                        let _ = e;
                        CryptoError::AuthenticationFailed
                    })
            }
        }
    }
}
