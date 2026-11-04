//! The errors of the scenario reader and writer, each a [`GenieError`] with its kind and reason.

use crate::ScxVersion;
use genie_error::GenieError;

/// A scenario format version this reader does not take.
#[must_use]
#[track_caller]
pub(crate) fn unknown_version(version: ScxVersion) -> GenieError {
    GenieError::unsupported("a scenario format version this reader does not take")
        .with("version", version)
}

/// A description longer than its length prefix holds.
#[must_use]
#[track_caller]
pub(crate) const fn too_long_description() -> GenieError {
    GenieError::invalid_input("a description too long for its length prefix")
}

/// More disabled buildings than the target version takes.
#[must_use]
#[track_caller]
pub(crate) fn too_many_disabled_buildings(count: i32, most: i32) -> GenieError {
    GenieError::invalid_input("more disabled buildings than the version takes")
        .with("count", count)
        .with("most", most)
}

/// More disabled technologies than the target version takes.
#[must_use]
#[track_caller]
pub(crate) fn too_many_disabled_techs(count: i32) -> GenieError {
    GenieError::invalid_input("more disabled technologies than the version takes")
        .with("count", count)
}

/// Disabled technologies, which versions before 1.04 cannot hold.
#[must_use]
#[track_caller]
pub(crate) const fn techs_not_disabled() -> GenieError {
    GenieError::invalid_input("disabled technologies in a version without them")
}

/// Disabled units, which versions before 1.18 cannot hold.
#[must_use]
#[track_caller]
pub(crate) const fn units_not_disabled() -> GenieError {
    GenieError::invalid_input("disabled units in a version without them")
}

/// Disabled buildings, which versions before 1.18 cannot hold.
#[must_use]
#[track_caller]
pub(crate) const fn buildings_not_disabled() -> GenieError {
    GenieError::invalid_input("disabled buildings in a version without them")
}

#[cfg(test)]
mod tests {
    use super::*;
    use genie_error::ErrorKind::{InvalidInput, Unsupported};

    /// Each error's kind and reason.
    #[test]
    fn each_scenario_error_has_its_kind_and_reason() {
        let cases = [
            (
                unknown_version(ScxVersion(*b"9.99")),
                Unsupported,
                "a scenario format version this reader does not take",
            ),
            (
                too_many_disabled_techs(21),
                InvalidInput,
                "more disabled technologies than the version takes",
            ),
            (
                too_many_disabled_buildings(31, 30),
                InvalidInput,
                "more disabled buildings than the version takes",
            ),
            (
                techs_not_disabled(),
                InvalidInput,
                "disabled technologies in a version without them",
            ),
            (
                units_not_disabled(),
                InvalidInput,
                "disabled units in a version without them",
            ),
            (
                buildings_not_disabled(),
                InvalidInput,
                "disabled buildings in a version without them",
            ),
        ];
        for (e, kind, reason) in cases {
            assert_eq!((e.kind(), e.reason()), (kind, reason));
        }
    }
}
