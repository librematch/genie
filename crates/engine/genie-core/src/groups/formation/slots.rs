//! The slots' units: the unit categories, `ClosestUnit`, the assignment by variance, the
//! population in order and a line after a column.

use crate::GenieError;
use crate::groups::FormationCategory;
use crate::groups::FormationKind;
use crate::groups::GroupId;
use crate::groups::UnitType;
use crate::groups::{Formation, UnitGroup, sub_formation};
use crate::map::Distance;
use crate::objects::ObjectId;
use crate::world::slots::CheckedIndex;

/// A line order that lacks a place up to the list's length: the game reads past the order.
const NO_PLACE_IN_ORDER: GenieError =
    GenieError::unreachable("a line order that lacks a place up to the list's length");
use crate::groups::WorldObject;
use crate::math::fmath::sqrt;
use crate::math::fmath::{BVector, sub};
use crate::math::image::group::FARTHEST;
use crate::world::World;

/// The capacity of `ClosestUnit`'s list of the units the slots took (`DAT_143f20710`, 0x118
/// bytes).
pub(super) const USED_UNITS: usize = 70;
/// The feature's assignment's arrays (`FUN_14100a0c0`): 70 members or slots a side
/// (`DAT_143f1fb50`, `DAT_143f1fc70`, 0x118 bytes each) ...
pub(super) const ASSIGNMENT_SIDE: usize = 70;
/// ... the costs and the preferences 70 × 70 (`DAT_143f161e0`, `DAT_143f1ae70`, 0x4c90 bytes
/// each) ...
pub(super) const ASSIGNMENT_CELLS: usize = ASSIGNMENT_SIDE * ASSIGNMENT_SIDE;
/// ... and the taken marks 80 bytes (`DAT_143f1fb00`).
pub(super) const ASSIGNMENT_MARKS: usize = 80;

impl World {
    /// `FUN_141011300` of `object` (`param_1`, as the world's table gives it,
    /// [`World::world_object`]): -1 for none (a null object), else its master's sub-formation
    /// ([`sub_formation`], -1 for none); a ghost's master (type 25) has none.
    ///
    /// The routine's target (`param_2`) enters only the rams' rule: a target of type 80 and a
    /// unit of class 13, of master 1258, 35, 422 or 548 and of a player not a computer
    /// (`FUN_141659cf0`), are 0 while the byte `DAT_143f01837` is 1. The image holds 0 there and
    /// no instruction writes it: its only references are the reads at 0x141011603 (here) and
    /// 0x14102b73d (`readdRams`, `FUN_14102b730`); `FUN_140ecabe0`, which sets the settings'
    /// bytes before it, stops at `DAT_143f01834`. So the rule never holds and the result does
    /// not depend on the target.
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have.
    pub(crate) fn unit_category(
        &self,
        object: Option<WorldObject<'_>>,
    ) -> Result<FormationCategory, GenieError> {
        let Some(object) = object else {
            return Ok(FormationCategory::ANY);
        };
        let master = self.master(object.player(), object.master_id(self.model.slots))?;
        Ok(sub_formation(
            master.kind,
            master.creatable_part().hero_flag,
            master.creatable_part().formation_category,
        )
        .unwrap_or(FormationCategory::ANY))
    }

    /// `ClosestUnit` (`FUN_14100e370`) of group `g` to `point`: of the members the world holds
    /// of sub-formation `category` and master `sub_category` (-1 for any) that `used` does not
    /// hold, the one nearest the point (3D, `FUN_14087e980`, where it last moved to), the first
    /// of equals; none for none.
    ///
    /// # Errors
    ///
    /// A member of a master the slice does not have.
    pub fn closest_unit(
        &self,
        g: &UnitGroup,
        point: BVector,
        category: FormationCategory,
        sub_category: UnitType,
        used: &[Option<ObjectId>],
    ) -> Result<Option<ObjectId>, GenieError> {
        let mut best = FARTHEST;
        let mut closest = None;
        for m in &g.members {
            let Some(object) = self.world_object(m.id) else {
                continue;
            };
            if category != FormationCategory::ANY && self.unit_category(Some(object))? != category {
                continue;
            }
            if !sub_category.is_none()
                && UnitType::of(
                    self.master(object.player(), object.master_id(self.model.slots))?
                        .id,
                ) != sub_category
            {
                continue;
            }
            if used.contains(&Some(m.id)) {
                continue;
            }
            let at = object.moved_vector();
            let d = (point.height() - at.height()) * (point.height() - at.height())
                + (point.x() - at.x()) * (point.x() - at.x())
                + (point.y() - at.y()) * (point.y() - at.y());
            if d < best {
                closest = Some(m.id);
                best = d;
            }
        }
        Ok(closest)
    }

    /// The ground distance from slot place `at` to unit `id`'s (`FUN_14087e560`, its height
    /// zeroed, `sqrt(x² + 0 + y²)`), None for a unit the world does not hold.
    pub(super) fn slot_distance(&self, at: BVector, id: ObjectId) -> Option<Distance> {
        let object = self.world_object(id)?;
        let mut d = sub(at, object.moved_vector());
        d = d.with_height(0.0);
        Some(Distance::new(sqrt(d.x() * d.x() + 0.0 + d.y() * d.y())))
    }

    /// `FUN_141009760` of `formation` for group `g` without the settings' flag `DAT_143f01834`
    /// (the world's saved `mUseFeatureFlags` being 1, which `World::load_world` hands to
    /// `FUN_140ecabe0`; 0 in every recording of this build), with which the game takes the
    /// feature's assignment instead ([`World::assign_slots_by_variance`]; the slice holds
    /// neither the flag nor that routine's process-global costs yet). The transformed active
    /// slots in use take the closest units of their sub-formation and type
    /// ([`World::closest_unit`]), none taken twice. Two orders are measured first, as the sum of
    /// the ground distances (slot to unit) and the count found: the slots in order, and the same
    /// with the first slot's unit of that order barred from the first slot that finds one (whose
    /// unit then takes its place in the list). The second wins unless the first is shorter or
    /// finds more; then the slots take their units in the winner's order and are filled.
    ///
    /// # Errors
    ///
    /// A member of a master the slice does not have.
    pub fn populate_slots(
        &self,
        formation: &mut Formation,
        g: &UnitGroup,
    ) -> Result<(), GenieError> {
        // what the slots ask for (the assignments change only their units)
        let asks: Vec<(BVector, FormationCategory, UnitType)> = formation
            .slots
            .iter()
            .map(|s| {
                (
                    s.transform_position,
                    s.desired_category,
                    s.desired_sub_category,
                )
            })
            .collect();
        let wanted: Vec<usize> = formation
            .slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.transformed != 0 && s.active != 0)
            .map(|(i, _)| i)
            .collect();
        let ask = |i: usize, used: &[Option<ObjectId>]| {
            let (at, category, sub_category) = *asks.checked(i)?;
            self.closest_unit(g, at, category, sub_category, used)
        };
        let dist = |i: usize, id: ObjectId| self.slot_distance(asks.get(i)?.0, id);
        let pick = Picker {
            ask: &ask,
            dist: &dist,
        };
        // the slots in order
        // zeros, object 0's id, as the game's array starts
        let mut used = vec![Some(ObjectId::default()); USED_UNITS];
        let (sum_first, found_first, first) = pick.trial(&wanted, &mut used, (0, false))?;
        // the first unit barred from the first slot that finds one
        let (sum_second, found_second, _) = pick.trial(&wanted, &mut used, (1, true))?;
        if sum_first < sum_second || found_second < found_first {
            pick.fill(formation, &wanted, &mut used, (0, false))?;
        } else {
            *used.checked_mut(0)? = first;
            pick.fill(formation, &wanted, &mut used, (1, true))?;
        }
        Ok(())
    }

    /// The feature's assignment (`FUN_14100a0c0`, [`World::populate_slots`] with the settings'
    /// flag) of `formation`'s slots in use to group `g`'s members, `n` of them:
    ///
    /// - The costs (`costs`, the process global `DAT_143f161e0` of 70 × 70 floats that only this
    ///   routine reads and writes, all 0 as the process starts; grown to that when shorter): for
    ///   each member the world holds ([`World::world_object`]), slot by slot, the squared distance from where it last
    ///   moved to (slot 0x518, as slot 0x520) to the slot's place (`+0x1c`), summed x, height,
    ///   y; [`FARTHEST`] for a slot not transformed or not active, of another sub-formation
    ///   ([`World::unit_category`]) or, before both, of another master. A member's row starts at
    ///   its index times `n`, so with more slots than members a row runs into the next one's,
    ///   which overwrites it; a member the world does not hold leaves its row as it was.
    /// - The preferences: each member's slots by cost, each the cheapest not yet taken (the
    ///   first untaken one, then any strictly cheaper, `COMISS`/`JBE` at 0x14100a47e: a NaN cost
    ///   neither wins nor loses), in rows laid like the costs.
    /// - Each start member in turn takes its first preferred slot not taken, then the next
    ///   member (wrapping), until every slot is taken or a member has none left (a start that
    ///   placed nobody counts for nothing). Its mean is the sum of the costs over `n`, its
    ///   variance the mean of the squared differences over the pairs placed; a variance below
    ///   the best so far (`COMISS`/`JBE` at 0x14100a9be, from [`FARTHEST`]) keeps the start's
    ///   pairs.
    /// - Each kept pair's slot takes its member, filled (`+0x34`); the other slots are left as
    ///   they are, and a member the world does not hold may take one.
    ///
    /// The routine's other arrays are the process's too but are filled before each read: the
    /// preferences for the rows read (`DAT_143f1ae70`), the taken marks (`DAT_143f1fb00`), the
    /// start's pairs (`DAT_143f1fd90`, `DAT_143f1feb0`, -1 each start) and the kept ones
    /// (`DAT_143f1fb50`, `DAT_143f1fc70`, -1 at the start).
    ///
    /// # Errors
    ///
    /// A held member of a master the slice does not have.
    ///
    /// For more than 70 members or slots, which the game's arrays do not hold either: a group
    /// takes fewer than 70 members ([`crate::groups::create`]'s `AddUnit`) and its layouts a slot
    /// each.
    pub fn assign_slots_by_variance(
        &self,
        formation: &mut Formation,
        g: &UnitGroup,
        costs: &mut Vec<f32>,
    ) -> Result<(), GenieError> {
        let n = g.members.len();
        let s = formation.slots.len();
        if costs.len() < ASSIGNMENT_CELLS {
            costs.resize(ASSIGNMENT_CELLS, 0.0);
        }
        self.member_costs(formation, g, costs)?;
        let preferences = slot_preferences(costs, n, s)?;
        let kept = best_assignment(&preferences, costs, (n, s))?;
        for (&r, &c) in kept.0.head(n)?.iter().zip(kept.1.head(n)?) {
            if r == -1 {
                continue;
            }
            let slot = formation.slots.checked_mut(c as usize)?;
            slot.position_filled = 1;
            slot.unit_id = Some(g.members.checked(r as usize)?.id);
        }
        Ok(())
    }

    /// [`World::assign_slots_by_variance`]'s costs: for each member the world holds (rows of `n`
    /// cells) the squared distance to each slot, [`FARTHEST`] for a slot not transformed or
    /// active, of another category or of another master than the member's.
    fn member_costs(
        &self,
        formation: &Formation,
        g: &UnitGroup,
        costs: &mut [f32],
    ) -> Result<(), GenieError> {
        let n = g.members.len();
        let s = formation.slots.len();
        for (i, m) in g.members.iter().enumerate() {
            let Some(object) = self.world_object(m.id) else {
                continue;
            };
            if s == 0 {
                continue;
            }
            let category = self.unit_category(Some(object))?;
            let master = UnitType::of(object.master_id(self.model.slots));
            let at = object.moved_vector();
            for (j, slot) in formation.slots.iter().enumerate() {
                let mut skip = slot.transformed == 0 || slot.active == 0;
                if slot.desired_category != FormationCategory::ANY
                    && category != slot.desired_category
                {
                    skip = true;
                }
                let other_master =
                    !slot.desired_sub_category.is_none() && master != slot.desired_sub_category;
                *costs.checked_mut(i * n + j)? = if other_master || skip {
                    FARTHEST
                } else {
                    let d = sub(at, slot.transform_position);
                    d.x() * d.x() + d.height() * d.height() + d.y() * d.y()
                };
            }
        }
        Ok(())
    }

    /// `populateSlotsWithUnits` (`FUN_14102b230`) of group `g` over the first `n` slots of
    /// `formation` (at most the members), `order` their places in the line order: the members
    /// whose index has a transformed active slot (and that the world holds) form a list in
    /// member order; then, for each `k` of the list, the slot whose place in `order` holds `k`
    /// takes the first unit of the list left that the member of the same index (as the game
    /// reads it) matches in sub-formation and master (-1 any), filled.
    ///
    /// # Errors
    ///
    /// A member of a master the slice does not have.
    ///
    /// An order that lacks a place ([`World::from_previous_column`]'s is a permutation of the
    /// places; the game reads past one that lacks a place).
    pub(super) fn populate_in_order(
        &self,
        formation: &mut Formation,
        g: &UnitGroup,
        n: usize,
        order: &[i32],
    ) -> Result<(), GenieError> {
        let n = n.min(g.members.len());
        let mut list: Vec<Option<ObjectId>> = Vec::new();
        for i in 0..n {
            let s = formation.slot_mut(i)?;
            if s.transformed != 0
                && s.active != 0
                && self.world_object(g.members.checked(i)?.id).is_some()
            {
                list.push(Some(g.members.checked(i)?.id));
            }
        }
        for k in 0..list.len() {
            let at = order
                .iter()
                .position(|&o| o == k as i32)
                .ok_or(NO_PLACE_IN_ORDER)?;
            let (category, sub_category) = {
                let s = formation.slot_mut(at)?;
                (s.desired_category, s.desired_sub_category)
            };
            for (u, taken) in list.iter_mut().enumerate() {
                if taken.is_none() {
                    continue;
                }
                let Some(object) = g.members.get(u).and_then(|m| self.world_object(m.id)) else {
                    continue;
                };
                if category != FormationCategory::ANY
                    && self.unit_category(Some(object))? != category
                {
                    continue;
                }
                if !sub_category.is_none()
                    && UnitType::of(
                        self.master(object.player(), object.master_id(self.model.slots))?
                            .id,
                    ) != sub_category
                {
                    continue;
                }
                let s = formation.slot_mut(at)?;
                s.unit_id = taken.take();
                s.position_filled = 1;
                break;
            }
        }
        Ok(())
    }

    /// `FUN_141009060`: a line (not a column or a box) after a column (`previous`) takes its
    /// units line by line from the middle out: with a whole number of lines transformed (the
    /// transformed slots a multiple of the width), each line's places run from its middle
    /// (left of it for an even width) alternately right and left ([`World::populate_in_order`]
    /// over width times depth); else the slots take the closest units
    /// ([`World::populate_slots`]). Nothing without a previous column.
    ///
    /// # Errors
    ///
    /// As the routines it runs.
    ///
    /// For a line of no width, which the game divides by too (createColumnLine lays a line of a
    /// column at least).
    pub fn from_previous_column(
        &self,
        formation: &mut Formation,
        g: &UnitGroup,
    ) -> Result<(), GenieError> {
        let previous_column = g
            .previous_formation
            .as_ref()
            .is_some_and(|p| p.kind == FormationKind::COLUMN);
        if matches!(formation.kind, FormationKind::COLUMN | FormationKind::BOX) || !previous_column
        {
            return Ok(());
        }
        let width = formation.maximum_width;
        if formation.number_transformed_slots % width != 0 {
            return self.populate_slots(formation, g);
        }
        let depth = formation.maximum_depth;
        let mut order = vec![0i32; (width * depth).max(0) as usize];
        for line in 0..depth {
            let base = (width * line) as usize;
            let mut start = width / 2 + width * line;
            let mut magnitude = -1;
            if width & 1 == 0 {
                magnitude = 1;
                start -= 1;
            }
            *order.checked_mut(base)? = start;
            let mut uses = 0;
            for k in 1..width {
                let v = *order.checked(base)? + magnitude;
                *order.checked_mut(base + k as usize)? = v;
                if (line + 1) * width - 1 < v || v < width * line {
                    break;
                }
                magnitude = -magnitude;
                uses += 1;
                if 1 < uses {
                    uses = 0;
                    magnitude += if magnitude < 1 { -1 } else { 1 };
                }
            }
        }
        self.populate_in_order(formation, g, (depth * width).max(0) as usize, &order)
    }

    /// createColumnLine's end (`FUN_141012280` after its layout), and the phalanx's and the
    /// box's (`after_column` false): the transform ([`World::formation_transform`]); unless it
    /// took the units anew, every slot of the array loses its unit and its filled mark, and the
    /// units take their slots: after a column with slots transformed, the line keeps the
    /// column's order ([`World::from_previous_column`]), else the closest
    /// ([`World::populate_slots`]).
    ///
    /// # Errors
    ///
    /// As the routines it runs.
    pub fn lay_units(
        &mut self,
        formation: &mut Formation,
        group: GroupId,
        after_column: bool,
    ) -> Result<(), GenieError> {
        if self.formation_transform(formation, group, after_column)? {
            return Ok(());
        }
        formation.clear_units();
        let g = self.groups.held(group)?;
        if formation.number_transformed_slots < 1 || g.previous_formation.is_none() || !after_column
        {
            self.populate_slots(formation, g)
        } else {
            self.from_previous_column(formation, g)
        }
    }
}

/// What [`World::populate_slots`] reads: the unit nearest a slot not in `used`, and its distance.
struct Picker<'a> {
    ask: &'a AskUnit<'a>,
    dist: &'a dyn Fn(usize, ObjectId) -> Option<Distance>,
}

/// The unit nearest a slot, not among those taken.
type AskUnit<'a> = dyn Fn(usize, &[Option<ObjectId>]) -> Result<Option<ObjectId>, GenieError> + 'a;

impl Picker<'_> {
    /// One trial over the `wanted` slots, `used` holding the units taken (`count` of them, the
    /// first place kept for the unit of the first slot that finds one while `leading`): the
    /// sum of the distances, how many slots found a unit, and the first unit found.
    fn trial(
        &self,
        wanted: &[usize],
        used: &mut [Option<ObjectId>],
        (mut count, mut leading): (usize, bool),
    ) -> Result<(f32, i32, Option<ObjectId>), GenieError> {
        let (mut sum, mut found, mut first) = (0.0f32, 0, None);
        for &i in wanted {
            let Some(id) = (self.ask)(i, used.head(count)?)? else {
                continue;
            };
            let Some(d) = (self.dist)(i, id) else {
                continue;
            };
            sum += d.get();
            if leading {
                leading = false;
                *used.checked_mut(0)? = Some(id);
            } else {
                *used.checked_mut(count)? = Some(id);
                count += 1;
            }
            if first.is_none() {
                first = Some(id);
            }
            found += 1;
        }
        Ok((sum, found, first))
    }

    /// The final pass: each wanted slot takes its unit (the first place of `used` kept for the
    /// first while `leading`), filled when it found one.
    fn fill(
        &self,
        formation: &mut Formation,
        wanted: &[usize],
        used: &mut [Option<ObjectId>],
        (mut count, mut leading): (usize, bool),
    ) -> Result<(), GenieError> {
        for &i in wanted {
            let id = (self.ask)(i, used.head(count)?)?;
            formation.slots.checked_mut(i)?.unit_id = id;
            if id.is_some() {
                formation.slots.checked_mut(i)?.position_filled = 1;
                if leading {
                    leading = false;
                    *used.checked_mut(0)? = id;
                } else {
                    *used.checked_mut(count)? = id;
                    count += 1;
                }
            }
        }
        Ok(())
    }
}

/// [`World::assign_slots_by_variance`]'s search: from each starting member, the members in turn
/// take their cheapest slot left (`preferences`), `s` pairs at most; the pairs of the least
/// variance win.
fn best_assignment(
    preferences: &[i32],
    costs: &[f32],
    (n, s): (usize, usize),
) -> Result<Pairs, GenieError> {
    let mut taken = [false; ASSIGNMENT_MARKS];
    let mut kept = ([-1i32; ASSIGNMENT_SIDE], [-1i32; ASSIGNMENT_SIDE]);
    let mut best_variance = FARTHEST;
    for start in 0..n {
        if s == 0 {
            continue;
        }
        taken.head_mut(s)?.fill(false);
        let (pairs, sum, placed) = take_turns(start, (preferences, costs), (n, s), &mut taken)?;
        if placed == 0 {
            continue;
        }
        let variance =
            pairs_variance(pairs.0.head(s)?, pairs.1.head(s)?, costs, n, sum / n as f32)?;
        if variance < best_variance {
            best_variance = variance;
            kept.0.head_mut(s)?.copy_from_slice(pairs.0.head(s)?);
            kept.1.head_mut(s)?.copy_from_slice(pairs.1.head(s)?);
        }
    }
    Ok(kept)
}

/// One start of [`best_assignment`]: from member `start` the members in turn take their cheapest
/// slot left in `taken`, until `s` pairs are placed or a member finds none: the pairs, their
/// costs' sum and their count.
fn take_turns(
    start: usize,
    (preferences, costs): (&[i32], &[f32]),
    (n, s): (usize, usize),
    taken: &mut [bool; ASSIGNMENT_MARKS],
) -> Result<(Pairs, f32, usize), GenieError> {
    let mut pairs = ([-1i32; ASSIGNMENT_SIDE], [-1i32; ASSIGNMENT_SIDE]);
    let (mut placed, mut sum, mut member) = (0usize, 0.0f32, start);
    while placed < s {
        let row = member * n;
        let slot = first_free(preferences, taken, row, s)?;
        if slot == -1 {
            break;
        }
        *pairs.0.checked_mut(placed)? = member as i32;
        *pairs.1.checked_mut(placed)? = slot;
        *taken.checked_mut(slot as usize)? = true;
        sum += *costs.checked(row + slot as usize)?;
        member = if member + 1 < n { member + 1 } else { 0 };
        placed += 1;
    }
    Ok((pairs, sum, placed))
}

/// The first of a member's `s` preferred slots (the cells from `row`) not taken, -1 for none.
fn first_free(
    preferences: &[i32],
    taken: &[bool],
    row: usize,
    s: usize,
) -> Result<i32, GenieError> {
    let mut l = 0;
    let mut slot = *preferences.checked(row)?;
    while slot != -1 && *taken.checked(slot as usize)? {
        l += 1;
        if s <= l {
            return Ok(-1);
        }
        slot = *preferences.checked(row + l)?;
    }
    Ok(slot)
}

/// A side of an assignment: the member and the slot of each pair.
type Pairs = ([i32; ASSIGNMENT_SIDE], [i32; ASSIGNMENT_SIDE]);

/// [`World::assign_slots_by_variance`]'s preferences: for each of the `n` members (rows of `n`
/// cells in `costs`) its `s` slots from the cheapest on, the first untaken one winning a tie.
pub(super) fn slot_preferences(costs: &[f32], n: usize, s: usize) -> Result<Vec<i32>, GenieError> {
    let mut preferences = vec![-1i32; ASSIGNMENT_CELLS];
    let mut taken = [false; ASSIGNMENT_MARKS];
    for i in 0..n {
        let row = i * n;
        taken.head_mut(s)?.fill(false);
        for k in 0..s {
            let first = taken.head(s)?.iter().position(|&t| !t).unwrap_or(s);
            let mut best = first;
            for j in first + 1..s {
                if !*taken.checked(j)? && *costs.checked(row + j)? < *costs.checked(row + best)? {
                    best = j;
                }
            }
            *preferences.checked_mut(row + k)? = best as i32;
            *taken.checked_mut(best)? = true;
        }
    }
    Ok(preferences)
}

/// [`World::assign_slots_by_variance`]'s variance of one pairing: of the pairs (`members`,
/// `slots`) with neither −1, the mean squared gap of their costs to `mean` (the sum over the
/// group's size).
pub(super) fn pairs_variance(
    members: &[i32],
    slots: &[i32],
    costs: &[f32],
    n: usize,
    mean: f32,
) -> Result<f32, GenieError> {
    let (mut variance, mut count) = (0.0f32, 0i32);
    for (&r, &c) in members.iter().zip(slots) {
        if r != -1 && c != -1 {
            count += 1;
            let d = *costs.checked(r as usize * n + c as usize)? - mean;
            variance += d * d;
        }
    }
    Ok(variance / count as f32)
}
