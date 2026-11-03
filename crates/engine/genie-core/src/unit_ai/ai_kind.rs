//! A unit AI's factory type as one type: which `Tribe…UnitAIModule` a master's unit AI is
//! ([`crate::unit_ai::unit_ai_kind`]), the value the module's constructor takes.

use genie_diff::{Diff, Differ};
use std::fmt::{Display, Error, Formatter};

/// A unit AI's factory type (`UnitAi::ai_kind`), 0 to 0x13 in the game's switch
/// (`FUN_140c5c170`'s constructor). The simulation tells some apart (the boar, the soldier, the
/// siege unit, the civilian ...), which this module names; the others are values of the type
/// too, so it is a newtype with named constants, as [`crate::objects::masters::ClassId`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitAiKind(
    /// Its value, as the factory switch holds it.
    u32,
);

impl UnitAiKind {
    /// Its value, for the constructor's switch, a packet and the checkpoint's words.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The type a value names: any value is one (the game's switch takes the others as no module).
    #[must_use]
    pub const fn of_value(value: u32) -> Self {
        Self(value)
    }

    /// The hunted animal's factory type (`TribeHuntedUnitAIModule`: deer, sheep before capture).
    pub const HUNTED: Self = Self(2);
    /// The ranged units' first factory type (6, the base's behaviour: stance 3, no cared list).
    pub const RANGED_6: Self = Self(6);
    /// The ranged units' second factory type (7, as [`UnitAiKind::RANGED_6`]).
    pub const RANGED_7: Self = Self(7);
    /// The predator's factory type (`TribePreditorUnitAIModule`, `FUN_140c11da0`).
    pub const PREDATOR: Self = Self(3);
    /// The boar's factory type (`TribePreditorUnitAIModule`).
    pub const BOAR: Self = Self(4);
    /// The lion's factory type (`TribeLionUnitAIModule`, `FUN_140c11bd0`).
    pub const LION: Self = Self(5);
    /// The civilian's factory type (`TribeCivilianUnitAIModule`, the villagers').
    pub const CIVILIAN: Self = Self(8);
    /// The priest's factory type (`TribePriestUnitAIModule`: classes 18, 43 and 53).
    pub const PRIEST: Self = Self(9);
    /// The trade ship's and the trade cart's factory type (`TribeTradeShipUnitAIModule`).
    pub const TRADE: Self = Self(0xa);
    /// The transport ship's factory type (`TribeTransportShipUnitAIModule`).
    pub const TRANSPORT: Self = Self(11);
    /// The fishing ship's factory type (`TribeFishingShipUnitAIModule`, class 21).
    pub const FISHING: Self = Self(0xc);
    /// The soldier's factory type (`TribeSoldierUnitAIModule`).
    pub const SOLDIER: Self = Self(13);
    /// The artifact's factory type (`TribeArtifactUnitAIModule`, class 1's).
    pub const ARTIFACT: Self = Self(14);
    /// The tower's factory type (`TribeTowerUnitAIModule`, the town centre's too).
    pub const TOWER: Self = Self(15);
    /// The siege unit's factory type (`TribeSiegeUnitAIModule`).
    pub const SIEGE: Self = Self(0x10);
    /// The building unit AI's factory type (`TribeBuildingUnitAIModule`).
    pub const BUILDING: Self = Self(17);
    /// The siege tower's factory type (`TribeSiegeTowerUnitAIModule`, master 1105's).
    pub const SIEGE_TOWER: Self = Self(0x12);
    /// The livestock's factory type (`TribeLivestockUnitAIModule`, `FUN_140c11d70`).
    pub const LIVESTOCK: Self = Self(0x13);
}

impl Display for UnitAiKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl Diff for UnitAiKind {
    fn diff(&self, other: &Self, d: &mut Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::unit_ai::UnitAiKind;

    /// Any value is a type, and the named ones keep the game's values.
    #[test]
    fn kind_is_any_value_and_the_named_ones_keep_the_games() {
        assert_eq!(UnitAiKind::of_value(13), UnitAiKind::SOLDIER);
        assert_eq!(UnitAiKind::BOAR.get(), 4);
        assert_eq!(UnitAiKind::of_value(99).get(), 99);
    }
}
