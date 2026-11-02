//! The game's files as genie's model (FD-046): the `.dat`'s masters per civ, technologies,
//! effects and terrains ([`dat`]), a save's masters ([`save`]) and the JSON tables beside the
//! `.dat` ([`tables`]). Each importer reads the bytes or text it is given; [`dat::MasterTable::read`]
//! also opens the `.dat` and the tables beside it.

// The port's exceptions to the workspace's lints, genie-oracle's own for the code that left it
// (FD-046-D35): the importers convert the files' values as the game's code does.
#![allow(
    // the game's conversions (movsx, movzx, truncating stores), written as `as`
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    // a routine is ported whole, its documentation opens with the routine and its addresses
    clippy::too_many_lines,
    clippy::too_long_first_doc_paragraph,
    // every error is `GenieError`, which names the part the import does not read
    clippy::missing_errors_doc
)]
#![warn(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::unimplemented,
    clippy::todo
)]

pub mod dat;
pub mod save;
pub mod tables;
