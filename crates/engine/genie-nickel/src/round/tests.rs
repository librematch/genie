use crate::round::round_to_f32;
use crate::round::round_to_f64;
use num_bigint::BigUint;
use std::iter::repeat_with;

/// The exact rational of a float of `bits` with `mantissa_bits` stored bits and exponent `bias`,
/// as decimal numerator and denominator strings.
fn exact(
    negative: bool,
    exponent: i64,
    mantissa: u64,
    mantissa_bits: i64,
    bias: i64,
) -> (String, String) {
    let significand = mantissa | if exponent == 0 { 0 } else { 1 << mantissa_bits };
    let shift = if exponent == 0 {
        1 - bias - mantissa_bits
    } else {
        exponent - bias - mantissa_bits
    };
    let sign = if negative { "-" } else { "" };
    if shift >= 0 {
        (
            format!(
                "{sign}{}",
                BigUint::from(significand) << usize::try_from(shift).unwrap()
            ),
            "1".into(),
        )
    } else {
        (
            format!("{sign}{significand}"),
            (BigUint::from(1u8) << usize::try_from(-shift).unwrap()).to_string(),
        )
    }
}

/// The exact rational of an f32.
fn exact_f32(x: f32) -> (String, String) {
    let bits = x.to_bits();
    exact(
        x.is_sign_negative(),
        i64::from((bits >> 23) & 0xff),
        u64::from(bits & 0x7f_ffff),
        23,
        127,
    )
}

/// The exact rational of an f64.
fn exact_f64(x: f64) -> (String, String) {
    let bits = x.to_bits();
    exact(
        x.is_sign_negative(),
        i64::try_from((bits >> 52) & 0x7ff).unwrap(),
        bits & 0xf_ffff_ffff_ffff,
        52,
        1023,
    )
}

/// The rational of a decimal string (`0.010000001`, `-3.5`, `1e-7` as Rust prints it).
fn decimal(s: &str) -> (String, String) {
    let (mantissa, exponent) = s
        .split_once('e')
        .map_or((s, 0), |(m, e)| (m, e.parse::<i64>().unwrap()));
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = format!("{int}{frac}");
    let scale = exponent - i64::try_from(frac.len()).unwrap();
    if scale >= 0 {
        (
            format!("{digits}{}", "0".repeat(usize::try_from(scale).unwrap())),
            "1".into(),
        )
    } else {
        (
            digits,
            format!("1{}", "0".repeat(usize::try_from(-scale).unwrap())),
        )
    }
}

/// xorshift32, the samples' draws.
fn draws(seed: u32) -> impl Iterator<Item = u32> {
    let mut state = seed;
    repeat_with(move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    })
}

/// Every sampled f32 comes back from its exact rational and from its shortest decimal: the
/// named edge values, the subnormals and 100,000 drawn bit patterns.
#[test]
fn every_sampled_f32_comes_back_from_its_exact_rational_and_its_shortest_decimal() {
    let mut samples = vec![
        0.01f32,
        0.010_000_001,
        0.1,
        1.2,
        75.0,
        625.0,
        60000.0,
        f32::MAX,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        f32::from_bits(0x007f_ffff),
        -0.5,
        1e-7,
        -f32::MAX,
    ];
    samples.extend(
        draws(0x2545_f491)
            .take(100_000)
            .map(f32::from_bits)
            .filter(|x| x.is_finite()),
    );
    for x in samples {
        let (n, d) = exact_f32(x);
        assert_eq!(
            round_to_f32(&n, &d).unwrap().to_bits(),
            x.to_bits(),
            "exact {x:e}"
        );
        let (n, d) = decimal(&format!("{x:e}"));
        assert_eq!(
            round_to_f32(&n, &d).unwrap().to_bits(),
            x.to_bits(),
            "shortest {x:e}"
        );
    }
}

/// Every sampled f64 comes back from its exact rational and its shortest decimal.
#[test]
fn every_sampled_f64_comes_back_from_its_exact_rational_and_its_shortest_decimal() {
    let mut samples = vec![
        0.1f64,
        0.12,
        0.15,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        -2.5,
    ];
    let mut high = draws(0x9e37_79b9);
    samples.extend(
        draws(0x2545_f491)
            .zip(&mut high)
            .take(50_000)
            .map(|(lo, hi)| f64::from_bits((u64::from(hi) << 32) | u64::from(lo)))
            .filter(|x| x.is_finite()),
    );
    for x in samples {
        let (n, d) = exact_f64(x);
        assert_eq!(
            round_to_f64(&n, &d).unwrap().to_bits(),
            x.to_bits(),
            "exact {x:e}"
        );
        let (n, d) = decimal(&format!("{x:e}"));
        assert_eq!(
            round_to_f64(&n, &d).unwrap().to_bits(),
            x.to_bits(),
            "shortest {x:e}"
        );
    }
}

/// A value halfway between two f32s rounds to the even one.
#[test]
fn halfway_values_round_to_the_even_neighbour() {
    // 1 + 2^-24 lies halfway between 1.0 (even) and 1.0000001 (odd)
    let half_ulp = BigUint::from(1u8) << 24usize;
    let n = (&half_ulp + 1u8).to_string();
    assert_eq!(
        round_to_f32(&n, &half_ulp.to_string()).unwrap().to_bits(),
        1.0f32.to_bits()
    );
    // 1 + 3·2^-24 lies halfway between 1.0000001 (odd) and 1.0000002 (even)
    let n = (&half_ulp + 3u8).to_string();
    assert_eq!(
        round_to_f32(&n, &half_ulp.to_string()).unwrap().to_bits(),
        0x3f80_0002
    );
    // 1 + 2^-53 lies halfway between 1.0 and the next f64: the even one, 1.0
    let half = BigUint::from(1u8) << 53usize;
    let text = (&half + 1u8).to_string();
    assert_eq!(
        round_to_f64(&text, &half.to_string()).unwrap().to_bits(),
        1.0f64.to_bits()
    );
}

/// Just above halfway rounds up, where rounding through f64 first rounds to the halfway value
/// and then to even, down.
#[test]
fn values_just_above_halfway_round_up_where_rounding_twice_through_f64_rounds_down() {
    // 1 + 2^-24 + 2^-80: f64 rounds it to the halfway 1 + 2^-24, which then rounds to even, 1.0
    let d = BigUint::from(1u8) << 80usize;
    let n = &d + (BigUint::from(1u8) << 56usize) + 1u8;
    assert_eq!(
        round_to_f32(&n.to_string(), &d.to_string())
            .unwrap()
            .to_bits(),
        0x3f80_0001
    );
}

/// A value beyond the largest float, a zero denominator and a string that is no integer fail; a
/// zero numerator is 0.
#[test]
fn values_beyond_the_largest_float_or_no_rational_fail() {
    let (n, d) = decimal("340282360000000000000000000000000000000");
    assert!(round_to_f32(&n, &d).is_err());
    assert!(round_to_f32("1", "0").is_err());
    assert!(round_to_f32("1.5", "1").is_err());
    assert_eq!(round_to_f32("0", "7").unwrap().to_bits(), 0);
    assert!(round_to_f64(&format!("1{}", "0".repeat(309)), "1").is_err());
}
