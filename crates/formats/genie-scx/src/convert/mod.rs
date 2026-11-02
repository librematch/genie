//! Automated scenario conversions.
//!
//! This module implements conversions between different scenario formats and game versions.
mod aoc_to_wk;
mod hd_to_wk;

use crate::{Scenario, ScenarioObject};

pub use aoc_to_wk::AocToWk;
pub use hd_to_wk::HdToWk;

/// Error indicating scenario conversion failure.
#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum ConvertError {
    /// The input scenario version is not supported by the converter.
    #[error("invalid version")]
    InvalidVersion,
}

/// Convert an `AoC` or HD Edition scenario file to a `WololoKingdoms` one.
///
/// It will auto-detect the version of the file, and output a WK compatible scenario.
/// `AoC` scenarios will have their unit and terrain IDs switched around so they have the correct
/// look in WK.
/// HD Edition scenarios will have all the new unit and terrain IDs mapped to WK IDs.
///
/// ## Usage
///
/// ```rust,ignore
/// use genie_scx::convert::AutoToWk;
/// AutoToWk::default().convert(&mut scenario)?
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct AutoToWk {}

/// Check if a scenario object is likely a `WololoKingdoms` one.
const fn is_wk_object(object: &ScenarioObject) -> bool {
    // Stormy Dog is the highest ID in AoC 1.0c.
    const STORMY_DOG: i32 = 862;
    object.id > STORMY_DOG
}

impl AutoToWk {
    /// Convert a scenario to `WololoKingdoms` in place, choosing the converter by its version.
    ///
    /// # Errors
    ///
    /// Returns [`ConvertError::InvalidVersion`] if the scenario is neither an HD Edition nor an
    /// `AoK`/`AoC` scenario.
    pub fn convert(&self, scen: &mut Scenario) -> Result<(), ConvertError> {
        if scen.version().is_hd_edition() {
            HdToWk::default().convert(scen)
        } else if scen.version().is_aok() || scen.version().is_aoc() {
            if scen.objects().any(is_wk_object) {
                // No conversion necessary.
                Ok(())
            } else {
                AocToWk::default().convert(scen)
            }
        } else {
            Err(ConvertError::InvalidVersion)
        }
    }
}

impl From<ConvertError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: ConvertError) -> Self {
        let to = match &e {
            ConvertError::InvalidVersion => {
                Self::unsupported("a scenario version the converter does not take")
            }
        };
        to.with_source(e)
    }
}

#[cfg(test)]
mod genie_error_tests {
    use super::*;
    #[cfg(test)]
    use std::error::Error;

    /// The errors convert into `GenieError`s with their kinds and reasons, the original as
    /// source.
    #[test]
    fn convert_error_converts_into_genie_error() {
        let e: genie_error::GenieError = ConvertError::InvalidVersion.into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::Unsupported,
                "a scenario version the converter does not take"
            )
        );
        assert!(Error::source(&e).is_some());
    }
}
