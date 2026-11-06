use super::*;
use std::f32::consts::{FRAC_PI_2, LN_2, PI};

/// `routine` gives the game's bits on each `(input, output)` pair, a NaN's payload included. The
/// outputs are build 158041's own routines, run on these inputs in unicorn.
fn assert_bits(name: &str, routine: impl Fn(f32) -> f32, cases: &[(u32, u32)]) {
    for &(x, game) in cases {
        let got = routine(f32::from_bits(x)).to_bits();
        assert_eq!(got, game, "{name}({x:08x}): {got:08x}, game {game:08x}");
    }
}

/// logf: exact at 1 and correctly rounded at a few known values (ln 2, ln 10, ln 1/2, ln 1.5,
/// ln 1.005, the mill's 1 + 1/200), −∞ at the zeros, ∞ at ∞, the default NaN below 0, a NaN
/// quieted; a subnormal goes through the scaling (ln 2^-149).
#[test]
fn log_is_the_correctly_rounded_logarithm() {
    let cases: [(f32, u32); 8] = [
        (1.0, 0x0000_0000),
        (2.0, 0x3f31_7218),
        (10.0, 0x4013_5d8e),
        (0.5, 0xbf31_7218),
        (1.5, 0x3ecf_991f),
        (1.005, 0x3ba3_6e7e),
        (f32::from_bits(1), 0xc2ce_8ed0),
        (f32::MAX, 0x42b1_7218),
    ];
    for (x, want) in cases {
        assert_eq!(log(x).to_bits(), want, "log({x:e})");
    }
    assert_eq!(log(0.0), f32::NEG_INFINITY);
    assert_eq!(log(-0.0), f32::NEG_INFINITY);
    assert_eq!(log(f32::INFINITY), f32::INFINITY);
    assert_eq!(log(-1.0).to_bits(), INDEFINITE);
    assert_eq!(log(f32::NEG_INFINITY).to_bits(), INDEFINITE);
    assert_eq!(log(f32::from_bits(0x7f80_0001)).to_bits(), 0x7fc0_0001);
    assert_eq!(log(f32::from_bits(0xffc0_1234)).to_bits(), 0xffc0_1234);
}

/// The double logarithm's branches: the short series near 1 (|f| below 2^-20, with and
/// without an exponent), the exact powers of two and both arms of the long series agree with
/// the f32 values they round to.
#[test]
fn the_double_logarithm_takes_each_branch() {
    // 1 + 2^-30: the short series, log ≈ 2^-30
    let tiny = log_f64(1.0 + f64::from_bits(0x3e10_0000_0000_0000));
    assert_eq!(tiny as f32, 9.313_226e-10);
    // 4: k·ln 2 alone
    assert_eq!(
        log_f64(4.0).to_bits(),
        (2.0 * LN2_HI + 2.0 * LN2_LO).to_bits()
    );
    // 2 · (1 + 2^-30): the short series with an exponent
    assert_eq!(
        log_f64(2.0 + f64::from_bits(0x3e20_0000_0000_0000)) as f32,
        LN_2
    );
    // 1.2 and 1.4 sit on either side of the i > 0 test
    assert_eq!(log_f64(1.2) as f32, 0.182_321_56);
    assert_eq!(log_f64(1.4) as f32, 0.336_472_23);
    assert_eq!(log_f64(3.0) as f32, 1.098_612_3);
}

/// acosf `0x14099dc60` at its branches' bounds (2^-26, 0.5, 1), zeros, subnormals, infinities
/// and NaNs: |x| above 1 is the invalid 0/0, a NaN comes back quieted.
#[test]
fn acos_is_the_games_acosf_at_its_bounds() {
    assert_bits(
        "acos",
        acos,
        &[
            (0x0000_0000, 0x3fc9_0fda),
            (0x8000_0000, 0x3fc9_0fda),
            (0x0000_0001, 0x3fc9_0fda),
            (0x8000_0001, 0x3fc9_0fda),
            (0x007f_ffff, 0x3fc9_0fda),
            (0x807f_ffff, 0x3fc9_0fda),
            (0x0080_0000, 0x3fc9_0fda),
            (0x8080_0000, 0x3fc9_0fda),
            (0x7f7f_ffff, 0xffc0_0000),
            (0xff7f_ffff, 0xffc0_0000),
            (0x7f80_0000, 0xffc0_0000),
            (0xff80_0000, 0xffc0_0000),
            (0x7fc0_0000, 0x7fc0_0000),
            (0xffc0_0000, 0xffc0_0000),
            (0x7f80_0001, 0x7fc0_0001),
            (0xff81_2345, 0xffc1_2345),
            (0x3280_0000, 0x3fc9_0fda),
            (0x3280_0001, 0x3fc9_0fdb),
            (0xb280_0000, 0x3fc9_0fda),
            (0xb280_0001, 0x3fc9_0fdb),
            (0x3eff_ffff, 0x3f86_0a92),
            (0x3f00_0000, 0x3f86_0a92),
            (0xbeff_ffff, 0x4006_0a92),
            (0xbf00_0000, 0x4006_0a92),
            (0x3f7f_ffff, 0x39b5_04f3),
            (0x3f80_0000, 0x0000_0000),
            (0x3f80_0001, 0xffc0_0000),
            (0xbf7f_ffff, 0x4049_0a32),
            (0xbf80_0000, 0x4049_0fda),
            (0xbf80_0001, 0xffc0_0000),
            (0x4000_0000, 0xffc0_0000),
            (0x3f35_04f3, 0x3f49_0fdb),
            (0xbf35_04f3, 0x4016_cbe4),
        ],
    );
}

/// atanf `0x14099e2f0` at its branches' bounds (2^-12, 7/16, 11/16, 19/16, 39/16, 2^26),
/// zeros, subnormals, infinities and NaNs: a NaN comes back as it is, a signalling one too.
#[test]
fn atan_is_the_games_atanf_at_its_bounds() {
    assert_bits(
        "atan",
        atan,
        &[
            (0x0000_0000, 0x0000_0000),
            (0x8000_0000, 0x8000_0000),
            (0x0000_0001, 0x0000_0001),
            (0x8000_0001, 0x8000_0001),
            (0x007f_ffff, 0x007f_ffff),
            (0x807f_ffff, 0x807f_ffff),
            (0x0080_0000, 0x0080_0000),
            (0x8080_0000, 0x8080_0000),
            (0x7f7f_ffff, 0x3fc9_0fda),
            (0xff7f_ffff, 0xbfc9_0fda),
            (0x7f80_0000, 0x3fc9_0fda),
            (0xff80_0000, 0xbfc9_0fda),
            (0x7fc0_0000, 0x7fc0_0000),
            (0xffc0_0000, 0xffc0_0000),
            (0x7f80_0001, 0x7f80_0001),
            (0xff81_2345, 0xff81_2345),
            (0x397f_ffff, 0x397f_ffff),
            (0x3980_0000, 0x3980_0000),
            (0xb980_0000, 0xb980_0000),
            (0x3edf_ffff, 0x3ed3_2775),
            (0x3ee0_0000, 0x3ed3_2776),
            (0xbee0_0000, 0xbed3_2776),
            (0x3f2f_ffff, 0x3f1a_2f80),
            (0x3f30_0000, 0x3f1a_2f81),
            (0x3f97_ffff, 0x3f5e_f386),
            (0x3f98_0000, 0x3f5e_f387),
            (0xbf98_0000, 0xbf5e_f387),
            (0x401b_ffff, 0x3f97_3ab9),
            (0x401c_0000, 0x3f97_3ab9),
            (0xc01c_0000, 0xbf97_3ab9),
            (0x4c7f_ffff, 0x3fc9_0fdb),
            (0x4c80_0000, 0x3fc9_0fda),
            (0xcc80_0000, 0xbfc9_0fda),
            (0x3f80_0000, 0x3f49_0fdb),
            (0xbf80_0000, 0xbf49_0fdb),
            (0x3f00_0000, 0x3eed_6338),
            (0x4049_0fdb, 0x3fa1_9dc5),
        ],
    );
}

/// atan2f `0x14099e020` on every pair of ±0, ±1, 1/2, −3, the smallest subnormal, 1e20, ±∞
/// and a NaN: the game's outputs (unicorn).
#[test]
fn atan2_is_the_games_atan2f_on_special_pairs() {
    for (y, x, game) in [
        (0x0000_0000, 0x0000_0000, 0x0000_0000),
        (0x0000_0000, 0x8000_0000, 0x4049_0fdb),
        (0x0000_0000, 0x3f80_0000, 0x0000_0000),
        (0x0000_0000, 0xbf80_0000, 0x4049_0fdb),
        (0x0000_0000, 0x3f00_0000, 0x0000_0000),
        (0x0000_0000, 0xc040_0000, 0x4049_0fdb),
        (0x0000_0000, 0x0000_0001, 0x0000_0000),
        (0x0000_0000, 0x60ad_78ec, 0x0000_0000),
        (0x0000_0000, 0x7f80_0000, 0x0000_0000),
        (0x0000_0000, 0xff80_0000, 0x4049_0fdb),
        (0x0000_0000, 0x7fc0_0001, 0x7fc0_0001),
        (0x8000_0000, 0x0000_0000, 0x8000_0000),
        (0x8000_0000, 0x8000_0000, 0xc049_0fdb),
        (0x8000_0000, 0x3f80_0000, 0x8000_0000),
        (0x8000_0000, 0xbf80_0000, 0xc049_0fdb),
        (0x8000_0000, 0x3f00_0000, 0x8000_0000),
        (0x8000_0000, 0xc040_0000, 0xc049_0fdb),
        (0x8000_0000, 0x0000_0001, 0x8000_0000),
        (0x8000_0000, 0x60ad_78ec, 0x8000_0000),
        (0x8000_0000, 0x7f80_0000, 0x8000_0000),
        (0x8000_0000, 0xff80_0000, 0xc049_0fdb),
        (0x8000_0000, 0x7fc0_0001, 0x7fc0_0001),
        (0x3f80_0000, 0x0000_0000, 0x3fc9_0fdb),
        (0x3f80_0000, 0x8000_0000, 0x3fc9_0fdb),
        (0x3f80_0000, 0x3f80_0000, 0x3f49_0fdb),
        (0x3f80_0000, 0xbf80_0000, 0x4016_cbe4),
        (0x3f80_0000, 0x3f00_0000, 0x3f8d_b70d),
        (0x3f80_0000, 0xc040_0000, 0x4034_784b),
        (0x3f80_0000, 0x0000_0001, 0x3fc9_0fdb),
        (0x3f80_0000, 0x60ad_78ec, 0x1e3c_e508),
        (0x3f80_0000, 0x7f80_0000, 0x0000_0000),
        (0x3f80_0000, 0xff80_0000, 0x4049_0fdb),
        (0x3f80_0000, 0x7fc0_0001, 0x7fc0_0001),
        (0xbf80_0000, 0x0000_0000, 0xbfc9_0fdb),
        (0xbf80_0000, 0x8000_0000, 0xbfc9_0fdb),
        (0xbf80_0000, 0x3f80_0000, 0xbf49_0fdb),
        (0xbf80_0000, 0xbf80_0000, 0xc016_cbe4),
        (0xbf80_0000, 0x3f00_0000, 0xbf8d_b70d),
        (0xbf80_0000, 0xc040_0000, 0xc034_784b),
        (0xbf80_0000, 0x0000_0001, 0xbfc9_0fdb),
        (0xbf80_0000, 0x60ad_78ec, 0x9e3c_e508),
        (0xbf80_0000, 0x7f80_0000, 0x8000_0000),
        (0xbf80_0000, 0xff80_0000, 0xc049_0fdb),
        (0xbf80_0000, 0x7fc0_0001, 0x7fc0_0001),
        (0x3f00_0000, 0x0000_0000, 0x3fc9_0fdb),
        (0x3f00_0000, 0x8000_0000, 0x3fc9_0fdb),
        (0x3f00_0000, 0x3f80_0000, 0x3eed_6338),
        (0x3f00_0000, 0xbf80_0000, 0x402b_6374),
        (0x3f00_0000, 0x3f00_0000, 0x3f49_0fdb),
        (0x3f00_0000, 0xc040_0000, 0x403e_7e0f),
        (0x3f00_0000, 0x0000_0001, 0x3fc9_0fdb),
        (0x3f00_0000, 0x60ad_78ec, 0x1dbc_e508),
        (0x3f00_0000, 0x7f80_0000, 0x0000_0000),
        (0x3f00_0000, 0xff80_0000, 0x4049_0fdb),
        (0x3f00_0000, 0x7fc0_0001, 0x7fc0_0001),
        (0xc040_0000, 0x0000_0000, 0xbfc9_0fdb),
        (0xc040_0000, 0x8000_0000, 0xbfc9_0fdb),
        (0xc040_0000, 0x3f80_0000, 0xbf9f_e0bb),
        (0xc040_0000, 0xbf80_0000, 0xbff2_3efa),
        (0xc040_0000, 0x3f00_0000, 0xbfb3_ec43),
        (0xc040_0000, 0xc040_0000, 0xc016_cbe4),
        (0xc040_0000, 0x0000_0001, 0xbfc9_0fdb),
        (0xc040_0000, 0x60ad_78ec, 0x9f0d_abc6),
        (0xc040_0000, 0x7f80_0000, 0x8000_0000),
        (0xc040_0000, 0xff80_0000, 0xc049_0fdb),
        (0xc040_0000, 0x7fc0_0001, 0x7fc0_0001),
        (0x0000_0001, 0x0000_0000, 0x3fc9_0fdb),
        (0x0000_0001, 0x8000_0000, 0x3fc9_0fdb),
        (0x0000_0001, 0x3f80_0000, 0x0000_0001),
        (0x0000_0001, 0xbf80_0000, 0x4049_0fdb),
        (0x0000_0001, 0x3f00_0000, 0x0000_0002),
        (0x0000_0001, 0xc040_0000, 0x4049_0fdb),
        (0x0000_0001, 0x0000_0001, 0x3f49_0fdb),
        (0x0000_0001, 0x60ad_78ec, 0x0000_0000),
        (0x0000_0001, 0x7f80_0000, 0x0000_0000),
        (0x0000_0001, 0xff80_0000, 0x4049_0fdb),
        (0x0000_0001, 0x7fc0_0001, 0x7fc0_0001),
        (0x60ad_78ec, 0x0000_0000, 0x3fc9_0fdb),
        (0x60ad_78ec, 0x8000_0000, 0x3fc9_0fdb),
        (0x60ad_78ec, 0x3f80_0000, 0x3fc9_0fda),
        (0x60ad_78ec, 0xbf80_0000, 0x3fc9_0fdb),
        (0x60ad_78ec, 0x3f00_0000, 0x3fc9_0fdb),
        (0x60ad_78ec, 0xc040_0000, 0x3fc9_0fdb),
        (0x60ad_78ec, 0x0000_0001, 0x3fc9_0fdb),
        (0x60ad_78ec, 0x60ad_78ec, 0x3f49_0fdb),
        (0x60ad_78ec, 0x7f80_0000, 0x0000_0000),
        (0x60ad_78ec, 0xff80_0000, 0x4049_0fdb),
        (0x60ad_78ec, 0x7fc0_0001, 0x7fc0_0001),
        (0x7f80_0000, 0x0000_0000, 0x3fc9_0fdb),
        (0x7f80_0000, 0x8000_0000, 0x3fc9_0fdb),
        (0x7f80_0000, 0x3f80_0000, 0x3fc9_0fda),
        (0x7f80_0000, 0xbf80_0000, 0x3fc9_0fdb),
        (0x7f80_0000, 0x3f00_0000, 0x3fc9_0fdb),
        (0x7f80_0000, 0xc040_0000, 0x3fc9_0fdb),
        (0x7f80_0000, 0x0000_0001, 0x3fc9_0fdb),
        (0x7f80_0000, 0x60ad_78ec, 0x3fc9_0fdb),
        (0x7f80_0000, 0x7f80_0000, 0x3f49_0fdb),
        (0x7f80_0000, 0xff80_0000, 0x4016_cbe4),
        (0x7f80_0000, 0x7fc0_0001, 0x7fc0_0001),
        (0xff80_0000, 0x0000_0000, 0xbfc9_0fdb),
        (0xff80_0000, 0x8000_0000, 0xbfc9_0fdb),
        (0xff80_0000, 0x3f80_0000, 0xbfc9_0fda),
        (0xff80_0000, 0xbf80_0000, 0xbfc9_0fdb),
        (0xff80_0000, 0x3f00_0000, 0xbfc9_0fdb),
        (0xff80_0000, 0xc040_0000, 0xbfc9_0fdb),
        (0xff80_0000, 0x0000_0001, 0xbfc9_0fdb),
        (0xff80_0000, 0x60ad_78ec, 0xbfc9_0fdb),
        (0xff80_0000, 0x7f80_0000, 0xbf49_0fdb),
        (0xff80_0000, 0xff80_0000, 0xc016_cbe4),
        (0xff80_0000, 0x7fc0_0001, 0x7fc0_0001),
        (0x7fc0_0001, 0x0000_0000, 0x7fc0_0001),
        (0x7fc0_0001, 0x8000_0000, 0x7fc0_0001),
        (0x7fc0_0001, 0x3f80_0000, 0x7fc0_0001),
        (0x7fc0_0001, 0xbf80_0000, 0x7fc0_0001),
        (0x7fc0_0001, 0x3f00_0000, 0x7fc0_0001),
        (0x7fc0_0001, 0xc040_0000, 0x7fc0_0001),
        (0x7fc0_0001, 0x0000_0001, 0x7fc0_0001),
        (0x7fc0_0001, 0x60ad_78ec, 0x7fc0_0001),
        (0x7fc0_0001, 0x7f80_0000, 0x7fc0_0001),
        (0x7fc0_0001, 0xff80_0000, 0x7fc0_0001),
        (0x7fc0_0001, 0x7fc0_0001, 0x7fc0_0001),
    ] {
        let got = atan2(f32::from_bits(y), f32::from_bits(x)).to_bits();
        assert_eq!(
            got, game,
            "atan2({y:08x}, {x:08x}): {got:08x}, game {game:08x}"
        );
    }
}

/// A NaN in either argument gives `y + x` (`ADDSS y, x`): SSE returns the first operand's NaN
/// (y's) when both are NaNs, quieted. (Unicorn's softfloat picks the larger payload there, as
/// the x87 does, so these outputs follow Intel's rule, not the emulator.)
#[test]
fn atan2_of_nan_is_the_sum_of_its_arguments() {
    for (y, x, game) in [
        (0xffc0_0002, 0x7fc0_0003, 0xffc0_0002),
        (0x7f80_0001, 0xff80_0002, 0x7fc0_0001),
        (0x7f80_0001, 0x3f80_0000, 0x7fc0_0001),
        (0x3f80_0000, 0x7f80_0001, 0x7fc0_0001),
    ] {
        let got = atan2(f32::from_bits(y), f32::from_bits(x)).to_bits();
        assert_eq!(
            got, game,
            "atan2({y:08x}, {x:08x}): {got:08x}, game {game:08x}"
        );
    }
}

/// cosf `0x14099e4d0` at its branches' bounds (2^-12, π/4, 3π/4, 5π/4, 7π/4, 9π/4, 2^28·π/2),
/// the multiples of π/4 up to 4π, zeros, subnormals, huge values, infinities and NaNs.
#[test]
fn cos_is_the_games_cosf_at_its_bounds() {
    assert_bits(
        "cos",
        cos,
        &[
            (0x0000_0000, 0x3f80_0000),
            (0x8000_0000, 0x3f80_0000),
            (0x0000_0001, 0x3f80_0000),
            (0x8000_0001, 0x3f80_0000),
            (0x007f_ffff, 0x3f80_0000),
            (0x807f_ffff, 0x3f80_0000),
            (0x0080_0000, 0x3f80_0000),
            (0x8080_0000, 0x3f80_0000),
            (0x7f7f_ffff, 0x3f5a_5f96),
            (0xff7f_ffff, 0x3f5a_5f96),
            (0x7f80_0000, 0xffc0_0000),
            (0xff80_0000, 0xffc0_0000),
            (0x7fc0_0000, 0x7fc0_0000),
            (0xffc0_0000, 0xffc0_0000),
            (0x7f80_0001, 0x7fc0_0001),
            (0xff81_2345, 0xffc1_2345),
            (0x397f_ffff, 0x3f80_0000),
            (0x3980_0000, 0x3f80_0000),
            (0xb980_0000, 0x3f80_0000),
            (0x3f49_0fda, 0x3f35_04f4),
            (0x3f49_0fdb, 0x3f35_04f3),
            (0xbf49_0fdb, 0x3f35_04f3),
            (0x4016_cbe3, 0xbf35_04f0),
            (0x4016_cbe4, 0xbf35_04f3),
            (0xc016_cbe4, 0xbf35_04f3),
            (0x407b_53d1, 0xbf35_04f4),
            (0x407b_53d2, 0xbf35_04f1),
            (0xc07b_53d2, 0xbf35_04f1),
            (0x40af_eddf, 0x3f35_04f1),
            (0x40af_ede0, 0x3f35_04f7),
            (0xc0af_ede0, 0x3f35_04f7),
            (0x40e2_31d5, 0x3f35_04f9),
            (0x40e2_31d6, 0x3f35_04f3),
            (0xc0e2_31d6, 0x3f35_04f3),
            (0x4dc9_0fda, 0x3e1d_1cf0),
            (0x4dc9_0fdb, 0x3f2c_42b6),
            (0xcdc9_0fdb, 0x3f2c_42b6),
            (0x5015_02f9, 0x3f5f_84c5),
            (0x7149_f2ca, 0xbf1c_9222),
            (0x6c4a_8ae4, 0x3f61_9f67),
            (0x3f49_0fdb, 0x3f35_04f3),
            (0x3fc9_0fdb, 0xb33b_bd2e),
            (0x4016_cbe4, 0xbf35_04f3),
            (0x4049_0fdb, 0xbf80_0000),
            (0x407b_53d1, 0xbf35_04f4),
            (0x4096_cbe4, 0x324c_de2e),
            (0x40af_eddf, 0x3f35_04f1),
            (0x40c9_0fdb, 0x3f80_0000),
            (0x40e2_31d6, 0x3f35_04f3),
            (0x40fb_53d1, 0x3415_5386),
            (0x410a_3ae6, 0xbf35_04f0),
            (0x4116_cbe4, 0xbf80_0000),
            (0x4123_5ce2, 0xbf35_04ef),
            (0x412f_eddf, 0xb49b_ba77),
            (0x413c_7edd, 0x3f35_04f4),
            (0x4149_0fdb, 0x3f80_0000),
        ],
    );
}

/// sinf `0x14099ed60` on the inputs of the cosf test.
#[test]
fn sin_is_the_games_sinf_at_its_bounds() {
    assert_bits(
        "sin",
        sin,
        &[
            (0x0000_0000, 0x0000_0000),
            (0x8000_0000, 0x8000_0000),
            (0x0000_0001, 0x0000_0001),
            (0x8000_0001, 0x8000_0001),
            (0x007f_ffff, 0x007f_ffff),
            (0x807f_ffff, 0x807f_ffff),
            (0x0080_0000, 0x0080_0000),
            (0x8080_0000, 0x8080_0000),
            (0x7f7f_ffff, 0xbf05_99b3),
            (0xff7f_ffff, 0x3f05_99b3),
            (0x7f80_0000, 0xffc0_0000),
            (0xff80_0000, 0xffc0_0000),
            (0x7fc0_0000, 0x7fc0_0000),
            (0xffc0_0000, 0xffc0_0000),
            (0x7f80_0001, 0x7fc0_0001),
            (0xff81_2345, 0xffc1_2345),
            (0x397f_ffff, 0x397f_ffff),
            (0x3980_0000, 0x3980_0000),
            (0xb980_0000, 0xb980_0000),
            (0x3f49_0fda, 0x3f35_04f3),
            (0x3f49_0fdb, 0x3f35_04f3),
            (0xbf49_0fdb, 0xbf35_04f3),
            (0x4016_cbe3, 0x3f35_04f6),
            (0x4016_cbe4, 0x3f35_04f3),
            (0xc016_cbe4, 0xbf35_04f3),
            (0x407b_53d1, 0xbf35_04f2),
            (0x407b_53d2, 0xbf35_04f5),
            (0xc07b_53d2, 0x3f35_04f5),
            (0x40af_eddf, 0xbf35_04f5),
            (0x40af_ede0, 0xbf35_04ef),
            (0xc0af_ede0, 0x3f35_04ef),
            (0x40e2_31d5, 0x3f35_04ee),
            (0x40e2_31d6, 0x3f35_04f3),
            (0xc0e2_31d6, 0xbf35_04f3),
            (0x4dc9_0fda, 0xbf7c_f804),
            (0x4dc9_0fdb, 0xbf3d_5f98),
            (0xcdc9_0fdb, 0x3f3d_5f98),
            (0x5015_02f9, 0xbef9_9a64),
            (0x7149_f2ca, 0xbf4a_89b0),
            (0x6c4a_8ae4, 0x3ef1_e998),
            (0x3f49_0fdb, 0x3f35_04f3),
            (0x3fc9_0fdb, 0x3f80_0000),
            (0x4016_cbe4, 0x3f35_04f3),
            (0x4049_0fdb, 0xb3bb_bd2e),
            (0x407b_53d1, 0xbf35_04f2),
            (0x4096_cbe4, 0xbf80_0000),
            (0x40af_eddf, 0xbf35_04f5),
            (0x40c9_0fdb, 0x343b_bd2e),
            (0x40e2_31d6, 0x3f35_04f3),
            (0x40fb_53d1, 0x3f80_0000),
            (0x410a_3ae6, 0x3f35_04f7),
            (0x4116_cbe4, 0xb2cc_de2e),
            (0x4123_5ce2, 0xbf35_04f7),
            (0x412f_eddf, 0xbf80_0000),
            (0x413c_7edd, 0xbf35_04f3),
            (0x4149_0fdb, 0x34bb_bd2e),
        ],
    );
}

/// `0x14099db70` (`__kernel_sindf`) and `0x14099cad0` (`__kernel_cosdf`) on doubles: zeros, a
/// subnormal, small values, ±π/4 (the reduction's float bound and the double), 1, 2 and the
/// non-finite, rounded to f32 as the game does.
#[test]
fn the_kernels_are_the_games_on_doubles() {
    for (x, game) in [
        (0x0000_0000_0000_0000, 0x0000_0000),
        (0x8000_0000_0000_0000, 0x0000_0000),
        (0x0000_0000_0000_0001, 0x0000_0000),
        (0x3e40_0000_0000_0000, 0x3200_0000),
        (0xbe40_0000_0000_0000, 0xb200_0000),
        (0x3fe0_0000_0000_0000, 0x3ef5_7744),
        (0xbfe0_0000_0000_0000, 0xbef5_7744),
        (0x3fe9_21fb_6000_0000, 0x3f35_04f3),
        (0xbfe9_21fb_6000_0000, 0xbf35_04f3),
        (0x3fe9_21fb_5444_2d18, 0x3f35_04f3),
        (0x3fd5_5555_5555_5555, 0x3ea7_8610),
        (0x3f50_624d_d2f1_a9fc, 0x3a83_126d),
        (0x3ff0_0000_0000_0000, 0x3f57_6aa5),
        (0x4000_0000_0000_0000, 0x3f68_c9e4),
        (0x7ff0_0000_0000_0000, 0x7f80_0000),
        (0xfff0_0000_0000_0000, 0xff80_0000),
        (0x7ff8_0000_0000_0001, 0x7fc0_0000),
    ] {
        let got = kernel_sin(f64::from_bits(x)).to_bits();
        assert_eq!(got, game, "k_sin({x:016x}): {got:08x}, game {game:08x}");
    }
    for (x, game) in [
        (0x0000_0000_0000_0000, 0x3f80_0000),
        (0x8000_0000_0000_0000, 0x3f80_0000),
        (0x0000_0000_0000_0001, 0x3f80_0000),
        (0x3e40_0000_0000_0000, 0x3f80_0000),
        (0xbe40_0000_0000_0000, 0x3f80_0000),
        (0x3fe0_0000_0000_0000, 0x3f60_a940),
        (0xbfe0_0000_0000_0000, 0x3f60_a940),
        (0x3fe9_21fb_6000_0000, 0x3f35_04f3),
        (0xbfe9_21fb_6000_0000, 0x3f35_04f3),
        (0x3fe9_21fb_5444_2d18, 0x3f35_04f3),
        (0x3fd5_5555_5555_5555, 0x3f71_e8b3),
        (0x3f50_624d_d2f1_a9fc, 0x3f7f_fff8),
        (0x3ff0_0000_0000_0000, 0x3f0a_5141),
        (0x4000_0000_0000_0000, 0xbed4_f969),
        (0x7ff0_0000_0000_0000, 0xffc0_0000),
        (0xfff0_0000_0000_0000, 0xffc0_0000),
        (0x7ff8_0000_0000_0001, 0x7fc0_0000),
    ] {
        let got = kernel_cos(f64::from_bits(x)).to_bits();
        assert_eq!(got, game, "k_cos({x:016x}): {got:08x}, game {game:08x}");
    }
}

/// floor `0x14099e7d0`: halves, both signs, 2^51 and 2^52 around, a tiny
/// negative (−1), zeros kept with their sign, the non-finite as they are.
#[test]
fn floor_is_the_games_floor() {
    for (x, game) in [
        (0x0000_0000_0000_0000, 0x0000_0000_0000_0000),
        (0x8000_0000_0000_0000, 0x8000_0000_0000_0000),
        (0x3fe0_0000_0000_0000, 0x0000_0000_0000_0000),
        (0xbfe0_0000_0000_0000, 0xbff0_0000_0000_0000),
        (0x3fef_fffd_e721_0be9, 0x0000_0000_0000_0000),
        (0xbfef_fffd_e721_0be9, 0xbff0_0000_0000_0000),
        (0x3ff0_0000_0000_0000, 0x3ff0_0000_0000_0000),
        (0xbff0_0000_0000_0000, 0xbff0_0000_0000_0000),
        (0x3ff8_0000_0000_0000, 0x3ff0_0000_0000_0000),
        (0xbff8_0000_0000_0000, 0xc000_0000_0000_0000),
        (0x4008_0000_0000_0000, 0x4008_0000_0000_0000),
        (0xc008_0000_0000_0000, 0xc008_0000_0000_0000),
        (0x4320_0000_0000_0001, 0x4320_0000_0000_0000),
        (0xc320_0000_0000_0001, 0xc320_0000_0000_0002),
        (0x4330_0000_0000_0000, 0x4330_0000_0000_0000),
        (0xc330_0000_0000_0000, 0xc330_0000_0000_0000),
        (0x4330_0000_0000_0001, 0x4330_0000_0000_0001),
        (0x7e37_e43c_8800_759c, 0x7e37_e43c_8800_759c),
        (0x81a5_6e1f_c2f8_f359, 0xbff0_0000_0000_0000),
        (0x0000_0000_0000_0001, 0x0000_0000_0000_0000),
        (0x7ff0_0000_0000_0000, 0x7ff0_0000_0000_0000),
        (0xfff0_0000_0000_0000, 0xfff0_0000_0000_0000),
        (0x40c8_1cd6_c8b4_3958, 0x40c8_1c80_0000_0000),
        (0x7ff8_0000_0000_0123, 0x7ff8_0000_0000_0123),
    ] {
        let got = floor(f64::from_bits(x)).to_bits();
        assert_eq!(got, game, "floor({x:016x}): {got:016x}, game {game:016x}");
    }
}

/// scalbn `0x14099ecd0`: exponents past 1023 (one or two steps, then capped), below −1022
/// (into the subnormals, rounded once), and the non-finite.
#[test]
fn scalbn_is_the_games_scalbn() {
    for (x, exponent, game) in [
        (0x3ff0_0000_0000_0000, 5, 0x4040_0000_0000_0000),
        (0x3ff0_0000_0000_0000, 0, 0x3ff0_0000_0000_0000),
        (0x4008_0000_0000_0000, -2, 0x3fe8_0000_0000_0000),
        (0x3ff0_0000_0000_0000, 1023, 0x7fe0_0000_0000_0000),
        (0x3ff0_0000_0000_0000, 1024, 0x7ff0_0000_0000_0000),
        (0x3ff0_0000_0000_0000, 2046, 0x7ff0_0000_0000_0000),
        (0x3ff0_0000_0000_0000, 2047, 0x7ff0_0000_0000_0000),
        (0x3fe0_0000_0000_0000, 3000, 0x7ff0_0000_0000_0000),
        (0x3ff0_0000_0000_0000, -1022, 0x0010_0000_0000_0000),
        (0x3ff0_0000_0000_0000, -1023, 0x0008_0000_0000_0000),
        (0x3ff0_0000_0000_0000, -1074, 0x0000_0000_0000_0001),
        (0x3ff0_0000_0000_0000, -1075, 0x0000_0000_0000_0000),
        (0x3ff8_0000_0000_0000, -1074, 0x0000_0000_0000_0002),
        (0x4008_0000_0000_0000, -1076, 0x0000_0000_0000_0001),
        (0x3ff0_0000_0000_0000, -2000, 0x0000_0000_0000_0000),
        (0x3ff0_0000_0000_0000, -3000, 0x0000_0000_0000_0000),
        (0x01a5_6e1f_c2f8_f359, 900, 0x39e5_6e1f_c2f8_f359),
        (0x7e37_e43c_8800_759c, -2000, 0x0137_e43c_8800_759c),
        (0x7ff0_0000_0000_0000, -5, 0x7ff0_0000_0000_0000),
        (0x8000_0000_0000_0000, 7, 0x8000_0000_0000_0000),
        (0xc01d_0000_0000_0000, 3, 0xc04d_0000_0000_0000),
    ] {
        let got = scalbn(f64::from_bits(x), exponent).to_bits();
        assert_eq!(
            got, game,
            "scalbn({x:016x}, {exponent}): {got:016x}, game {game:016x}"
        );
    }
}

/// `__rem_pio2f` `0x14099da00`: the quadrant count and the remainder, below 2^28·π/2 by the
/// rounded quotient, above by `__rem_pio2_large`, the non-finite as `x − x` with 0.
#[test]
fn rem_pio2_is_the_games_reduction() {
    for (x, quadrant, game) in [
        (0x3f49_0fdb, 1, 0xbfe9_21fb_4888_5a31),
        (0x3fc9_0fdb, 1, 0x3e67_77a5_cf72_cece),
        (0x4049_0fdb, 2, 0x3e77_77a5_cf72_cece),
        (0x40c9_0fdb, 4, 0x3e87_77a5_cf72_cece),
        (0xc0c9_0fdb, -4, 0xbe87_77a5_cf72_cece),
        (0x40e2_31d6, 5, 0xbfe9_21fb_4aa9_c2f3),
        (0x4b80_0000, 10_680_707, 0x3fe5_a892_0fc7_46b2),
        (0x4dc9_0fda, 268_435_443, 0x3fc3_b78e_1866_0580),
        (0x4dc9_0fdb, 7, 0x3fe7_9e9e_5b72_7593),
        (0xcdc9_0fdb, -7, 0xbfe7_9e9e_5b72_7593),
        (0x4e00_0000, 6, 0xbfd5_4a76_7d18_30ee),
        (0x5a00_0000, 3, 0xbfe1_d1af_db06_8d79),
        (0x6c4a_8ae4, 4, 0x3fdf_7eb4_e77f_f98c),
        (0x7149_f2ca, 7, 0xbfe5_0f0d_49e8_d06f),
        (0x7f7f_ffff, 4, 0xbfe1_91cf_e681_daf7),
        (0xff7f_ffff, -4, 0x3fe1_91cf_e681_daf7),
        (0x7f80_0000, 0, 0xfff8_0000_0000_0000),
        (0xff80_0000, 0, 0xfff8_0000_0000_0000),
        (0x7fc0_0000, 0, 0x7ff8_0000_0000_0000),
        (0x5d7f_9c2a, 7, 0xbfe6_5821_b8a0_00a7),
        (0x47c3_4f80, 63661, 0x3fe1_1f0c_047c_0469),
        (0x4216_cbe4, 24, 0x3e79_9bc5_b961_b1b0),
        (0xc800_0000, -83443, 0xbfa5_8e8e_c12c_0aa8),
    ] {
        let (got_n, got) = rem_pio2(f32::from_bits(x));
        assert_eq!(
            (got_n, got.to_bits()),
            (quadrant, game),
            "rem_pio2({x:08x}): {got_n} {:016x}, game {quadrant} {game:016x}",
            got.to_bits()
        );
    }
}

/// `__rem_pio2f` on huge inputs whose reduction takes the rare branches: the fraction's chunks
/// all 0 (one or two more terms of 2/π, with and without the carry of 1 − q), q0 of 0, 1 and
/// 2 with the high chunk's bit test (ih 1), and a negative input.
#[test]
fn rem_pio2_takes_more_terms_when_the_fraction_chunks_are_zero() {
    for (x, quadrant, game) in [
        (0x6f79_be45, 5, 0x3e1b_bdd5_2a58_eafb),
        (0x7b16_75c0, 3, 0x3e70_e91f_c449_51b6),
        (0x57b8_2989, 3, 0x3e6c_b7dc_b9c8_a533),
        (0x63e6_00c1, 1, 0x3e6d_a72e_8575_cfdf),
        (0x6ff9_be45, 2, 0x3e2b_bdd5_2a58_eafb),
        (0x5838_2989, 6, 0x3e7c_b7dc_b9c8_a533),
        (0x6466_00c1, 2, 0x3e7d_a72e_8575_cfdf),
        (0x7034_9fe1, 7, 0x3e7b_0431_1a18_8133),
        (0x636c_1b4a, 6, 0xbe74_5d0d_4e3e_7488),
        (0x7b46_9595, 7, 0xbe74_714e_b3c1_cc19),
        (0x5790_3500, 1, 0xbe77_b153_aba4_8465),
        (0x63ec_1b4a, 4, 0xbe84_5d0d_4e3e_7488),
        (0x6f8a_3cc8, 5, 0xbe79_4853_c772_f283),
        (0x5810_3500, 2, 0xbe87_b153_aba4_8465),
        (0x5858_4f80, 3, 0xbe91_c4fe_c0bb_634c),
        (0x642e_0733, 7, 0xbe56_25d8_2e0e_3264),
        (0x62ec_1b4a, 7, 0xbe64_5d0d_4e3e_7488),
        (0x5700_0002, 1, 0xbfc3_7ba6_e1fe_0b25),
        (0x5780_0001, 3, 0xbfb6_5807_4029_c626),
        (0x5800_0001, 6, 0xbfc6_5807_4029_c626),
        (0x5700_0000, 2, 0x3fb0_9f46_83d2_5023),
        (0x5780_0000, 4, 0x3fc0_9f46_83d2_5023),
        (0x5800_0000, 0, 0x3fd0_9f46_83d2_5023),
        (0x4dc9_43ed, 4, 0xbec7_abaf_e9a6_faf6),
        (0x4dcd_a377, 4, 0x3ebf_8c97_7e1e_2550),
        (0xd810_3500, -2, 0x3e87_b153_aba4_8465),
    ] {
        let (got_n, got) = rem_pio2(f32::from_bits(x));
        assert_eq!(
            (got_n, got.to_bits()),
            (quadrant, game),
            "rem_pio2({x:08x}): {got_n} {:016x}, game {quadrant} {game:016x}",
            got.to_bits()
        );
    }
}

/// cosf and sinf on the rare reductions' inputs: `(x, cos, sin)`, the game's outputs.
#[test]
fn cos_and_sin_are_the_games_on_the_rare_reductions() {
    for (x, game_cos, game_sin) in [
        (0x6f79_be45, 0xb0dd_eea9, 0x3f80_0000),
        (0x7b16_75c0, 0x3387_48fe, 0xbf80_0000),
        (0x57b8_2989, 0x3365_bee6, 0xbf80_0000),
        (0x63e6_00c1, 0xb36d_3974, 0x3f80_0000),
        (0x6ff9_be45, 0xbf80_0000, 0xb15d_eea9),
        (0x5838_2989, 0xbf80_0000, 0xb3e5_bee6),
        (0x6466_00c1, 0xbf80_0000, 0xb3ed_3974),
        (0x7034_9fe1, 0x33d8_2189, 0xbf80_0000),
        (0x636c_1b4a, 0xbf80_0000, 0x33a2_e86a),
        (0x7b46_9595, 0xb3a3_8a76, 0xbf80_0000),
        (0x5790_3500, 0x33bd_8a9d, 0x3f80_0000),
        (0x63ec_1b4a, 0x3f80_0000, 0xb422_e86a),
        (0x6f8a_3cc8, 0x33ca_429e, 0x3f80_0000),
        (0x5810_3500, 0xbf80_0000, 0x343d_8a9d),
        (0x5858_4f80, 0xb48e_27f6, 0xbf80_0000),
        (0x642e_0733, 0xb2b1_2ec1, 0xbf80_0000),
        (0x62ec_1b4a, 0xb322_e86a, 0xbf80_0000),
        (0x5700_0002, 0x3e1b_4352, 0x3f7d_0a4a),
        (0x5780_0001, 0xbdb2_8626, 0xbf7f_0689),
        (0x5800_0001, 0xbf7c_1c09, 0x3e31_d82f),
        (0x5700_0000, 0xbf7f_75e6, 0xbd84_e24a),
        (0x5780_0000, 0x3f7d_d82f, 0x3e04_9a9b),
        (0x5800_0000, 0x3f77_6a06, 0x3e83_7cc6),
        (0x4dc9_43ed, 0x3f80_0000, 0xb63d_5d7f),
        (0x4dcd_a377, 0x3f80_0000, 0x35fc_64bc),
        (0xd810_3500, 0xbf80_0000, 0xb43d_8a9d),
    ] {
        let x = f32::from_bits(x);
        assert_eq!(
            (cos(x).to_bits(), sin(x).to_bits()),
            (game_cos, game_sin),
            "{x:?}"
        );
    }
}

/// `__rem_pio2_large` `0x14099cbc0` on one to three 24-bit chunks, exponents 0 to 1000 and
/// every precision (0: one double, 1 and 2: two, 3: three), the game's results (unicorn).
#[test]
fn rem_pio2_large_is_the_games_at_every_precision() {
    // the chunks, e0, the precision, then the game's count and remainder pieces
    type Case = (&'static [f64], i32, usize, i32, [u64; 3]);
    let cases: [Case; 15] = [
        (
            &[13_821_620.0],
            5,
            0,
            3,
            [
                0xbfe9_1730_74dc_99bc,
                0x0000_0000_0000_0000,
                0x0000_0000_0000_0000,
            ],
        ),
        (
            &[10_919_437.0],
            26,
            0,
            4,
            [
                0x3fd5_8c3c_18eb_c3f0,
                0x0000_0000_0000_0000,
                0x0000_0000_0000_0000,
            ],
        ),
        (
            &[15_012_647.0],
            81,
            0,
            7,
            [
                0xbfe3_72c7_4c2b_c5f8,
                0x0000_0000_0000_0000,
                0x0000_0000_0000_0000,
            ],
        ),
        (
            &[9_198_719.0],
            104,
            0,
            2,
            [
                0x3fe3_7f17_e976_be82,
                0x0000_0000_0000_0000,
                0x0000_0000_0000_0000,
            ],
        ),
        (
            &[9_603_887.0],
            104,
            1,
            1,
            [
                0x3fc4_104e_da58_bb36,
                0x3c0c_7e38_6f19_3dbe,
                0x0000_0000_0000_0000,
            ],
        ),
        (
            &[9_967_848.0],
            104,
            2,
            7,
            [
                0x3fd4_eeb9_53fc_b992,
                0xbc79_e1f2_2945_67f5,
                0x0000_0000_0000_0000,
            ],
        ),
        (
            &[14_523_849.0],
            104,
            3,
            2,
            [
                0xbf7e_b8b2_5b58_25b3,
                0xbc10_b095_8f07_3545,
                0x388e_03aa_a94e_e803,
            ],
        ),
        (
            &[9_361_668.0, 7_204_075.0],
            200,
            0,
            4,
            [
                0xbfe2_2ad2_07d0_6f51,
                0x0000_0000_0000_0000,
                0x0000_0000_0000_0000,
            ],
        ),
        (
            &[9_017_680.0, 2_883_910.0],
            200,
            1,
            6,
            [
                0x3fe3_d963_aa52_f9f5,
                0x3c65_588c_1f11_0910,
                0x0000_0000_0000_0000,
            ],
        ),
        (
            &[15_663_975.0, 14_031_529.0, 2_343_959.0],
            500,
            2,
            5,
            [
                0x3fdf_0d54_52d1_af31,
                0xbc71_35e3_0246_7fe3,
                0x0000_0000_0000_0000,
            ],
        ),
        (
            &[12_426_263.0, 3_043_823.0, 14_244_500.0],
            500,
            3,
            4,
            [
                0xbfdc_71b5_67ff_6277,
                0x3c54_c672_ee3f_1526,
                0xb904_6060_e167_ecab,
            ],
        ),
        (
            &[9_380_317.0, 4_154_104.0, 7_490_656.0],
            1000,
            0,
            2,
            [
                0xbfe7_f8c7_1112_a643,
                0x0000_0000_0000_0000,
                0x0000_0000_0000_0000,
            ],
        ),
        (
            &[9_426_480.0, 13_310_388.0],
            1000,
            3,
            4,
            [
                0xbfda_f745_087e_eb9a,
                0x3c71_136e_7c96_c64f,
                0x3910_16b1_04fb_25a4,
            ],
        ),
        (
            &[9_220_578.0],
            1,
            0,
            5,
            [
                0xbfe7_60b2_3212_f1db,
                0x0000_0000_0000_0000,
                0x0000_0000_0000_0000,
            ],
        ),
        (
            &[12_097_745.0],
            0,
            3,
            0,
            [
                0xbfe0_af60_dbe9_eea1,
                0xbc81_c508_f6a3_b340,
                0xb926_67b1_ee52_51a0,
            ],
        ),
    ];
    for (x, e0, prec, quadrant, game) in cases {
        let (got_n, y) = rem_pio2_large(x, e0, prec);
        assert_eq!(
            (got_n, y.map(f64::to_bits)),
            (quadrant, game),
            "rem_pio2_large({x:?}, {e0}, {prec})"
        );
    }
}

/// fmodf: exact remainders with x's sign (zeros kept signed, subnormals, the largest finite
/// by the smallest subnormal, x below y), x itself by an infinite y; the invalid cases give
/// SSE's default NaN and a NaN argument comes back quieted.
#[test]
fn fmod_is_the_exact_remainder_with_the_dividend_sign() {
    for (x, y, want) in [
        (0x40b0_0000, 0x4000_0000, 0x3fc0_0000),
        (0xc0b0_0000, 0x4000_0000, 0xbfc0_0000),
        (0x40b0_0000, 0xc000_0000, 0x3fc0_0000),
        (0x8000_0000, 0x4040_0000, 0x8000_0000),
        (0x0000_0000, 0x4040_0000, 0x0000_0000),
        (0x4000_0000, 0x4000_0000, 0x0000_0000),
        (0xc000_0000, 0x4000_0000, 0x8000_0000),
        (0x3f80_0000, 0x4040_0000, 0x3f80_0000),
        (0x4180_0000, 0x4180_0000, 0x0000_0000),
        (0x4188_0000, 0x4180_0000, 0x3f80_0000),
        (0xbf80_0000, 0x4180_0000, 0xbf80_0000),
        (0xc0e8_0000, 0x3f00_0000, 0xbe80_0000),
        (0x42c8_3333, 0x3e99_999a, 0x3e4c_cb5c),
        (0x3f80_0000, 0x7f80_0000, 0x3f80_0000),
        (0xbf80_0000, 0xff80_0000, 0xbf80_0000),
        (0x7f7f_ffff, 0x3fc0_0000, 0x0000_0000),
        (0x7f7f_ffff, 0x0000_0001, 0x0000_0000),
        (0x0000_0001, 0x0000_0001, 0x0000_0000),
        (0x0020_b5c6, 0x0000_0001, 0x0000_0000),
        (0x0036_a3b1, 0x0020_b5c6, 0x0015_edeb),
        (0x4b7f_ffff, 0x3f80_0001, 0x34c0_0000),
        (0x0080_0000, 0x007f_ffff, 0x0000_0001),
        (0x7f7f_ffff, 0x007f_ffff, 0x0000_0001),
        (0x5e80_0000, 0x3dcc_cccd, 0x3dcc_c4cd),
        (0x7f80_0000, 0x3f80_0000, 0xffc0_0000),
        (0x3f80_0000, 0x0000_0000, 0xffc0_0000),
        (0x7f80_0000, 0x0000_0000, 0xffc0_0000),
        (0x0000_0000, 0x8000_0000, 0xffc0_0000),
        (0x7fc0_0001, 0x3f80_0000, 0x7fc0_0001),
        (0x3f80_0000, 0xffc0_0002, 0xffc0_0002),
        (0x7f80_0001, 0x4000_0000, 0x7fc0_0001),
    ] {
        let got = fmod(f32::from_bits(x), f32::from_bits(y)).to_bits();
        assert_eq!(
            got, want,
            "fmod({x:08x}, {y:08x}): {got:08x}, want {want:08x}"
        );
    }
}

/// fabsf `0x14099e7b0` clears the sign bit and nothing else, a NaN's payload kept.
#[test]
fn fabs_clears_the_sign_bit() {
    assert_bits(
        "fabs",
        fabs,
        &[
            (0x0000_0000, 0x0000_0000),
            (0x8000_0000, 0x0000_0000),
            (0x8000_0001, 0x0000_0001),
            (0x807f_ffff, 0x007f_ffff),
            (0xff7f_ffff, 0x7f7f_ffff),
            (0xff80_0000, 0x7f80_0000),
            (0xffc0_0000, 0x7fc0_0000),
            (0x7f80_0001, 0x7f80_0001),
            (0xff81_2345, 0x7f81_2345),
        ],
    );
}

/// sqrtf `0x14099ef70` off its normal path: ±0 and +∞ come back as they are; a NaN or a value
/// below 0 goes to `0x14099cb70`'s `(x - x) / (x - x)`, so a NaN comes back quieted with its
/// sign and payload and a negative value (−∞ and the least negative denormal included) gives
/// the default NaN. (The `crt_math` fixture draws no NaN and no negative input, and its compare
/// takes any NaN for any other; these outputs follow the asm and Intel's rule for `SUBSS` and
/// `DIVSS`.)
#[test]
fn sqrt_is_the_games_sqrtf_at_its_special_values() {
    assert_bits(
        "sqrt",
        sqrt,
        &[
            (0x0000_0000, 0x0000_0000),
            (0x8000_0000, 0x8000_0000),
            (0x7f80_0000, 0x7f80_0000),
            (0xbf80_0000, 0xffc0_0000),
            (0x8000_0001, 0xffc0_0000),
            (0xff80_0000, 0xffc0_0000),
            (0x7fc0_0000, 0x7fc0_0000),
            (0x7f80_0001, 0x7fc0_0001),
            (0xff81_2345, 0xffc1_2345),
            (0x7fc0_1234, 0x7fc0_1234),
            (0xffff_ffff, 0xffff_ffff),
        ],
    );
}

/// `0x14099e870` (isnan through the double): true for every NaN, quiet or signalling, of
/// either sign; false for the infinities and the largest finite value.
#[test]
fn is_nan_is_true_for_every_nan_only() {
    for (x, game) in [
        (0x0000_0000, false),
        (0x8000_0001, false),
        (0x007f_ffff, false),
        (0x7f7f_ffff, false),
        (0xff7f_ffff, false),
        (0x7f80_0000, false),
        (0xff80_0000, false),
        (0x7fc0_0000, true),
        (0xffc0_0000, true),
        (0x7f80_0001, true),
        (0xff80_0001, true),
        (0xff81_2345, true),
        (0x7fff_ffff, true),
        (0xffff_ffff, true),
    ] {
        assert_eq!(is_nan(f32::from_bits(x)), game, "{x:08x}");
    }
}

#[test]
fn jc_after_comiss_is_taken_below_and_unordered() {
    assert!(below_or_unordered(1.0, 2.0));
    assert!(!below_or_unordered(2.0, 2.0) && !below_or_unordered(3.0, 2.0));
    assert!(below_or_unordered(f32::NAN, 2.0) && below_or_unordered(2.0, f32::NAN));
    assert!(at_most_or_unordered(2.0, 2.0) && !at_most_or_unordered(3.0, 2.0));
    assert!(at_most_or_unordered(f32::NAN, 2.0) && at_most_or_unordered(2.0, f32::NAN));
}

#[test]
fn minss_and_maxss_take_the_second_operand_when_either_is_nan() {
    assert_eq!((minss(1.0, 2.0), minss(2.0, 1.0)), (1.0, 1.0));
    assert_eq!((maxss(1.0, 2.0), maxss(2.0, 1.0)), (2.0, 2.0));
    assert_eq!(minss(f32::NAN, 2.0), 2.0);
    assert!(minss(2.0, f32::NAN).is_nan());
    assert_eq!(maxss(f32::NAN, 2.0), 2.0);
    assert!(maxss(2.0, f32::NAN).is_nan());
}

#[test]
fn the_angle_between_unit_vectors_follows_the_games_compares() {
    // FUN_14087e650: |d| below 1e-7 gives π/2, 1 and up 0, -1 and down π, else acosf; a NaN
    // dot falls through all three unordered compares (JBE, JC, JC) to acosf(NaN)
    let x = BVector::new(1.0, 0.0, 0.0);
    assert_eq!(angle_between(x, BVector::new(0.0, 0.0, 1.0)), FRAC_PI_2);
    assert_eq!(angle_between(x, x), 0.0);
    assert_eq!(angle_between(x, BVector::new(-1.0, 0.0, 0.0)), PI);
    assert_eq!(angle_between(x, BVector::new(0.5, 0.0, 0.0)), acos(0.5));
    assert!(angle_between(x, BVector::new(f32::NAN, 0.0, 0.0)).is_nan());
}

/// `FUN_14087e710`: (3, 4, 0) lies 4 from the x axis, whatever the direction's length.
#[test]
fn points_distance_from_line() {
    assert_eq!(
        line_distance(
            BVector::new(3.0, 4.0, 0.0),
            BVector::ZERO,
            BVector::new(2.0, 0.0, 0.0)
        ),
        4.0
    );
    assert_eq!(
        line_distance(
            BVector::new(3.0, 0.0, 0.0),
            BVector::ZERO,
            BVector::new(1.0, 0.0, 0.0)
        ),
        0.0
    );
}

/// powf (`FUN_14099e9e0`): exact powers come out exact; the special cases as the routine's
/// branches give them: an exponent of 0 or a base of 1 gives 1, a NaN operand the first NaN
/// quieted, |x| = 1 against ±∞ 1, |x| < 1 against +∞ 0 and against −∞ +∞; a zero base its
/// square (signed for an odd exponent), inverted for a negative exponent; a negative base
/// with a non-integer exponent the default NaN, with an odd one a negative result; a
/// subnormal base normalised; beyond 2^128 +∞ (−∞ with the sign bias), below 2^-150 ±0.
#[test]
fn powf_takes_the_image_branches_and_exact_powers() {
    let bits = |value: f32| value.to_bits();
    for (x, y, want) in [
        (2.0f32, 10.0f32, 1024.0f32),
        (1.5, 2.0, 2.25),
        (3.0, 5.0, 243.0),
        (0.25, 0.5, 0.5),
        (0.5, 2.0, 0.25),
        (-2.0, 3.0, -8.0),
        (-2.0, 2.0, 4.0),
        (2.0, -2.0, 0.25),
    ] {
        assert_eq!(bits(powf(x, y)), bits(want), "{x}^{y}");
    }
    assert_eq!(powf(7.5, 0.0), 1.0);
    assert_eq!(powf(f32::NAN, 0.0), 1.0);
    assert_eq!(powf(1.0, f32::NAN), 1.0);
    let nan = f32::from_bits(0x7fa0_0001);
    assert_eq!(bits(powf(nan, 2.5)), 0x7fe0_0001);
    assert_eq!(bits(powf(2.0, nan)), 0x7fe0_0001);
    assert_eq!(powf(-1.0, f32::INFINITY), 1.0);
    assert_eq!(powf(0.5, f32::INFINITY), 0.0);
    assert_eq!(powf(0.5, f32::NEG_INFINITY), f32::INFINITY);
    assert_eq!(powf(2.0, f32::INFINITY), f32::INFINITY);
    assert_eq!(bits(powf(-0.0, 3.0)), bits(-0.0));
    assert_eq!(bits(powf(-0.0, 2.0)), bits(0.0));
    assert_eq!(powf(0.0, -1.0), f32::INFINITY);
    assert_eq!(powf(f32::INFINITY, 2.0), f32::INFINITY);
    assert_eq!(bits(powf(-2.0, 0.5)), INDEFINITE);
    assert_eq!(bits(powf(-8.0, 1.0 / 3.0)), INDEFINITE);
    let tiny = f32::from_bits(1);
    assert_eq!(powf(tiny, 0.5), f32::from_bits(0x1a35_04f3));
    assert_eq!(powf(2.0, 200.0), f32::INFINITY);
    assert_eq!(powf(-2.0, 201.0), f32::NEG_INFINITY);
    assert_eq!(bits(powf(2.0, -200.0)), bits(0.0));
    assert_eq!(bits(powf(-2.0, -201.0)), bits(-0.0));
}

/// The square of each radius from 0.025 to 7.5 by 0.025 and from 0.1 to 30 by 0.1 is the f32
/// product: on these powf's double log2 and exp2 round as f32 multiplication does.
#[test]
fn powf_squares_the_radii_as_f32_multiplication_does() {
    for index in 1..=300 {
        for x in [index as f32 * 0.025, index as f32 * 0.1] {
            assert_eq!(powf(x, 2.0).to_bits(), (x * x).to_bits(), "{x}");
        }
    }
}
