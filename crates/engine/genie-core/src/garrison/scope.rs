//! Which shelters a garrison takes ([`GarrisonScope`]) and how far a unit goes to one
//! ([`ShelterReach`]).

/// `UNIT_ORDER`'s `+0x1e`: which shelters a garrison order takes ([`crate::world::World::garrison_order`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GarrisonScope {
    /// 0: the target alone (an annex its main building, but a gate's).
    #[default]
    Target,
    /// Any other byte: each object of the target's line near it ([`crate::garrison::NEARBY`]).
    Nearby,
}

impl GarrisonScope {
    /// The packet's byte: 0 is the target alone, any other value the nearby ones (the
    /// decoder's test).
    #[must_use]
    pub const fn of_byte(byte: u8) -> Self {
        if byte == 0 {
            Self::Target
        } else {
            Self::Nearby
        }
    }

    /// Its byte in a packet: 0 or 1.
    #[must_use]
    pub const fn byte(self) -> u8 {
        match self {
            Self::Target => 0,
            Self::Nearby => 1,
        }
    }
}

/// How far a unit goes to a shelter ([`crate::world::World::shelter`]'s `param_6`): the town
/// bell's reach, or the garrison's far one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShelterReach {
    /// The town bell's ([`crate::garrison::TOWN_BELL_REACH`]).
    TownBell,
    /// A garrison order's and a seek shelter's ([`crate::garrison::FAR`]).
    Far,
}

#[cfg(test)]
mod tests {
    use crate::garrison::GarrisonScope;

    /// Any byte but 0 is the nearby ones, which writes back as 1.
    #[test]
    fn any_byte_but_0_is_the_nearby_ones() {
        assert_eq!(GarrisonScope::of_byte(0), GarrisonScope::Target);
        assert_eq!(GarrisonScope::of_byte(3), GarrisonScope::Nearby);
        assert_eq!(GarrisonScope::of_byte(3).byte(), 1);
        assert_eq!(GarrisonScope::Target.byte(), 0);
    }
}
