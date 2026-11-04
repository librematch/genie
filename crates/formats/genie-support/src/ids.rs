use crate::{fallible_try_from, fallible_try_into, infallible_try_into};
use genie_error::{GenieError, GenieResult};
use std::convert::{TryFrom, TryInto};
use std::fmt;

/// An ID identifying a unit type.
#[derive(Debug, Hash, Default, Clone, Copy, PartialEq, Eq)]
pub struct UnitKindId(u16);

impl From<u16> for UnitKindId {
    #[inline]
    fn from(n: u16) -> Self {
        Self(n)
    }
}

impl From<UnitKindId> for u16 {
    #[inline]
    fn from(n: UnitKindId) -> Self {
        n.0
    }
}

impl From<UnitKindId> for i32 {
    #[inline]
    fn from(n: UnitKindId) -> Self {
        n.0.into()
    }
}

impl From<UnitKindId> for u32 {
    #[inline]
    fn from(n: UnitKindId) -> Self {
        n.0.into()
    }
}

impl From<UnitKindId> for usize {
    #[inline]
    fn from(n: UnitKindId) -> Self {
        n.0.into()
    }
}

fallible_try_into!(UnitKindId, i16);
fallible_try_from!(UnitKindId, i16);
fallible_try_from!(UnitKindId, i32);
fallible_try_from!(UnitKindId, u32);

/// An ID identifying a tech.
#[derive(Debug, Hash, Default, Clone, Copy, PartialEq, Eq)]
pub struct TechId(u16);

impl From<u16> for TechId {
    fn from(n: u16) -> Self {
        Self(n)
    }
}

impl From<TechId> for u16 {
    fn from(n: TechId) -> Self {
        n.0
    }
}

impl From<TechId> for usize {
    fn from(n: TechId) -> Self {
        n.0.into()
    }
}

fallible_try_into!(TechId, i16);
infallible_try_into!(TechId, u32);
fallible_try_from!(TechId, i32);
fallible_try_from!(TechId, u32);

/// An ID identifying a sprite.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SpriteId(u16);
impl From<u16> for SpriteId {
    fn from(n: u16) -> Self {
        Self(n)
    }
}

impl From<SpriteId> for u16 {
    fn from(n: SpriteId) -> Self {
        n.0
    }
}

impl From<SpriteId> for i32 {
    fn from(n: SpriteId) -> Self {
        n.0.into()
    }
}

impl From<SpriteId> for u32 {
    fn from(n: SpriteId) -> Self {
        n.0.into()
    }
}

impl From<SpriteId> for usize {
    fn from(n: SpriteId) -> Self {
        n.0.into()
    }
}

fallible_try_into!(SpriteId, i16);
fallible_try_from!(SpriteId, i16);
fallible_try_from!(SpriteId, i32);
fallible_try_from!(SpriteId, u32);

/// A key in a language file.
///
/// A key may be either a nonnegative integer or an arbitrary string.
///
/// The original game supports only nonnegative integers.
/// The HD Edition allows for integers as well as Strings to serve as keys in a
/// key value file.
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub enum StringKey {
    /// An integer string key.
    Num(u32),

    /// A named string key.
    /// The string must not represent a `u32` value (such keys must be `Num`).
    Name(String),
}

impl Default for StringKey {
    #[inline]
    fn default() -> Self {
        Self::Num(0)
    }
}

impl StringKey {
    /// Returns `true` if and only if this `StringKey` is a number.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_support::StringKey;
    /// assert!(StringKey::from(0u32).is_numeric());
    /// assert!(!StringKey::from("").is_numeric());
    /// ```
    #[inline]
    #[must_use]
    pub const fn is_numeric(&self) -> bool {
        match self {
            Self::Num(_) => true,
            Self::Name(_) => false,
        }
    }

    /// Returns `true` if and only if this `StringKey` is a string name.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_support::StringKey;
    /// assert!(!StringKey::from(0u32).is_named());
    /// assert!(StringKey::from("").is_named());
    /// ```
    #[inline]
    #[must_use]
    pub const fn is_named(&self) -> bool {
        match self {
            Self::Num(_) => false,
            Self::Name(_) => true,
        }
    }
}

impl fmt::Display for StringKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Num(n) => write!(f, "{n}"),
            Self::Name(s) => write!(f, "{s}"),
        }
    }
}

impl From<u32> for StringKey {
    #[inline]
    fn from(n: u32) -> Self {
        Self::Num(n)
    }
}

impl From<u16> for StringKey {
    #[inline]
    fn from(n: u16) -> Self {
        Self::Num(n.into())
    }
}

impl TryFrom<i32> for StringKey {
    type Error = GenieError;
    #[inline]
    fn try_from(n: i32) -> GenieResult<Self> {
        u32::try_from(n)
            .map(Self::Num)
            .map_err(|_| GenieError::invalid_input("a negative string key").with("key", n))
    }
}

impl TryFrom<i16> for StringKey {
    type Error = GenieError;
    #[inline]
    fn try_from(n: i16) -> GenieResult<Self> {
        u32::try_from(n)
            .map(Self::Num)
            .map_err(|_| GenieError::invalid_input("a negative string key").with("key", n))
    }
}

impl From<&str> for StringKey {
    #[inline]
    fn from(s: &str) -> Self {
        s.parse()
            .map_or_else(|_| Self::Name(String::from(s)), Self::Num)
    }
}

impl From<String> for StringKey {
    #[inline]
    fn from(s: String) -> Self {
        Self::from(s.as_ref())
    }
}

/// A string key that is named, or whose number does not fit the integer.
#[track_caller]
fn not_fitting(key: &StringKey) -> GenieError {
    GenieError::invalid_input("a string key that does not fit the integer").with("key", key)
}

// Implement TryFrom<&StringKey> conversions for a bunch of stuff
macro_rules! try_from_string_key {
    ($type:ty) => {
        impl TryFrom<&StringKey> for $type {
            type Error = GenieError;
            #[inline]
            fn try_from(key: &StringKey) -> GenieResult<Self> {
                match key {
                    StringKey::Num(n) => (*n).try_into().map_err(|_| not_fitting(key)),
                    _ => Err(not_fitting(key)),
                }
            }
        }
    };
}

try_from_string_key!(u32);
try_from_string_key!(i32);
try_from_string_key!(u16);
try_from_string_key!(i16);

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests converting from an int to a string key.
    #[test]
    fn string_key_from_int() {
        if let StringKey::Num(n) = StringKey::from(0u32) {
            assert_eq!(0, n);
        } else {
            panic!();
        }
    }

    /// Tests converting from a string representing an int to a string key.
    #[test]
    fn string_key_from_str_to_int() {
        let s = "57329";
        if let StringKey::Num(n) = StringKey::from(s) {
            assert_eq!(57329, n);
        } else {
            panic!();
        }
    }

    /// Tests converting from a string not representing an int to a string key.
    #[test]
    fn string_key_from_str_to_str() {
        let s = "grassDaut";
        if let StringKey::Name(n) = StringKey::from(s) {
            assert_eq!(s, n);
        } else {
            panic!();
        }
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;

    /// A named key and a number past the integer's range do not convert.
    #[test]
    fn named_or_too_large_key_does_not_fit() {
        for key in [StringKey::from("grassDaut"), StringKey::from(70_000u32)] {
            let e = u16::try_from(&key).unwrap_err();
            assert_eq!(
                (e.kind(), e.reason()),
                (
                    genie_error::ErrorKind::InvalidInput,
                    "a string key that does not fit the integer"
                )
            );
        }
        assert_eq!(u16::try_from(&StringKey::from(7u32)), Ok(7));
    }
}
