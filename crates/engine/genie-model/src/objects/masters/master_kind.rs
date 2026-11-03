//! A master's type as one type: the byte at `+0x8` that tells a static object from a combat unit
//! or a building, which the `.dat` names `unit_type`.

use genie_diff::{Diff, Differ};
use std::fmt::{Display, Error, Formatter};

/// A master's type (`+0x8`, the `.dat`'s `unit_type`): 10 to 80 in steps the game names (its
/// constants, the one definition of the type bytes: every routine that tests a type names it
/// here, and the attribute setters, slots 0x18, 0x20 and 0x28, differ by type). The routines that
/// test a type compare it with a threshold (`kind >= MasterKind::COMBAT`), so it is ordered. The
/// `.dat` holds values the code does not name (0 for an empty slot, 90 and above for the data's
/// own kinds), so the type is a newtype with named constants, not an enum: any byte the data holds
/// is a type, and the named ones are values of the type (the reasoning of
/// [`crate::objects::masters::ClassId`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MasterKind(
    /// Its value, as the `.dat` and the save hold it.
    u8,
);

impl MasterKind {
    /// `RGE_Master_Static_Object`.
    pub const STATIC: Self = Self::named(10);

    /// `TRIBE_Master_Tree_Object`, with the static object's setters.
    pub const TREE: Self = Self::named(15);

    /// `RGE_Master_Animated_Object`.
    pub const ANIMATED: Self = Self::named(20);

    /// `RGE_Master_Doppleganger_Object`, with the animated object's setters; the type of a ghost
    /// (`genie_core::vision::ghost`).
    pub const DOPPELGANGER: Self = Self::named(25);

    /// `RGE_Master_Moving_Object`.
    pub const MOVING: Self = Self::named(30);

    /// `RGE_Master_Action_Object`.
    pub const ACTION: Self = Self::named(40);

    /// `RGE_Master_Combat_Object`.
    pub const RGE_COMBAT: Self = Self::named(50);

    /// `RGE_Master_Missile_Object` (`'<'`).
    pub const MISSILE: Self = Self::named(60);

    /// `TRIBE_Master_Combat_Object` (`'F'`).
    pub const COMBAT: Self = Self::named(70);

    /// `TRIBE_Master_Building_Object` (`'P'`).
    pub const BUILDING: Self = Self::named(80);

    /// Its value, for a packet, a dump and the checkpoint's words.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The type a master holds (`+0x8`, the `.dat`'s `unit_type`): any byte is one.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// Whether a master of this type holds the action layer (`RGE_Master_Action_Object`'s
    /// values, `+0x790`..`+0x7b9`): the `.dat` reads the type-40 section (genieutils' `Bird`)
    /// for the types 40 and up but 90; the save holds the RGE action part for 40 and up.
    #[must_use]
    pub const fn has_action_layer(self) -> bool {
        Self::ACTION.0 <= self.0 && self.0 != 90
    }

    /// Whether a master of this type holds the combat layer (`RGE_Master_Combat_Object`'s
    /// values, `+0x7f8`..`+0x87f`): the `.dat` reads the type-50 section for the types 50 and up
    /// but 90; the save holds the RGE combat part for 50, the missiles (60), 70 and 80.
    #[must_use]
    pub const fn has_combat_layer(self) -> bool {
        Self::RGE_COMBAT.0 <= self.0 && self.0 != 90
    }

    /// Whether a master of this type holds the creatable layer (`TRIBE_Master_Combat_Object`'s
    /// values, `+0x880`..`+0x94f`): the `.dat` reads the creatable section for the types 70 and
    /// up but 90, one of the data's own types; the save holds the TRIBE combat part for 70 and 80.
    #[must_use]
    pub const fn has_creatable_layer(self) -> bool {
        Self::COMBAT.0 <= self.0 && self.0 != 90
    }

    /// A type the code names, in one of the constants above: only they make them.
    const fn named(value: u8) -> Self {
        Self(value)
    }
}

impl Display for MasterKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl Diff for MasterKind {
    fn diff(&self, other: &Self, d: &mut Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::MasterKind;

    /// Any byte is a type, and the named ones compare as the game's thresholds do.
    #[test]
    fn type_is_any_byte_and_the_named_ones_are_ordered() {
        assert_eq!(MasterKind::of_data(70), MasterKind::COMBAT);
        assert_eq!(MasterKind::of_data(0).get(), 0);
        assert!(MasterKind::ACTION < MasterKind::COMBAT);
        assert!(MasterKind::BUILDING >= MasterKind::COMBAT);
    }
}
