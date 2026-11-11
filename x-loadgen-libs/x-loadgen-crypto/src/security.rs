//! 算法安全等级。
//!
//! `Legacy` 仅可在 Profile 显式 `allow_insecure=true` 时启用；
//! 业务侧必须经 [`crate::legacy::assert_not_legacy_or_allowed`] 校验，禁止自行散落 `if`。

use serde::{Deserialize, Serialize};

/// 算法在闭集中的安全分级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityClass {
    /// 2026-11 推荐默认（如 `aes-256-gcm`、`hmac-sha256`）。
    Recommended,
    /// 主流可用、非默认首选。
    Mainstream,
    /// 国密一等公民（SM2 / SM3 / SM4 等）。
    Gm,
    /// 弱/弃用；默认拒绝，须显式 `allow_insecure`。
    Legacy,
}

impl SecurityClass {
    /// 是否必须经 `allow_insecure` 门闩才可启用。
    #[must_use]
    pub const fn requires_allow_insecure(self) -> bool {
        matches!(self, Self::Legacy)
    }
}
