//! What a building's placement test answers.

/// A building placement test's answer (`FUN_14123aee0`'s code): 0 lets the building go there,
/// any other says why not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlacementCode {
    /// 0: the building may go there.
    Allowed = 0,
    /// 1: the place misses the master's placement terrains.
    PlacementTerrain = 1,
    /// 2: a tile of a terrain the restriction forbids.
    Terrain = 2,
    /// 3: a tile the hill mode refuses.
    Hill = 3,
    /// 5: a human player explored none of the tiles.
    Unexplored = 5,
    /// 6: an object stands in the way.
    Blocked = 6,
    /// 7: the box leaves the map.
    OffMap = 7,
}

impl PlacementCode {
    /// Whether the building may go there (0).
    #[must_use]
    pub const fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed)
    }

    /// The game's code.
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }
}

#[cfg(test)]
mod tests {
    use crate::production::spawn::PlacementCode;

    /// Only 0 allows; each answer keeps the game's code.
    #[test]
    fn only_zero_allows() {
        assert!(PlacementCode::Allowed.is_allowed());
        assert!(!PlacementCode::Blocked.is_allowed());
        assert_eq!(
            [
                PlacementCode::PlacementTerrain,
                PlacementCode::Hill,
                PlacementCode::OffMap
            ]
            .map(PlacementCode::code),
            [1, 3, 7]
        );
    }
}
