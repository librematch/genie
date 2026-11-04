//! The formation's slots their units cannot reach (`move_unreachable`): each one's place moves
//! to the cell its unit can reach.

use crate::GenieResult;
use crate::groups::Formation;
use crate::groups::formation::passability::{PASS_HALF, PASS_ONE, PassGrid, cell_mark, slot_tile};
use crate::map::Distance;
use crate::map::GroundOffset;
use crate::map::spatial::RecordId;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::below_or_unordered;
use crate::world::World;
use crate::world::slots::CheckedIndexExt;

impl World {
    /// The unreachable slots' places: each one's unit searches the grid for the cell it can
    /// reach and the slot's place moves there (nudged for a spacing under [`PASS_ONE`]).
    pub(super) fn move_unreachable(
        &self,
        formation: &mut Formation,
        grid: &mut PassGrid,
        (members, un): (&[RecordId], Unreachable<'_>),
    ) -> GenieResult<()> {
        let (width, height) = (grid.size.width(), grid.size.height());
        let (ox, oz) = (grid.origin.x(), grid.origin.y());
        let (nearest, grow) = (un.nearest, un.grow);
        let mut visited = vec![0u8; (width * height).max(0) as usize];
        for &slot_index in un.slots {
            visited.fill(0);
            let Some(unit) = self.world_object(formation.slots.checked(slot_index)?.unit_id) else {
                continue;
            };
            let unit_mask = self.master_mask(unit.player(), unit.master_id(self.model.slots))?;
            let unit_at = unit.position();
            let gx = cvttss2si(unit_at.x() - ox as f32);
            let gy = cvttss2si(unit_at.y() - oz as f32);
            let Some((found, nudge)) = self.search_cell(grid, &mut visited, (gx, gy), unit_mask)
            else {
                continue;
            };
            // CVTTSS2SI at 0x14103cb6a and 0x14103cb70
            let tile = if slot_tile(formation.slots.checked(nearest)?.transform_position) == found {
                found
            } else {
                let Some(at) = self.step_once_more(grid, found, unit_at.ground(), grow, members)?
                else {
                    continue;
                };
                at
            };
            let (x, y) = (tile.x(), tile.y());
            let slot = formation.slots.checked_mut(slot_index)?;
            // COMISS/JC at 0x14103cde3: a NaN spacing nudges too
            if below_or_unordered(un.spacing.get(), PASS_ONE) {
                slot.transform_position = slot
                    .transform_position
                    .with_x(
                        x as f32
                            + PASS_HALF
                            + GroundOffset::table([
                                [
                                    self.model.tuning.group.slot_nudge_x[0],
                                    self.model.tuning.group.slot_nudge_z[0],
                                ],
                                [
                                    self.model.tuning.group.slot_nudge_x[1],
                                    self.model.tuning.group.slot_nudge_z[1],
                                ],
                                [
                                    self.model.tuning.group.slot_nudge_x[2],
                                    self.model.tuning.group.slot_nudge_z[2],
                                ],
                                [
                                    self.model.tuning.group.slot_nudge_x[3],
                                    self.model.tuning.group.slot_nudge_z[3],
                                ],
                                [0.0, 0.25],
                            ])
                            .checked(nudge)?
                            .x(),
                    )
                    .with_y(
                        y as f32
                            + PASS_HALF
                            + GroundOffset::table([
                                [
                                    self.model.tuning.group.slot_nudge_x[0],
                                    self.model.tuning.group.slot_nudge_z[0],
                                ],
                                [
                                    self.model.tuning.group.slot_nudge_x[1],
                                    self.model.tuning.group.slot_nudge_z[1],
                                ],
                                [
                                    self.model.tuning.group.slot_nudge_x[2],
                                    self.model.tuning.group.slot_nudge_z[2],
                                ],
                                [
                                    self.model.tuning.group.slot_nudge_x[3],
                                    self.model.tuning.group.slot_nudge_z[3],
                                ],
                                [0.0, 0.25],
                            ])
                            .checked(nudge)?
                            .y(),
                    );
            } else {
                slot.transform_position = slot
                    .transform_position
                    .with_x(x as f32 + PASS_HALF)
                    .with_y(y as f32 + PASS_HALF);
            }
        }
        Ok(())
    }
}

/// What [`World::move_unreachable`] reads of the passability: the slots to move, the nearest
/// slot, the group's spacing and the manager's growth.
#[derive(Clone, Copy)]
pub(super) struct Unreachable<'slots> {
    pub(super) slots: &'slots [usize],
    pub(super) nearest: usize,
    pub(super) spacing: Distance,
    pub(super) grow: Distance,
}

/// The reachable slots' marks on the grid: each of `reachable`'s places marks its cell toward
/// the place of the nearest slot `near_index`.
pub(super) fn mark_reachable(
    grid: &mut PassGrid,
    formation: &Formation,
    (reachable, near_index): (&[usize], usize),
) -> GenieResult<()> {
    let (width, height) = (grid.size.width(), grid.size.height());
    let (ox, oz) = (grid.origin.x(), grid.origin.y());
    let near = formation.slots.checked(near_index)?.transform_position;
    for &reached in reachable {
        let checked = formation.slots.checked(reached)?.transform_position;
        let gx = cvttss2si(checked.x() - ox as f32);
        let gz = cvttss2si(checked.y() - oz as f32);
        if !(0 <= gx && gx < width && 0 <= gz && gz < height) {
            continue;
        }
        let cell = (gz * width + gx) as usize;
        *grid.cells.checked_mut(cell)? = cell_mark(
            *grid.cells.checked(cell)?,
            near.x() - checked.x(),
            near.y() - checked.y(),
        );
    }
    Ok(())
}
