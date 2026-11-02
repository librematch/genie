//! A task's terrain (`+0x16`): the terrain a task finder's task is for, or a terrain look task's
//! flag mask.

use crate::map::TerrainId;

/// A task's terrain (`+0x16`), every short of the data kept, −1 for none. A task finder's task
/// is for the terrain its low 16 bits equal ([`TaskTerrain::is`]); a terrain look task reads it as
/// a flag mask (`genie_core::combat::abilities::terrain_mask`), a negative one too.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskTerrain(i16);

impl Default for TaskTerrain {
    /// None (−1), the data's value for a task of no terrain.
    fn default() -> Self {
        Self::NONE
    }
}

impl TaskTerrain {
    /// No terrain (−1).
    pub const NONE: Self = Self(-1);

    /// The terrain of the data's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// Whether the task is for terrain `terrain`: its short as 16 bits against the terrain's byte
    /// (`FUN_1411a8ba0`), so no negative one is.
    #[must_use]
    pub fn is(self, terrain: TerrainId) -> bool {
        self.0 as u16 == u16::from(terrain.get())
    }

    /// Whether it holds −1, a task of no terrain.
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == Self::NONE.0
    }

    /// The short, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl genie_diff::Diff for TaskTerrain {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::map::TerrainId;
    use crate::objects::masters::TaskTerrain;

    /// A task is for the terrain whose byte its 16 bits equal: −1 and 256 + 3 are for none.
    #[test]
    fn task_is_for_the_terrain_its_short_names() {
        assert!(TaskTerrain::of_data(3).is(TerrainId::named(3)));
        assert!(!TaskTerrain::of_data(259).is(TerrainId::named(3)));
        assert!(!TaskTerrain::NONE.is(TerrainId::named(255)));
        assert!(TaskTerrain::default().is_none());
        assert!(!TaskTerrain::of_data(-2).is_none());
        assert_eq!(TaskTerrain::of_data(-32).get(), -32);
    }
}
