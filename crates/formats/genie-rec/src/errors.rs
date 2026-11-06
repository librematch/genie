//! The errors of the older formats' reader, each a [`GenieError`] with its kind and reason.

use genie_error::GenieError;

/// A string that is not valid UTF-8.
#[must_use]
#[track_caller]
pub(crate) const fn not_utf8() -> GenieError {
    GenieError::invalid_input("a string that is not valid UTF-8")
}

/// A part of the header the crate does not read yet.
#[must_use]
#[track_caller]
pub(crate) fn not_read(part: &'static str) -> GenieError {
    GenieError::missing_data("a part of the header this reader does not read").with("part", part)
}

/// A part of the header the crate does not write yet.
#[must_use]
#[track_caller]
pub(crate) const fn not_written() -> GenieError {
    GenieError::missing_data("a header element without a writer")
}

/// A sync message `ticks` time steps before the one the body's meta action announced.
#[must_use]
#[track_caller]
pub(crate) fn early_sync(ticks: u32) -> GenieError {
    GenieError::invalid_input("a sync message earlier than the log header said")
        .with("ticks", ticks)
}

/// A time message where the body's meta action announced a sync.
#[must_use]
#[track_caller]
pub(crate) const fn missing_sync() -> GenieError {
    GenieError::invalid_input("a time message where a sync was due")
}

/// A body action of a type the reader does not know.
#[must_use]
#[track_caller]
pub(crate) fn unknown_action(kind: i32) -> GenieError {
    GenieError::invalid_input("an action type this reader does not know")
        .with("action", format_args!("{kind:#x}"))
}

/// A unit action of a type the reader does not know.
#[must_use]
#[track_caller]
pub(crate) fn unknown_unit_action(kind: u16) -> GenieError {
    GenieError::invalid_input("a unit action type this reader does not know").with("action", kind)
}
