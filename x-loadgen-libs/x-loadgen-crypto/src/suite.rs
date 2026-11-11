//! 命名套件解析：将 `suite/...` 映射为具体算法字段。
//!
//! # 规则
//!
//! - 仅识别闭集命名套件；**未知名报错**，禁止静默回落 AES。
//! - 国密侧不提供假 `sm4-gcm`；用 [`SUITE_GM_SM4_HMAC_SM3`]（SM4-CBC + HMAC-SM3）。

use crate::id::AlgorithmId;
use crate::traits::CryptoError;

/// 租户传输 2026-11 默认命名套件（数据算法 = `aes-256-gcm`）。
pub const SUITE_TENANT_DEFAULT_2026_11: &str = "suite/tenant-default-2026-11";

/// 国密命名套件：SM4-CBC + HMAC-SM3（**替代**不可用的 SM4-GCM）。
pub const SUITE_GM_SM4_HMAC_SM3: &str = "suite/gm-sm4-hmac-sm3";

/// 租户传输 ChaCha20-Poly1305 命名套件。
pub const SUITE_TENANT_CHACHA20: &str = "suite/tenant-chacha20";

/// 命名套件解析结果（数据算法 + 可选外挂 MAC）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuiteSpec {
    /// 规范套件名（`suite/...`）。
    pub name: &'static str,
    /// 主数据算法（AEAD 或需配 MAC 的对称算法）。
    pub data_alg: AlgorithmId,
    /// 外挂 MAC；AEAD 套件为 `None`。
    pub mac_alg: Option<AlgorithmId>,
}

/// 闭集全部已知命名套件（稳定顺序，供 UI / 文档）。
#[must_use]
pub fn known_suites() -> &'static [&'static str] {
    &[
        SUITE_TENANT_DEFAULT_2026_11,
        SUITE_GM_SM4_HMAC_SM3,
        SUITE_TENANT_CHACHA20,
    ]
}

/// 解析命名套件；未知名返回 [`CryptoError::UnknownSuite`]。
///
/// # Errors
///
/// [`CryptoError::UnknownSuite`]：名称不在闭集。
pub fn resolve_suite(suite: &str) -> Result<SuiteSpec, CryptoError> {
    match suite {
        SUITE_TENANT_DEFAULT_2026_11 => Ok(SuiteSpec {
            name: SUITE_TENANT_DEFAULT_2026_11,
            data_alg: AlgorithmId::Aes256Gcm,
            mac_alg: None,
        }),
        SUITE_GM_SM4_HMAC_SM3 => Ok(SuiteSpec {
            name: SUITE_GM_SM4_HMAC_SM3,
            data_alg: AlgorithmId::Sm4CbcPkcs7,
            mac_alg: Some(AlgorithmId::HmacSm3),
        }),
        SUITE_TENANT_CHACHA20 => Ok(SuiteSpec {
            name: SUITE_TENANT_CHACHA20,
            data_alg: AlgorithmId::Chacha20Poly1305,
            mac_alg: None,
        }),
        other => Err(CryptoError::UnknownSuite {
            name: other.to_owned(),
        }),
    }
}

/// 解析命名套件的主数据算法。
///
/// # Errors
///
/// [`CryptoError::UnknownSuite`]：未知套件名。
pub fn suite_data_alg(suite: &str) -> Result<AlgorithmId, CryptoError> {
    Ok(resolve_suite(suite)?.data_alg)
}

/// 解析命名套件的配套 MAC（AEAD 套件返回 `Ok(None)`）。
///
/// # Errors
///
/// [`CryptoError::UnknownSuite`]：未知套件名。
pub fn suite_mac_alg(suite: &str) -> Result<Option<AlgorithmId>, CryptoError> {
    Ok(resolve_suite(suite)?.mac_alg)
}
