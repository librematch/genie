//! A technology's definition: its costs, its effect's id and the techs it links (FD-046).

use crate::objects::masters::CostAmount;
use crate::objects::masters::MasterId;
use crate::players::CivId;
use crate::players::attribute::AttributeId;
use crate::technology::CostPaid;
use crate::technology::EffectId;
use crate::technology::TechId;
use crate::technology::TechKind;
use crate::technology::TechMode;
use crate::world::WholeSeconds;

/// How a group of the game's `linkedTechs.json` links its technologies: the `+4` byte of the
/// record the game settings' table (`FUN_140f39f40`) holds for each of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Linked {
    /// `MutuallyExclusive` (0).
    MutuallyExclusive,
    /// `Toggleable` (1): `FUN_140f39e80` asks the tech tree (`FUN_1413cc3d0`).
    Toggleable,
    /// `Cycle` (2): `FUN_140f39e80` holds it researchable in any state.
    Cycle,
}

impl Linked {
    /// The kind of the file's `Type`; None for one the game does not know.
    #[must_use]
    pub fn of(kind: &str) -> Option<Self> {
        match kind {
            "MutuallyExclusive" => Some(Self::MutuallyExclusive),
            "Toggleable" => Some(Self::Toggleable),
            "Cycle" => Some(Self::Cycle),
            _ => None,
        }
    }
}

/// One cost entry of a technology in the `.dat`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Cost {
    /// The attribute it takes, none for −1.
    pub attribute: Option<AttributeId>,
    /// How much.
    pub amount: CostAmount,
    /// 0 not paid, 1 paid from the player's attribute, 2 from the building (`FUN_141204310`).
    pub paid: CostPaid,
}

/// A technology of the `.dat` as the tech tree holds it (its 0x88-byte record).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TechDef {
    /// `+0x2a`, `+0x30`, `+0x36`: the cost entries' attributes, amounts and paid flags.
    pub costs: [Cost; 3],
    /// `+0x3c`, the technology's type.
    pub kind: TechKind,
    /// Its effect (`+0x3a`): an id of the world's effect table (`+0x128`,
    /// [`crate::technology::Effects`]), −1 for none.
    pub effect_id: EffectId,
    /// `+0x18`, `+0x24`: the technologies it requires and how many of them.
    pub required: [Option<TechId>; 6],
    /// How many of them it requires.
    pub required_count: i16,
    /// `+0x26`: the civ it belongs to (below 1 every civ's); `+0x28`: the game modes it is for
    /// (0 every one, `FUN_1413cf2f0`).
    pub civ: Option<CivId>,
    /// `+0x28`: the game modes it is for (0 and 1 every one, `FUN_1413cf2f0`).
    pub mode: TechMode,
    /// `+0x70`: where it is researched, the building's master and the seconds it takes.
    pub locations: Vec<(Option<MasterId>, WholeSeconds)>,
}
