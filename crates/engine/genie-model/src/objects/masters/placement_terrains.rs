//! A master's placement terrains ([`PlacementTerrains`]).

use crate::map::TerrainId;
use genie_diff::Diff;

/// A master's placement terrains (`+0x5f2`..`+0x5f8`, the `.dat`'s `placement_side_terrain` and
/// `placement_terrain`, each none for −1): the terrains it must stand beside and the terrains it
/// must stand on (`genie_core::production::spawn::placement_terrains`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct PlacementTerrains {
    /// The side terrains (`+0x5f2`, `+0x5f4`).
    pub beside: [Option<TerrainId>; 2],
    /// The terrains (`+0x5f6`, `+0x5f8`).
    pub on: [Option<TerrainId>; 2],
}

impl PlacementTerrains {
    /// No placement terrain.
    pub const NONE: Self = Self {
        beside: [None; 2],
        on: [None; 2],
    };

    /// The terrains of the save's or the `.dat`'s four shorts in their order: the two side
    /// terrains, then the two terrains ([`TerrainId::of_data`]).
    #[must_use]
    pub const fn of_data([side1, side2, t1, t2]: [i16; 4]) -> Self {
        Self {
            beside: [TerrainId::of_data(side1), TerrainId::of_data(side2)],
            on: [TerrainId::of_data(t1), TerrainId::of_data(t2)],
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::map::TerrainId;
    use crate::objects::masters::PlacementTerrains;

    /// The side terrains come first, then the terrains; −1 is none.
    #[test]
    fn placement_terrains_take_the_sides_first() {
        let p = PlacementTerrains::of_data([4, -1, 2, 23]);
        assert_eq!(p.beside, [Some(TerrainId::named(4)), None]);
        assert_eq!(
            p.on,
            [Some(TerrainId::named(2)), Some(TerrainId::named(23))]
        );
        assert_eq!(PlacementTerrains::of_data([-1; 4]), PlacementTerrains::NONE);
        assert_eq!(PlacementTerrains::NONE, PlacementTerrains::default());
    }
}
