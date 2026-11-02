//! The sine and the cosine (cosf, sinf) with their kernels.

use crate::math::fmath::{ONE, narrow, rem_pio2, sse_nan, sse_nan_f64};

/// 1 in double (`DAT_142ea7c58`).
pub(super) const ONE_F64: f64 = f64::from_bits(0x3ff0_0000_0000_0000);
/// −1 in double (`DAT_14330ec28`).
pub(super) const MINUS_ONE_F64: f64 = f64::from_bits(0xbff0_0000_0000_0000);
/// 1/2 in double (`DAT_142ea7c50`).
pub(super) const HALF_F64: f64 = f64::from_bits(0x3fe0_0000_0000_0000);
/// π/2 in double (`DAT_142ec8a10`): cosf's and sinf's step to the next quadrant.
pub(super) const FRAC_PI_2_F64: f64 = f64::from_bits(0x3ff9_21fb_5444_2d18);
/// π in double (`DAT_142ec8a78`).
pub(super) const PI_F64: f64 = f64::from_bits(0x4009_21fb_5444_2d18);
/// 3π/2 in double (`DAT_142ec8a80`).
pub(super) const THREE_FRAC_PI_2_F64: f64 = f64::from_bits(0x4012_d97c_7f33_21d2);
/// 2π in double (`DAT_142ec8a88`).
pub(super) const TWO_PI_F64: f64 = f64::from_bits(0x4019_21fb_5444_2d18);

/// The sine kernel's series x + s (S2 z − S1) + s w (S4 z − S3) with z = x², w = z², s = z x:
/// S1, subtracted: msun's S1 negated (`DAT_142ec89a8`).
pub(super) const SIN_S1: f64 = f64::from_bits(0x3fc5_5555_54cb_ac77);
/// The sine kernel's S2 (`DAT_142ec89a0`).
pub(super) const SIN_S2: f64 = f64::from_bits(0x3f81_1110_896e_fbb2);
/// The sine kernel's S3, subtracted: msun's S3 negated (`DAT_142ec8998`).
pub(super) const SIN_S3: f64 = f64::from_bits(0x3f2a_00f9_e2ca_e774);
/// The sine kernel's S4 (`DAT_142ec8990`).
pub(super) const SIN_S4: f64 = f64::from_bits(0x3ec6_cd87_8c3b_46a7);
/// The cosine kernel's series (1 − C0 z) + C1 w + (C3 z − C2) w z with z = x², w = z²: C0,
/// subtracted: msun's C0 negated (`DAT_142ec87c8`).
pub(super) const COS_C0: f64 = f64::from_bits(0x3fdf_ffff_fd0c_5e81);
/// The cosine kernel's C1 (`DAT_142ec87c0`).
pub(super) const COS_C1: f64 = f64::from_bits(0x3fa5_5553_e105_3a42);
/// The cosine kernel's C2, subtracted: msun's C2 negated (`DAT_142ec87b8`).
pub(super) const COS_C2: f64 = f64::from_bits(0x3f56_c087_e80f_1e27);
/// The cosine kernel's C3 (`DAT_142ec87b0`).
pub(super) const COS_C3: f64 = f64::from_bits(0x3ef9_9342_e0ee_5069);

/// `0x14099db70` (msun's `__kernel_sindf`): the sine of a double up to about π/4 in size, rounded
/// to f32 (`CVTSD2SS`).
pub(super) fn kernel_sin(x: f64) -> f32 {
    let z = x * x;
    let w = z * z;
    let r = z * SIN_S4 - SIN_S3;
    let s = z * x;
    narrow(sse_nan_f64(
        w * s * r + ((z * SIN_S2 - SIN_S1) * s + x),
        &[x],
    ))
}

/// `0x14099cad0` (msun's `__kernel_cosdf`): the cosine of a double up to about π/4 in size,
/// rounded to f32.
pub(super) fn kernel_cos(x: f64) -> f32 {
    let z = x * x;
    let w = z * z;
    let r = z * COS_C3 - COS_C2;
    narrow(sse_nan_f64(
        r * (w * z) + ((ONE_F64 - z * COS_C0) + w * COS_C1),
        &[x],
    ))
}

/// The cosine: the game's cosf `0x14099e4d0` (msun's `s_cosf.c`). Up to 9π/4 in size x moves by
/// a multiple of π/2 in double to the sine or cosine kernel; beyond, [`rem_pio2`] reduces it.
/// Below 2^-12 it is 1; infinities and NaN give `x − x`.
#[must_use]
pub fn cos(x: f32) -> f32 {
    let hx = x.to_bits();
    let negative = hx >> 31 != 0;
    let ix = hx & 0x7fff_ffff;
    if ix <= 0x3f49_0fda {
        if ix < 0x3980_0000 {
            // x + 2^120 into a dead store (msun raises inexact): 1
            return ONE;
        }
        return kernel_cos(f64::from(x));
    }
    if ix <= 0x407b_53d1 {
        let d = f64::from(x);
        if ix > 0x4016_cbe3 {
            return -kernel_cos(if negative { d + PI_F64 } else { d - PI_F64 });
        }
        return if negative {
            kernel_sin(d + FRAC_PI_2_F64)
        } else {
            kernel_sin(FRAC_PI_2_F64 - d)
        };
    }
    if ix <= 0x40e2_31d5 {
        if ix > 0x40af_eddf {
            let d = f64::from(x);
            return kernel_cos(if negative {
                d + TWO_PI_F64
            } else {
                d - TWO_PI_F64
            });
        }
        return kernel_sin(f64::from(if negative { -x } else { x }) - THREE_FRAC_PI_2_F64);
    }
    if ix >= 0x7f80_0000 {
        #[allow(clippy::eq_op)]
        return sse_nan(x - x, &[x]);
    }
    let (n, y) = rem_pio2(x);
    match n & 3 {
        0 => kernel_cos(y),
        1 => kernel_sin(-y),
        2 => -kernel_cos(y),
        _ => kernel_sin(y),
    }
}

/// The sine: the game's sinf `0x14099ed60` (msun's `s_sinf.c`), its branches cosf's. Below 2^-12
/// it is x; infinities and NaN give `x − x`.
#[must_use]
pub fn sin(x: f32) -> f32 {
    let hx = x.to_bits();
    let negative = hx >> 31 != 0;
    let ix = hx & 0x7fff_ffff;
    if ix <= 0x3f49_0fda {
        if ix < 0x3980_0000 {
            // x · 2^-120 or x + 2^120 into a dead store (msun raises underflow or inexact): x
            return x;
        }
        return kernel_sin(f64::from(x));
    }
    if ix <= 0x407b_53d1 {
        let d = f64::from(x);
        if ix <= 0x4016_cbe3 {
            return if negative {
                -kernel_cos(d + FRAC_PI_2_F64)
            } else {
                kernel_cos(d - FRAC_PI_2_F64)
            };
        }
        return kernel_sin(-(if negative { d + PI_F64 } else { d - PI_F64 }));
    }
    if ix <= 0x40e2_31d5 {
        let d = f64::from(x);
        if ix <= 0x40af_eddf {
            return if negative {
                kernel_cos(d + THREE_FRAC_PI_2_F64)
            } else {
                -kernel_cos(d - THREE_FRAC_PI_2_F64)
            };
        }
        return kernel_sin(if negative {
            d + TWO_PI_F64
        } else {
            d - TWO_PI_F64
        });
    }
    if ix >= 0x7f80_0000 {
        #[allow(clippy::eq_op)]
        return sse_nan(x - x, &[x]);
    }
    let (n, y) = rem_pio2(x);
    match n & 3 {
        0 => kernel_sin(y),
        1 => kernel_cos(y),
        2 => kernel_sin(-y),
        _ => -kernel_cos(y),
    }
}
