//! A group's formation (FD-036): `BFormation`, its slots and lines, and the layouts that fill
//! them (createColumnLine `FUN_141012280`, ...).

use crate::GenieError;
use crate::groups::FormationCategory;
use crate::groups::UnitType;
use crate::groups::{Formation, FormationLine, Slot};
use crate::math::fmath::BVector;
use crate::world::slots::CheckedIndex;

mod column_line;
mod desired_positions;
mod passability;
mod slots;

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
#[cfg(test)]
use crate::groups::UnitGroup;
use crate::math::image::group::{
    EPSILON as GROUP_EPSILON, FLANK_SPACING as GROUP_FLANK_SPACING, SPACING_FACTOR,
    STAGGER_SHARE as GROUP_STAGGER_SHARE,
};
#[cfg(test)]
use crate::movement::PathData;
#[cfg(test)]
use crate::world::World;
use std::mem::take;

/// The sub-formations a layout goes through (`FUN_141012280`'s `0..=5`).
const CATEGORIES: i32 = 6;
/// A flanked line's slots move apart by this less the line's share of the depth
/// (`DAT_142eb3634`).
const FLANK_SPACING: f32 = GROUP_FLANK_SPACING;
/// A staggered line moves by a quarter of the slot offset (`DAT_142eb0f48`).
const STAGGER_SHARE: f32 = GROUP_STAGGER_SHARE;
/// A staggered line's offset is twice the spacing, and an even line moves by half the offset
/// (`DAT_142e8c150`).
const TWO: f32 = SPACING_FACTOR;
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

    /// `FUN_14102d580`: a larger slots' array of `n`, each new slot without a unit (-1), its
    /// desired category and sub-category -1, the rest 0; the slots in use are copied but for
    /// their transform mark, the spare ones not.
    pub(crate) fn resize_slots(&mut self, n: usize) {
        if n == 0 || n <= self.slot_capacity() {
            return;
        }
        let fresh = Slot {
            unit_id: None,
            desired_category: FormationCategory::ANY,
            desired_sub_category: UnitType::NONE,
            ..Slot::default()
        };
        self.spare_slots = vec![fresh; n - self.slots.len()];
        // the copy moves `+0x00..+0x43` (0x14102d646..0x14102d6bf): the transform mark (`+0x44`)
        // is the new array's 0
        for s in &mut self.slots {
            s.transformed = 0;
        }
    }

    /// `FUN_14102d490`: a new lines' array of `n` (the old lines go), each line empty.
    pub(crate) fn resize_lines(&mut self, n: usize) {
        if n == 0 || n <= self.lines.len() {
            return;
        }
        self.lines = vec![FormationLine::default(); n];
    }

    /// `FUN_141041810`: every slot of the array (in use and spare) loses its unit and its filled
    /// mark.
    pub fn clear_units(&mut self) {
        for s in self.every_slot_mut() {
            s.unit_id = None;
            s.position_filled = 0;
        }
    }

    /// The slot at `i` of the whole array (in use or spare).
    pub(super) fn slot_mut(&mut self, i: usize) -> Result<&mut Slot, GenieError> {
        let used = self.slots.len();
        if i < used {
            self.slots.checked_mut(i)
        } else {
            self.spare_slots.checked_mut(i - used)
        }
    }

    /// Every slot of the array, in use and spare.
    pub(crate) fn every_slot_mut(&mut self) -> impl Iterator<Item = &mut Slot> {
        self.slots.iter_mut().chain(self.spare_slots.iter_mut())
    }

    /// `mNumberSlots` (`+0x4`) set to `n`: the array's first `n` slots are the ones in use.
    pub(super) fn use_slots(&mut self, n: usize) {
        let mut all: Vec<Slot> = take(&mut self.slots);
        all.append(&mut self.spare_slots);
        let n = n.min(all.len());
        self.spare_slots = all.split_off(n);
        self.slots = all;
    }
}

#[cfg(test)]
pub(crate) mod tests;
