//! A master's id fields as its data holds them: shorts the effects set, add to and scale, every
//! value kept, which name a master ([`MasterField`]) or a terrain restriction
//! ([`RestrictionField`]) when 0 or more.

use crate::map::RestrictionId;
use crate::objects::masters::MasterId;

/// A master id as a master's data holds it (`missile_unit_id`, a trail's unit): a short the
/// effects set, add to and scale (`PROJECTILE_UNIT`), so every value is kept, −1 and any other
/// negative one alike; it names a master when 0 or more ([`MasterField::master`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MasterField(i16);

impl Default for MasterField {
    /// None (−1), the data's value for no master.
    fn default() -> Self {
        Self::NONE
    }
}

impl MasterField {
    /// No master (−1).
    pub const NONE: Self = Self(-1);

    /// The field of the data's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The field naming `master`, −1 for none.
    #[must_use]
    pub const fn of(master: Option<MasterId>) -> Self {
        Self(MasterId::field(master))
    }

    /// The master it names: none below 0, as the game's lookup of a negative id finds none.
    #[must_use]
    pub fn master(self) -> Option<MasterId> {
        MasterId::of_data(self.0)
    }

    /// The short plus `amount` (an effect's add, wrapping as the game's 16-bit add).
    #[must_use]
    pub const fn plus(self, amount: i16) -> Self {
        Self(self.0.wrapping_add(amount))
    }

    /// The short, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl genie_diff::Diff for MasterField {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// A terrain restriction as a master's data holds it (`defense_terrain_bonus`, `+0x820`): a
/// short the effects set, add to and scale (`TERRAIN_DEFENSE_BONUS`), every value kept; it names
/// a restriction when 0 or more ([`RestrictionField::restriction`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RestrictionField(i16);

impl RestrictionField {
    /// No restriction (−1).
    pub const NONE: Self = Self(-1);

    /// The field of the data's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The restriction it names: none below 0.
    #[must_use]
    pub fn restriction(self) -> Option<RestrictionId> {
        RestrictionId::of_data(self.0)
    }

    /// The short plus `amount` (an effect's add, wrapping as the game's 16-bit add).
    #[must_use]
    pub const fn plus(self, amount: i16) -> Self {
        Self(self.0.wrapping_add(amount))
    }

    /// The short, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl genie_diff::Diff for RestrictionField {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::map::RestrictionId;
    use crate::objects::masters::{MasterField, MasterId, RestrictionField};

    /// A field keeps every short: −3 plus 5 is 2, where an `Option` would have started from −1;
    /// it names an id only when 0 or more.
    #[test]
    fn a_field_keeps_its_negatives_through_an_add() {
        let m = MasterField::of_data(-3);
        assert_eq!(m.master(), None);
        assert_eq!(m.plus(5).master(), MasterId::of_data(2));
        assert_eq!(m.plus(5).get(), 2);
        assert_eq!(MasterField::default(), MasterField::NONE);
        assert_eq!(MasterField::of(MasterId::of_data(7)).get(), 7);
        assert_eq!(MasterField::of(None), MasterField::NONE);
        assert_eq!(MasterField::of_data(i16::MAX).plus(1).get(), i16::MIN);
        let r = RestrictionField::of_data(-2);
        assert_eq!(
            (r.restriction(), r.plus(6).restriction()),
            (None, RestrictionId::of_data(4))
        );
        assert_eq!(RestrictionField::default().get(), 0);
        assert_eq!(RestrictionField::NONE.plus(1).get(), 0);
    }
}
