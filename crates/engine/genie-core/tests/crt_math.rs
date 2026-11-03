//! genie-core's f32 math against build 158041's own routines (sqrtf `0x14099ef70`, acosf
//! `0x14099dc60`, cosf `0x14099e4d0`, sinf `0x14099ed60`, atan2f `0x14099e020`), bit for bit on the
//! inputs of
//! `fixtures/crt_math_158041.txt`: edge cases and seeded random inputs over the ranges movement
//! uses, recorded in the tick oracle's emulator (`load_oracle/probes/crt_math.py`).
//!
//! The ignored sweeps run every f32 through a port and through the `libm` crate, which ports the
//! same msun sources the game links (`cargo test --release -- --ignored`).

use genie_core::math::fmath::{acos, atan, atan2, cos, fmod, log, sin, sqrt};
use std::num::NonZero;
use std::thread::{available_parallelism, scope};

/// Each routine's calls: its name, then per call the inputs and the output as f32 bits.
fn fixture() -> Vec<(String, Vec<Vec<u32>>)> {
    let text = include_str!("fixtures/crt_math_158041.txt");
    let mut out: Vec<(String, Vec<Vec<u32>>)> = Vec::new();
    for line in text.lines() {
        if !line.contains(' ') {
            out.push((line.to_string(), Vec::new()));
            continue;
        }
        let call = line
            .split(' ')
            .map(|s| u32::from_str_radix(s, 16).expect("a hex f32"))
            .collect();
        out.last_mut()
            .expect("a routine name before its calls")
            .1
            .push(call);
    }
    out
}

/// The calls where `ours` gives other bits than the game, as `inputs -> ours, game`.
fn mismatches(name: &str, ours: impl Fn(&[f32]) -> f32) -> Vec<String> {
    let (_, calls) = fixture()
        .into_iter()
        .find(|(n, _)| n == name)
        .expect("the routine is in the fixture");
    assert!(calls.len() > 4000, "{name}: {} calls", calls.len());
    calls
        .into_iter()
        .filter_map(|call| {
            let (game, inputs) = call.split_last().expect("inputs and an output");
            let args: Vec<f32> = inputs.iter().map(|&b| f32::from_bits(b)).collect();
            let got = ours(&args).to_bits();
            // NaN payloads differ between implementations; any NaN matches any NaN
            let both_nan = f32::from_bits(got).is_nan() && f32::from_bits(*game).is_nan();
            (got != *game && !both_nan)
                .then(|| format!("{inputs:08x?} -> {got:08x}, game {game:08x}"))
        })
        .collect()
}

#[test]
fn sqrt_is_the_games_sqrtf() {
    assert_eq!(mismatches("sqrtf", |a| sqrt(a[0])), Vec::<String>::new());
}

#[test]
fn acos_is_the_games_acosf() {
    assert_eq!(mismatches("acosf", |a| acos(a[0])), Vec::<String>::new());
}

#[test]
fn cos_is_the_games_cosf() {
    assert_eq!(mismatches("cosf", |a| cos(a[0])), Vec::<String>::new());
}

#[test]
fn sin_is_the_games_sinf() {
    assert_eq!(mismatches("sinf", |a| sin(a[0])), Vec::<String>::new());
}

#[test]
fn atan2_is_the_games_atan2f() {
    assert_eq!(
        mismatches("atan2f", |a| atan2(a[0], a[1])),
        Vec::<String>::new()
    );
}

/// The f32 inputs (bits) where `ours` and `oracle` give other bits, over all 2^32 of them (at
/// most 16 per thread).
fn sweep(ours: fn(f32) -> f32, oracle: fn(f32) -> f32) -> Vec<u32> {
    let threads = available_parallelism().map_or(4, NonZero::get);
    let threads = u32::try_from(threads).unwrap_or(4);
    let span = u32::MAX / threads + 1;
    scope(|s| {
        let mut workers = Vec::new();
        for t in 0..threads {
            let start = t * span;
            let end = start.saturating_add(span - 1);
            workers.push(s.spawn(move || {
                (start..=end)
                    .filter(|&b| {
                        let x = f32::from_bits(b);
                        ours(x).to_bits() != oracle(x).to_bits()
                    })
                    .take(16)
                    .collect::<Vec<u32>>()
            }));
        }
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("a sweep thread"))
            .collect()
    })
}

#[test]
#[ignore = "expensive: all 2^32 inputs, in release; just test-expensive"]
fn acos_agrees_with_libm_on_every_f32() {
    assert_eq!(sweep(acos, libm::acosf), Vec::<u32>::new());
}

#[test]
#[ignore = "expensive: all 2^32 inputs, in release; just test-expensive"]
fn atan_agrees_with_libm_on_every_f32() {
    assert_eq!(sweep(atan, libm::atanf), Vec::<u32>::new());
}

#[test]
#[ignore = "expensive: 2^30 pairs, in release; just test-expensive"]
fn atan2_agrees_with_libm_on_random_pairs() {
    // libm returns `x + y` for a NaN, the game `y + x`: two NaNs may differ in payload only
    let differ = |y: f32, x: f32| {
        let (ours, theirs) = (atan2(y, x), libm::atan2f(y, x));
        ours.to_bits() != theirs.to_bits() && !(ours.is_nan() && theirs.is_nan())
    };
    let threads = 16u64;
    let bad: Vec<(u32, u32)> = scope(|s| {
        let mut workers = Vec::new();
        for t in 0..threads {
            workers.push(s.spawn(move || {
                // a 64-bit LCG per thread, its high words the two arguments' bits
                let mut state = 0x9e37_79b9_7f4a_7c15_u64 ^ t;
                let mut bad = Vec::new();
                for _ in 0..(1u64 << 30) / threads {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    let (y, x) = (
                        (state >> 32) as u32,
                        u32::try_from(state & 0xffff_ffff).expect("a word"),
                    );
                    if differ(f32::from_bits(y), f32::from_bits(x)) && bad.len() < 16 {
                        bad.push((y, x));
                    }
                }
                bad
            }));
        }
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("a sweep thread"))
            .collect()
    });
    assert_eq!(bad, Vec::new());
}

#[test]
#[ignore = "expensive: all 2^32 inputs, in release; just test-expensive"]
fn cos_agrees_with_libm_on_every_f32() {
    assert_eq!(sweep(cos, libm::cosf), Vec::<u32>::new());
}

#[test]
#[ignore = "expensive: all 2^32 inputs, in release; just test-expensive"]
fn sin_agrees_with_libm_on_every_f32() {
    assert_eq!(sweep(sin, libm::sinf), Vec::<u32>::new());
}

#[test]
#[ignore = "expensive: 2^30 pairs, in release; just test-expensive"]
fn fmod_is_the_exact_remainder_on_random_pairs() {
    // the exact remainder of two floats is a float: the double's `%` (fmod) gives it; libm's
    // fmodf is a second opinion; NaNs match any NaN
    let differ = |x: f32, y: f32| {
        let ours = fmod(x, y);
        // the remainder is a float already: the cast does not round
        #[allow(clippy::cast_possible_truncation)]
        let exact = (f64::from(x) % f64::from(y)) as f32;
        let theirs = libm::fmodf(x, y);
        [exact, theirs]
            .iter()
            .any(|r| r.to_bits() != ours.to_bits() && !(r.is_nan() && ours.is_nan()))
    };
    let threads = 16u64;
    let bad: Vec<(u32, u32)> = scope(|s| {
        let mut workers = Vec::new();
        for t in 0..threads {
            workers.push(s.spawn(move || {
                let mut state = 0x2545_f491_4f6c_dd1d_u64 ^ t;
                let mut bad = Vec::new();
                for k in 0..(1u64 << 30) / threads {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    let (mut x, mut y) = (
                        (state >> 32) as u32,
                        u32::try_from(state & 0xffff_ffff).expect("a word"),
                    );
                    if k % 2 == 0 {
                        // exponents close together, where the subtraction loop runs long
                        y = (y & 0x807f_ffff)
                            | (x & 0x7f80_0000)
                                .saturating_sub(u32::try_from(k % 30).expect("below 30") << 23);
                        x &= 0xff7f_ffff;
                    }
                    if differ(f32::from_bits(x), f32::from_bits(y)) && bad.len() < 16 {
                        bad.push((x, y));
                    }
                }
                bad
            }));
        }
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("a sweep thread"))
            .collect()
    });
    assert_eq!(bad, Vec::new());
}

/// logf against the game's CRT import (`api-ms-win-crt-math-l1-1-0.dll!logf` through the thunk
/// `0x142a8e85a`) as the tick oracle's emulator runs it, bit for bit on the positive inputs of
/// `fixtures/logf_158041.txt`: the `fuzz` probe's edge cases and two seeded draws (158041 and
/// 26969) of 8192 calls each, the calls the stand-in answered (it raises at 0 and below, as
/// Python's `math.log` does).
#[test]
fn log_is_the_games_logf() {
    let text = include_str!("fixtures/logf_158041.txt");
    let mut calls = 0;
    let bad: Vec<String> = text
        .lines()
        .skip(1)
        .filter_map(|line| {
            let (x, game) = line.split_once(' ').expect("an input and an output");
            let (x, game) = (
                u32::from_str_radix(x, 16).expect("a hex f32"),
                u32::from_str_radix(game, 16).expect("a hex f32"),
            );
            calls += 1;
            let got = log(f32::from_bits(x)).to_bits();
            let both_nan = f32::from_bits(got).is_nan() && f32::from_bits(game).is_nan();
            (got != game && !both_nan).then(|| format!("{x:08x} -> {got:08x}, game {game:08x}"))
        })
        .collect();
    assert!(calls > 8000, "{calls} calls");
    assert_eq!(bad, Vec::<String>::new());
}
