//! A Nickel number into an f32 or f64 in one rounding (FD-046-D16).
//!
//! Nickel holds an exact rational, and rounding it straight to the nearest float (ties to even) gives the float a
//! compiler gives the same decimal, where rounding an f32 through f64 first would differ in rare
//! halfway cases.

use genie_error::{GenieError, GenieResult};
use num_bigint::BigInt;
use num_bigint::BigUint;
use num_bigint::Sign;

/// The bits of a float format the rounding needs.
struct Format {
    /// The significand's bits, the hidden one included.
    significand: u64,
    /// The scale of the smallest subnormal: 2^-149 for f32, 2^-1074 for f64.
    least_scale: i64,
    /// The largest scale of a finite value: 2^104 times a 24-bit significand is `f32::MAX`'s.
    most_scale: i64,
}

/// The f32 format.
const F32: Format = Format {
    significand: 24,
    least_scale: -149,
    most_scale: 104,
};

/// The f64 format.
const F64: Format = Format {
    significand: 53,
    least_scale: -1074,
    most_scale: 971,
};

/// `numerator / denominator` (decimal integers, the denominator above 0) as the nearest f32,
/// ties to even.
///
/// # Errors
///
/// A [`GenieError`] when a string is no integer, the denominator is not positive or the value
/// lies beyond the largest f32.
pub fn round_to_f32(numerator: &str, denominator: &str) -> GenieResult<f32> {
    let (negative, significand, exponent) = round(numerator, denominator, &F32)?;
    let bits = (u32::from(negative) << 31)
        | (u32::try_from(exponent).unwrap_or(0) << 23)
        | (u32::try_from(significand).unwrap_or(0) & 0x7f_ffff);
    Ok(f32::from_bits(bits))
}

/// `numerator / denominator` as the nearest f64, ties to even ([`round_to_f32`]'s rounding with
/// 53 bits).
///
/// # Errors
///
/// As [`round_to_f32`], beyond the largest f64.
pub fn round_to_f64(numerator: &str, denominator: &str) -> GenieResult<f64> {
    let (negative, significand, exponent) = round(numerator, denominator, &F64)?;
    let bits = (u64::from(negative) << 63) | (exponent << 52) | (significand & 0xf_ffff_ffff_ffff);
    Ok(f64::from_bits(bits))
}

/// The rational's sign, significand (its hidden bit included) and biased exponent field in
/// `format`, rounded once to nearest, ties to even.
fn round(numerator: &str, denominator: &str, format: &Format) -> GenieResult<(bool, u64, u64)> {
    let (Ok(n), Ok(d)) = (numerator.parse::<BigInt>(), denominator.parse::<BigUint>()) else {
        return Err(invalid_rational(numerator, denominator));
    };
    if d == BigUint::ZERO {
        return Err(invalid_rational(numerator, denominator));
    }
    let negative = n.sign() == Sign::Minus;
    let magnitude = n.magnitude();
    if *magnitude == BigUint::ZERO {
        return Ok((negative, 0, 0));
    }
    // the scale e with 2^(p-1) <= magnitude / (denominator * 2^e) < 2^p, no lower than the least
    let bits = |x: &BigUint| i64::try_from(x.bits()).unwrap_or(i64::MAX);
    let significand_bits = i64::try_from(format.significand).unwrap_or(0);
    let estimate = bits(magnitude) - bits(&d) - significand_bits;
    let mut scale = estimate.max(format.least_scale);
    // the estimate leaves a quotient of p or p + 1 bits (fewer only at the least scale)
    let (mut quotient, mut remainder, mut divisor) = divide(magnitude, &d, scale);
    if quotient.bits() > format.significand {
        scale += 1;
        (quotient, remainder, divisor) = divide(magnitude, &d, scale);
    }
    let twice = remainder << 1usize;
    if twice > divisor || (twice == divisor && quotient.bit(0)) {
        quotient += 1u8;
    }
    if quotient.bits() > format.significand {
        quotient >>= 1usize;
        scale += 1;
    }
    if scale > format.most_scale {
        return Err(
            GenieError::invalid_input("a Nickel number beyond the largest float")
                .with("numerator", numerator)
                .with("denominator", denominator),
        );
    }
    let significand = u64::try_from(&quotient).unwrap_or(0);
    let normal = quotient.bits() == format.significand;
    let exponent = if normal {
        u64::try_from(scale - format.least_scale + 1).unwrap_or(0)
    } else {
        0
    };
    Ok((negative, significand, exponent))
}

/// `magnitude / (denominator * 2^scale)` as quotient, remainder and the divisor they are of.
fn divide(magnitude: &BigUint, denominator: &BigUint, scale: i64) -> (BigUint, BigUint, BigUint) {
    let (dividend, divisor) = if scale >= 0 {
        (
            magnitude.clone(),
            denominator << usize::try_from(scale).unwrap_or(0),
        )
    } else {
        (
            magnitude << usize::try_from(-scale).unwrap_or(0),
            denominator.clone(),
        )
    };
    let quotient = &dividend / &divisor;
    let remainder = dividend - &quotient * &divisor;
    (quotient, remainder, divisor)
}

/// The error of a string pair that is no rational.
fn invalid_rational(numerator: &str, denominator: &str) -> GenieError {
    GenieError::invalid_input("a Nickel number that is no rational")
        .with("numerator", numerator)
        .with("denominator", denominator)
}

#[cfg(test)]
mod tests;
