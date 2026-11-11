//! Legacy 算法唯一门闩入口。
//!
//! 业务与 Profile 校验**必须**调用本模块，禁止在业务 crate 内自行
//! `if security == Legacy` 散落判断。

use crate::id::AlgorithmId;
use crate::traits::CryptoError;

/// 若 `id` 为 Legacy 且未显式允许不安全算法，则返回 [`CryptoError::LegacyNotAllowed`]。
///
/// # Errors
///
/// - [`CryptoError::LegacyNotAllowed`]：算法为 Legacy 且 `allow_insecure` 为 `false`。
pub fn assert_not_legacy_or_allowed(
    id: AlgorithmId,
    allow_insecure: bool,
) -> Result<(), CryptoError> {
    if id.security_class().requires_allow_insecure() && !allow_insecure {
        return Err(CryptoError::LegacyNotAllowed { id });
    }
    Ok(())
}
