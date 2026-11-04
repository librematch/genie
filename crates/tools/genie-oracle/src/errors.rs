//! The oracle's own errors, each a [`GenieError`] with its kind and reason: a recording, a save
//! or a dump that the comparison cannot use.

use genie_core::GenieError;

/// A recording without a checkpoint: it ends within the first interval.
#[must_use]
#[track_caller]
pub const fn no_checkpoint() -> GenieError {
    GenieError::missing_data("a recording without a checkpoint")
}

/// A checkpoint lane that names a player the header save does not have.
#[must_use]
#[track_caller]
pub fn no_such_player(id: u32) -> GenieError {
    GenieError::invalid_input("a checkpoint lane naming a player the save does not have")
        .with("player", id)
}

/// A command before the body's first sync, so no tick runs it.
#[must_use]
#[track_caller]
pub const fn command_before_first_sync() -> GenieError {
    GenieError::invalid_input("a command before the body's first sync")
}

/// A mesh dump without its header line.
#[must_use]
#[track_caller]
pub const fn bad_mesh_dump() -> GenieError {
    GenieError::invalid_input("a mesh dump without its mesh line")
}

/// A registry dump without its header line, or a line out of shape.
#[must_use]
#[track_caller]
pub const fn bad_registry_dump() -> GenieError {
    GenieError::invalid_input("a registry dump out of shape")
}

/// A tile list dump's line out of shape.
#[must_use]
#[track_caller]
pub const fn bad_tile_dump() -> GenieError {
    GenieError::invalid_input("a tile list dump out of shape")
}

/// A recycled pools dump's line out of shape.
#[must_use]
#[track_caller]
pub const fn bad_pool_dump() -> GenieError {
    GenieError::invalid_input("a recycled pools dump out of shape")
}

/// A recording without a chapter.
#[must_use]
#[track_caller]
pub const fn no_chapter() -> GenieError {
    GenieError::invalid_input("a recording without a chapter")
}

/// A player the save does not have.
#[must_use]
#[track_caller]
pub fn no_player(player: usize) -> GenieError {
    GenieError::missing_data("a player the save does not have").with("player", player)
}
