//! The command-line arguments of the examples: a number, and the usage line when the arguments
//! do not match.

use genie_core::{GenieError, GenieResult};
use std::num::ParseIntError;
use std::str::FromStr;

/// The number `text` holds.
///
/// # Errors
///
/// When `text` is not a number of type `T`: the error names the argument.
pub fn number<T: FromStr<Err = ParseIntError>>(text: &str) -> GenieResult<T> {
    text.parse().map_err(|source| {
        GenieError::invalid_input("an argument is not a number")
            .with("argument", text)
            .with_source(source)
    })
}

/// The error of arguments that do not match the example's usage line.
#[track_caller]
pub const fn usage(line: &'static str) -> GenieError {
    GenieError::invalid_input(line)
}
