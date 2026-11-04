//! The errors of binding or reaching a hotkey or a group that does not exist.

use genie_error::GenieError;

/// A group index past the file's groups: `index` of `num_groups`.
#[track_caller]
pub(crate) fn no_group(index: usize, num_groups: usize) -> GenieError {
    GenieError::invalid_input("a hotkey group index past the groups")
        .with("group", index)
        .with("groups", num_groups)
}

/// A hotkey index past its group's hotkeys: `index` of `num_hotkeys`.
#[track_caller]
pub(crate) fn no_hotkey(index: usize, num_hotkeys: usize) -> GenieError {
    GenieError::invalid_input("a hotkey index past the group's hotkeys")
        .with("hotkey", index)
        .with("hotkeys", num_hotkeys)
}
