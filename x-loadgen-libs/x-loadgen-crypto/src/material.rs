//! 密钥材料引用与 nonce 策略（软件 only，无 HSM 句柄）。

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::traits::CryptoError;

/// 密钥用途标记（与具体算法解耦）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyPurpose {
    /// 对称数据加密 / AEAD。
    DataEncryption,
    /// MAC / HMAC。
    Mac,
    /// 非对称加密或密钥包装。
    AsymBox,
    /// 非对称签名。
    AsymSign,
    /// 信封包装（落库 / 会话密钥 wrap）。
    KeyWrap,
}

/// 在 drop 时清零的字节缓冲。
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct ZeroizingBytes(Vec<u8>);

impl ZeroizingBytes {
    /// 从字节向量构造。
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// 字节切片视图。
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for ZeroizingBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Serialize for ZeroizingBytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for ZeroizingBytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        Ok(Self(bytes))
    }
}

/// 密钥材料引用。
///
/// 本步仅定义形态；后续步骤由实现侧校验长度/格式。
/// 原始字节在 drop 时 zeroize。
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "form", rename_all = "snake_case")]
pub enum KeyRef {
    /// 原始密钥字节。
    Raw {
        /// 用途。
        purpose: KeyPurpose,
        /// 密钥字节（敏感）。
        bytes: ZeroizingBytes,
    },
    /// PEM 文本（公钥或私钥，由用途约束）。
    Pem {
        /// 用途。
        purpose: KeyPurpose,
        /// PEM 文本。
        pem: String,
    },
}

impl core::fmt::Debug for KeyRef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Raw { purpose, bytes } => f
                .debug_struct("Raw")
                .field("purpose", purpose)
                .field("bytes_len", &bytes.as_slice().len())
                .finish_non_exhaustive(),
            Self::Pem { purpose, .. } => f
                .debug_struct("Pem")
                .field("purpose", purpose)
                .field("pem", &"<redacted>")
                .finish(),
        }
    }
}

impl KeyRef {
    /// 构造原始字节密钥。
    #[must_use]
    pub fn raw(purpose: KeyPurpose, bytes: impl Into<Vec<u8>>) -> Self {
        Self::Raw {
            purpose,
            bytes: ZeroizingBytes::new(bytes.into()),
        }
    }

    /// 构造 PEM 密钥。
    #[must_use]
    pub fn pem(purpose: KeyPurpose, pem: impl Into<String>) -> Self {
        Self::Pem {
            purpose,
            pem: pem.into(),
        }
    }

    /// 取原始密钥字节。
    ///
    /// # Errors
    ///
    /// [`CryptoError::InvalidKey`]：材料为 PEM 而非原始字节。
    pub fn raw_bytes(&self) -> Result<&[u8], CryptoError> {
        match self {
            Self::Raw { bytes, .. } => Ok(bytes.as_slice()),
            Self::Pem { .. } => Err(CryptoError::InvalidKey {
                reason: "expected raw key bytes, got PEM".to_owned(),
            }),
        }
    }

    /// 取 PEM 文本。
    ///
    /// # Errors
    ///
    /// [`CryptoError::InvalidKey`]：材料为原始字节而非 PEM。
    pub fn pem_str(&self) -> Result<&str, CryptoError> {
        match self {
            Self::Pem { pem, .. } => Ok(pem.as_str()),
            Self::Raw { .. } => Err(CryptoError::InvalidKey {
                reason: "expected PEM key material, got raw bytes".to_owned(),
            }),
        }
    }

    /// 用途。
    #[must_use]
    pub const fn purpose(&self) -> KeyPurpose {
        match self {
            Self::Raw { purpose, .. } | Self::Pem { purpose, .. } => *purpose,
        }
    }
}

/// Nonce / IV 生成与放置策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoncePolicy {
    /// 每次随机生成（AEAD 推荐）。
    Random,
    /// 随机并前缀到密文。
    RandomPrefix,
    /// 由密钥派生（须在实现侧固定盐/上下文）。
    KeyDerive,
    /// 由业务字段承载（上游互操作）。
    Field,
}
