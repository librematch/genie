//! Internal utilities for the genie format crates.

#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(rust_2018_idioms)]
#![deny(unsafe_code)]
#![warn(unused)]
#![warn(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::unimplemented,
    clippy::todo
)]

mod bounded;
mod ids;
mod macros;
mod map_into;
mod read;
#[cfg(feature = "strings")]
mod strings;

pub use bounded::*;
pub use ids::*;
pub use map_into::*;
pub use read::*;
#[cfg(feature = "strings")]
pub use strings::*;
