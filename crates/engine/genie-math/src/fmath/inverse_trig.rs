//! The inverse trigonometry: acosf, atanf and atan2f (msun's).

use crate::fmath::{HALF, ONE, fabs, is_nan, sqrt, sse_nan};

/// π/2's leading bits (`DAT_142ec89f0`): acosf near 0, also atanf's bound.
pub(super) const PIO2_HI: f32 = f32::from_bits(0x3fc9_0fda);
/// π/2's next bits (`DAT_142ec89e0`).
pub(super) const PIO2_LO: f32 = f32::from_bits(0x33a2_2168);
/// acosf(-1), twice [`PIO2_HI`] (`DAT_142ec89f4`).
pub(super) const ACOS_OF_MINUS_ONE: f32 = f32::from_bits(0x4049_0fda);
/// acosf's rational R(z) = z (P0 + z (P1 − z P2)) / (1 − z Q1): P0 (`DAT_142ec89e8`).
pub(super) const ACOS_P0: f32 = f32::from_bits(0x3e2a_aa75);
/// R's P1 (`DAT_142ec89f8`).
pub(super) const ACOS_P1: f32 = f32::from_bits(0xbd2f_13ba);
/// R's P2, subtracted: msun's pS2 negated (`DAT_142ec89e4`).
pub(super) const ACOS_P2: f32 = f32::from_bits(0x3c0d_d36b);
/// R's Q1, subtracted: msun's qS1 negated (`DAT_142ec89ec`).
pub(super) const ACOS_Q1: f32 = f32::from_bits(0x3f34_e5ae);

/// acosf's R(z), each product and sum in the game's order (inlined three times in acosf).
pub(super) fn acos_r(z: f32) -> f32 {
    ((ACOS_P1 - z * ACOS_P2) * z + ACOS_P0) * z / (ONE - z * ACOS_Q1)
}

/// The arc cosine: the game's acosf `0x14099dc60` (msun's `e_acosf.tail`). |x| from 1 up and NaN
/// take the bit test's branch: ±1 give 0 and π, the rest the invalid `0 / (x − x)`. Below 1/2 it
/// is π/2 less x's series, above 1/2 twice the arc sine of `sqrt((1 ∓ x) / 2)`.
#[must_use]
pub fn acos(x: f32) -> f32 {
    let hx = x.to_bits();
    let ix = hx & 0x7fff_ffff;
    if ix >= 0x3f80_0000 {
        if ix != 0x3f80_0000 {
            // SUBSS x, x; DIVSS 0, (x − x): NaN for |x| above 1, infinities and NaN
            #[allow(clippy::eq_op)]
            return sse_nan(0.0 / (x - x), &[x]);
        }
        return if hx >> 31 != 0 {
            ACOS_OF_MINUS_ONE
        } else {
            0.0
        };
    }
    if ix < 0x3f00_0000 {
        if ix <= 0x3280_0000 {
            return PIO2_HI;
        }
        return PIO2_HI - (x - (PIO2_LO - acos_r(x * x) * x));
    }
    if hx >> 31 != 0 {
        let z = (x + ONE) * HALF;
        let root = sqrt(z);
        let half_angle = PIO2_HI - (acos_r(z) * root - PIO2_LO + root);
        return half_angle + half_angle;
    }
    let z = (ONE - x) * HALF;
    let root = sqrt(z);
    let df = f32::from_bits(root.to_bits() & 0xffff_f000);
    let tail = (z - df * df) / (df + root);
    let half_angle = acos_r(z) * root + tail + df;
    half_angle + half_angle
}

/// atanf of a negative x from 2^26 down: −π/2's leading bits (`DAT_142ec8a74`).
pub(super) const ATAN_OF_MINUS_HUGE: f32 = f32::from_bits(0xbfc9_0fda);
/// 2 (`DAT_142e8c150`).
pub(super) const TWO: f32 = f32::from_bits(0x4000_0000);
/// 3/2 (`DAT_142eb4838`).
pub(super) const THREE_HALVES: f32 = f32::from_bits(0x3fc0_0000);
/// −1 (`DAT_14330ec20`).
pub(super) const MINUS_ONE: f32 = f32::from_bits(0xbf80_0000);
/// atanf's series in z = t², w = z²: (T1 − w T3) w + ((w T4 + T2) w + T0) z. T0 (`DAT_142ec8a6c`).
pub(super) const ATAN_T0: f32 = f32::from_bits(0x3eaa_aaa9);
/// The series' T1 (`DAT_142ec8a70`).
pub(super) const ATAN_T1: f32 = f32::from_bits(0xbe4c_ca98);
/// The series' T2 (`DAT_142ec8a68`).
pub(super) const ATAN_T2: f32 = f32::from_bits(0x3e11_f50d);
/// The series' T3, subtracted: msun's aT3 negated (`DAT_142ec8a64`).
pub(super) const ATAN_T3: f32 = f32::from_bits(0x3dda_1247);
/// The series' T4 (`DAT_142ec8a60`).
pub(super) const ATAN_T4: f32 = f32::from_bits(0x3d7c_ac25);
/// atan(1/2)'s leading bits (`DAT_142ec8a40`).
pub(super) const ATAN_HI_0: f32 = f32::from_bits(0x3eed_6338);
/// atan(1)'s leading bits (`DAT_142ec8a44`).
pub(super) const ATAN_HI_1: f32 = f32::from_bits(0x3f49_0fda);
/// atan(3/2)'s leading bits (`DAT_142ec8a48`).
pub(super) const ATAN_HI_2: f32 = f32::from_bits(0x3f7b_985e);
/// atan(∞)'s leading bits (`DAT_142ec8a4c`).
pub(super) const ATAN_HI_3: f32 = f32::from_bits(0x3fc9_0fda);
/// atan(1/2)'s next bits (`DAT_142ec8a50`).
pub(super) const ATAN_LO_0: f32 = f32::from_bits(0x31ac_3769);
/// atan(1)'s next bits (`DAT_142ec8a54`).
pub(super) const ATAN_LO_1: f32 = f32::from_bits(0x3322_2168);
/// atan(3/2)'s next bits (`DAT_142ec8a58`).
pub(super) const ATAN_LO_2: f32 = f32::from_bits(0x3314_0fb4);
/// atan(∞)'s next bits (`DAT_142ec8a5c`).
pub(super) const ATAN_LO_3: f32 = f32::from_bits(0x33a2_2168);
/// The breakpoints' angles, the image's table at `0x142ec8a40`.
pub(super) const ATAN_HI: [f32; 4] = [ATAN_HI_0, ATAN_HI_1, ATAN_HI_2, ATAN_HI_3];
/// Their next bits, the table at `0x142ec8a50`.
pub(super) const ATAN_LO: [f32; 4] = [ATAN_LO_0, ATAN_LO_1, ATAN_LO_2, ATAN_LO_3];

/// The arc tangent: the game's atanf `0x14099e2f0` (msun's `s_atanf.c`). |x| from 2^26 up is
/// ±π/2 (a NaN comes back as it is), below 2^-12 it is x. Below 7/16 the series runs on x; above,
/// |x| is moved next to a breakpoint (1/2, 1, 3/2, ∞) and the series added to its angle.
#[must_use]
pub fn atan(x: f32) -> f32 {
    let hx = x.to_bits();
    let negative = hx >> 31 != 0;
    let ix = hx & 0x7fff_ffff;
    if ix >= 0x4c80_0000 {
        if is_nan(x) {
            return x;
        }
        return if negative {
            ATAN_OF_MINUS_HUGE
        } else {
            PIO2_HI
        };
    }
    let (id, reduced) = if ix < 0x3ee0_0000 {
        if ix < 0x3980_0000 {
            // a subnormal x is squared into a dead store (msun raises underflow): x it is
            return x;
        }
        (None, x)
    } else {
        let abs_x = fabs(x);
        if ix < 0x3f98_0000 {
            if ix < 0x3f30_0000 {
                (Some(0), (abs_x * TWO - ONE) / (abs_x + TWO))
            } else {
                (Some(1), (abs_x - ONE) / (abs_x + ONE))
            }
        } else if ix < 0x401c_0000 {
            (
                Some(2),
                (abs_x - THREE_HALVES) / (abs_x * THREE_HALVES + ONE),
            )
        } else {
            (Some(3), MINUS_ONE / abs_x)
        }
    };
    let z = reduced * reduced;
    let fourth_power = z * z;
    let s2 = (ATAN_T1 - fourth_power * ATAN_T3) * fourth_power;
    let s1 = ((fourth_power * ATAN_T4 + ATAN_T2) * fourth_power + ATAN_T0) * z;
    let series = (s2 + s1) * reduced;
    let Some(id) = id else {
        return reduced - series;
    };
    let angle = ATAN_HI[id] - ((series - ATAN_LO[id]) - reduced);
    if negative { -angle } else { angle }
}

/// π (`DAT_142eb0c68`).
pub(super) const PI: f32 = f32::from_bits(0x4049_0fdb);
/// −π (`DAT_142ec8a3c`).
pub(super) const MINUS_PI: f32 = f32::from_bits(0xc049_0fdb);
/// π/2 (`DAT_142eb0c64`).
pub(super) const FRAC_PI_2: f32 = f32::from_bits(0x3fc9_0fdb);
/// −π/2 (`DAT_142ec8a34`).
pub(super) const MINUS_FRAC_PI_2: f32 = f32::from_bits(0xbfc9_0fdb);
/// π/4 (`DAT_142eb6e44`).
pub(super) const FRAC_PI_4: f32 = f32::from_bits(0x3f49_0fdb);
/// −π/4 (`DAT_142ec8a30`).
pub(super) const MINUS_FRAC_PI_4: f32 = f32::from_bits(0xbf49_0fdb);
/// 3π/4 (`DAT_142ec8a24`).
pub(super) const THREE_FRAC_PI_4: f32 = f32::from_bits(0x4016_cbe4);
/// −3π/4 (`DAT_142ec8a38`).
pub(super) const MINUS_THREE_FRAC_PI_4: f32 = f32::from_bits(0xc016_cbe4);
/// −0 (`DAT_142ec8a28`).
pub(super) const MINUS_ZERO: f32 = f32::from_bits(0x8000_0000);
/// π's next bits below [`PI`] (`DAT_142ec8a2c`).
pub(super) const PI_LO: f32 = f32::from_bits(0xb3bb_bd2e);

/// `atan2f(y, x)`: the game's atan2f `0x14099e020` (msun's `e_atan2f.c`), the angle of the point
/// (x, y). A NaN in either gives `y + x`; x = 1 is atanf(y). `signs` holds x's sign (bit 1) and y's
/// (bit 0): it picks the quadrant of the zeros, the infinities and of atanf(|y / x|).
#[must_use]
pub fn atan2(y: f32, x: f32) -> f32 {
    if is_nan(x) || is_nan(y) {
        // ADDSS y, x: the first NaN operand, quieted
        return sse_nan(y + x, &[y, x]);
    }
    let hx = x.to_bits();
    if hx == 0x3f80_0000 {
        return atan(y);
    }
    let hy = y.to_bits();
    let signs = ((hx & 0xbfff_ffff) | (hy >> 1)) >> 30;
    let ix = hx & 0x7fff_ffff;
    let iy = hy & 0x7fff_ffff;
    if iy == 0 {
        return match signs {
            0 | 1 => y,
            2 => PI,
            _ => MINUS_PI,
        };
    }
    let up_or_down = if signs & 1 == 0 {
        FRAC_PI_2
    } else {
        MINUS_FRAC_PI_2
    };
    if ix == 0 {
        return up_or_down;
    }
    if ix == 0x7f80_0000 {
        return if iy == 0x7f80_0000 {
            match signs {
                0 => FRAC_PI_4,
                1 => MINUS_FRAC_PI_4,
                2 => THREE_FRAC_PI_4,
                _ => MINUS_THREE_FRAC_PI_4,
            }
        } else {
            match signs {
                0 => 0.0,
                1 => MINUS_ZERO,
                2 => PI,
                _ => MINUS_PI,
            }
        };
    }
    // |y / x| above 2^26, or y infinite
    if ix + 0x0d00_0000 < iy || iy == 0x7f80_0000 {
        return up_or_down;
    }
    // |y / x| below 2^-26 with x negative: 0, else atanf(|y / x|)
    let z = if signs & 2 != 0 && iy + 0x0d00_0000 < ix {
        0.0
    } else {
        atan(fabs(y / x))
    };
    match signs {
        0 => z,
        1 => -z,
        2 => PI - (z - PI_LO),
        _ => (z - PI_LO) - PI,
    }
}
