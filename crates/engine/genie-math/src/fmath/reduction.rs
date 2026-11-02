//! The argument reduction modulo π/2 (`__rem_pio2f`, `__kernel_rem_pio2`) and the floor and
//! scalbn it uses.

use crate::fmath::{HALF_F64, MINUS_ONE_F64, ONE_F64, sse_nan, widen};

/// floor's 2^52, added and taken off to round away the fraction (`DAT_142ec8bf0`).
pub(super) const FLOOR_TO_INT: f64 = f64::from_bits(0x4330_0000_0000_0000);

/// floor `0x14099e7d0` (musl's): `x` from 2^52 up, zeros and the non-finite as they are; below 1
/// in size 0 or −1 by the sign; else `x` plus the distance to its rounded neighbour, less 1 when
/// that rounded up.
pub(super) fn floor(x: f64) -> f64 {
    let u = x.to_bits();
    let e = (u >> 52) as u32 & 0x7ff;
    if e >= 0x433 || x == 0.0 {
        return x;
    }
    let negative = u >> 63 != 0;
    let y = if negative {
        x - FLOOR_TO_INT + FLOOR_TO_INT - x
    } else {
        x + FLOOR_TO_INT - FLOOR_TO_INT - x
    };
    if e <= 0x3fe {
        return if negative { -1.0 } else { 0.0 };
    }
    if y > 0.0 { y + x - ONE_F64 } else { y + x }
}

/// 2^1023 (`DAT_142ec8e70`).
pub(super) const TWO_POW_1023: f64 = f64::from_bits(0x7fe0_0000_0000_0000);
/// 2^-969, scalbn's step down: 2^-1022 · 2^53 (`DAT_142ec8e68`).
pub(super) const TWO_POW_MINUS_969: f64 = f64::from_bits(0x0360_0000_0000_0000);

/// scalbn `0x14099ecd0` (musl's): `x` · 2^n, the exponent brought into range by at most two steps
/// of 2^1023 or 2^-969 (so a subnormal result rounds once), then capped.
pub(super) fn scalbn(x: f64, n: i32) -> f64 {
    let mut y = x;
    let mut n = n;
    if n > 1023 {
        y *= TWO_POW_1023;
        n -= 1023;
        if n > 1023 {
            y *= TWO_POW_1023;
            n -= 1023;
            if n > 1023 {
                n = 1023;
            }
        }
    } else if n < -1022 {
        y *= TWO_POW_MINUS_969;
        n += 969;
        if n < -1022 {
            y *= TWO_POW_MINUS_969;
            n += 969;
            if n < -1022 {
                n = -1022;
            }
        }
    }
    f64::from_bits(((0x3ff + n) as u64) << 52) * y
}

/// The chunks' 2^-24 (`DAT_142ec8940`).
pub(super) const TWO_POW_MINUS_24: f64 = f64::from_bits(0x3e70_0000_0000_0000);
/// The chunks' 2^24 (`DAT_142ec8958`).
pub(super) const TWO_POW_24: f64 = f64::from_bits(0x4170_0000_0000_0000);
/// 1/8 (`DAT_142ec8948`).
pub(super) const EIGHTH: f64 = f64::from_bits(0x3fc0_0000_0000_0000);
/// 8 (`DAT_142ec8950`).
pub(super) const EIGHT: f64 = f64::from_bits(0x4020_0000_0000_0000);
/// The terms per precision (`init_jk`, the table at `0x142ec87e0`).
pub(super) const INIT_JK: [usize; 4] = [3, 4, 4, 6];
/// 2/π in 24-bit chunks (`ipio2`, the table at `0x142ec87f0`).
pub(super) const IPIO2: [i32; 66] = [
    0xa2_f983, 0x6e_4e44, 0x15_29fc, 0x27_57d1, 0xf5_34dd, 0xc0_db62, 0x95_993c, 0x43_9041,
    0xfe_5163, 0xab_debb, 0xc5_61b7, 0x24_6e3a, 0x42_4dd2, 0xe0_0649, 0x2e_ea09, 0xd1_921c,
    0xfe_1deb, 0x1c_b129, 0xa7_3ee8, 0x82_35f5, 0x2e_bb44, 0x84_e99c, 0x70_26b4, 0x5f_7e41,
    0x39_91d6, 0x39_8353, 0x39_f49c, 0x84_5f8b, 0xbd_f928, 0x3b_1ff8, 0x97_ffde, 0x05_980f,
    0xef_2f11, 0x8b_5a0a, 0x6d_1f6d, 0x36_7ecf, 0x27_cb09, 0xb7_4f46, 0x3f_669e, 0x5f_ea2d,
    0x75_27ba, 0xc7_ebe5, 0xf1_7b3d, 0x07_39f7, 0x8a_5292, 0xea_6bfb, 0x5f_b11f, 0x8d_5d08,
    0x56_0330, 0x46_fc7b, 0x6b_abf0, 0xcf_bc20, 0x9a_f436, 0x1d_a9e3, 0x91_615e, 0xe6_1b08,
    0x65_9985, 0x5f_14a0, 0x68_408d, 0xff_d880, 0x4d_7327, 0x31_0606, 0x15_56ca, 0x73_a8c9,
    0x60_e27b, 0xc0_8c6b,
];
/// π/2's first 24 bits (`DAT_142ec8900`).
pub(super) const PIO2_0: f64 = f64::from_bits(0x3ff9_21fb_4000_0000);
/// π/2's next 24 bits (`DAT_142ec8908`).
pub(super) const PIO2_1: f64 = f64::from_bits(0x3e74_442d_0000_0000);
/// π/2's third 24 bits (`DAT_142ec8910`).
pub(super) const PIO2_2: f64 = f64::from_bits(0x3cf8_4698_8000_0000);
/// π/2's fourth 24 bits (`DAT_142ec8918`).
pub(super) const PIO2_3: f64 = f64::from_bits(0x3b78_cc51_6000_0000);
/// π/2's fifth 24 bits (`DAT_142ec8920`).
pub(super) const PIO2_4: f64 = f64::from_bits(0x39f0_1b83_8000_0000);
/// π/2's sixth 24 bits (`DAT_142ec8928`).
pub(super) const PIO2_5: f64 = f64::from_bits(0x387a_2520_4000_0000);
/// π/2's seventh 24 bits (`DAT_142ec8930`).
pub(super) const PIO2_6: f64 = f64::from_bits(0x36e3_8222_8000_0000);
/// π/2's eighth 24 bits (`DAT_142ec8938`).
pub(super) const PIO2_7: f64 = f64::from_bits(0x3569_f31d_0000_0000);
/// π/2 in 24-bit pieces (`PIo2`, the table at `0x142ec8900`).
pub(super) const PIO2: [f64; 8] = [
    PIO2_0, PIO2_1, PIO2_2, PIO2_3, PIO2_4, PIO2_5, PIO2_6, PIO2_7,
];

/// `0x14099cbc0` (msun's `__kernel_rem_pio2`): `x` modulo π/2 for a huge argument, by its bits
/// against 2/π's (Payne and Hanek). `x` holds the argument's 24-bit chunks (at most 3, the first
/// scaled by 2^`e0`); `prec` (0 to 3) picks how many doubles carry the remainder. Returns the
/// quadrant count's low 3 bits and the remainder's pieces (the unused ones 0).
pub(super) fn rem_pio2_large(x: &[f64], e0: i32, prec: usize) -> (i32, [f64; 3]) {
    let jk = INIT_JK[prec];
    let jp = jk;
    let jx = x.len() - 1;
    let jv = ((e0 - 3) / 24).max(0) as usize;
    let mut q0 = e0 - 24 * (jv as i32 + 1);
    let mut f = [0.0; 20];
    let mut q = [0.0; 20];
    let mut fq = [0.0; 20];
    let mut iq = [0_i32; 20];

    // f[0..=jx + jk]: 2/π's chunks from jv − jx on, 0 before the table
    for (i, f) in f.iter_mut().enumerate().take(jx + jk + 1) {
        let j = (jv + i).checked_sub(jx);
        *f = j.map_or(0.0, |j| f64::from(IPIO2[j]));
    }
    // q[0..=jk]: the products x · f
    for i in 0..=jk {
        let mut fw = 0.0;
        for j in 0..=jx {
            fw += x[j] * f[jx + i - j];
        }
        q[i] = fw;
    }

    let mut jz = jk;
    let (mut n, mut ih, mut z);
    loop {
        // q[] into the integer chunks iq[], highest last
        z = q[jz];
        let mut j = jz;
        let mut i = 0;
        while j > 0 {
            let fw = f64::from((TWO_POW_MINUS_24 * z) as i32);
            iq[i] = (z - TWO_POW_24 * fw) as i32;
            z = q[j - 1] + fw;
            i += 1;
            j -= 1;
        }

        // the quadrant count n, z its fraction
        z = scalbn(z, q0);
        z -= EIGHT * floor(z * EIGHTH);
        n = z as i32;
        z -= f64::from(n);
        ih = 0;
        if q0 > 0 {
            // iq[jz − 1] holds n's low bits
            let i = iq[jz - 1] >> (24 - q0);
            n += i;
            iq[jz - 1] -= i << (24 - q0);
            ih = iq[jz - 1] >> (23 - q0);
        } else if q0 == 0 {
            ih = iq[jz - 1] >> 23;
        } else if z >= HALF_F64 {
            ih = 2;
        }

        if ih > 0 {
            // the fraction above 1/2: take 1 − q
            n += 1;
            let carry = complement_chunks(&mut iq[..jz], q0);
            if ih == 2 {
                z = ONE_F64 - z;
                if carry {
                    z -= scalbn(ONE_F64, q0);
                }
            }
        }

        // all of the fraction's chunks 0: take more terms of 2/π and compute again
        if z == 0.0 && iq[jk..jz].iter().fold(0, |j, &chunk| j | chunk) == 0 {
            let mut k = 1;
            while iq[jk - k] == 0 {
                k += 1;
            }
            for i in jz + 1..=jz + k {
                f[jx + i] = f64::from(IPIO2[jv + i]);
                let mut fw = 0.0;
                for j in 0..=jx {
                    fw += x[j] * f[jx + i - j];
                }
                q[i] = fw;
            }
            jz += k;
            continue;
        }
        break;
    }

    // chop off the zero chunks, or split z into 24-bit ones
    if z == 0.0 {
        jz -= 1;
        q0 -= 24;
        while iq[jz] == 0 {
            jz -= 1;
            q0 -= 24;
        }
    } else {
        z = scalbn(z, -q0);
        if z >= TWO_POW_24 {
            let fw = f64::from((TWO_POW_MINUS_24 * z) as i32);
            iq[jz] = (z - TWO_POW_24 * fw) as i32;
            jz += 1;
            q0 += 24;
            iq[jz] = fw as i32;
        } else {
            iq[jz] = z as i32;
        }
    }

    // the chunks back to doubles
    let mut fw = scalbn(ONE_F64, q0);
    for i in (0..=jz).rev() {
        q[i] = fw * f64::from(iq[i]);
        fw *= TWO_POW_MINUS_24;
    }
    // fq[jz − i] = π/2 · q[i..], at most jp + 1 pieces of π/2
    for i in (0..=jz).rev() {
        let mut fw = 0.0;
        let mut k = 0;
        while k <= jp && k <= jz - i {
            fw += PIO2[k] * q[i + k];
            k += 1;
        }
        fq[jz - i] = fw;
    }

    (n & 7, remainder_pieces(&mut fq, jz, prec, ih != 0))
}

/// [`rem_pio2_large`]'s `1 − q` once the fraction lies above 1/2: from the lowest chunk on, each
/// 0 stays until the first other one becomes 2^24 less it, and every later one 2^24 − 1 less it;
/// with `q0` 1 or 2 the highest chunk keeps only its bits below the quadrant's. Returns whether
/// a chunk was other than 0 (the borrow).
pub(super) fn complement_chunks(iq: &mut [i32], q0: i32) -> bool {
    let mut carry = false;
    for chunk in iq.iter_mut() {
        if carry {
            *chunk = 0xff_ffff - *chunk;
        } else if *chunk != 0 {
            carry = true;
            *chunk = 0x100_0000 - *chunk;
        }
    }
    if let Some(top) = iq.last_mut() {
        match q0 {
            1 => *top &= 0x7f_ffff,
            2 => *top &= 0x3f_ffff,
            _ => {}
        }
    }
    carry
}

/// [`rem_pio2_large`]'s last step, `fq[0..=jz]` into the remainder's pieces by `prec`: 0 one
/// sum, 1 and 2 the sum and its rounding error, 3 three pieces after two compensated passes;
/// each negated when the fraction was complemented (`negative`). The unused pieces are 0.
pub(super) fn remainder_pieces(
    fq: &mut [f64; 20],
    jz: usize,
    prec: usize,
    negative: bool,
) -> [f64; 3] {
    let sign = |v: f64| if negative { -v } else { v };
    let mut y = [0.0; 3];
    match prec {
        0 => {
            let mut fw = 0.0;
            for i in (0..=jz).rev() {
                fw += fq[i];
            }
            y[0] = sign(fw);
        }
        1 | 2 => {
            let mut fw = 0.0;
            for i in (0..=jz).rev() {
                fw += fq[i];
            }
            y[0] = sign(fw);
            fw = fq[0] - fw;
            for v in &fq[1..=jz] {
                fw += v;
            }
            y[1] = sign(fw);
        }
        _ => {
            for i in (1..=jz).rev() {
                let fw = fq[i - 1] + fq[i];
                fq[i] += fq[i - 1] - fw;
                fq[i - 1] = fw;
            }
            for i in (2..=jz).rev() {
                let fw = fq[i - 1] + fq[i];
                fq[i] += fq[i - 1] - fw;
                fq[i - 1] = fw;
            }
            let mut fw = 0.0;
            for i in (2..=jz).rev() {
                fw += fq[i];
            }
            y = [sign(fq[0]), sign(fq[1]), sign(fw)];
        }
    }
    y
}

/// 2/π (`DAT_142ec8968`).
pub(super) const INV_PIO2: f64 = f64::from_bits(0x3fe4_5f30_6dc9_c883);
/// 1.5 · 2^52: added and taken off, it rounds to an integer (`DAT_142ec8980`).
pub(super) const TO_INT: f64 = f64::from_bits(0x4338_0000_0000_0000);
/// π/2's first 33 bits (`DAT_142ec8978`).
pub(super) const PIO2_HI_F64: f64 = f64::from_bits(0x3ff9_21fb_5000_0000);
/// π/2 less [`PIO2_HI_F64`] (`DAT_142ec8960`).
pub(super) const PIO2_TAIL_F64: f64 = f64::from_bits(0x3e51_10b4_611a_6263);
/// π/4 as the float rounds it, in double (`DAT_142ec8970`).
pub(super) const FRAC_PI_4_F64: f64 = f64::from_bits(0x3fe9_21fb_6000_0000);
/// −π/4 as the float rounds it, in double (`DAT_142ec8988`).
pub(super) const MINUS_FRAC_PI_4_F64: f64 = f64::from_bits(0xbfe9_21fb_6000_0000);

/// `0x14099da00` (msun's `__rem_pio2f`): `x` less n · π/2, as the count n and a double remainder
/// within about π/4. Below 2^28 · π/2 n is x · 2/π rounded, the remainder taken with π/2 in two
/// parts (and n moved by one past ±π/4); above, [`rem_pio2_large`] reduces x's 24 bits scaled by
/// its exponent; infinities and NaN give `x − x` and 0.
pub(super) fn rem_pio2(x: f32) -> (i32, f64) {
    let hx = x.to_bits();
    let ix = hx & 0x7fff_ffff;
    if ix < 0x4dc9_0fdb {
        let d = f64::from(x);
        let mut fw = d * INV_PIO2 + TO_INT - TO_INT;
        let mut n = fw as i32;
        let mut y = d - fw * PIO2_HI_F64 - fw * PIO2_TAIL_F64;
        // COMISD −π/4, y then JBE; COMISD y, π/4 then JBE
        if MINUS_FRAC_PI_4_F64 > y {
            fw += MINUS_ONE_F64;
            n -= 1;
            y = d - fw * PIO2_HI_F64 - fw * PIO2_TAIL_F64;
        } else if y > FRAC_PI_4_F64 {
            fw += ONE_F64;
            n += 1;
            y = d - fw * PIO2_HI_F64 - fw * PIO2_TAIL_F64;
        }
        return (n, y);
    }
    if ix >= 0x7f80_0000 {
        #[allow(clippy::eq_op)]
        return (0, widen(sse_nan(x - x, &[x])));
    }
    // x's 24 bits with the exponent 23, scaled by 2^e0
    let e0 = (ix >> 23) as i32 - 150;
    let chunk = f64::from(f32::from_bits(ix - ((e0 as u32) << 23)));
    let (n, y) = rem_pio2_large(&[chunk], e0, 0);
    if hx >> 31 != 0 {
        (-n, -y[0])
    } else {
        (n, y[0])
    }
}
