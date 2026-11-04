//! A group's formation (FD-036): `BFormation`, its slots and lines, and the layouts that fill
//! them (createColumnLine `FUN_141012280`, ...).

use crate::GenieResult;
use crate::groups::FormationCategory;
use crate::groups::UnitKind;
use crate::groups::{Formation, FormationLine, Slot};
use crate::math::fmath::BVector;
use crate::world::slots::CheckedIndexExt;

mod column_line;
mod desired_positions;
mod passability;
mod slots;
mod unreachable;

pub(crate) use self::column_line::LineKind;
#[cfg(test)]
pub(crate) use self::column_line::column_line_slots;
#[cfg(test)]
use self::column_line::sizes;
pub(in crate::groups) use self::desired_positions::on_map;
pub(crate) use self::desired_positions::top_path;
use self::desired_positions::{RIGHT_ANGLE, transform_slot};
pub use self::desired_positions::{find_point_from_front, on_last_high_level_waypoint};
#[cfg(test)]
use self::passability::{cell_mark, count_up, passability_box, slot_tile};
pub use self::slots::{SlotFill, SlotOrder};
#[cfg(test)]
use crate::groups::UnitGroup;
use crate::math::image::group::EPSILON as GROUP_EPSILON;
#[cfg(test)]
use crate::movement::PathData;
#[cfg(test)]
use crate::world::World;
use std::mem::take;

/// The sub-formations a layout goes through (`FUN_141012280`'s `0..=5`).
const CATEGORIES: i32 = 6;
/// Below this a distance back counts as none (`DAT_142eb0c60`).
const EPSILON: f32 = GROUP_EPSILON;

impl Formation {
    /// `BFormation`'s constructor (the 0x58 bytes `FUN_14103fd00` allocates): no slots, no
    /// lines, horde (0), facing x (`+0x3c` 1.0).
    #[must_use]
    pub fn new() -> Self {
        Self {
            transform_orientation: BVector::new(1.0, 0.0, 0.0),
            ..Self::default()
        }
    }

    /// The slots' array's capacity (`+0x8`): the slots in use and the spare ones.
    #[must_use]
    pub const fn slot_capacity(&self) -> usize {
        self.slots.len() + self.spare_slots.len()
    }

    /// `FUN_14102d580`: a larger slots' array of `count`, each new slot without a unit (-1), its
    /// desired category and sub-category -1, the rest 0; the slots in use are copied but for
    /// their transform mark, the spare ones not.
    pub(crate) fn resize_slots(&mut self, count: usize) {
        if count == 0 || count <= self.slot_capacity() {
            return;
        }
        let fresh = Slot {
            unit_id: None,
            desired_category: FormationCategory::ANY,
            desired_sub_category: UnitKind::NONE,
            ..Slot::default()
        };
        self.spare_slots = vec![fresh; count - self.slots.len()];
        // the copy moves `+0x00..+0x43` (0x14102d646..0x14102d6bf): the transform mark (`+0x44`)
        // is the new array's 0
        for slot in &mut self.slots {
            slot.transformed = 0;
        }
    }

    /// `FUN_14102d490`: a new lines' array of `count` (the old lines go), each line empty.
    pub(crate) fn resize_lines(&mut self, count: usize) {
        if count == 0 || count <= self.lines.len() {
            return;
        }
        self.lines = vec![FormationLine::default(); count];
    }

    /// `FUN_141041810`: every slot of the array (in use and spare) loses its unit and its filled
    /// mark.
    pub fn clear_units(&mut self) {
        for slot in self.every_slot_mut() {
            slot.unit_id = None;
            slot.position_filled = 0;
        }
    }

    /// The slot at `index` of the whole array (in use or spare).
    pub(super) fn slot_mut(&mut self, index: usize) -> GenieResult<&mut Slot> {
        let used = self.slots.len();
        if index < used {
            self.slots.checked_mut(index)
        } else {
            self.spare_slots.checked_mut(index - used)
        }
    }

    /// Every slot of the array, in use and spare.
    pub(crate) fn every_slot_mut(&mut self) -> impl Iterator<Item = &mut Slot> {
        self.slots.iter_mut().chain(self.spare_slots.iter_mut())
    }

    /// `mNumberSlots` (`+0x4`) set to `count`: the array's first `count` slots are the ones in use.
    pub(super) fn use_slots(&mut self, in_use: usize) {
        let mut all: Vec<Slot> = take(&mut self.slots);
        all.append(&mut self.spare_slots);
        let count = in_use.min(all.len());
        self.spare_slots = all.split_off(count);
        self.slots = all;
    }
}

#[cfg(test)]
pub(crate) mod tests;
