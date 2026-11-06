//! `FormationTransform` (`FUN_14103a1f0`) and its passability: the grid over the box, the
//! slots' reach and the search for an unreachable slot's unit.

use crate::groups::GroupId;
use crate::groups::TransformKind;
use crate::groups::formation::SlotFill;
use crate::groups::formation::SlotOrder;
use crate::groups::formation::unreachable::{Unreachable, mark_reachable};
use crate::groups::formation::{RIGHT_ANGLE, find_point_from_front, transform_slot};
use crate::groups::motion::YawLimit;
use crate::groups::{Formation, Slot};
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::spatial::RecordId;
use crate::map::{Point, Rect, Tile};
use crate::world::slots::CheckedIndexExt;
use crate::{GenieError, GenieResult};

/// No active slot lies nearer than `f32::MAX`: the game reads the slot before the array then.
const NO_NEAREST_SLOT: GenieError =
    GenieError::unreachable("no active slot lies nearer than f32::MAX");
use crate::map::TerrainMask;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::BVector;
use crate::math::fmath::{at_most_or_unordered, rotation};
use crate::math::image::group::FARTHEST;
use crate::math::image::group::{PASS_HALF as GROUP_PASS_HALF, PASS_ONE as GROUP_PASS_ONE};
use crate::world::World;
use crate::world::layer::generated::GroupTuning;
use crate::world::objects_ref::ObjectsRef;

/// The grid's extra cell, and the spacing below which slots are nudged (`DAT_14330eb8c`).
pub(super) const PASS_ONE: f32 = GROUP_PASS_ONE;
/// A slot lies in its cell's middle (`DAT_142e7f384`).
pub(super) const PASS_HALF: f32 = GROUP_PASS_HALF;
/// The grid's sides are at most this many cells (the literal `0x64` at `0x14103bf01`).
pub(super) const PASS_CELLS: i32 = 100;
/// The grid's bytes (`local_2948`, cleared by the literal `0x2710` at `0x14103bf36`).
pub(super) const GRID_BYTES: usize = 10000;

impl World {
    /// `FormationTransform` (`FUN_14103a1f0`) of `formation` for group `held`, the slots' `order`
    /// (`param_4`, [`SlotOrder::PreviousColumn`] for a line laid out after a column): nothing
    /// without slots in use. The group's yaw from the canon forward (x, or -x about faced) and the
    /// formation's last one (`calculateYawAngle`) give the rotation about the group's up; the
    /// formation takes the group's forward as its orientation, every slot's transform mark clears
    /// and none counts as transformed. The line transform (type 0) takes each line's point behind
    /// the group (its distance back, or its index times the group's spacing): the stored positions'
    /// point there ([`find_point_from_front`]), else the group's place less its forward times the
    /// distance; each of the line's slots is placed there ([`transform_slot`]) and the line counts
    /// as transformed (below 256 places), while fewer than the array's slots are. The group (1) and
    /// horde (3) transforms place every slot of the array from the group's place, the target
    /// transform (2) from the target's original place, all counting. A turn of more than
    /// [`RIGHT_ANGLE`] with fewer than two sub-formations takes the units anew: every slot of the
    /// array loses its unit and its filled mark and the slots take their units (from the previous
    /// column, [`World::from_previous_column`], in that order, else the closest,
    /// [`World::populate_slots`]). Then the slots' passability ([`World::passable_slots`]).
    /// [`SlotFill::Anew`] when the slots took their units anew (`*param_3`).
    ///
    /// # Errors
    ///
    /// A formation line past the lines' array, and the routines it runs.
    #[expect(
        clippy::too_many_lines,
        reason = "`FormationTransform` (`FUN_14103a1f0`) ported as one function, its branches in the game's order"
    )]
    pub fn formation_transform(
        &mut self,
        formation: &mut Formation,
        group: GroupId,
        order: SlotOrder,
    ) -> GenieResult<SlotFill> {
        if formation.slots.is_empty() {
            return Ok(SlotFill::Kept);
        }
        let held = self.groups.held(group)?;
        let world = (self.world_time, self.spatial.map);
        let canon = BVector::new(
            if formation.about_faced == 0 {
                1.0
            } else {
                -1.0
            },
            0.0,
            0.0,
        );
        let [position, forward, up, _right] = held.motion.position;
        // calculateYawAngle without its clamp (`param_5` 0), with the world's update time
        let dt = self.dt_seconds;
        let yaw = held
            .motion
            .calculate_yaw_angle(canon, forward, dt, YawLimit::Free);
        let old = held.motion.calculate_yaw_angle(
            canon,
            formation.transform_orientation,
            dt,
            YawLimit::Free,
        );
        let flips = RIGHT_ANGLE < (yaw - old).get().abs() && held.unit_kinds.len() < 2;
        let rotation = rotation(yaw.get(), up);
        formation.transform_orientation = forward;
        for slot in formation.every_slot_mut() {
            slot.transformed = 0;
        }
        formation.number_transformed_slots = 0;
        let capacity = formation.slot_capacity() as i32;
        match formation.transform_kind {
            TransformKind::ByLines => {
                for index in 0..formation.number_lines.max(0) {
                    if capacity <= formation.number_transformed_slots {
                        break;
                    }
                    let line =
                        formation
                            .lines
                            .get(index as usize)
                            .ok_or(GenieError::unsupported(
                                "a formation line past the lines' array",
                            ))?;
                    let mut back = 0.0;
                    if 0 < index {
                        back = line.distance_back.get();
                        // COMISS/JA at 0x14103a850: a NaN distance back takes the spacing too
                        if at_most_or_unordered(back, 0.0) {
                            back = index as f32 * held.desired_spacing.get();
                        }
                    }
                    let point = find_point_from_front(&held.stored_positions, Distance::new(back))
                        .unwrap_or_else(|| position - back * forward);
                    let indices = line.indices.clone();
                    let width = indices.len() as i32;
                    for index in indices {
                        transform_slot(
                            formation,
                            &rotation,
                            point,
                            index as i32,
                            TransformKind::ByLines,
                            world,
                            &self.model.tuning.group,
                        )?;
                    }
                    if width < 0x100 {
                        formation.number_transformed_slots += width;
                    }
                }
            }
            kind @ (TransformKind::Turned | TransformKind::AtTarget | TransformKind::Unturned) => {
                let point = if kind == TransformKind::AtTarget {
                    held.current_target.original_position
                } else {
                    position
                };
                let mut index = 0;
                loop {
                    transform_slot(
                        formation,
                        &rotation,
                        point,
                        index,
                        kind,
                        world,
                        &self.model.tuning.group,
                    )?;
                    index += 1;
                    formation.number_transformed_slots += 1;
                    if capacity <= formation.number_transformed_slots {
                        break;
                    }
                }
            }
            _ => {}
        }
        let mut took = false;
        if flips {
            formation.clear_units();
            if order == SlotOrder::PreviousColumn {
                self.from_previous_column(formation, held)?;
            } else {
                self.populate_slots(formation, held)?;
            }
            took = true;
        }
        self.passable_slots(formation, group)?;
        Ok(if took { SlotFill::Anew } else { SlotFill::Kept })
    }

    /// `FormationTransform`'s passability (`FUN_14103a1f0` after the transform) of `formation`
    /// for group `held`, tested with the object of the group's lead ([`World::lead_object`], the
    /// lead -1 the world's object -1), else that of the first active slot's unit the world's
    /// table holds ([`World::world_object`], an empty slot's -1 too; none: nothing). The slots' places are their transform positions
    /// (`+0x1c`, the save's `mTransformPosition`). At the first active slot whose place's tile
    /// the tester cannot stand on (`FUN_141185690`, [`crate::map::spatial::Spatial::standable`] with
    /// its master's terrain mask): the active slot nearest the group's place (3D) must be
    /// standable, else it takes the group's place on x and y if that is (else nothing more).
    /// The box around the tester's place and the active slots' places gets the
    /// temporary records of the tiles its mask lacks (`FUN_141052b90`), the members are ignored
    /// (`FUN_1410562e0`), and each active slot is reachable when the line from the group's place
    /// to it meets nothing standing ([`crate::map::spatial::Spatial::still_line_hit`]). With fewer
    /// than `GroupTuning::pass_share` of the slots' array reachable, all are unreachable but the nearest,
    /// which takes the group's place. On a grid of the box plus `GroupTuning::pass_half_margin` each way
    /// (at most [`PASS_CELLS`] cells a side) the reachable slots' cells count their slots (bits 1 to 5, bit
    /// 0 set) and point toward the nearest slot (bits 6 and 7: +y, +x, -y, -x). Each
    /// unreachable slot's unit then searches breadth first from its own place's cell, through
    /// cells it can stand on, x+1, x-1, y+1, y-1 in turn, for a reachable cell: one of fewer
    /// than five slots takes it (counted), a fuller one has the searching cell take its place
    /// (one slot, pointing to it). A cell found that is not the nearest slot's steps once more
    /// the way its cell points, when that cell holds at most five and the line from the unit
    /// there is clear. The slot then lies in the middle of its cell, below a spacing of
    /// [`PASS_ONE`] nudged by its count (`GroupTuning::slot_nudge_x`, `GroupTuning::slot_nudge_z`). The temporary
    /// records and the ignore list go after.
    ///
    /// # Errors
    ///
    /// A tester or slot unit of a master the slice does not have or of no terrain restriction,
    /// a map without its terrain, and a line query near a record whose place the slice does not
    /// know.
    ///
    /// Never: an active slot was found, so the nearest one is too (its distance is finite).
    pub(super) fn passable_slots(
        &mut self,
        formation: &mut Formation,
        group: GroupId,
    ) -> GenieResult<()> {
        let held = self.groups.held(group)?;
        let tester = self.lead_object(&held.motion).or_else(|| {
            formation
                .slots
                .iter()
                .filter(|slot| slot.active != 0)
                .find_map(|slot| self.world_object(slot.unit_id))
        });
        let Some(tester) = tester else {
            return Ok(());
        };
        let mask = self.master_mask(tester.player(), tester.master_id(self.model.slots))?;
        let stands = |at: BVector| {
            let tile = slot_tile(at);
            self.spatial.standable(tile, mask)
        };
        let position = held.motion.position[0];
        let Some(_) = formation
            .slots
            .iter()
            .position(|slot| slot.active != 0 && !stands(slot.transform_position))
        else {
            return Ok(());
        };
        // the active slot nearest the group's place, and the box around the tester and them
        let (bounds, nearest) = passability_box(
            &formation.slots,
            tester.moved_vector(),
            position,
            &self.model.tuning.group,
        );
        let slot_index = nearest.ok_or(NO_NEAREST_SLOT)?;
        if !stands(formation.slots.checked(slot_index)?.transform_position) {
            if !stands(position) {
                return Ok(());
            }
            let slot = formation.slots.checked_mut(slot_index)?;
            slot.transform_position = slot
                .transform_position
                .with_x(position.x())
                .with_y(position.y());
        }
        // the box's temporary tile records (FUN_141052b90 at 0x14103b8fa) with the buffer the
        // last FUN_1410562d0 left, which the lines grow by too
        let grow = self.spatial.manager_grow();
        let (min_x, min_z, max_x, max_z) = (bounds.x0(), bounds.y0(), bounds.x1(), bounds.y1());
        self.spatial
            .put_tile_records(bounds.corner0(), bounds.corner1(), grow, mask);
        let members: Vec<RecordId> = held
            .members
            .iter()
            .map(|member| RecordId::from(member.id))
            .collect();
        // the members are the manager's ignore list while the passability runs
        // (FUN_1410562e0, 0x14103b916), which every way out of it clears (0x14103ce93,
        // 0x14103cf93): none after it
        self.spatial.set_manager_ignore(&[]);
        let (mut reachable, mut unreachable) =
            self.slot_reach(formation, position, grow, &members)?;
        if (reachable.len() as f32) / (formation.slot_capacity() as f32)
            < self.model.tuning.group.pass_share
        {
            unreachable.append(&mut reachable);
            reachable.push(slot_index);
            let slot = formation.slots.checked_mut(slot_index)?;
            slot.transform_position = slot
                .transform_position
                .with_x(position.x())
                .with_y(position.y());
        }
        // CVTTSS2SI at 0x14103bedd, 0x14103bef9, 0x14103bf1c and 0x14103bf2e
        let width = cvttss2si((max_x - min_x) + self.model.tuning.group.pass_margin + PASS_ONE);
        let height = cvttss2si((max_z - min_z) + self.model.tuning.group.pass_margin + PASS_ONE);
        if PASS_CELLS < width || PASS_CELLS < height {
            // the records go (FUN_1410561f0 at 0x14103cf9f)
            self.spatial.clear_tile_records();
            return Ok(());
        }
        let ox = cvttss2si(min_x - self.model.tuning.group.pass_half_margin);
        let oz = cvttss2si(min_z - self.model.tuning.group.pass_half_margin);
        let mut grid = PassGrid {
            cells: vec![0u8; GRID_BYTES],
            size: GridSize::new(width, height),
            origin: Tile::new(ox, oz),
        };
        mark_reachable(&mut grid, formation, (&reachable, slot_index))?;
        let moved = Unreachable {
            slots: &unreachable,
            nearest: slot_index,
            spacing: held.desired_spacing,
            grow,
        };
        self.move_unreachable(formation, &mut grid, (&members, moved))?;
        // the records go (FUN_1410561f0 at 0x14103ce9f)
        self.spatial.clear_tile_records();
        Ok(())
    }

    /// [`World::passable_slots`]'s reach of the active slots from the group's `position`: a
    /// slot the line to which hits a record (the members ignored) is unreachable; the reachable
    /// and the unreachable slots' indices, in the slots' order.
    ///
    /// # Errors
    ///
    /// A line query near a record whose place the slice does not know.
    pub(super) fn slot_reach(
        &self,
        formation: &Formation,
        position: BVector,
        grow: Distance,
        members: &[RecordId],
    ) -> GenieResult<(Vec<usize>, Vec<usize>)> {
        let mut reachable = Vec::new();
        let mut unreachable = Vec::new();
        for (index, slot) in formation.slots.iter().enumerate() {
            if slot.active == 0 {
                continue;
            }
            let transform_position = slot.transform_position;
            if self.spatial.still_line_hit(
                position.ground(),
                transform_position.ground(),
                grow,
                members,
                ObjectsRef::new(
                    &self.players,
                    &self.index,
                    &self.model.masters,
                    &self.parts,
                    &self.model.roles,
                    &self.model.tuning,
                ),
            )? {
                unreachable.push(index);
            } else {
                reachable.push(index);
            }
        }
        Ok((reachable, unreachable))
    }

    /// [`World::passable_slots`]'s breadth-first search of an unreachable slot's unit from its
    /// cell `start` through the cells it can stand on (`unit_mask`), x+1, x-1, y+1, y-1 in turn,
    /// for a reachable cell: one holding fewer than five slots takes it (counted: its tile and
    /// its count before), a fuller one has the searching cell take its place (one slot, pointing
    /// to it: the searching cell's tile and 0). None from a start off the grid or without a
    /// reachable cell.
    pub(super) fn search_cell(
        &self,
        grid: &mut PassGrid,
        visited: &mut [u8],
        (gx, gy): (i32, i32),
        unit_mask: TerrainMask,
    ) -> Option<(Tile, usize)> {
        let (width, height) = (grid.size.width(), grid.size.height());
        let (ox, oz) = (grid.origin.x(), grid.origin.y());
        if !(0 <= gx && 0 <= gy && gx < width && gy < height) {
            return None;
        }
        let mut queue = vec![(gx, gy)];
        *visited.get_mut((gy * width + gx) as usize)? = 1;
        let mut qi = 0;
        while qi < queue.len() {
            let (cx, cy) = *queue.get(qi)?;
            qi += 1;
            if !(0 <= cx
                && cx < width
                && 0 <= cy
                && cy < height
                && self
                    .spatial
                    .standable(Tile::new(ox + cx, oz + cy), unit_mask))
            {
                continue;
            }
            let steps = [
                (cx + 1, cy, cx + 1 < width, 0x43u8),
                (cx - 1, cy, 0 < cx, 0xc3),
                (cx, cy + 1, cy + 1 < height, 3),
                (cx, cy - 1, 0 < cy, 0x83),
            ];
            for (nx, ny, inside, mark) in steps {
                if !inside {
                    continue;
                }
                let cell = (width * ny + nx) as usize;
                if *visited.get(cell)? != 0 {
                    continue;
                }
                *visited.get_mut(cell)? = 1;
                let cell_mark = *grid.cells.get(cell)?;
                if cell_mark & 1 != 0 {
                    if cell_mark & 0x3e < 10 {
                        *grid.cells.get_mut(cell)? = count_up(cell_mark);
                        return Some((
                            Tile::new(ox + nx, oz + ny),
                            usize::from(cell_mark >> 1 & 0x1f),
                        ));
                    }
                    *grid.cells.get_mut((width * cy + cx) as usize)? = mark;
                    return Some((Tile::new(ox + cx, oz + cy), 0));
                }
                queue.push((nx, ny));
            }
        }
        None
    }

    /// [`World::passable_slots`]'s step once more of a found cell (tile `found`) that is not the
    /// nearest slot's: the way its cell points, when that cell lies on the grid, holds at most
    /// five and the line from the unit (`unit_at`) to its middle is clear (the members ignored),
    /// the slot takes it (counted); the tile the slot takes, or none when the cell pointed to is
    /// off the grid (the slot stays where it is).
    ///
    /// # Errors
    ///
    /// A line query near a record whose place the slice does not know.
    pub(super) fn step_once_more(
        &self,
        grid: &mut PassGrid,
        found: Tile,
        unit_at: Point,
        grow: Distance,
        members: &[RecordId],
    ) -> GenieResult<Option<Tile>> {
        let (fx, fy) = (found.x(), found.y());
        let (width, height) = (grid.size.width(), grid.size.height());
        let (rx, ry) = (fx - grid.origin.x(), fy - grid.origin.y());
        let (ddx, ddy) = match *grid.cells.checked((width * ry + rx) as usize)? >> 6 {
            0 => (0, 1),
            1 => (1, 0),
            2 => (0, -1),
            _ => (-1, 0),
        };
        let (nx, ny) = (ddx + rx, ddy + ry);
        if ny < 0 || height <= ny || nx < 0 || width <= nx {
            return Ok(None);
        }
        let cell = (width * ny + nx) as usize;
        let checked = *grid.cells.checked(cell)?;
        if checked & 0x3e < 0xb
            && !self.spatial.still_line_hit(
                unit_at,
                Point::new((ddx + fx) as f32 + PASS_HALF, (fy + ddy) as f32 + PASS_HALF),
                grow,
                members,
                ObjectsRef::new(
                    &self.players,
                    &self.index,
                    &self.model.masters,
                    &self.parts,
                    &self.model.roles,
                    &self.model.tuning,
                ),
            )?
        {
            *grid.cells.checked_mut(cell)? = count_up(checked);
            return Ok(Some(Tile::new(ddx + fx, fy + ddy)));
        }
        Ok(Some(found))
    }
}

/// [`World::passable_slots`]'s grid over the box, a byte per tile (`size` from the tile
/// `origin`): bit 0 a reachable slot's cell, bits 1 to 5 the slots it holds, bits 6 and 7
/// the way to the nearest slot.
pub(super) struct PassGrid {
    pub(super) cells: Vec<u8>,
    pub(super) size: GridSize,
    pub(super) origin: Tile,
}

/// `FUN_14103a1f0`'s grid cell count up: bits 1 to 5 (the slots a cell holds) count on (mod 32),
/// the others kept.
pub(super) const fn count_up(mark: u8) -> u8 {
    (((mark & 0xfe).wrapping_add(2) ^ mark) & 0x3e) ^ mark
}

/// The tile of a slot's place the passability tests (`FUN_141185690`'s arguments): x and y
/// by `CVTTSS2SI` (0x14103b5df, 0x14103b777), a NaN or out of range one `i32::MIN`.
pub(super) fn slot_tile(at: BVector) -> Tile {
    Tile::new(cvttss2si(at.x()), cvttss2si(at.y()))
}

/// The passability's box and nearest slot (0x14103b650..0x14103b758): from the box
/// (`GroupTuning::pass_far`, `GroupTuning::pass_far`) to (0, 0), each active slot of `slots` widens it first by the
/// tester's place `at` (slot 0x520) and then by its own, as `MINSS new, old` and `MAXSS new, old`
/// take them (the old bound kept for a NaN either side); the nearest is the first active slot
/// whose squared distance to the group's place `position` (summed x, height, y) lies below the
/// best so far, from [`FARTHEST`] (`COMISS`/`CMOVBE`: a NaN distance keeps the earlier one, none
/// when no slot lies below `f32::MAX`). Returns the box and the nearest.
pub(super) fn passability_box(
    slots: &[Slot],
    at: BVector,
    position: BVector,
    group_tuning: &GroupTuning,
) -> (Rect, Option<usize>) {
    use crate::math::fmath::{maxss, minss};
    let (mut min_x, mut min_z, mut max_x, mut max_z) =
        (group_tuning.pass_far, group_tuning.pass_far, 0.0f32, 0.0f32);
    let mut best = FARTHEST;
    let mut nearest = None;
    for (index, slot) in slots.iter().enumerate() {
        if slot.active == 0 {
            continue;
        }
        let d3 = slot.transform_position - position;
        let distance = d3.x() * d3.x() + d3.height() * d3.height() + d3.y() * d3.y();
        let earlier = best;
        if distance < best {
            best = distance;
        }
        let (low_x, low_z) = (minss(at.x(), min_x), minss(at.y(), min_z));
        let (high_x, high_z) = (maxss(at.x(), max_x), maxss(at.y(), max_z));
        let transform_position = slot.transform_position;
        min_x = minss(transform_position.x(), low_x);
        min_z = minss(transform_position.y(), low_z);
        max_x = maxss(transform_position.x(), high_x);
        max_z = maxss(transform_position.y(), high_z);
        if distance < earlier {
            nearest = Some(index);
        }
    }
    (Rect::new(min_x, min_z, max_x, max_z), nearest)
}

/// A reachable slot's grid cell mark (0x14103bfd4..0x14103c046): the cell's count up
/// ([`count_up`]) with bit 0, then its way toward the nearest slot `dx`, `dz` off: along y when
/// `|dx|` is at most `|dz|` (`COMISS`/`JBE`: also for a NaN), with bit 7 for a `dz` below 0
/// (`JNC`: also NaN); else along x, bits 6 and 7 for a `dx` below 0 (`JC`: also NaN), bit 6 alone
/// otherwise.
pub(super) fn cell_mark(mark: u8, dx: f32, dz: f32) -> u8 {
    use crate::math::fmath::{at_most_or_unordered, below_or_unordered};
    let count_up = count_up(mark) | 1;
    if at_most_or_unordered(dx.abs(), dz.abs()) {
        if below_or_unordered(dz, 0.0) {
            count_up & 0x3f | 0x80
        } else {
            count_up & 0x3f
        }
    } else if below_or_unordered(dx, 0.0) {
        count_up | 0xc0
    } else {
        count_up & 0x3f | 0x40
    }
}
