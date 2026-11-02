//! What an object's life reads of its master (FD-046: the model's part of the lifecycle).

use crate::players::AmountRate;
use genie_diff::Diff;

/// What the dead-object steps read from an object's master that the header save does not hold;
/// they come from the `.dat`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MasterData {
    /// Master `+0x620`: how much of the held attribute a dead object loses per second.
    pub decay: AmountRate,
    /// Master `+0x490`: a dead object turns undead (state 6) instead of leaving.
    pub undead: bool,
}
