//! The facets a master's sprites have (FD-046: the model's part of the facing).

use crate::objects::FacetCount;
use genie_diff::Diff;
use serde::Deserialize;
use serde::Serialize;

/// What the facing routines read of the current sprite: its facets and whether its sequence
/// flags have bit 1 (`+0x1a`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SpriteFacets {
    /// How many facets.
    pub count: FacetCount,
    /// Its sequence flags hold [`crate::objects::SpriteFlags::TURNS`] (`+0x1a`): the sprite
    /// turns.
    pub turns: bool,
}
