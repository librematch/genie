//! A unit AI's factory type as one type: which `Tribe…UnitAIModule` a master's unit AI is
//! ([`crate::unit_ai::unit_ai_kind`]), the value the module's constructor takes.

use crate::GenieError;
use genie_diff::{Diff, Differ};
use std::fmt::{Display, Error, Formatter};

/// A unit AI's factory type (`UnitAi::ai_kind`): the case of `FUN_141215560`'s switch that
/// constructs its module, one RTTI class each. The set is closed: the switch constructs none for
/// another value, and the factory (`FUN_141215a90`) constructs no other. The load
/// (`FUN_14121ec90`) reads the save's type: 0 holds no unit AI, 1 has the factory pick the type
/// by the master's class (not modelled: [`UnitAiKind::try_from`] refuses it), 2 or more is the
/// switch's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum UnitAiKind {
    /// 2: the hunted animal (`TribeHuntedAnimalUnitAIModule`, `FUN_140c11920`: deer, sheep
    /// before capture).
    Hunted = 2,
    /// 3: the predator (`TribePreditorUnitAIModule`, `FUN_140c11da0`).
    Predator = 3,
    /// 4: the boar (`TribeElephantUnitAIModule`, `FUN_140c11720`).
    Boar = 4,
    /// 5: the lion (`TribeLionUnitAIModule`, `FUN_140c11bd0`).
    Lion = 5,
    /// 6: the ranged unit (`TribeRangedUnitAIModule`, `FUN_140c11f80`: the base's behaviour,
    /// stance 3, no cared list).
    Ranged = 6,
    /// 7: the second ranged unit (`TribeRangedUnitAIModule2`, `FUN_140c11f50`, as
    /// [`UnitAiKind::Ranged`]).
    Ranged2 = 7,
    /// 8: the civilian (`TribeCivilianUnitAIModule`, `FUN_140c115d0`: the villagers).
    Civilian = 8,
    /// 9: the priest (`TribePriestUnitAIModule`, `FUN_140c11e30`: classes 18, 43 and 53).
    Priest = 9,
    /// 0xa: the trade ship and the trade cart (`TribeTradeShipUnitAIModule`, `FUN_140c12ab0`).
    Trade = 0xa,
    /// 0xb: the transport ship (`TribeTransportShipUnitAIModule`, `FUN_140c12c30`).
    Transport = 0xb,
    /// 0xc: the fishing ship (`TribeFishingShipUnitAIModule`, `FUN_140c11860`, class 21).
    Fishing = 0xc,
    /// 0xd: the soldier (`TribeSoldierUnitAIModule`, `FUN_140c12300`), the factory's type for
    /// any class it names no other for.
    #[default]
    Soldier = 0xd,
    /// 0xe: the artifact (`TribeArtifactUnitAIModule`, `FUN_140c11340`, class 1's).
    Artifact = 0xe,
    /// 0xf: the tower (`TribeTowerUnitAIModule`, `FUN_140c12960`, the town centre's too).
    Tower = 0xf,
    /// 0x10: the siege unit (`TribeSiegeUnitAIModule`, `FUN_140c12040`).
    Siege = 0x10,
    /// 0x11: the building (`TribeBuildingUnitAIModule`, `FUN_140c11470`).
    Building = 0x11,
    /// 0x12: the siege tower (`TribeSiegeTowerUnitAIModule`, `FUN_140c11fb0`, master 1105's).
    SiegeTower = 0x12,
    /// 0x13: the livestock (`TribeLivestockUnitAIModule`, `FUN_140c11d70`).
    Livestock = 0x13,
}

impl UnitAiKind {
    /// Its value, for the constructor's switch, a packet and the checkpoint's words.
    #[must_use]
    pub const fn get(self) -> u32 {
        self as u32
    }
}

impl TryFrom<u32> for UnitAiKind {
    type Error = GenieError;

    /// The type of a saved unit AI's `need_unit_ai`.
    ///
    /// # Errors
    ///
    /// A value the switch constructs no module for.
    fn try_from(value: u32) -> Result<Self, GenieError> {
        Ok(match value {
            2 => Self::Hunted,
            3 => Self::Predator,
            4 => Self::Boar,
            5 => Self::Lion,
            6 => Self::Ranged,
            7 => Self::Ranged2,
            8 => Self::Civilian,
            9 => Self::Priest,
            0xa => Self::Trade,
            0xb => Self::Transport,
            0xc => Self::Fishing,
            0xd => Self::Soldier,
            0xe => Self::Artifact,
            0xf => Self::Tower,
            0x10 => Self::Siege,
            0x11 => Self::Building,
            0x12 => Self::SiegeTower,
            0x13 => Self::Livestock,
            _ => {
                return Err(
                    GenieError::invalid_input("a unit-AI type no constructor makes")
                        .with("ai_kind", value),
                );
            }
        })
    }
}

impl Display for UnitAiKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.get().fmt(f)
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

    /// Each of the switch's 2 to 0x13 is its variant and back; the switch's others are refused.
    #[test]
    fn every_constructed_kind_maps_to_its_variant_and_back() {
        for value in 2..=0x13 {
            assert_eq!(UnitAiKind::try_from(value).map(UnitAiKind::get), Ok(value));
        }
        assert_eq!(UnitAiKind::try_from(13), Ok(UnitAiKind::Soldier));
        for value in [0, 1, 0x14, 99] {
            let e = UnitAiKind::try_from(value).unwrap_err();
            assert!(e.to_string().contains("no constructor makes"), "{e}");
        }
    }
}
