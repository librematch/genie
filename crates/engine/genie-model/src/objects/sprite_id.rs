//! A sprite's id ([`SpriteId`]): the `.dat`'s graphic a master or a task shows.

use genie_diff::Diff;
use std::fmt::{Display, Error, Formatter};

/// A sprite (a `.dat` graphic) by its id, 0 and up: what a master's and a task's sprite
/// fields name and the world's sprite table holds ([`Model::graphics`](crate::model::Model::graphics)). The
/// game's −1 (or any id below 0) is none, which an `Option` holds ([`SpriteId::of_data`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Diff)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SpriteId(i16);

impl SpriteId {
    /// The sprite of id `id`, for a known id (`id` 0 and up).
    #[must_use]
    pub const fn named(id: i16) -> Self {
        Self(id)
    }

    /// The sprite a saved or `.dat` id names: None for −1 or any id below 0, as the master's
    /// sprite pointer has none.
    #[must_use]
    pub const fn of_data(id: i16) -> Option<Self> {
        if 0 <= id { Some(Self(id)) } else { None }
    }

    /// The id, as the save and the game's tables hold it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl Display for SpriteId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::SpriteId;

    /// An id 0 and up names a sprite; −1 and below are none.
    #[test]
    fn sprite_id_below_zero_is_none() {
        assert_eq!(SpriteId::of_data(0), Some(SpriteId::named(0)));
        assert_eq!(SpriteId::of_data(1903).map(SpriteId::get), Some(1903));
        assert_eq!(SpriteId::of_data(-1), None);
        assert_eq!(SpriteId::of_data(i16::MIN), None);
        assert_eq!(SpriteId::named(7).to_string(), "7");
    }
}
