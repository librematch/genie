//! The game's f32 math, bit for bit (checked by `tests/crt_math.rs` against build 158041's own
//! routines on recorded inputs).
//!
//! The game links musl's math routines, which take FreeBSD msun's sources: acosf `0x14099dc60`,
//! atan2f `0x14099e020`, cosf `0x14099e4d0`, sinf `0x14099ed60` and the kernels and argument
//! reduction they call. Each is ported here from its machine code: its constants (read from the
//! image), its operation order, its branches and its integer bit tests. Its sqrtf `0x14099ef70` is
//! an integer Newton iteration that rounds correctly, so IEEE's square root gives the same bits.
//! Its fmodf is the CRT's (an import): the exact remainder, ported as musl's ([`fmod`]).
//!
//! A NaN result's bits are the game's on every host: SSE gives an invalid operation on operands
//! that are not NaN its default NaN with the sign set (`0xffc00000`), where ARM gives it without
//! the sign and WebAssembly either way, and it hands on the first NaN operand quieted, where ARM
//! prefers a signalling one. A routine whose result is a NaN therefore makes its bits itself
//! ([`sse_nan`]).

mod fmod_and_log;
mod inverse_trig;
mod pow;
mod reduction;
mod sin_cos;
mod vectors;

#[cfg(test)]
use self::fmod_and_log::{LN2_HI, LN2_LO, log_f64};
pub use self::fmod_and_log::{fmod, log};
pub use self::inverse_trig::{acos, atan, atan2};
pub use self::pow::powf;
use self::reduction::rem_pio2;
#[cfg(test)]
use self::reduction::{floor, rem_pio2_large, scalbn};
use self::sin_cos::{HALF_F64, MINUS_ONE_F64, ONE_F64};
pub use self::sin_cos::{cos, sin};
#[cfg(test)]
use self::sin_cos::{kernel_cos, kernel_sin};
pub use self::vectors::{
    BVector, Rotation, add, angle_between, cross, dot, length, line_distance, normalize, rotate,
    rotation, scale, sub,
};

/// 1 (`DAT_14330eb8c`).
const ONE: f32 = f32::from_bits(0x3f80_0000);
/// 1/2 (`DAT_142e7f384`).
const HALF: f32 = f32::from_bits(0x3f00_0000);

/// SSE's default NaN in f32 (Intel's quiet-NaN floating-point indefinite): an invalid operation's
/// result on operands that are not NaN.
const INDEFINITE: u32 = 0xffc0_0000;
/// SSE's default NaN in f64.
const INDEFINITE_F64: u64 = 0xfff8_0000_0000_0000;
/// The quiet bit of an f32 NaN.
const QUIET: u32 = 0x0040_0000;
/// The quiet bit of an f64 NaN.
const QUIET_F64: u64 = 0x0008_0000_0000_0000;

/// `result` as SSE gives it: a NaN result is the first NaN of `operands` (in the game's operand
/// order), quieted, else the default NaN ([`INDEFINITE`]).
fn sse_nan(result: f32, operands: &[f32]) -> f32 {
    if !result.is_nan() {
        return result;
    }
    let nan = operands.iter().find(|o| o.is_nan());
    f32::from_bits(nan.map_or(INDEFINITE, |o| o.to_bits() | QUIET))
}

/// [`sse_nan`] in f64.
fn sse_nan_f64(result: f64, operands: &[f64]) -> f64 {
    if !result.is_nan() {
        return result;
    }
    let nan = operands.iter().find(|o| o.is_nan());
    f64::from_bits(nan.map_or(INDEFINITE_F64, |o| o.to_bits() | QUIET_F64))
}

/// An f64 rounded to f32 as `CVTSD2SS` does: a NaN keeps its sign and the top 23 bits of its
/// payload, quieted.
const fn narrow(x: f64) -> f32 {
    if !x.is_nan() {
        return x as f32;
    }
    let b = x.to_bits();
    f32::from_bits(
        ((b >> 32) as u32 & 0x8000_0000) | 0x7f80_0000 | ((b >> 29) as u32 & 0x007f_ffff) | QUIET,
    )
}

/// An f32 widened to f64 as `CVTSS2SD` does: a NaN keeps its sign and payload, quieted.
fn widen(x: f32) -> f64 {
    if !x.is_nan() {
        return f64::from(x);
    }
    let b = u64::from(x.to_bits());
    f64::from_bits(
        ((b & 0x8000_0000) << 32) | 0x7ff0_0000_0000_0000 | ((b & 0x007f_ffff) << 29) | QUIET_F64,
    )
}

/// The square root, as the game's (correctly rounded); below 0 the default NaN.
#[must_use]
pub fn sqrt(x: f32) -> f32 {
    sse_nan(x.sqrt(), &[x])
}

/// fabsf `0x14099e7b0`: `x` with its sign bit cleared (an `AND` on its bits).
const fn fabs(x: f32) -> f32 {
    f32::from_bits(x.to_bits() & 0x7fff_ffff)
}

/// `0x14099e870`, msun's isnan as the game compiles it: `x` widened to a double (`CVTSS2SD`), the
/// high word without its sign plus 1 for a non-zero low word, above the infinity's `0x7ff00000`.
fn is_nan(x: f32) -> bool {
    let d = f64::from(x).to_bits();
    ((d >> 32) as u32 & 0x7fff_ffff) + u32::from(d as u32 != 0) > 0x7ff0_0000
}

/// `COMISS a, b` then `JC`: taken when `a` is less than `b` or either is NaN (unordered).
#[must_use]
pub fn below_or_unordered(a: f32, b: f32) -> bool {
    a < b || a.is_nan() || b.is_nan()
}

/// `COMISS a, b` then `JBE`: taken when `a` is at most `b` or either is NaN (unordered).
#[must_use]
pub fn at_most_or_unordered(a: f32, b: f32) -> bool {
    a <= b || a.is_nan() || b.is_nan()
}

/// `MINSS a, b`: `a` when it is less than `b`, else `b` (also when either is NaN).
#[must_use]
pub fn minss(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

/// `MAXSS a, b`: `a` when it is greater than `b`, else `b` (also when either is NaN).
#[must_use]
pub fn maxss(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

#[cfg(test)]
mod tests;
