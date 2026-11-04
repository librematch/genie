//! A main building's annexes (`+0xbc0`..`+0xbd8`): its four slots, each an annex or none.

use crate::objects::ObjectId;
use crate::{GenieError, GenieResult};
use genie_diff::Diff;

/// A main building's annexes (`+0xbc0`..`+0xbd8`, the save's `mLinkedObjectIds`): four slots,
/// the slot of each annex its master's annex index (the building constructor's link). The
/// annex's own `+0xbb8` names the building ([`crate::objects::layers::Building::annex_of`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct Annexes([Option<ObjectId>; 4]);

impl Annexes {
    /// The slots a building holds (a master's annexes, `+0xbc0` to `+0xbd8`).
    pub const SLOTS: usize = 4;

    /// No annexes.
    pub const NONE: Self = Self([None; Self::SLOTS]);

    /// The slots as the save holds them, in order.
    #[must_use]
    pub const fn new(slots: [Option<ObjectId>; Self::SLOTS]) -> Self {
        Self(slots)
    }

    /// The annexes in slot order, the empty slots left out.
    pub fn ids(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.0.iter().flatten().copied()
    }

    /// Slot `slot` takes the annex `id` (the building constructor's link).
    ///
    /// # Errors
    ///
    /// A slot past the building's four.
    pub fn link(&mut self, slot: usize, id: ObjectId) -> GenieResult<()> {
        let held = self.0.get_mut(slot).ok_or_else(|| {
            GenieError::unreachable("an annex past a building's four slots").with("slot", slot)
        })?;
        *held = Some(id);
        Ok(())
    }

    /// Every slot clears (the building's death lets its annexes go).
    pub const fn clear(&mut self) {
        self.0 = [None; Self::SLOTS];
    }

    /// Each slot whose annex `gone` tells is gone clears, in slot order (the building's update
    /// lets an annex past state 6 go).
    ///
    /// # Errors
    ///
    /// The first error of `gone`.
    pub fn let_go(
        &mut self,
        mut gone: impl FnMut(ObjectId) -> GenieResult<bool>,
    ) -> GenieResult<()> {
        for slot in &mut self.0 {
            if let Some(id) = *slot
                && gone(id)?
            {
                *slot = None;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_ids::object_id;

    /// The constructor links each annex into its master's slot; the slots read in order, the
    /// empty ones left out; a fifth slot is not the building's; the death clears them all.
    #[test]
    fn building_annexes_take_their_master_slots() {
        let mut annexes = Annexes::NONE;
        assert_eq!(annexes.ids().count(), 0);
        annexes.link(2, object_id(12)).unwrap();
        annexes.link(0, object_id(10)).unwrap();
        assert_eq!(
            annexes.ids().collect::<Vec<_>>(),
            [object_id(10), object_id(12)]
        );
        assert_eq!(
            annexes,
            Annexes::new([Some(object_id(10)), None, Some(object_id(12)), None])
        );
        assert!(annexes.link(Annexes::SLOTS, object_id(14)).is_err());
        annexes.clear();
        assert_eq!(annexes, Annexes::NONE);
    }

    /// The update lets go the slots of the gone annexes and keeps the others in their slots; an
    /// error stops it at its slot.
    #[test]
    fn building_lets_its_gone_annexes_go() {
        let mut annexes = Annexes::new([
            Some(object_id(10)),
            None,
            Some(object_id(12)),
            Some(object_id(13)),
        ]);
        annexes.let_go(|id| Ok(id != object_id(12))).unwrap();
        assert_eq!(
            annexes,
            Annexes::new([None, None, Some(object_id(12)), None])
        );
        let mut annexes = Annexes::new([Some(object_id(10)), Some(object_id(11)), None, None]);
        let failed = annexes.let_go(|id| {
            if id == object_id(11) {
                Err(GenieError::unreachable("test"))
            } else {
                Ok(true)
            }
        });
        assert!(failed.is_err());
        assert_eq!(
            annexes,
            Annexes::new([None, Some(object_id(11)), None, None])
        );
    }
}
