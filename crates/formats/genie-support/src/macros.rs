#[macro_export]
/// Create a `From` implementation for an ID container type that returns its contained number as
/// the given target type.
///
/// The standard library's blanket implementation turns it into an infallible `TryFrom`/`TryInto`
/// (with `Infallible` as its error), which is what the macro's name refers to.
///
/// ## Example
///
/// ```rust
/// use genie_support::infallible_try_into;
/// use std::convert::TryInto;
/// struct Container(u16);
/// # fn main() -> Result<(), std::convert::Infallible> {
/// infallible_try_into!(Container, u32);
/// let num: u32 = Container(12).try_into()?;
/// assert_eq!(num, 12u32);
/// # Ok(())
/// # }
/// ```
macro_rules! infallible_try_into {
    ($from:ident, $to:ty) => {
        impl ::std::convert::From<$from> for $to {
            #[inline]
            fn from(n: $from) -> Self {
                n.0.into()
            }
        }
    };
}

#[macro_export]
/// Create a `TryInto` implementation for an ID container type that tries to returns its contained number as the given target type.
///
/// ## Example
///
/// ```rust
/// use genie_support::fallible_try_into;
/// use std::convert::{TryFrom, TryInto};
/// struct Container(u16);
/// # fn main() -> genie_error::GenieResult<()> {
/// fallible_try_into!(Container, i16);
/// let num: i16 = Container(12).try_into()?;
/// assert_eq!(num, 12i16);
/// assert!(i16::try_from(Container(50000u16)).is_err());
/// # Ok(())
/// # }
/// ```
macro_rules! fallible_try_into {
    ($from:ident, $to:ty) => {
        impl ::std::convert::TryFrom<$from> for $to {
            type Error = ::genie_error::GenieError;
            #[inline]
            fn try_from(n: $from) -> ::genie_error::GenieResult<Self> {
                use ::std::convert::TryInto;
                n.0.try_into().map_err(|_| {
                    ::genie_error::GenieError::invalid_input("an id that does not fit the integer")
                        .with("id", n.0)
                })
            }
        }
    };
}

#[macro_export]
/// Create a `TryFrom` implementation for an ID container type that tries to wrap the given number
/// type into the container.
///
/// ## Example
///
/// ```rust
/// use genie_support::fallible_try_from;
/// use std::convert::{TryFrom, TryInto};
/// #[derive(Debug, PartialEq, Eq)]
/// struct Container(u16);
/// # fn main() -> genie_error::GenieResult<()> {
/// fallible_try_from!(Container, i16);
/// assert_eq!(Container::try_from(1i16)?, Container(1));
/// assert!(Container::try_from(-1i16).is_err());
/// # Ok(())
/// # }
/// ```
macro_rules! fallible_try_from {
    ($to:ty, $from:ident) => {
        impl ::std::convert::TryFrom<$from> for $to {
            type Error = ::genie_error::GenieError;
            #[inline]
            fn try_from(n: $from) -> ::genie_error::GenieResult<Self> {
                use ::std::convert::TryInto;
                n.try_into().map(Self).map_err(|_| {
                    ::genie_error::GenieError::invalid_input("an id that does not fit the integer")
                        .with("id", n)
                })
            }
        }
    };
}

#[macro_export]
/// Check if two 32 bit floating point numbers are equal, with some error.
///
/// ```rust
/// use genie_support::f32_eq;
/// let zero = 0.0;
/// assert!(f32_eq!(zero, 0.0));
/// assert!(!f32_eq!(zero, 1.0));
/// ```
macro_rules! f32_eq {
    ($left:expr, $right:expr) => {
        f32::abs($left - $right) < f32::EPSILON
    };
}

#[macro_export]
/// Check if two 32 bit floating point numbers are not equal, with some error.
///
/// ```rust
/// use genie_support::f32_neq;
/// let zero = 0.0;
/// assert!(f32_neq!(zero, 1.0));
/// assert!(!f32_neq!(zero, 0.0));
/// ```
macro_rules! f32_neq {
    ($left:expr, $right:expr) => {
        f32::abs($left - $right) > f32::EPSILON
    };
}
