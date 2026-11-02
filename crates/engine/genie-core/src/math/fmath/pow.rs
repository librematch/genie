//! powf (musl's) with its log2 and exp2 tables.

use crate::math::fmath::{INDEFINITE, ONE, sse_nan};

/// `checkint` (`FUN_14099e9a0`) of powf's exponent bits: 0 not an integer, 1 an odd one, 2 an
/// even one.
pub(super) const fn powf_checkint(iy: u32) -> u32 {
    let e = iy >> 23 & 0xff;
    if e < 0x7f {
        return 0;
    }
    if e > 0x7f + 23 {
        return 2;
    }
    let bit = 1u32 << (0x7f + 23 - e);
    if iy & (bit - 1) != 0 {
        return 0;
    }
    if iy & bit != 0 {
        return 1;
    }
    2
}

/// powf's log2 table (`0x142ec8d30`): 16 pairs of `invc` and `logc` (f64 bits).
pub(super) const POWF_LOG2: [(u64, u64); 16] = [
    (0x3ff6_61ec_79f8_f3be, 0xbfde_fec6_5b96_3019),
    (0x3ff5_71ed_4aaf_883d, 0xbfdb_0b68_32d4_fca4),
    (0x3ff4_9539_f0f0_10b0, 0xbfd7_418b_0a1f_b77b),
    (0x3ff3_c995_b0b8_0385, 0xbfd3_9de9_1a6d_cf7b),
    (0x3ff3_0d19_0c88_64a5, 0xbfd0_1d9b_f3f2_b631),
    (0x3ff2_5e22_7b0b_8ea0, 0xbfc9_7c1d_1b3b_7af0),
    (0x3ff1_bb4a_4a1a_343f, 0xbfc2_f9e3_93af_3c9f),
    (0x3ff1_2358_f08a_e5ba, 0xbfb9_60cb_bf78_8d5c),
    (0x3ff0_953f_4199_00a7, 0xbfaa_6f9d_b647_5fce),
    (0x3ff0_0000_0000_0000, 0x0000_0000_0000_0000),
    (0x3fee_608c_fd9a_47ac, 0x3fb3_38ca_9f24_f53d),
    (0x3fec_a4b3_1f02_6aa0, 0x3fc4_76a9_5438_91ba),
    (0x3feb_2036_576a_fce6, 0x3fce_840b_4ac4_e4d2),
    (0x3fe9_c2d1_63a1_aa2d, 0x3fd4_0645_f0c6_651c),
    (0x3fe8_86e6_0378_41ed, 0x3fd8_8e9c_2c1b_9ff8),
    (0x3fe7_67dc_f553_4862, 0x3fdc_e0a4_4eb1_7bcc),
];

/// powf's log2 polynomial (`0x142ec8e30`..`0x142ec8e50`, f64 bits).
pub(super) const POWF_LOG2_POLY: [u64; 5] = [
    0x3fd2_7616_c949_6e0b,
    0xbfd7_1969_a075_c67a,
    0x3fde_c70a_6ca7_badd,
    0xbfe7_1547_48be_f6c8,
    0x3ff7_1547_652a_b82b,
];

/// powf's exp2 table (`0x142ec8aa0`): 2^(i/32) less i << 47, as f64 bits.
pub(super) const POWF_EXP2: [u64; 32] = [
    0x3ff0_0000_0000_0000,
    0x3fef_d9b0_d315_8574,
    0x3fef_b558_6cf9_890f,
    0x3fef_9301_d012_5b51,
    0x3fef_72b8_3c7d_517b,
    0x3fef_5487_3168_b9aa,
    0x3fef_387a_6e75_6238,
    0x3fef_1e9d_f51f_dee1,
    0x3fef_06fe_0a31_b715,
    0x3fee_f1a7_373a_a9cb,
    0x3fee_dea6_4c12_3422,
    0x3fee_ce08_6061_892d,
    0x3fee_bfda_d536_2a27,
    0x3fee_b42b_569d_4f82,
    0x3fee_ab07_dd48_5429,
    0x3fee_a47e_b03a_5585,
    0x3fee_a09e_667f_3bcd,
    0x3fee_9f75_e8ec_5f74,
    0x3fee_a114_73eb_0187,
    0x3fee_a589_994c_ce13,
    0x3fee_ace5_422a_a0db,
    0x3fee_b737_b0cd_c5e5,
    0x3fee_c491_82a3_f090,
    0x3fee_d503_b23e_255d,
    0x3fee_e89f_995a_d3ad,
    0x3fee_ff76_f2fb_5e47,
    0x3fef_199b_dd85_529c,
    0x3fef_3720_dcef_9069,
    0x3fef_5818_dcfb_a487,
    0x3fef_7c97_337b_9b5f,
    0x3fef_a4af_a2a4_90da,
    0x3fef_d076_5b6e_4540,
];

/// powf's exp2 shift (`0x142ec8ba0`, 0x1.8p+52 / 32) and polynomial (`0x142ec8ba8`..`0x142ec8bb8`).
pub(super) const POWF_SHIFT: u64 = 0x42e8_0000_0000_0000;
pub(super) const POWF_EXP2_POLY: [u64; 3] = [
    0x3fac_6af8_4b91_2394,
    0x3fce_bfce_50fa_c4f3,
    0x3fe6_2e42_ff0c_52d6,
];
/// The sign bias of a negative result (1 << 16), which the exp2 table index carries into the
/// sign bit.
pub(super) const POWF_SIGN_BIAS: u64 = 0x1_0000;

/// powf's overflow and underflow (`FUN_14099cb80`, `FUN_14099cb90` through `FUN_14099cba0`):
/// `x` (2^97, 2^-95) times itself, negated with a sign bias.
pub(super) fn powf_xflow(sign_bias: u64, x: f32) -> f32 {
    let s = if sign_bias == 0 { x } else { -x };
    x * s
}

/// powf (`FUN_14099e9e0`): musl's (Arm's optimized-routines) power, its log2 and exp2 in f64 over
/// the image's tables, in its machine code's operation order: the special cases of an exponent
/// that is 0, infinite or NaN and of a base that is 0, subnormal, infinite, NaN or negative
/// (`checkint`), the log2 of the base times the exponent, the overflow and underflow beyond
/// 126 (`0x142ec8e58`, −150), then its exp2 rounded to f32 (`CVTPD2PS`).
#[must_use]
pub fn powf(x: f32, y: f32) -> f32 {
    let mut ix = x.to_bits();
    let iy = y.to_bits();
    let mut sign_bias = 0u64;
    let special_y = iy.wrapping_mul(2).wrapping_sub(1) >= 0xfeff_ffff;
    if ix.wrapping_sub(0x0080_0000) >= 0x7f00_0000 || special_y {
        if special_y {
            let y2 = iy.wrapping_mul(2);
            if y2 == 0 || ix == 0x3f80_0000 {
                return ONE;
            }
            let x2 = ix.wrapping_mul(2);
            if x2 > 0xff00_0000 || y2 > 0xff00_0000 {
                return sse_nan(x + y, &[x, y]);
            }
            if x2 == 0x7f00_0000 {
                return ONE;
            }
            // |x| < 1 against a positive y: 0; else y * y
            if (x2 < 0x7f00_0000) == (iy >> 31 == 0) {
                return 0.0;
            }
            return y * y;
        }
        if ix.wrapping_mul(2).wrapping_sub(1) >= 0xfeff_ffff {
            let mut x2 = x * x;
            if ix >> 31 != 0 && powf_checkint(iy) == 1 {
                x2 = -x2;
            }
            if iy >> 31 != 0 {
                return ONE / x2;
            }
            return x2;
        }
        if ix >> 31 != 0 {
            let yint = powf_checkint(iy);
            if yint == 0 {
                return f32::from_bits(INDEFINITE);
            }
            if yint == 1 {
                sign_bias = POWF_SIGN_BIAS;
            }
            ix &= 0x7fff_ffff;
        }
        if ix < 0x0080_0000 {
            // a subnormal base, normalised (0x1p23, `0x142ec8d20`)
            ix = (x * f32::from_bits(0x4b00_0000)).to_bits() & 0x7fff_ffff;
            ix = ix.wrapping_sub(23 << 23);
        }
    }
    // log2(x) = log1p(z / c - 1) / ln 2 + log2(c) + k
    let tmp = ix.wrapping_sub(0x3f33_0000);
    let i = (tmp >> 19 & 0xf) as usize;
    let top = tmp & 0xff80_0000;
    let iz = ix.wrapping_sub(top);
    let k = (top as i32) >> 23;
    let (invc, logc) = POWF_LOG2[i];
    let a = POWF_LOG2_POLY.map(f64::from_bits);
    let r = f64::from(f32::from_bits(iz)) * f64::from_bits(invc) - 1.0;
    let y0 = f64::from(k) + f64::from_bits(logc);
    let q = y0 + r * a[4];
    let p = r * a[2] + a[3];
    let poly = r * a[0] + a[1];
    let r2 = r * r;
    let q = q + p * r2;
    let ylogx = (q + poly * (r2 * r2)) * f64::from(y);
    if (ylogx.to_bits() >> 47 & 0xffff) as u16 >= 0x80bf {
        // |y log2 x| at least 126 (`COMISD`: above 0x1.fffffffd1d571p+6, at most −150)
        if ylogx > f64::from_bits(0x405f_ffff_ffd1_d571) {
            return powf_xflow(sign_bias, f32::from_bits(0x7000_0000));
        }
        if ylogx <= -150.0 {
            return powf_xflow(sign_bias, f32::from_bits(0x1000_0000));
        }
    }
    // exp2(ylogx) = 2^(k / 32) * 2^r, r within 1/64
    let shift = f64::from_bits(POWF_SHIFT);
    let kd = ylogx + shift;
    let ki = kd.to_bits();
    let r = ylogx - (kd - shift);
    let t = POWF_EXP2[(ki & 0x1f) as usize].wrapping_add(ki.wrapping_add(sign_bias) << 47);
    let c = POWF_EXP2_POLY.map(f64::from_bits);
    let z = r * c[0] + c[1];
    let rest = r * c[2] + 1.0;
    ((z * (r * r) + rest) * f64::from_bits(t)) as f32
}
