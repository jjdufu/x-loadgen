//! 本 crate 专用的数值转换小工具（基于 `mazbri_core::num`，饱和而非截断）。

use mazbri_core::num::{f64_to_u64, u64_to_f64};

/// `u128` → `f64`；超过 `u64::MAX` 的值饱和到 `u64::MAX`（延迟统计不会触及）。
pub fn u128_to_f64(v: u128) -> f64 {
    u64::try_from(v).map_or(u64_to_f64(u64::MAX), u64_to_f64)
}

/// `f64` → `usize`；负数/NaN 为 0，超出范围饱和。
#[must_use]
pub fn f64_to_usize(x: f64) -> usize {
    usize::try_from(f64_to_u64(x)).unwrap_or(usize::MAX)
}

/// `usize` → `u32`，饱和到 `u32::MAX`。
#[must_use]
pub fn usize_to_u32(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

/// `u64` → `u32`，饱和到 `u32::MAX`。
#[must_use]
pub fn u64_to_u32(v: u64) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

/// Re-export for `cast_precision_loss` avoidance.
pub use mazbri_core::num::u64_to_f64 as u64_as_f64;
