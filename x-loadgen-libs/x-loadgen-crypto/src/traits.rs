//! 密码学原语 traits 与统一错误类型。
//!
//! 具体算法实现在后续步骤注册；本步只定契约，禁止在 pub API 留无文档的 `todo!`。

use crate::id::AlgorithmId;
use crate::material::KeyRef;

/// 统一密码学错误（无 stub panic 出口）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CryptoError {
    /// 字符串不是闭集 kebab-case id（含大小写/历史别名）。
    #[error("unknown algorithm id: {raw}")]
    UnknownAlgorithmId {
        /// 原始输入。
        raw: String,
    },
    /// 算法 id 合法但尚未向注册表登记实现/元数据。
    #[error("algorithm not registered: {id}")]
    NotRegistered {
        /// 算法 id。
        id: AlgorithmId,
    },
    /// Legacy 算法未显式允许。
    #[error("legacy algorithm requires allow_insecure: {id}")]
    LegacyNotAllowed {
        /// 算法 id。
        id: AlgorithmId,
    },
    /// 密钥材料不符合算法要求。
    #[error("invalid key material: {reason}")]
    InvalidKey {
        /// 原因说明。
        reason: String,
    },
    /// Nonce / IV 非法或策略不匹配。
    #[error("invalid nonce: {reason}")]
    InvalidNonce {
        /// 原因说明。
        reason: String,
    },
    /// 明文/密文/标签等输入非法。
    #[error("invalid input: {reason}")]
    InvalidInput {
        /// 原因说明。
        reason: String,
    },
    /// 认证失败（AEAD tag / MAC / 签名校验失败）。
    #[error("authentication failed")]
    AuthenticationFailed,
    /// 算法或参数组合当前不支持（如 SM9）。
    #[error("unsupported: {reason}")]
    Unsupported {
        /// 原因说明。
        reason: String,
    },
    /// 底层运算失败。
    #[error("crypto operation failed: {reason}")]
    OperationFailed {
        /// 原因说明。
        reason: String,
    },
    /// 未知命名套件（禁止静默回落默认算法）。
    #[error("unknown suite: {name}")]
    UnknownSuite {
        /// 原始套件名。
        name: String,
    },
    /// Profile 字段组合非法（安全规则 / 顺序 / 槽位等）。
    #[error("invalid profile: {reason}")]
    InvalidProfile {
        /// 原因说明。
        reason: String,
    },
}

/// AEAD 密封 / 打开。
pub trait AeadSeal: Send + Sync {
    /// 密封明文，返回实现约定的密文编码（通常含 nonce+ciphertext+tag 或仅 ciphertext+tag）。
    ///
    /// # Errors
    ///
    /// 密钥/nonce/输入非法或底层运算失败时返回 [`CryptoError`]。
    fn seal(
        &self,
        key: &KeyRef,
        nonce: &[u8],
        aad: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoError>;

    /// 打开密文。
    ///
    /// # Errors
    ///
    /// 认证失败或输入非法时返回 [`CryptoError`]。
    fn open(
        &self,
        key: &KeyRef,
        nonce: &[u8],
        aad: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CryptoError>;
}

/// 对称分组/流式加解密（非 AEAD；安全性依赖外挂 MAC）。
pub trait BlockCipher: Send + Sync {
    /// 加密。
    ///
    /// # Errors
    ///
    /// 密钥/IV/填充非法或运算失败时返回 [`CryptoError`]。
    fn encrypt(&self, key: &KeyRef, iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError>;

    /// 解密。
    ///
    /// # Errors
    ///
    /// 密钥/IV/填充非法或运算失败时返回 [`CryptoError`]。
    fn decrypt(&self, key: &KeyRef, iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError>;
}

/// 消息认证码。
pub trait Mac: Send + Sync {
    /// 计算 MAC 标签。
    ///
    /// # Errors
    ///
    /// 密钥非法或运算失败时返回 [`CryptoError`]。
    fn sign(&self, key: &KeyRef, message: &[u8]) -> Result<Vec<u8>, CryptoError>;

    /// 校验 MAC 标签（常量时间比较由实现保证）。
    ///
    /// # Errors
    ///
    /// 校验失败或输入非法时返回 [`CryptoError`]。
    fn verify(&self, key: &KeyRef, message: &[u8], tag: &[u8]) -> Result<(), CryptoError>;
}

/// 非对称签名。
pub trait AsymSign: Send + Sync {
    /// 签名。
    ///
    /// # Errors
    ///
    /// 密钥/输入非法或运算失败时返回 [`CryptoError`]。
    fn sign(&self, signer: &KeyRef, message: &[u8]) -> Result<Vec<u8>, CryptoError>;

    /// 验签。
    ///
    /// # Errors
    ///
    /// 验签失败或输入非法时返回 [`CryptoError`]。
    fn verify(
        &self,
        verifier: &KeyRef,
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), CryptoError>;
}
