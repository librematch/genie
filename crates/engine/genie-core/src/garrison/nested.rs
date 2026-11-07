//! The units inside the units inside a container (`+0x9a8`, [`NestedCount`]): what a unit holds
//! comes in with it (slot 0x1a0) and goes out with it (slot 0x1a8).

use crate::GenieResult;
use crate::garrison::InsideCount;
use crate::objects::ObjectId;
use crate::objects::masters::MasterKind;
use crate::world::World;
use genie_diff::Diff;

/// A combat object's `inside_garrison_count` (`+0x9a8`, a byte, saved): what the units inside it
/// held as they came in (their slot 0x498, `FUN_141212e10`) less what they held as they left
/// (`FUN_141213120`). Its slot 0x498 adds it to the units inside ([`World::garrisoned`]). Only
/// a container of type 70 or more keeps one; an object below type 70 has none (0).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Diff)]
pub struct NestedCount(u8);

impl NestedCount {
    /// None held: a new object's, and every object's below type 70.
    pub const ZERO: Self = Self(0);

    /// The count of the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// What slot 0x498 adds (`FUN_141213500`'s `MOVZX`).
    #[must_use]
    pub const fn count(self) -> InsideCount {
        InsideCount::new(self.0 as i32)
    }

    /// The count after a unit that holds `held` came in (`FUN_141212e10`'s `ADD byte`): the
    /// count's low byte added, wrapping.
    #[must_use]
    pub const fn took_in(self, held: InsideCount) -> Self {
        Self(self.0.wrapping_add(held.get() as u8))
    }

    /// The count after a unit that holds `held` went out (`FUN_141213120`): the count's low byte
    /// taken off, or 0 when the count is below it (`CMP AL, CL`, `JBE`, unsigned).
    #[must_use]
    pub const fn let_out(self, held: InsideCount) -> Self {
        let held = held.get() as u8;
        if self.0 < held {
            Self::ZERO
        } else {
            Self(self.0 - held)
        }
    }
}

/// Whether a unit of type `unit` takes what it holds into a container of type `container` and
/// out again. Its slots 0x1a0 and 0x1a8 must be `TRIBE_Combat_Object`'s (`FUN_141212e10`,
/// `FUN_141213120`, types 70 and 80; below, `FUN_141084b30` or `FUN_1411921d0` and
/// `FUN_141192460` keep no count), and the container of type 70 or more (`CMP byte [master +
/// 8], 0x46`, `JC`).
fn carries_nested(unit: MasterKind, container: MasterKind) -> bool {
    unit.has_creatable_layer() && MasterKind::COMBAT <= container
}

impl World {
    /// Slot 0x1a0's count (`FUN_141212e10`, after `FUN_141084b30` took the unit in) for unit `id`
    /// of type `unit` that went into `container` of type `container_kind`: the container's
    /// `+0x9a8` takes what the unit holds (its slot 0x498, [`World::garrisoned`]).
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the combat table lost the layer it took).
    pub(crate) fn nest_in(
        &mut self,
        id: ObjectId,
        unit: MasterKind,
        container: ObjectId,
        container_kind: MasterKind,
    ) -> GenieResult<()> {
        if !carries_nested(unit, container_kind) {
            return Ok(());
        }
        let held = self.garrisoned(id);
        let combat = self.parts.combat_mut(container)?;
        combat.nested = combat.nested.took_in(held);
        Ok(())
    }

    /// Slot 0x1a8's count (`FUN_141213120`, after the container's slot 0x1c8 let the unit go) for
    /// unit `id` of type `unit` that left `container` of type `container_kind`: the container's
    /// `+0x9a8` gives back what the unit holds (its slot 0x498, [`World::garrisoned`]), down to 0.
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the combat table lost the layer it took).
    pub(crate) fn nest_out(
        &mut self,
        id: ObjectId,
        unit: MasterKind,
        container: ObjectId,
        container_kind: MasterKind,
    ) -> GenieResult<()> {
        if !carries_nested(unit, container_kind) {
            return Ok(());
        }
        let held = self.garrisoned(id);
        let combat = self.parts.combat_mut(container)?;
        combat.nested = combat.nested.let_out(held);
        Ok(())
    }
}
