//! A terrain restriction's values ([`Restriction`]): one a terrain, which terrains an object of
//! the restriction may stand on and the terrain bonus's damage multipliers.

use crate::map::TerrainId;
use serde::Deserialize;
use serde::Serialize;

/// A terrain restriction's values (the `.dat`'s `passable_buildable_dmg_multiplier`, the
/// world's table `+0x18` each), one a terrain by its id. An object of the restriction may stand
/// on a terrain whose value is above 0; a terrain bonus multiplies the damage by it. A terrain
/// past the values has none.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Restriction(Vec<f32>);

impl Restriction {
    /// The restriction of these values, terrain 0's first.
    #[must_use]
    pub const fn new(values: Vec<f32>) -> Self {
        Self(values)
    }

    /// Terrain `t`'s value; none past the values.
    #[must_use]
    pub fn value(&self, t: TerrainId) -> Option<f32> {
        self.0.get(t.index()).copied()
    }

    /// Whether an object of the restriction may stand on terrain `t`: its value is above 0
    /// (`0 < v`, a NaN is not); a terrain past the values is not passable.
    #[must_use]
    pub fn passes(&self, t: TerrainId) -> bool {
        self.value(t).is_some_and(|v| 0.0 < v)
    }

    /// The values, terrain 0's first, as the data and the save hold them.
    #[must_use]
    pub fn values(&self) -> &[f32] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::map::{Restriction, TerrainId};

    /// A value above 0 passes, 0, a negative value and a NaN do not, and a terrain past the
    /// values has no value and does not pass.
    #[test]
    fn terrain_passes_on_value_above_zero() {
        let r = Restriction::new(vec![1.0, 0.0, -1.0, f32::NAN, 0.25]);
        let t = TerrainId::named;
        assert_eq!(
            [0, 1, 2, 3, 4, 5].map(|k| r.passes(t(k))),
            [true, false, false, false, true, false]
        );
        assert_eq!((r.value(t(4)), r.value(t(5))), (Some(0.25), None));
        assert_eq!(r.values().len(), 5);
        assert_eq!(Restriction::default(), Restriction::new(Vec::new()));
    }
}
