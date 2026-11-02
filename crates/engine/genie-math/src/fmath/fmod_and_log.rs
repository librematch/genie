//! fmodf, and logf with the double logarithm it rounds.

use crate::fmath::{narrow, sse_nan, sse_nan_f64, widen};

/// fmodf: the remainder of x / y with x's sign, exact. The game calls the CRT's
/// (`api-ms-win-crt-math-l1-1-0.dll!fmodf`, through the import thunk `0x142a8e7fa`), whose body is
/// not in the image; an exact remainder has one value, so musl's `fmodf` is ported: a shift and
/// subtract on the significands. A zero or NaN y and a non-finite x give `(x · y) / (x · y)`.
#[must_use]
pub fn fmod(x: f32, y: f32) -> f32 {
    let mut uxi = x.to_bits();
    let mut uyi = y.to_bits();
    let mut ex = (uxi >> 23 & 0xff) as i32;
    let mut ey = (uyi >> 23 & 0xff) as i32;
    let sx = uxi & 0x8000_0000;
    if uyi << 1 == 0 || y.is_nan() || ex == 0xff {
        // the invalid operation's default NaN, or the NaN argument quieted
        let p = x * y;
        #[allow(clippy::eq_op)]
        return sse_nan(p / p, &[x, y]);
    }
    if uxi << 1 <= uyi << 1 {
        // |x| at most |y|: a signed zero when equal, else x
        return if uxi << 1 == uyi << 1 {
            f32::from_bits(sx)
        } else {
            x
        };
    }
    // the significands with their leading bit, a subnormal's shifted up
    if ex == 0 {
        let mut i = uxi << 9;
        while i >> 31 == 0 {
            ex -= 1;
            i <<= 1;
        }
        uxi <<= 1 - ex;
    } else {
        uxi &= u32::MAX >> 9;
        uxi |= 1 << 23;
    }
    if ey == 0 {
        let mut i = uyi << 9;
        while i >> 31 == 0 {
            ey -= 1;
            i <<= 1;
        }
        uyi <<= 1 - ey;
    } else {
        uyi &= u32::MAX >> 9;
        uyi |= 1 << 23;
    }
    // x mod y, one exponent step at a time
    while ex > ey {
        let i = uxi.wrapping_sub(uyi);
        if i >> 31 == 0 {
            if i == 0 {
                return f32::from_bits(sx);
            }
            uxi = i;
        }
        uxi <<= 1;
        ex -= 1;
    }
    let i = uxi.wrapping_sub(uyi);
    if i >> 31 == 0 {
        if i == 0 {
            return f32::from_bits(sx);
        }
        uxi = i;
    }
    while uxi >> 23 == 0 {
        uxi <<= 1;
        ex -= 1;
    }
    // back to a float: normal, or shifted down into a subnormal
    if ex > 0 {
        uxi -= 1 << 23;
        uxi |= (ex as u32) << 23;
    } else {
        uxi >>= 1 - ex;
    }
    f32::from_bits(uxi | sx)
}

/// ln 2's leading bits, whose products with an exponent are exact (fdlibm's `ln2_hi`).
pub(super) const LN2_HI: f64 = f64::from_bits(0x3fe6_2e42_fee0_0000);
/// ln 2's next bits (fdlibm's `ln2_lo`).
pub(super) const LN2_LO: f64 = f64::from_bits(0x3dea_39ef_3579_3c76);
/// 2^54, which scales a subnormal up.
pub(super) const TWO_POW_54: f64 = f64::from_bits(0x4350_0000_0000_0000);
/// The series log(1 + f) = 2s + s·R(s²) with s = f / (2 + f): R's coefficients Lg1 to Lg7.
pub(super) const LOG_LG: [f64; 7] = [
    f64::from_bits(0x3fe5_5555_5555_5593),
    f64::from_bits(0x3fd9_9999_9997_fa04),
    f64::from_bits(0x3fd2_4924_9422_9359),
    f64::from_bits(0x3fcc_71c5_1d8e_78af),
    f64::from_bits(0x3fc7_4664_96cb_03de),
    f64::from_bits(0x3fc3_9a09_d078_c69f),
    f64::from_bits(0x3fc2_f112_df3e_5244),
];
/// 1/3, the short series' cubic term (`f·f·(1/2 − f/3)`).
pub(super) const THIRD_F64: f64 = f64::from_bits(0x3fd5_5555_5555_5555);

/// The natural logarithm of a double, fdlibm's `__ieee754_log` (FreeBSD msun's `e_log.c`, below
/// 1 ulp): x = 2^k · (1 + f) with 1 + f in [√2/2, √2), then k·ln 2 plus the series of
/// log(1 + f), each operation in fdlibm's order. Zero gives −∞, a negative x the invalid
/// operation's NaN, +∞ and a NaN themselves (quieted).
pub(super) fn log_f64(x: f64) -> f64 {
    let bits = x.to_bits();
    let (mut hx, lx) = ((bits >> 32) as i32, bits as u32);
    let mut x = x;
    let mut k = 0;
    if hx < 0x0010_0000 {
        if (hx & 0x7fff_ffff) as u32 | lx == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            #[allow(clippy::eq_op)]
            return sse_nan_f64((x - x) / 0.0, &[x]);
        }
        // a subnormal, scaled up
        k -= 54;
        x *= TWO_POW_54;
        hx = (x.to_bits() >> 32) as i32;
    }
    if hx >= 0x7ff0_0000 {
        return sse_nan_f64(x + x, &[x]);
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000f_ffff;
    let i = (hx + 0x95f64) & 0x0010_0000;
    // x or x/2 normalised into [√2/2, √2)
    let high = (hx | (i ^ 0x3ff0_0000)) as u32;
    x = f64::from_bits((u64::from(high) << 32) | (x.to_bits() & 0xffff_ffff));
    k += i >> 20;
    let f = x - 1.0;
    let dk = f64::from(k);
    if (0x000f_ffff & (2 + hx)) < 3 {
        // |f| below 2^-20
        if f == 0.0 {
            return if k == 0 {
                0.0
            } else {
                dk * LN2_HI + dk * LN2_LO
            };
        }
        let r = f * f * (0.5 - THIRD_F64 * f);
        return if k == 0 {
            f - r
        } else {
            dk * LN2_HI - ((r - dk * LN2_LO) - f)
        };
    }
    let s = f / (2.0 + f);
    let z = s * s;
    let mut i = hx - 0x6147a;
    let w = z * z;
    let j = 0x6b851 - hx;
    let t1 = w * (LOG_LG[1] + w * (LOG_LG[3] + w * LOG_LG[5]));
    let t2 = z * (LOG_LG[0] + w * (LOG_LG[2] + w * (LOG_LG[4] + w * LOG_LG[6])));
    i |= j;
    let r = t2 + t1;
    if i > 0 {
        let hfsq = 0.5 * f * f;
        if k == 0 {
            f - (hfsq - s * (hfsq + r))
        } else {
            dk * LN2_HI - ((hfsq - (s * (hfsq + r) + dk * LN2_LO)) - f)
        }
    } else if k == 0 {
        f - s * (f - r)
    } else {
        dk * LN2_HI - ((s * (f - r) - dk * LN2_LO) - f)
    }
}

/// logf: the natural logarithm. The game calls the CRT's (`api-ms-win-crt-math-l1-1-0.dll!logf`,
/// through the import thunk `0x142a8e85a`), whose body is not in the image; the tick oracle
/// stands in for it with the double logarithm rounded to f32. This is the same: x widened
/// (`CVTSS2SD`), fdlibm's double logarithm (below 1 ulp, [`log_f64`]) and the rounding
/// (`CVTSD2SS`), which gives the correctly rounded logarithm but within 2^-28 or so of a halfway
/// point. Zero gives −∞, a negative x the default NaN, a NaN itself (quieted).
#[must_use]
pub fn log(x: f32) -> f32 {
    narrow(log_f64(widen(x)))
}
