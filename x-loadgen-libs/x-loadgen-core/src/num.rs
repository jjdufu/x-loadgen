//! 无 `as` 的数值转换：饱和 / 截断语义显式化，供各服务替代隐式截断的 `as` 转换。
//!
//! - 整数之间：`TryFrom` 失败时饱和到目标类型边界（负数到无符号为 0）。
//! - 浮点 → 整数：向零截断，NaN 为 0，越界饱和（与 Rust `as` 语义一致，但不经 `as`）。
//! - 整数 → 浮点：按 2^32 分段转换，结果与最近可表示值误差不超过 1 ULP。

const TWO_POW_32: f64 = 4_294_967_296.0;
const MANTISSA_BITS: u32 = 52;
const EXP_BIAS: i32 = 1023;

/// 整数饱和转换：值在目标范围内原样转换，否则取目标类型最靠近的边界。
pub trait SaturatingFrom<T>: Sized {
    fn saturating_from(v: T) -> Self;
}

macro_rules! impl_saturating_unsigned_src {
    ($($src:ty => [$($dst:ty),*]);* $(;)?) => {
        $($(
            impl SaturatingFrom<$src> for $dst {
                fn saturating_from(v: $src) -> Self {
                    <$dst>::try_from(v).unwrap_or(<$dst>::MAX)
                }
            }
        )*)*
    };
}

macro_rules! impl_saturating_signed_src {
    ($($src:ty => [$($dst:ty),*]);* $(;)?) => {
        $($(
            impl SaturatingFrom<$src> for $dst {
                fn saturating_from(v: $src) -> Self {
                    <$dst>::try_from(v).unwrap_or(if v.is_negative() { <$dst>::MIN } else { <$dst>::MAX })
                }
            }
        )*)*
    };
}

impl_saturating_unsigned_src! {
    u16 => [i16];
    u32 => [u16, i16, i32, usize];
    u64 => [u16, u32, i32, i64, usize];
    u128 => [u64, i64, i32, u32];
    usize => [u16, u32, i32, i64, u64];
}

impl_saturating_signed_src! {
    i16 => [u16];
    i32 => [u16, u32, u64, usize, i16];
    i64 => [u16, u32, i32, u64, usize];
}

/// `f64` → `u64`：向零截断，NaN / 负数为 0，越界为 `u64::MAX`。
#[must_use]
pub fn f64_to_u64(x: f64) -> u64 {
    if x.is_nan() || x < 1.0 {
        return 0;
    }
    if x >= 18_446_744_073_709_551_615.0 {
        return u64::MAX;
    }
    let bits = x.to_bits();
    let raw_exp = i32::try_from((bits >> MANTISSA_BITS) & 0x7ff).unwrap_or(0);
    let exp = raw_exp - EXP_BIAS;
    let mantissa = (bits & ((1_u64 << MANTISSA_BITS) - 1)) | (1_u64 << MANTISSA_BITS);
    let shift = exp - i32::try_from(MANTISSA_BITS).unwrap_or(52);
    let magnitude = shift.unsigned_abs();
    if shift >= 0 {
        mantissa.checked_shl(magnitude).unwrap_or(u64::MAX)
    } else {
        mantissa.checked_shr(magnitude).unwrap_or(0)
    }
}

/// `u64` → `f64`（最近值，误差 ≤ 1 ULP）。
#[must_use]
pub fn u64_to_f64(v: u64) -> f64 {
    let hi = u32::try_from(v >> 32).unwrap_or(u32::MAX);
    let lo = u32::try_from(v & 0xFFFF_FFFF).unwrap_or(0);
    f64::from(hi).mul_add(TWO_POW_32, f64::from(lo))
}

/// `usize` → `f64`（最近值，误差 ≤ 1 ULP）。
#[must_use]
pub fn usize_to_f64(v: usize) -> f64 {
    u64_to_f64(u64::try_from(v).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_to_int_matches_truncation() {
        for x in [
            0.0,
            0.4,
            1.0,
            1.9,
            2.5,
            123.999,
            4_294_967_296.5,
            9.007_199_254_740_992e15,
        ] {
            assert_eq!(
                f64_to_u64(x),
                x.trunc().to_string().parse::<u64>().unwrap_or(0)
            );
        }
        assert_eq!(f64_to_u64(f64::NAN), 0);
        assert_eq!(f64_to_u64(-3.0), 0);
        assert_eq!(f64_to_u64(1e30), u64::MAX);
    }

    #[test]
    fn int_to_float_close() {
        for v in [0_u64, 1, 42, 1 << 40, (1 << 53) + 1, u64::MAX] {
            let expected: f64 = v.to_string().parse().unwrap_or(0.0);
            let got = u64_to_f64(v);
            assert!((got - expected).abs() <= expected.abs() * f64::EPSILON);
        }
        assert!((usize_to_f64(7) - 7.0).abs() < f64::EPSILON);
    }

    #[test]
    fn saturating_ints() {
        assert_eq!(u64::saturating_from(-1_i64), 0);
        assert_eq!(i64::saturating_from(u64::MAX), i64::MAX);
        assert_eq!(u32::saturating_from(u64::MAX), u32::MAX);
        assert_eq!(i32::saturating_from(-5_i64), -5);
        assert_eq!(u64::saturating_from(u128::MAX), u64::MAX);
        assert_eq!(u64::saturating_from(2000_u128), 2000);
    }
}
