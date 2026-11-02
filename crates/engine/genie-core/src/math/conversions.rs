//! The x86 conversions the game's code runs where Rust's `as` gives other results:
//! `CVTTSS2SI` into a 32-bit and into a 64-bit register.

/// `CVTTSS2SI`: f32 to i32, truncating toward zero. NaN and values outside the i32 range give
/// `i32::MIN` (the "integer indefinite" value), where Rust's `as` would saturate.
#[must_use]
pub fn cvttss2si(value: f32) -> i32 {
    if value.is_nan() || !(-2_147_483_648.0..2_147_483_648.0).contains(&value) {
        i32::MIN
    } else {
        value as i32
    }
}

/// `CVTTSS2SI` into a 64-bit register: f32 to i64, truncating toward zero. NaN and values
/// outside the i64 range give `i64::MIN`; a caller that keeps the low 32 bits (`EAX`) wraps
/// where [`cvttss2si`] would give `i32::MIN`.
#[must_use]
pub fn cvttss2si_64(value: f32) -> i64 {
    if value.is_nan()
        || !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value)
    {
        i64::MIN
    } else {
        value as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cvttss2si_of_64_bits_keeps_the_low_word_past_the_i32_range() {
        assert_eq!((cvttss2si_64(2.9), cvttss2si_64(-2.9)), (2, -2));
        // 2^31 fits 64 bits; its low word is i32::MIN, 2^32 + 2^31 wraps the same way
        assert_eq!(cvttss2si_64(2_147_483_648.0) as i32, i32::MIN);
        assert_eq!(cvttss2si_64(6_442_450_944.0) as i32, i32::MIN);
        assert_eq!(cvttss2si_64(4_294_967_296.0) as i32, 0);
        for v in [f32::NAN, f32::INFINITY, -f32::INFINITY, 1.0e19] {
            assert_eq!(cvttss2si_64(v), i64::MIN, "{v}");
        }
    }

    #[test]
    fn cvttss2si_truncates_toward_zero() {
        assert_eq!((cvttss2si(2.9), cvttss2si(-2.9)), (2, -2));
    }

    #[test]
    fn cvttss2si_returns_integer_indefinite_for_nan_and_out_of_range() {
        for v in [f32::NAN, 2_147_483_648.0, -2_147_483_904.0, f32::INFINITY] {
            assert_eq!(cvttss2si(v), i32::MIN, "{v}");
        }
    }
}
