//! A master's charge event (`+0x920`): the range a ranged charge adds.

use crate::map::Distance;

/// A master's charge event (`mChargeEvent`, `+0x920`): the tiles a ready charge of type 6 or 7
/// ([`crate::objects::masters::ChargeType::ranged`]) adds to the range (slot 0x2f8). Every short
/// the data or an effect leaves is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChargeEvent(i16);

impl ChargeEvent {
    /// The event of the data's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short plus `amount` (an effect's add, wrapping as the game's 16-bit add).
    #[must_use]
    pub const fn plus(self, amount: i16) -> Self {
        Self(self.0.wrapping_add(amount))
    }

    /// The range it adds: the short as a float.
    #[must_use]
    pub fn range(self) -> Distance {
        Distance::new(f32::from(self.0))
    }

    /// The short, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl genie_diff::Diff for ChargeEvent {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::map::Distance;
    use crate::objects::masters::ChargeEvent;

    /// The short is the range in tiles, and an add wraps.
    #[test]
    fn a_charge_event_is_a_range() {
        assert_eq!(ChargeEvent::of_data(2).range(), Distance::new(2.0));
        assert_eq!(ChargeEvent::of_data(i16::MAX).plus(1).get(), i16::MIN);
        assert_eq!(ChargeEvent::default().range(), Distance::ZERO);
    }
}
