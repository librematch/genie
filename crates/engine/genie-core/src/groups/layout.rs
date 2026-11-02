//! The formations' layouts besides createColumnLine (FD-036): a horde (`FUN_141013eb0`), a user
//! defined formation (`FUN_141014f60`), a phalanx (`FUN_1410147b0`) and a box (`FUN_141011a00`
//! with `FUN_14102ab30` and `CreateLines` `FUN_1410143a0`), as `UpdateDesiredPositions`
//! (`FUN_14103fd00`) makes them.

use crate::GenieError;
use crate::groups::FormationCategory;
use crate::groups::FormationKind;
use crate::groups::GroupId;
use crate::groups::SlotState;
use crate::groups::TransformType;
use crate::groups::UnitType;
use crate::groups::{Formation, FormationLine, Slot, UnitGroup};
use crate::map::Distance;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::{self, BVector};
use crate::math::image::group::{
    BOX_HALF as GROUP_BOX_HALF, BOX_SIDE_SHARE as GROUP_BOX_SIDE_SHARE,
    BOX_SIDES as GROUP_BOX_SIDES, EPSILON, HORDE_REACH as GROUP_HORDE_REACH, SPACING_FACTOR,
    USER_DEFINED_BACK as GROUP_USER_DEFINED_BACK,
};
use crate::objects::ObjectId;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::slots::CheckedIndex;

/// A horde's and a user defined formation's slots lie at most this far from the group's place
/// (`DAT_14330ebb8`).
const HORDE_REACH: f32 = GROUP_HORDE_REACH;
/// A user defined formation's slots move back by the farthest one ahead plus this
/// (`DAT_142e7f384`).
const USER_DEFINED_BACK: f32 = GROUP_USER_DEFINED_BACK;
/// Half: a phalanx's even line's step, a box's first side and its middles (`DAT_142e7f384`, and
/// the phalanx's `DAT_142e8c150` as a divisor).
const BOX_HALF: f32 = GROUP_BOX_HALF;
/// A phalanx's even line moves back by its step over this (`DAT_142e8c150`).
const TWO: f32 = SPACING_FACTOR;
/// A box's perimeter is its side times this (`DAT_142eb0f48`).
const BOX_SIDES: f32 = GROUP_BOX_SIDES;
/// A box too small for its units grows to this share of their length (`DAT_142eb880c`).
const BOX_SIDE_SHARE: f32 = GROUP_BOX_SIDE_SHARE;
/// Two slots whose offsets on x differ by less than this share a box's line (`DAT_142eb0c60`).
const LINE_EPSILON: f32 = EPSILON;
/// The places of the box's positions buffer (`DAT_143f15f00`, `FUN_14102ab30`'s `0x3c`).
pub const BOX_POSITIONS: usize = 60;

/// What a horde and a user defined formation read of a member the world holds: its id, its place
/// (slot 0x520) and its sub-formation (`FUN_141011300` with 0, -1 for none).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemberPlace {
    /// The unit.
    pub id: ObjectId,
    /// Slot 0x520.
    pub at: BVector,
    /// `FUN_141011300(unit, 0)`.
    pub category: FormationCategory,
}

/// A slot's 3D length as the layouts take it (`x² + y² + z²` in that order).
fn length(v: BVector) -> f32 {
    fmath::sqrt(v.x() * v.x() + v.height() * v.height() + v.y() * v.y())
}

/// The slots of `createHorde` (`FUN_141013eb0`), or of `createUDF` (`FUN_141014f60`) for
/// `user_defined` ([`World::create_horde`], [`World::create_udf`],
/// which look the members up), of `formation` for `group`, `members` its members (None for one
/// the world does not hold): false without members. The slots' array grows to the members (`FUN_14102d580`) and every
/// slot turns inactive; the type is a horde (0) or user defined (5), the width and depth -1, no
/// lines, the transform type 3 (a horde) or 1. The slot of each held member's index takes the
/// unit, its offset from the group's place, its place as the transform and desired transform
/// positions (transformed, `+0x44`), the world's time, state and depth 0, filled, active, its
/// sub-formation and no sub-category; the slots in use are the members (an unheld member's slot
/// keeps what it had, inactive). A user defined formation's offsets on x move back by the
/// largest of them (0 at least) plus [`USER_DEFINED_BACK`]; then any offset longer than
/// [`HORDE_REACH`] is scaled down to it.
pub(crate) fn slots_by_place(
    formation: &mut Formation,
    group: &UnitGroup,
    members: &[Option<MemberPlace>],
    user_defined: bool,
    world_time: WorldTime,
) -> Result<bool, GenieError> {
    let n = group.members.len();
    if n == 0 {
        return Ok(false);
    }
    formation.resize_slots(n);
    for s in formation.every_slot_mut() {
        s.active = 0;
    }
    formation.kind = if user_defined {
        FormationKind::USER_DEFINED
    } else {
        FormationKind::HORDE
    };
    formation.maximum_width = -1;
    formation.maximum_depth = -1;
    formation.number_lines = 0;
    formation.transform_type = if user_defined {
        TransformType::TURNED
    } else {
        TransformType::UNTURNED
    };
    let position = group.motion.position[0];
    for (i, m) in members.iter().enumerate().take(n) {
        let Some(m) = m else {
            continue;
        };
        let s = formation.slot_mut(i)?;
        s.unit_id = Some(m.id);
        s.offset = fmath::sub(m.at, position);
        s.desired_transform_position = m.at;
        s.transform_position = m.at;
        s.transformed = 1;
        s.time = world_time;
        s.state = SlotState::Steering;
        s.depth = 0;
        s.position_filled = 1;
        s.active = 1;
        s.desired_category = m.category;
        s.desired_sub_category = UnitType::NONE;
    }
    formation.use_slots(n);
    if user_defined {
        let mut ahead = 0.0f32;
        for s in &formation.slots {
            // MAXSS at 0x141015208: a NaN offset keeps the farthest so far
            ahead = fmath::maxss(s.offset.x(), ahead);
        }
        let back = ahead + USER_DEFINED_BACK;
        for s in &mut formation.slots {
            s.offset = s.offset.with_x(s.offset.x() - back);
        }
    }
    for s in &mut formation.slots {
        let l = length(s.offset);
        if HORDE_REACH < l {
            let k = HORDE_REACH / l;
            s.offset = BVector::new(k * s.offset.x(), k * s.offset.height(), k * s.offset.y());
        }
    }
    Ok(true)
}

/// The clear createColumnLine, createPhalanx and `CreateLines` each make of a line before they
/// reset it (inlined, 0x14101357a..0x1410135a2 in createColumnLine): its places freed (`+8`), its width and
/// depth (`+0`, `+4`) and attack range (`+0x10`) 0; its distance back (`+0x14`) kept.
pub(crate) fn clear_line(line: &mut FormationLine) {
    line.indices = Vec::new();
    line.depth = 0;
    line.attack_range = Distance::ZERO;
}

/// `FUN_141008f50`: for `width` above 0 and `depth` not below 0 the line's places are `width`
/// of -1 (the old ones freed), at depth `depth`, its attack range 0, and the routine returns
/// true; for any other the line is left as it is and it returns false (the line load
/// `FUN_14101b290` fails on that). The allocation's failure, which leaves the line cleared, is
/// Rust's abort.
pub(crate) fn reset_line(line: &mut FormationLine, width: i32, depth: i32) -> bool {
    if 0 < width && 0 <= depth {
        line.indices = vec![u32::MAX; width as usize];
        line.depth = depth;
        line.attack_range = Distance::ZERO;
        true
    } else {
        false
    }
}

/// The unit type statistic a phalanx's next slot takes (`FUN_1410147b0`'s search): the current
/// one while it has units left, else the first of the next category with units (from 0 after
/// none, else from the current's category plus 1, up to 5); `placed` counts its slots.
fn next_statistic(group: &UnitGroup, current: Option<usize>, placed: &mut usize) -> Option<usize> {
    if let Some(i) = current
        && *placed < group.unit_types.get(i)?.units.len()
    {
        return current;
    }
    *placed = 0;
    let mut c = match current {
        Some(i) => group.unit_types.get(i)?.formation_category.next(),
        None => FormationCategory::named(0),
    };
    while c <= FormationCategory::PROTECTED {
        if let Some(i) = group
            .unit_types
            .iter()
            .position(|s| s.formation_category == c)
            && !group.unit_types.get(i)?.units.is_empty()
        {
            return Some(i);
        }
        c = c.next();
    }
    None
}

/// The slots of `createPhalanx` (`FUN_1410147b0`, [`World::create_phalanx`]),
/// up to its transform: false without members. The lines are
/// the fewest whose triangle (1 + 2 + ...) holds the members; the slots' array grows to their
/// square, the lines' to them, every slot turns inactive; a phalanx (3) of no slots yet,
/// transform type 0, the lines as its width, depth and lines. Line `i` holds `i + 1` slots, the
/// last (of two lines or more) the members left; it starts at half its slots (integer) times the
/// spacing straight back (`(0, 0, -spacing)`), less half a step for an even count, and steps by
/// it. Each slot takes no unit, the offset, the world's time, the line as its depth, active, and
/// the category of the unit type statistic in turn ([`next_statistic`], -1 when none is left);
/// the slots in use are the members.
pub(crate) fn phalanx_slots(
    formation: &mut Formation,
    group: &UnitGroup,
    world_time: WorldTime,
) -> Result<bool, GenieError> {
    let n = group.members.len() as i32;
    if n < 1 {
        return Ok(false);
    }
    let mut lines = 0;
    let mut left = n;
    let mut step = 1;
    while 0 < left {
        left -= step;
        lines += 1;
        step += 1;
    }
    formation.resize_slots((lines * lines) as usize);
    formation.resize_lines(lines as usize);
    for s in formation.every_slot_mut() {
        s.active = 0;
    }
    formation.kind = FormationKind::PHALANX;
    formation.use_slots(0);
    formation.transform_type = TransformType::BY_LINES;
    formation.maximum_width = lines;
    formation.maximum_depth = lines;
    formation.number_lines = lines;
    let single = fmath::scale(group.desired_spacing.get(), BVector::new(0.0, 0.0, -1.0));
    let mut slot = 0usize;
    let mut statistic = None;
    let mut placed = 0usize;
    for line in 0..lines {
        let mut width = line + 1;
        if 1 < lines && line == lines - 1 {
            width = n - slot as i32;
        }
        let l = formation.lines.checked_mut(line as usize)?;
        clear_line(l);
        let _ = reset_line(l, width, line);
        let mut at = fmath::add(fmath::scale((width / 2) as f32, single), BVector::ZERO);
        if width & 1 == 0 {
            let half = [single.x() / TWO, single.height() / TWO, single.y() / TWO];
            at = BVector::new(at.x() - half[0], at.height() - half[1], at.y() - half[2]);
        }
        for k in 0..width.max(0) as usize {
            statistic = next_statistic(group, statistic, &mut placed);
            *formation.slot_mut(slot)? = Slot {
                unit_id: None,
                offset: at,
                desired_transform_position: BVector::ZERO,
                transform_position: BVector::ZERO,
                time: world_time,
                state: SlotState::Steering,
                depth: line,
                position_filled: 0,
                active: 1,
                desired_category: match statistic {
                    Some(i) => group.unit_types.checked(i)?.formation_category,
                    None => FormationCategory::ANY,
                },
                desired_sub_category: UnitType::NONE,
                transformed: formation.slot_mut(slot)?.transformed,
            };
            let l = formation.lines.checked_mut(line as usize)?;
            if k < l.indices.len() {
                *l.indices.checked_mut(k)? = slot as u32;
            }
            placed += 1;
            at = fmath::sub(at, single);
            slot += 1;
        }
    }
    formation.use_slots(n as usize);
    Ok(true)
}

/// `placeBoxPerimeter` (`FUN_14102ab30`): `count` places on the box from `min` to `max` (x and z;
/// both grow by half of what a perimeter of `count` units of `spacing` lacks), into `out`'s x
/// and z; false (nothing written) for none or more than [`BOX_POSITIONS`]. Up to 8 units take
/// fixed places on the corners and middles of the sides at x `max` and `min`; more walk the
/// perimeter from (`max.x`, `min.z`) by the perimeter over the count, as the game walks it
/// (its first corner's x is `max.z` less the step's rest, which its third corner reuses).
pub fn place_box_perimeter(
    count: usize,
    out: &mut [BVector; BOX_POSITIONS],
    min: &mut BVector,
    max: &mut BVector,
    spacing: Distance,
) -> Result<bool, GenieError> {
    if !(1..=BOX_POSITIONS).contains(&count) {
        return Ok(false);
    }
    let side = max.x() - min.x();
    let total = side * BOX_SIDES;
    let n = count as f32;
    let units = n * spacing.get();
    let mut step = total / n;
    if total < units {
        step = units / n;
        let grow = (units * BOX_SIDE_SHARE - side) * BOX_HALF;
        *min = min.with_x(min.x() - grow).with_y(min.y() - grow);
        *max = max.with_x(max.x() + grow).with_y(max.y() + grow);
    }
    let mid_z = (min.y() + max.y()) * BOX_HALF;
    let mid_x = (max.x() + min.x()) * BOX_HALF;
    let fixed: &[(f32, f32)] = match count {
        1 => &[(max.x(), mid_z)],
        2 => &[(max.x(), mid_z), (min.x(), mid_z)],
        3 => &[(max.x(), min.y()), (max.x(), max.y()), (min.x(), mid_z)],
        4 => &[
            (max.x(), min.y()),
            (max.x(), max.y()),
            (min.x(), min.y()),
            (min.x(), max.y()),
        ],
        5 => &[
            (max.x(), min.y()),
            (max.x(), mid_z),
            (max.x(), max.y()),
            (min.x(), min.y()),
            (min.x(), max.y()),
        ],
        6 => &[
            (max.x(), min.y()),
            (max.x(), mid_z),
            (max.x(), max.y()),
            (min.x(), min.y()),
            (min.x(), mid_z),
            (min.x(), max.y()),
        ],
        7 => &[
            (max.x(), min.y()),
            (max.x(), mid_z),
            (max.x(), max.y()),
            (mid_x, min.y()),
            (mid_x, max.y()),
            (min.x(), min.y()),
            (min.x(), max.y()),
        ],
        8 => &[
            (max.x(), min.y()),
            (max.x(), mid_z),
            (max.x(), max.y()),
            (mid_x, min.y()),
            (mid_x, max.y()),
            (min.x(), min.y()),
            (min.x(), mid_z),
            (min.x(), max.y()),
        ],
        _ => &[],
    };
    if !fixed.is_empty() {
        for (p, &(x, z)) in out.iter_mut().zip(fixed) {
            *p = p.with_x(x).with_y(z);
        }
        return Ok(true);
    }
    let first = out.checked_mut(0)?;
    *first = first.with_x(max.x()).with_y(min.y());
    let mut corner = 0.0f32;
    let mut state = 0;
    for i in 1..count {
        let prev = *out.checked(i - 1)?;
        let (x, z) = match state {
            0 => {
                let z = prev.y() + step;
                // COMISS/JC (0x14102b17e): an unordered place stays on this side
                if fmath::below_or_unordered(z, max.y()) {
                    (max.x(), z)
                } else {
                    state = 1;
                    corner = max.y() - (step - (max.y() - prev.y()));
                    (corner, max.y())
                }
            }
            1 => {
                let x = prev.x() - step;
                // COMISS/JC (0x14102b132)
                if fmath::below_or_unordered(min.x(), x) {
                    (x, max.y())
                } else {
                    state = 2;
                    (min.x(), max.y() - (step - (prev.x() - min.x())))
                }
            }
            2 => {
                let z = prev.y() - step;
                // COMISS/JC (0x14102b0f1)
                if fmath::below_or_unordered(min.y(), z) {
                    (min.x(), z)
                } else {
                    state = 3;
                    (corner, min.y())
                }
            }
            3 => {
                let mut x = prev.x() - step;
                if x <= min.x() {
                    x = max.x();
                    state = 4;
                }
                (x, min.y())
            }
            _ => {
                *out.checked_mut(i)? = *out.checked_mut(0)?;
                continue;
            }
        };
        let o = out.checked_mut(i)?;
        *o = o.with_x(x).with_y(z);
    }
    Ok(true)
}

/// `CreateLines` (`FUN_1410143a0`): the slots in use (in order) form lines by their offsets on x,
/// a slot joining the first line whose x is within [`LINE_EPSILON`] of its own, else starting a
/// new one at its x; the lines' array grows to them (`FUN_14102d490`), the lines in use are
/// them, and each line (reset, [`reset_line`]) holds its slots at its index as depth.
pub fn create_lines(formation: &mut Formation) -> Result<(), GenieError> {
    let mut xs: Vec<f32> = Vec::new();
    let mut members: Vec<Vec<u32>> = Vec::new();
    for (i, s) in formation.slots.iter().enumerate() {
        let x = s.offset.x();
        if let Some(j) = xs.iter().position(|&lx| (lx - x).abs() < LINE_EPSILON) {
            members.checked_mut(j)?.push(i as u32);
        } else {
            xs.push(x);
            members.push(vec![i as u32]);
        }
    }
    let lines = xs.len();
    formation.resize_lines(lines);
    formation.number_lines = lines as i32;
    for (k, slots) in members.iter().enumerate() {
        let line = formation.lines.checked_mut(k)?;
        clear_line(line);
        let _ = reset_line(line, slots.len() as i32, k as i32);
        for (m, &slot) in slots.iter().enumerate() {
            if m < line.indices.len() {
                *line.indices.checked_mut(m)? = slot;
            }
        }
    }
    Ok(())
}

/// The slots of `createBox` (`FUN_141011a00`, [`World::create_box`]), up to its
/// transform: false without members. Every slot turns
/// inactive and none is in use; then category by category from 5 down to 0, the first unit type
/// statistic of the category with units puts that many slots on a box ([`place_box_perimeter`]
/// into the box's positions buffer, `DAT_143f15f00`, whose heights stay 0): the first such
/// category on the box of half the spacing each way (a lone unit in the middle, the box then a
/// point), each later one on a box a spacing wider each way than the last; the slots' array
/// grows to hold them, each slot takes no unit, its place as offset, the world's time, active
/// and the category. Then no lines, a box (4), transform type 1, the box's width on x as its
/// width and depth (truncated), and the lines of its slots ([`create_lines`]).
///
/// # Errors
///
/// A category of more than [`BOX_POSITIONS`] units: the perimeter refuses it and the game's
/// slots read the buffer's earlier places and the memory past it.
pub(crate) fn box_slots(
    formation: &mut Formation,
    group: &UnitGroup,
    world_time: WorldTime,
) -> Result<bool, GenieError> {
    if group.members.is_empty() {
        return Ok(false);
    }
    let s = group.desired_spacing.get();
    let mut max_x = s * BOX_HALF;
    let mut min_x = (-s) * BOX_HALF;
    let mut min_z = min_x;
    let mut max_z = max_x;
    for slot in formation.every_slot_mut() {
        slot.active = 0;
    }
    formation.use_slots(0);
    let mut positions = [BVector::ZERO; BOX_POSITIONS];
    let mut used = 0usize;
    for c in FormationCategory::EACH.into_iter().rev() {
        let Some(stat) = group
            .unit_types
            .iter()
            .find(|u| u.formation_category == c)
            .filter(|u| !u.units.is_empty())
        else {
            continue;
        };
        let count = stat.units.len();
        if BOX_POSITIONS < count {
            return Err(GenieError::unsupported(
                "a box side of more than 60 units (FUN_14102ab30 refuses it, the slots read past \
                 the buffer DAT_143f15f00)",
            ));
        }
        formation.resize_slots(used + count);
        if used == 0 && count == 1 {
            positions[0] = BVector::ZERO;
            (min_x, min_z, max_x, max_z) = (0.0, 0.0, 0.0, 0.0);
        } else {
            let (mut min, mut max) = if used == 0 {
                let low = (-s) * BOX_HALF;
                let high = s * BOX_HALF;
                (BVector::new(low, 0.0, low), BVector::new(high, 0.0, high))
            } else {
                (
                    BVector::new(min_x - s, 0.0, min_z - s),
                    BVector::new(max_x + s, 0.0, max_z + s),
                )
            };
            // the game ignores its result: false only for a count refused above
            let _ =
                place_box_perimeter(count, &mut positions, &mut min, &mut max, Distance::new(s));
            (min_x, min_z, max_x, max_z) = (min.x(), min.y(), max.x(), max.y());
        }
        for (j, &p) in positions.iter().enumerate().take(count) {
            let at = used + j;
            *formation.slot_mut(at)? = Slot {
                unit_id: None,
                offset: p,
                desired_transform_position: BVector::ZERO,
                transform_position: BVector::ZERO,
                time: world_time,
                state: SlotState::Steering,
                depth: 0,
                position_filled: 0,
                active: 1,
                desired_category: c,
                desired_sub_category: UnitType::NONE,
                transformed: formation.slot_mut(at)?.transformed,
            };
        }
        used += count;
        formation.use_slots(used);
    }
    formation.number_lines = 0;
    formation.kind = FormationKind::BOX;
    formation.transform_type = TransformType::TURNED;
    // CVTTSS2SI at 0x141012147 (a NaN or out of range width is i32::MIN)
    let width = cvttss2si(max_x - min_x);
    formation.maximum_width = width;
    formation.maximum_depth = width;
    create_lines(formation)?;
    Ok(true)
}

impl World {
    /// `createHorde` (`FUN_141013eb0`) of `formation` for group `g`: false without members.
    /// Each member the world's table holds ([`World::world_object`]) gives its
    /// place (slot 0x520) and its sub-formation (`FUN_141011300` with 0,
    /// [`World::unit_category`]) to the slots ([`slots_by_place`]).
    /// `UpdateDesiredPositions` transforms them after.
    ///
    /// # Errors
    ///
    /// A held member of a master the slice does not have.
    pub(crate) fn create_horde(
        &self,
        formation: &mut Formation,
        g: &UnitGroup,
    ) -> Result<bool, GenieError> {
        let members = self.member_places(g)?;
        slots_by_place(formation, g, &members, false, self.world_time)
    }

    /// `createUDF` (`FUN_141014f60`) of `formation` for group `g`, as [`Self::create_horde`]
    /// with a user defined formation's slots ([`slots_by_place`]).
    ///
    /// # Errors
    ///
    /// A held member of a master the slice does not have.
    pub(crate) fn create_udf(
        &self,
        formation: &mut Formation,
        g: &UnitGroup,
    ) -> Result<bool, GenieError> {
        let members = self.member_places(g)?;
        slots_by_place(formation, g, &members, true, self.world_time)
    }

    /// What `createHorde` and `createUDF` read of each member (`FUN_1415417a0`, slot 0x520,
    /// `FUN_141011300` with 0), None for one the world's table does not hold.
    fn member_places(&self, g: &UnitGroup) -> Result<Vec<Option<MemberPlace>>, GenieError> {
        let mut places = Vec::with_capacity(g.members.len());
        for m in &g.members {
            let place = match self.world_object(m.id) {
                Some(object) => Some(MemberPlace {
                    id: m.id,
                    at: object.moved_vector(),
                    category: self.unit_category(Some(object))?,
                }),
                None => None,
            };
            places.push(place);
        }
        Ok(places)
    }

    /// `createPhalanx` (`FUN_1410147b0`) of `formation` for group `g`: false without members.
    /// Its slots ([`phalanx_slots`]), then the transform and the units
    /// ([`World::lay_units`] without the column-to-line flag: unless the transform
    /// took the units anew, every slot of the array loses its unit and its filled mark and the
    /// slots take the closest, `FUN_141009760`).
    ///
    /// # Errors
    ///
    /// As the transform and the units' assignment.
    pub(crate) fn create_phalanx(
        &mut self,
        formation: &mut Formation,
        group: GroupId,
    ) -> Result<bool, GenieError> {
        if !phalanx_slots(formation, self.groups.held(group)?, self.world_time)? {
            return Ok(false);
        }
        self.lay_units(formation, group, false)?;
        Ok(true)
    }

    /// `createBox` (`FUN_141011a00`) of `formation` for group `g`: false without members. Its
    /// slots ([`box_slots`]), then the transform and the units as a phalanx's
    /// ([`World::lay_units`] without the column-to-line flag).
    ///
    /// # Errors
    ///
    /// As [`box_slots`], the transform and the units' assignment.
    pub(crate) fn create_box(
        &mut self,
        formation: &mut Formation,
        group: GroupId,
    ) -> Result<bool, GenieError> {
        if !box_slots(formation, self.groups.held(group)?, self.world_time)? {
            return Ok(false);
        }
        self.lay_units(formation, group, false)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use crate::groups::formation::tests::slot_world;
    use crate::groups::{Member, UnitTypeStatistic};
    use crate::test_ids::group_id;
    use crate::test_ids::object_id;
    use crate::test_ids::object_id_list;

    fn group(members: usize, spacing: f32, stats: &[(i32, usize)]) -> UnitGroup {
        let mut g = UnitGroup::new(group_id(1));
        g.members = (0..members)
            .map(|i| Member {
                id: object_id(i as i32),
                ..Member::default()
            })
            .collect();
        g.desired_spacing = Distance::new(spacing);
        g.motion.position[0] = BVector::new(10.0, 0.0, 10.0);
        g.unit_types = stats
            .iter()
            .map(|&(category, n)| UnitTypeStatistic {
                formation_category: FormationCategory::named(category),
                units: object_id_list(0..n as i32),
                ..UnitTypeStatistic::default()
            })
            .collect();
        g
    }

    const fn place(id: i32, x: f32, z: f32, category: i32) -> MemberPlace {
        MemberPlace {
            id: object_id(id),
            at: BVector::new(x, 0.0, z),
            category: FormationCategory::named(category),
        }
    }

    /// `FUN_141013eb0`: a held member's slot at its index takes it, its offset from the group's
    /// place (10, 0, 10), longer ones cut to 5; an unheld member's slot stays fresh and inactive.
    #[test]
    fn horde_takes_its_members_places_within_reach() {
        let g = group(3, 1.0, &[]);
        let mut f = Formation::new();
        let members = [
            Some(place(7, 12.0, 10.0, 1)),
            None,
            Some(place(9, 20.0, 10.0, 3)),
        ];
        assert!(slots_by_place(&mut f, &g, &members, false, WorldTime::from_millis(640)).unwrap());
        assert_eq!(
            (
                f.kind,
                f.transform_type,
                f.maximum_width,
                f.maximum_depth,
                f.number_lines
            ),
            (FormationKind::HORDE, TransformType::UNTURNED, -1, -1, 0)
        );
        assert_eq!(f.slots.len(), 3);
        let s = &f.slots[0];
        assert_eq!(
            (s.unit_id, s.offset, s.desired_transform_position),
            (
                Some(object_id(7)),
                BVector::new(2.0, 0.0, 0.0),
                BVector::new(12.0, 0.0, 10.0)
            )
        );
        assert_eq!(s.transform_position, BVector::new(12.0, 0.0, 10.0));
        assert_eq!(
            (
                s.transformed,
                s.time,
                s.position_filled,
                s.active,
                s.desired_category.get()
            ),
            (1, WorldTime::from_millis(640), 1, 1, 1)
        );
        assert_eq!(s.desired_sub_category, UnitType::NONE);
        assert_eq!((f.slots[1].unit_id, f.slots[1].active), (None, 0));
        assert_eq!(f.slots[2].offset, BVector::new(5.0, 0.0, 0.0));
        // no members, no layout
        let mut f = Formation::new();
        assert!(!slots_by_place(&mut f, &group(0, 1.0, &[]), &[], false, WorldTime::ZERO).unwrap());
        assert_eq!(f, Formation::new());
    }

    /// `FUN_141014f60`: the offsets on x move back by the farthest ahead (10) plus 0.5, the
    /// unheld member's slot too, then are cut to 5.
    #[test]
    fn user_defined_formation_moves_back_behind_its_farthest_member() {
        let g = group(3, 1.0, &[]);
        let mut f = Formation::new();
        let members = [
            Some(place(7, 12.0, 10.0, 1)),
            None,
            Some(place(9, 20.0, 10.0, 3)),
        ];
        assert!(slots_by_place(&mut f, &g, &members, true, WorldTime::from_millis(640)).unwrap());
        assert_eq!(
            (f.kind, f.transform_type),
            (FormationKind::USER_DEFINED, TransformType::TURNED)
        );
        let offsets: Vec<BVector> = f.slots.iter().map(|s| s.offset).collect();
        assert_eq!(
            offsets,
            [
                BVector::new(-5.0, 0.0, 0.0),
                BVector::new(-5.0, 0.0, 0.0),
                BVector::new(-0.5, 0.0, 0.0)
            ]
        );
    }

    /// `FUN_1410147b0` for 4 members of spacing 2: lines of 1, 2 and the rest (1); an even line
    /// starts half a step forward; the slots take category 1's three units, then category 3's.
    /// `MAXSS` at 0x141015208 skips a NaN offset in the search for the farthest slot ahead.
    #[test]
    fn user_defined_formation_search_ahead_skips_nan_offset() {
        let g = group(2, 1.0, &[]);
        let mut f = Formation::new();
        let members = [
            Some(place(9, 20.0, 10.0, 3)),
            Some(place(7, f32::NAN, 10.0, 1)),
        ];
        assert!(slots_by_place(&mut f, &g, &members, true, WorldTime::from_millis(640)).unwrap());
        // the farthest ahead stays finite, so the finite member's offset moves back by it
        assert!(f.slots[0].offset.x().is_finite(), "{:?}", f.slots[0].offset);
    }

    /// `FUN_14102ab30`'s walk tests are `COMISS`/`JC` (0x14102b17e, 0x14102b132, 0x14102b0f1):
    /// an unordered place keeps walking the side it is on. An infinite spacing grows the box to
    /// infinity, and the first step's `-inf + inf` stays on the first side.
    #[test]
    fn box_walk_on_nan_place_stays_on_its_side() {
        let mut out = [BVector::ZERO; BOX_POSITIONS];
        let (mut min, mut max) = (BVector::new(-1.0, 0.0, -1.0), BVector::new(1.0, 0.0, 1.0));
        assert!(
            place_box_perimeter(
                9,
                &mut out,
                &mut min,
                &mut max,
                Distance::new(f32::INFINITY)
            )
            .unwrap()
        );
        for p in &out[1..9] {
            assert_eq!(p.x(), f32::INFINITY, "{out:?}");
            assert!(p.y().is_nan(), "{out:?}");
        }
    }

    /// `FUN_141008f50`: a line of no width or of a negative depth is left as it is (false);
    /// else its places are -1, its depth set, its attack range 0 (true). Its callers' clear
    /// before it keeps only the distance back.
    #[test]
    fn line_reset_needs_width_and_depth_not_below_0() {
        let line = FormationLine {
            depth: 2,
            indices: vec![7],
            attack_range: Distance::new(1.5),
            distance_back: Distance::new(4.0),
        };
        let mut l = line.clone();
        assert!(!reset_line(&mut l, 0, 1));
        assert_eq!(l, line);
        assert!(!reset_line(&mut l, 2, -1));
        assert_eq!(l, line);
        assert!(reset_line(&mut l, 2, 3));
        assert_eq!(
            l,
            FormationLine {
                depth: 3,
                indices: vec![u32::MAX; 2],
                attack_range: Distance::ZERO,
                distance_back: Distance::new(4.0),
            }
        );
        let mut l = line;
        clear_line(&mut l);
        assert_eq!(
            l,
            FormationLine {
                distance_back: Distance::new(4.0),
                ..FormationLine::default()
            }
        );
    }

    /// Group 1 of `slot_world`'s units 10, 11 (category 1) and 12 (category 3) at (5.5, 5.5),
    /// (6.5, 5.5) and (5.5, 6.5), their statistics, the group at (6, 6), spacing 1.
    fn three_at_six() -> World {
        let mut t = slot_world(
            &[(10, 4, 5.5, 5.5), (11, 4, 6.5, 5.5), (12, 6, 5.5, 6.5)],
            &[],
        );
        let g = &mut t.groups.groups[0];
        g.desired_spacing = Distance::new(1.0);
        g.motion.position[0] = BVector::new(6.0, 0.0, 6.0);
        g.unit_types = vec![
            UnitTypeStatistic {
                formation_category: FormationCategory::named(1),
                units: vec![object_id(10), object_id(11)],
                desired_spacing: Distance::new(1.0),
                ..UnitTypeStatistic::default()
            },
            UnitTypeStatistic {
                formation_category: FormationCategory::named(3),
                units: vec![object_id(12)],
                desired_spacing: Distance::new(1.0),
                ..UnitTypeStatistic::default()
            },
        ];
        t
    }

    /// `FUN_1410147b0` and `FUN_141011a00` lay their slots, then transform them and take the
    /// units: unless the transform took them, every slot of the array (a spare one too) loses
    /// its unit and filled mark, and the slots take the closest (`FUN_141009760`). Each is its
    /// slots and `lay_units` without the column-to-line flag.
    #[test]
    fn phalanx_and_box_take_their_units_after_their_slots() {
        let mut t = three_at_six();
        let g = t.groups.get(group_id(1)).unwrap().clone();
        let stale = Slot {
            unit_id: Some(object_id(99)),
            position_filled: 1,
            ..Slot::default()
        };
        let mut start = Formation::new();
        start.spare_slots = vec![stale; 9];
        let mut whole = start.clone();
        assert!(t.create_phalanx(&mut whole, group_id(1)).unwrap());
        let mut parts = start.clone();
        assert!(phalanx_slots(&mut parts, &g, t.world_time).unwrap());
        t.lay_units(&mut parts, group_id(1), false).unwrap();
        assert_eq!(whole, parts);
        let mut ids: Vec<Option<ObjectId>> = whole.slots.iter().map(|s| s.unit_id).collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            [
                Some(object_id(10)),
                Some(object_id(11)),
                Some(object_id(12))
            ]
        );
        assert!(
            whole
                .spare_slots
                .iter()
                .all(|s| (s.unit_id, s.position_filled) == (None, 0))
        );
        let mut whole = start.clone();
        assert!(t.create_box(&mut whole, group_id(1)).unwrap());
        let mut parts = start;
        assert!(box_slots(&mut parts, &g, t.world_time).unwrap());
        t.lay_units(&mut parts, group_id(1), false).unwrap();
        assert_eq!(whole, parts);
        assert!(whole.slots.iter().all(|s| s.unit_id.is_some()));
        // without members, nothing
        let mut none = Formation::new();
        t.groups.groups[0] = group(0, 1.0, &[]);
        assert!(!t.create_box(&mut none, group_id(1)).unwrap());
        assert!(!t.create_phalanx(&mut none, group_id(1)).unwrap());
    }

    /// `FUN_141013eb0` and `FUN_141014f60` look their members up themselves: a held member's
    /// slot takes its place (slot 0x520) and sub-formation (`FUN_141011300` with 0); the slot
    /// of a member the world's table does not hold stays fresh and inactive.
    #[test]
    fn horde_looks_its_members_up_in_the_world() {
        let mut t = three_at_six();
        t.groups.groups[0].members.insert(
            1,
            Member {
                id: object_id(50),
                ..Member::default()
            },
        );
        let g = t.groups.get(group_id(1)).unwrap().clone();
        let mut f = Formation::new();
        assert!(t.create_horde(&mut f, &g).unwrap());
        let slots: Vec<(Option<ObjectId>, BVector, i32, u32)> = f
            .slots
            .iter()
            .map(|s| {
                (
                    s.unit_id,
                    s.transform_position,
                    s.desired_category.get(),
                    s.active,
                )
            })
            .collect();
        assert_eq!(
            slots,
            [
                (Some(object_id(10)), BVector::new(5.5, 0.0, 5.5), 1, 1),
                (None, BVector::ZERO, -1, 0),
                (Some(object_id(11)), BVector::new(6.5, 0.0, 5.5), 1, 1),
                (Some(object_id(12)), BVector::new(5.5, 0.0, 6.5), 3, 1),
            ]
        );
        assert_eq!(
            (f.kind, f.transform_type),
            (FormationKind::HORDE, TransformType::UNTURNED)
        );
        let mut f = Formation::new();
        assert!(t.create_udf(&mut f, &g).unwrap());
        assert_eq!(
            (f.kind, f.transform_type, f.slots[3].unit_id),
            (
                FormationKind::USER_DEFINED,
                TransformType::TURNED,
                Some(object_id(12))
            )
        );
    }

    /// `FUN_141011a00` truncates the box's width by `CVTTSS2SI` (0x141012147): a NaN spacing
    /// gives `i32::MIN`, where Rust's `as` would give 0.
    #[test]
    fn box_of_nan_spacing_is_i32_min_wide() {
        let mut f = Formation::new();
        assert!(box_slots(&mut f, &group(2, f32::NAN, &[(1, 2)]), WorldTime::ZERO).unwrap());
        assert_eq!((f.maximum_width, f.maximum_depth), (i32::MIN, i32::MIN));
    }

    #[test]
    fn phalanx_grows_by_slot_line() {
        let g = group(4, 2.0, &[(3, 1), (1, 3)]);
        let mut f = Formation::new();
        assert!(phalanx_slots(&mut f, &g, WorldTime::from_millis(99)).unwrap());
        assert_eq!(
            (
                f.kind,
                f.transform_type,
                f.maximum_width,
                f.maximum_depth,
                f.number_lines
            ),
            (FormationKind::PHALANX, TransformType::BY_LINES, 3, 3, 3)
        );
        let slots: Vec<(BVector, i32, i32)> = f
            .slots
            .iter()
            .map(|s| (s.offset, s.depth, s.desired_category.get()))
            .collect();
        assert_eq!(
            slots,
            [
                (BVector::new(0.0, 0.0, 0.0), 0, 1),
                (BVector::new(0.0, 0.0, -1.0), 1, 1),
                (BVector::new(0.0, 0.0, 1.0), 1, 1),
                (BVector::new(0.0, 0.0, 0.0), 2, 3),
            ]
        );
        let lines: Vec<(i32, Vec<u32>)> = f
            .lines
            .iter()
            .map(|l| (l.depth, l.indices.clone()))
            .collect();
        assert_eq!(lines, [(0, vec![0]), (1, vec![1, 2]), (2, vec![3])]);
        assert!(
            f.slots
                .iter()
                .all(|s| (s.unit_id, s.active, s.time) == (None, 1, WorldTime::from_millis(99)))
        );
        // without statistics the slots take no category
        let mut f = Formation::new();
        assert!(phalanx_slots(&mut f, &group(2, 2.0, &[]), WorldTime::ZERO).unwrap());
        assert!(
            f.slots
                .iter()
                .all(|s| s.desired_category == FormationCategory::named(-1))
        );
    }

    /// `FUN_14102ab30`'s fixed places for 3 units, a box too small for 2 units of spacing 10
    /// (grown by 1.5 each way), and the walk of 16 around a box 4 wide and 2 deep: up the side
    /// at max x, the first corner's x is max z less the step's rest (1), back along max z and
    /// down min x, then from that corner's x along min z until past min x, then the first place.
    #[test]
    fn the_box_perimeter_places_its_units_as_the_game_walks_it() {
        let mut out = [BVector::ZERO; BOX_POSITIONS];
        let (mut min, mut max) = (BVector::new(-1.0, 0.0, -1.0), BVector::new(1.0, 0.0, 1.0));
        assert!(place_box_perimeter(3, &mut out, &mut min, &mut max, Distance::new(1.0)).unwrap());
        assert_eq!(
            out[..3],
            [
                BVector::new(1.0, 0.0, -1.0),
                BVector::new(1.0, 0.0, 1.0),
                BVector::new(-1.0, 0.0, 0.0)
            ]
        );
        assert!(place_box_perimeter(2, &mut out, &mut min, &mut max, Distance::new(10.0)).unwrap());
        assert_eq!(
            (min, max),
            (BVector::new(-2.5, 0.0, -2.5), BVector::new(2.5, 0.0, 2.5))
        );
        assert_eq!(
            out[..2],
            [BVector::new(2.5, 0.0, 0.0), BVector::new(-2.5, 0.0, 0.0)]
        );
        let (mut min, mut max) = (BVector::new(-2.0, 0.0, -1.0), BVector::new(2.0, 0.0, 1.0));
        assert!(place_box_perimeter(16, &mut out, &mut min, &mut max, Distance::new(0.5)).unwrap());
        let walked: Vec<(f32, f32)> = out[..16].iter().map(|p| (p.x(), p.y())).collect();
        assert_eq!(
            walked,
            [
                (2.0, -1.0),
                (2.0, 0.0),
                (1.0, 1.0),
                (0.0, 1.0),
                (-1.0, 1.0),
                (-2.0, 1.0),
                (-2.0, 0.0),
                (1.0, -1.0),
                (0.0, -1.0),
                (-1.0, -1.0),
                (2.0, -1.0),
                (2.0, -1.0),
                (2.0, -1.0),
                (2.0, -1.0),
                (2.0, -1.0),
                (2.0, -1.0),
            ]
        );
        // the fixed places of 1 and of 4 to 8 units on a box 2 by 2: its corners at max x and
        // min x, the middles of those sides, then the middles of the sides at min z and max z
        let (hi, lo, mid) = (1.0, -1.0, 0.0);
        let fixed: [&[(f32, f32)]; 6] = [
            &[(hi, mid)],
            &[(hi, lo), (hi, hi), (lo, lo), (lo, hi)],
            &[(hi, lo), (hi, mid), (hi, hi), (lo, lo), (lo, hi)],
            &[(hi, lo), (hi, mid), (hi, hi), (lo, lo), (lo, mid), (lo, hi)],
            &[
                (hi, lo),
                (hi, mid),
                (hi, hi),
                (mid, lo),
                (mid, hi),
                (lo, lo),
                (lo, hi),
            ],
            &[
                (hi, lo),
                (hi, mid),
                (hi, hi),
                (mid, lo),
                (mid, hi),
                (lo, lo),
                (lo, mid),
                (lo, hi),
            ],
        ];
        for places in fixed {
            let n = places.len();
            let (mut min, mut max) = (BVector::new(lo, 0.0, lo), BVector::new(hi, 0.0, hi));
            assert!(
                place_box_perimeter(n, &mut out, &mut min, &mut max, Distance::new(0.1)).unwrap()
            );
            let got: Vec<(f32, f32)> = out[..n].iter().map(|p| (p.x(), p.y())).collect();
            assert_eq!(got, places, "{n}");
        }
        // none, or more than the buffer holds, writes nothing
        assert!(!place_box_perimeter(0, &mut out, &mut min, &mut max, Distance::new(1.0)).unwrap());
        assert!(
            !place_box_perimeter(61, &mut out, &mut min, &mut max, Distance::new(1.0)).unwrap()
        );
    }

    /// `FUN_1410143a0`: slots within 1e-7 on x share a line; a line of an array large enough
    /// keeps its distance back and loses its attack range.
    #[test]
    fn create_lines_groups_the_slots_by_their_offset_on_x() {
        let mut f = Formation::new();
        f.slots = [0.25, -1.0, 0.250_000_06, 3.0]
            .iter()
            .map(|&x| Slot {
                offset: BVector::new(x, 0.0, 0.0),
                ..Slot::default()
            })
            .collect();
        f.lines = vec![
            FormationLine {
                attack_range: Distance::new(4.0),
                distance_back: Distance::new(2.0),
                ..FormationLine::default()
            };
            3
        ];
        create_lines(&mut f).unwrap();
        assert_eq!(f.number_lines, 3);
        let lines: Vec<(i32, Vec<u32>)> = f
            .lines
            .iter()
            .map(|l| (l.depth, l.indices.clone()))
            .collect();
        assert_eq!(lines, [(0, vec![0, 2]), (1, vec![1]), (2, vec![3])]);
        assert_eq!(
            (f.lines[0].attack_range, f.lines[0].distance_back),
            (Distance::ZERO, Distance::new(2.0))
        );
    }

    /// `FUN_141011a00`: category 5's lone unit in the middle, then category 1's three on the box
    /// a spacing (2) wider each way; the box 4 wide; its lines by x.
    #[test]
    fn box_rings_its_categories_from_the_middle_out() {
        let g = group(4, 2.0, &[(1, 3), (5, 1)]);
        let mut f = Formation::new();
        assert!(box_slots(&mut f, &g, WorldTime::from_millis(5)).unwrap());
        assert_eq!(
            (f.kind, f.transform_type, f.maximum_width, f.maximum_depth),
            (FormationKind::BOX, TransformType::TURNED, 4, 4)
        );
        let slots: Vec<(BVector, i32)> = f
            .slots
            .iter()
            .map(|s| (s.offset, s.desired_category.get()))
            .collect();
        assert_eq!(
            slots,
            [
                (BVector::new(0.0, 0.0, 0.0), 5),
                (BVector::new(2.0, 0.0, -2.0), 1),
                (BVector::new(2.0, 0.0, 2.0), 1),
                (BVector::new(-2.0, 0.0, 0.0), 1),
            ]
        );
        let lines: Vec<Vec<u32>> = f.lines.iter().map(|l| l.indices.clone()).collect();
        assert_eq!(lines, [vec![0], vec![1, 2], vec![3]]);
        assert_eq!(f.number_lines, 3);
        // more than the buffer holds of one category
        let g = group(61, 2.0, &[(1, 61)]);
        assert!(box_slots(&mut Formation::new(), &g, WorldTime::from_millis(5)).is_err());
    }
}
