//! genie-core's ports against build 158041's own routines, call by call: each fixture is the
//! `fuzz` probe's output (`harness-oracle probe SOCKET fuzz ADDR SIG OUT`, the routine run on
//! its arguments' edge cases and seeded draws in the tick oracle's emulator), a header line
//! (`fuzz ADDR SIG`) and per call the arguments' and the result's bits in hex.

use genie_core::math::fmath::atan;
use genie_core::objects::facing::modulo;

/// A call: the arguments' bits and the result's (None where the call faulted in the emulator).
type Call = (Vec<u32>, Option<u32>);

/// A fixture's header and calls.
fn fixture(text: &str) -> (String, Vec<Call>) {
    let mut lines = text.lines();
    let header = lines.next().expect("a header line").to_owned();
    let calls = lines
        .map(|l| {
            let mut texts: Vec<&str> = l.split(' ').collect();
            let result = texts.pop().expect("a result");
            let args = texts
                .iter()
                .map(|s| u32::from_str_radix(s, 16).expect("hex bits"))
                .collect();
            (args, u32::from_str_radix(result, 16).ok())
        })
        .collect();
    (header, calls)
}

/// The calls where `ours` gives other bits than the game, as `arguments -> ours, game`.
fn mismatches(text: &str, header: &str, ours: impl Fn(&[f32]) -> f32) -> Vec<String> {
    let (h, calls) = fixture(text);
    assert_eq!(h, header);
    assert!(calls.len() >= 4096, "{} calls", calls.len());
    calls
        .iter()
        .filter_map(|(args, game)| {
            let game = (*game)?;
            let floats: Vec<f32> = args.iter().map(|&b| f32::from_bits(b)).collect();
            let got = ours(&floats).to_bits();
            (got != game).then(|| format!("{args:08x?} -> {got:08x}, {game:08x}"))
        })
        .collect()
}

/// `facing::modulo`, `FUN_141158f20`, with the CRT's fmodf inside.
#[test]
fn modulo_matches_the_game_bit_for_bit() {
    let texts = mismatches(
        include_str!("fixtures/fuzz_modulo_158041.txt"),
        "fuzz 0x141158f20 f32,f32->f32",
        |a| modulo(a[0], a[1]),
    );
    assert_eq!(texts, Vec::<String>::new());
}

/// `fmath::atan`, the game's atanf `0x14099e2f0`.
#[test]
fn atan_matches_the_game_bit_for_bit() {
    let texts = mismatches(
        include_str!("fixtures/fuzz_atanf_158041.txt"),
        "fuzz 0x14099e2f0 f32->f32",
        |a| atan(a[0]),
    );
    assert_eq!(texts, Vec::<String>::new());
}
