//! 闭集算法标识：wire / DB / Profile 统一 kebab-case 字符串。
//!
//! 未知 id **严格报错**，不做大小写或历史别名兼容（无兼容层）。

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::security::SecurityClass;
use crate::traits::CryptoError;

/// 算法能力分类（注册表元数据；与 traits 一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlgorithmKind {
    /// 带认证的对称加密（AEAD）。
    Aead,
    /// 分组/流式对称加密（通常需外挂 MAC）。
    BlockCipher,
    /// 消息认证码。
    Mac,
    /// 摘要。
    Digest,
    /// 非对称加密 / 密钥包装。
    AsymBox,
    /// 非对称签名。
    AsymSign,
}

/// 2026-11 软件闭集算法 id（kebab-case）。
///
/// 不含 SM9 / HSM / 后量子；不含厂商专用别名。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AlgorithmId {
    // —— AEAD（推荐 / 主流）——
    /// AES-256-GCM（租户传输推荐默认）。
    Aes256Gcm,
    /// AES-128-GCM。
    Aes128Gcm,
    /// AES-192-GCM。
    Aes192Gcm,
    /// ChaCha20-Poly1305。
    Chacha20Poly1305,

    // —— AES 分组/流（主流兼容；CBC/CTR/CFB 宜配 MAC）——
    /// AES-256-CBC + PKCS7。
    Aes256Cbc,
    /// AES-128-CBC + PKCS7。
    Aes128Cbc,
    /// AES-256-CTR。
    Aes256Ctr,
    /// AES-128-CTR。
    Aes128Ctr,
    /// AES-256-CFB。
    Aes256Cfb,
    /// AES-128-CFB。
    Aes128Cfb,

    // —— 国密对称 ——
    /// SM4-CBC + PKCS7（国密推荐路径之一）。
    Sm4CbcPkcs7,
    /// SM4-CTR。
    Sm4Ctr,

    // —— 摘要 / MAC ——
    /// SHA-256。
    Sha256,
    /// SHA-384。
    Sha384,
    /// SHA-512。
    Sha512,
    /// HMAC-SHA256。
    HmacSha256,
    /// HMAC-SHA384。
    HmacSha384,
    /// HMAC-SHA512。
    HmacSha512,
    /// SM3。
    Sm3,
    /// HMAC-SM3。
    HmacSm3,
    /// BLAKE3（内部完整性可选）。
    Blake3,

    // —— 非对称 ——
    /// RSA-OAEP-SHA256（密钥包装推荐）。
    RsaOaepSha256,
    /// RSA PKCS#1 v1.5 加密（旧互操作）。
    RsaPkcs1v15Encrypt,
    /// RSA PKCS#1 v1.5 签名 + SHA-256。
    RsaPkcs1v15SignSha256,
    /// RSA PKCS#1 v1.5 签名 + SHA-384。
    RsaPkcs1v15SignSha384,
    /// RSA PKCS#1 v1.5 签名 + SHA-512。
    RsaPkcs1v15SignSha512,
    /// ECDSA P-256 + SHA-256。
    EcdsaP256Sha256,
    /// ECDSA P-384 + SHA-384。
    EcdsaP384Sha384,
    /// Ed25519 签名。
    Ed25519,
    /// SM2 加密（C1C3C2 / C1C2C3 由参数承载）。
    Sm2Encrypt,
    /// SM2 签名（`user_id` 由参数承载）。
    Sm2Sign,

    // —— Legacy（默认拒绝）——
    /// AES-256-ECB + PKCS7。
    Aes256Ecb,
    /// AES-128-ECB + PKCS7。
    Aes128Ecb,
    /// SM4-ECB + PKCS7。
    Sm4EcbPkcs7,
    /// DES-CBC + PKCS5。
    DesCbcPkcs5,
    /// MD5 摘要。
    Md5,
    /// HMAC-MD5。
    HmacMd5,
    /// 私钥当「加密」的非标准 RSA PKCS#1 v1.5 用法。
    RsaPkcs1v15PrivateEncrypt,
}

impl AlgorithmId {
    /// 闭集全部 id（稳定顺序，供 UI / `OpenAPI` 枚举）。
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &ALL_IDS
    }

    /// Wire / DB 用 kebab-case 字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aes256Gcm => "aes-256-gcm",
            Self::Aes128Gcm => "aes-128-gcm",
            Self::Aes192Gcm => "aes-192-gcm",
            Self::Chacha20Poly1305 => "chacha20-poly1305",
            Self::Aes256Cbc => "aes-256-cbc",
            Self::Aes128Cbc => "aes-128-cbc",
            Self::Aes256Ctr => "aes-256-ctr",
            Self::Aes128Ctr => "aes-128-ctr",
            Self::Aes256Cfb => "aes-256-cfb",
            Self::Aes128Cfb => "aes-128-cfb",
            Self::Sm4CbcPkcs7 => "sm4-cbc-pkcs7",
            Self::Sm4Ctr => "sm4-ctr",
            Self::Sha256 => "sha-256",
            Self::Sha384 => "sha-384",
            Self::Sha512 => "sha-512",
            Self::HmacSha256 => "hmac-sha256",
            Self::HmacSha384 => "hmac-sha384",
            Self::HmacSha512 => "hmac-sha512",
            Self::Sm3 => "sm3",
            Self::HmacSm3 => "hmac-sm3",
            Self::Blake3 => "blake3",
            Self::RsaOaepSha256 => "rsa-oaep-sha256",
            Self::RsaPkcs1v15Encrypt => "rsa-pkcs1v15-encrypt",
            Self::RsaPkcs1v15SignSha256 => "rsa-pkcs1v15-sign-sha256",
            Self::RsaPkcs1v15SignSha384 => "rsa-pkcs1v15-sign-sha384",
            Self::RsaPkcs1v15SignSha512 => "rsa-pkcs1v15-sign-sha512",
            Self::EcdsaP256Sha256 => "ecdsa-p256-sha256",
            Self::EcdsaP384Sha384 => "ecdsa-p384-sha384",
            Self::Ed25519 => "ed25519",
            Self::Sm2Encrypt => "sm2-encrypt",
            Self::Sm2Sign => "sm2-sign",
            Self::Aes256Ecb => "aes-256-ecb",
            Self::Aes128Ecb => "aes-128-ecb",
            Self::Sm4EcbPkcs7 => "sm4-ecb-pkcs7",
            Self::DesCbcPkcs5 => "des-cbc-pkcs5",
            Self::Md5 => "md5",
            Self::HmacMd5 => "hmac-md5",
            Self::RsaPkcs1v15PrivateEncrypt => "rsa-pkcs1v15-private-encrypt",
        }
    }

    /// 该 id 的安全分级。
    #[must_use]
    pub const fn security_class(self) -> SecurityClass {
        match self {
            Self::Aes256Gcm
            | Self::Chacha20Poly1305
            | Self::HmacSha256
            | Self::HmacSha384
            | Self::HmacSha512
            | Self::RsaOaepSha256
            | Self::EcdsaP256Sha256
            | Self::Ed25519 => SecurityClass::Recommended,

            Self::Aes128Gcm
            | Self::Aes192Gcm
            | Self::Aes256Cbc
            | Self::Aes128Cbc
            | Self::Aes256Ctr
            | Self::Aes128Ctr
            | Self::Aes256Cfb
            | Self::Aes128Cfb
            | Self::Sha256
            | Self::Sha384
            | Self::Sha512
            | Self::Blake3
            | Self::RsaPkcs1v15Encrypt
            | Self::RsaPkcs1v15SignSha256
            | Self::RsaPkcs1v15SignSha384
            | Self::RsaPkcs1v15SignSha512
            | Self::EcdsaP384Sha384 => SecurityClass::Mainstream,

            Self::Sm4CbcPkcs7
            | Self::Sm4Ctr
            | Self::Sm3
            | Self::HmacSm3
            | Self::Sm2Encrypt
            | Self::Sm2Sign => SecurityClass::Gm,

            Self::Aes256Ecb
            | Self::Aes128Ecb
            | Self::Sm4EcbPkcs7
            | Self::DesCbcPkcs5
            | Self::Md5
            | Self::HmacMd5
            | Self::RsaPkcs1v15PrivateEncrypt => SecurityClass::Legacy,
        }
    }

    /// 该 id 对应的原语种类。
    #[must_use]
    pub const fn kind(self) -> AlgorithmKind {
        match self {
            Self::Aes256Gcm | Self::Aes128Gcm | Self::Aes192Gcm | Self::Chacha20Poly1305 => {
                AlgorithmKind::Aead
            }

            Self::Aes256Cbc
            | Self::Aes128Cbc
            | Self::Aes256Ctr
            | Self::Aes128Ctr
            | Self::Aes256Cfb
            | Self::Aes128Cfb
            | Self::Sm4CbcPkcs7
            | Self::Sm4Ctr
            | Self::Aes256Ecb
            | Self::Aes128Ecb
            | Self::Sm4EcbPkcs7
            | Self::DesCbcPkcs5 => AlgorithmKind::BlockCipher,

            Self::HmacSha256
            | Self::HmacSha384
            | Self::HmacSha512
            | Self::HmacSm3
            | Self::HmacMd5 => AlgorithmKind::Mac,

            Self::Sha256 | Self::Sha384 | Self::Sha512 | Self::Sm3 | Self::Blake3 | Self::Md5 => {
                AlgorithmKind::Digest
            }

            Self::RsaOaepSha256
            | Self::RsaPkcs1v15Encrypt
            | Self::Sm2Encrypt
            | Self::RsaPkcs1v15PrivateEncrypt => AlgorithmKind::AsymBox,

            Self::RsaPkcs1v15SignSha256
            | Self::RsaPkcs1v15SignSha384
            | Self::RsaPkcs1v15SignSha512
            | Self::EcdsaP256Sha256
            | Self::EcdsaP384Sha384
            | Self::Ed25519
            | Self::Sm2Sign => AlgorithmKind::AsymSign,
        }
    }
}

const ALL_IDS: [AlgorithmId; 38] = [
    AlgorithmId::Aes256Gcm,
    AlgorithmId::Aes128Gcm,
    AlgorithmId::Aes192Gcm,
    AlgorithmId::Chacha20Poly1305,
    AlgorithmId::Aes256Cbc,
    AlgorithmId::Aes128Cbc,
    AlgorithmId::Aes256Ctr,
    AlgorithmId::Aes128Ctr,
    AlgorithmId::Aes256Cfb,
    AlgorithmId::Aes128Cfb,
    AlgorithmId::Sm4CbcPkcs7,
    AlgorithmId::Sm4Ctr,
    AlgorithmId::Sha256,
    AlgorithmId::Sha384,
    AlgorithmId::Sha512,
    AlgorithmId::HmacSha256,
    AlgorithmId::HmacSha384,
    AlgorithmId::HmacSha512,
    AlgorithmId::Sm3,
    AlgorithmId::HmacSm3,
    AlgorithmId::Blake3,
    AlgorithmId::RsaOaepSha256,
    AlgorithmId::RsaPkcs1v15Encrypt,
    AlgorithmId::RsaPkcs1v15SignSha256,
    AlgorithmId::RsaPkcs1v15SignSha384,
    AlgorithmId::RsaPkcs1v15SignSha512,
    AlgorithmId::EcdsaP256Sha256,
    AlgorithmId::EcdsaP384Sha384,
    AlgorithmId::Ed25519,
    AlgorithmId::Sm2Encrypt,
    AlgorithmId::Sm2Sign,
    AlgorithmId::Aes256Ecb,
    AlgorithmId::Aes128Ecb,
    AlgorithmId::Sm4EcbPkcs7,
    AlgorithmId::DesCbcPkcs5,
    AlgorithmId::Md5,
    AlgorithmId::HmacMd5,
    AlgorithmId::RsaPkcs1v15PrivateEncrypt,
];

impl fmt::Display for AlgorithmId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for AlgorithmId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AlgorithmId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

impl FromStr for AlgorithmId {
    type Err = CryptoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for id in Self::all() {
            if id.as_str() == s {
                return Ok(*id);
            }
        }
        Err(CryptoError::UnknownAlgorithmId { raw: s.to_owned() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_kebab() {
        for id in AlgorithmId::all() {
            let parsed = id.as_str().parse::<AlgorithmId>().expect("roundtrip");
            assert_eq!(parsed, *id);
        }
    }

    #[test]
    fn rejects_unknown_and_legacy_alias() {
        assert!(matches!(
            "AES-256-GCM".parse::<AlgorithmId>(),
            Err(CryptoError::UnknownAlgorithmId { .. })
        ));
        assert!(matches!(
            "aes256gcm".parse::<AlgorithmId>(),
            Err(CryptoError::UnknownAlgorithmId { .. })
        ));
    }

    #[test]
    fn serde_json_uses_as_str_wire_ids() {
        for id in AlgorithmId::all() {
            let json = serde_json::to_string(id).expect("ser");
            assert_eq!(json, format!("\"{}\"", id.as_str()));
            let back: AlgorithmId = serde_json::from_str(&json).expect("de");
            assert_eq!(back, *id);
        }
        // 禁止 serde 默认 kebab 把 Aes256Gcm 编成 aes256-gcm
        assert!(serde_json::from_str::<AlgorithmId>("\"aes256-gcm\"").is_err());
    }
}
