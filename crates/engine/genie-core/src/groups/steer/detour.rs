//! The way round a record on the line to the slot: the short pather, the long pather and the
//! tile planner.

use crate::groups::GroupId;
use crate::groups::MovePath;
use crate::groups::MoveReason;
use crate::groups::SlotState;
use crate::groups::group_list::{NO_FORMATION, NO_MOVER, member_gone};
use crate::groups::steer::{
    EPSILON, ROUND_LENGTH, ROUND_RADIUS, ROUND_REACH, ROUND_STILL, ROUND_TIMER,
};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::math::fmath::{BVector, at_most_or_unordered};
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::pathing::ShortFound;
use crate::pathing::tile_path::{Found, Path};
use crate::world::World;
use crate::world::slots::CheckedIndex;
use crate::{GenieError, math::fmath};

impl World {
    /// The way round a record on the line to the slot (`0x141026867`..`0x1410277e5`), for member
    /// `index` (`id`) of group `group` in slot `slot`, `gap` the ground distance to it. A new path
    /// record (`FUN_14152eea0`: flags 6, made at the world's time) takes the first way that
    /// serves, each try emptying its points first; the short pather's radius is the member's on x
    /// times [`ROUND_RADIUS`] and its ignore list `differentiators`; a position test is slot 0x270
    /// ([`Spatial::point_clear_among`], the records grown by the same radius, the same list):
    /// 1. The slot, when the member could stand there: for a unit with a footprint and a height
    ///    (`FUN_141194060`) whose footprint is within [`ROUND_REACH`] of it (`FUN_140a847c0`, the
    ///    squared gap; `COMISS`, `JBE`), the short pather from where it last moved to (slot 0x520);
    ///    a path of fewer than 2 points or no longer than [`EPSILON`] is blocked (3, its points
    ///    emptied), a result but blocked or off the map (3, 4) serves. Otherwise the tile planner
    ///    of its restriction (`FUN_140f7cb30` -> `FUN_140f7c530`: no radii, no range, no unit),
    ///    which marks the path as planned.
    /// 2. Unless reached (1): one tile from where it last moved to toward the slot (the
    ///    direction on the ground, the group's forward when both its axes are within
    ///    [`ROUND_STILL`]), held inside the map by its radius on x and, on y, its height radius
    ///    (`+0x5e0`), when it could stand there: the long pather from its place's tile for a unit
    ///    without a footprint and a height (reached: 1), else the short pather; a path longer than
    ///    [`ROUND_LENGTH`], of fewer than 2 points, no longer than [`EPSILON`] or not reached is
    ///    blocked.
    /// 3. Unless reached or partial: the group's place, when it could stand there, by the short
    ///    pather (as in 1, the result kept only for a path of 2 points longer than [`EPSILON`])
    ///    or the tile planner.
    /// 4. Blocked or off the map: the long pather from where it last moved to toward the slot's
    ///    tile; a path of more than 2 points serves, its last point the goal.
    ///
    /// Any other result (in range, none) or a failed long path releases the record. A served way
    /// that would end farther from the slot than `gap` while the group neither moves nor waits
    /// stops the member with its standing sprite, its failures cleared, the record released and
    /// the slot in state 3. Else slot 0x540 (`FUN_14114e0f0`) gives the member the record's path
    /// as its path data, the walked level reset: as level 0 walking it (state 3,
    /// `FUN_140f9b7c0`), or, planned, as level 2 to plan (state 2, `FUN_140f9b800`), range 0, no
    /// target, its repath time kept; its own move takes the record (reason 0, the goal, timer and
    /// distance [`ROUND_TIMER`], [`World::set_specific_move`]). Either way the slot takes state 0,
    /// the member stops (slot 0x5d0) and its failures count one more. Not held: the record's id
    /// (the own move's `mPathID`, -1 here: the slice holds the path in the member's path data).
    ///
    /// # Errors
    ///
    /// What the pathers, the planner and the position test do not model.
    pub(super) fn way_round(
        &mut self,
        group: GroupId,
        index: usize,
        slot: usize,
        gap: Distance,
        differentiators: &[ObjectId],
    ) -> Result<(), GenieError> {
        let g = self.groups.held(group)?;
        let id = g.members.checked(index)?.id;
        let to = g
            .current_formation
            .as_ref()
            .ok_or(NO_FORMATION)?
            .slots
            .checked(slot)?
            .transform_position;
        let [place, forward, _, _] = g.motion.position;
        let (moving, waits) = (g.motion.moving(), g.motion.waits_to_move());
        let r = self.rounder(id, differentiators)?;
        let (way, serves) = self.find_way(&r, (to, forward, place))?;
        if serves && !moving && !waits && ends_farther(&way.record, to, gap) {
            self.stop_member(id)?;
            self.show_standing(id)?;
            self.move_record(group, index)?.consecutive_move_failures = 0;
            self.set_slot_state(group, slot, SlotState::InPlace)?;
            return Ok(());
        }
        if serves {
            let now = self.world_time;
            if let Some((_, u)) = self.object_mut(id)
                && let Some(m) = u.body.mover_mut()
            {
                m.take_record_path(way.record.points, way.record.length, now, way.planned);
            }
            let goal = (way.goal, ROUND_TIMER, ROUND_TIMER);
            let _ =
                self.set_specific_move(group, id, (MovePath::NONE, MoveReason::DETOUR), goal)?;
        }
        self.set_slot_state(group, slot, SlotState::Steering)?;
        self.stop_member(id)?;
        self.move_record(group, index)?.consecutive_move_failures += 1;
        Ok(())
    }

    /// The member `index` of group `group`'s record of its specific move.
    fn move_record(
        &mut self,
        group: GroupId,
        index: usize,
    ) -> Result<&mut crate::groups::SpecificMove, GenieError> {
        Ok(&mut self
            .groups
            .held_mut(group)?
            .members
            .checked_mut(index)?
            .specific_move)
    }

    /// The way [`World::way_round`] finds for the rounder `r` to the slot's place `to`: round to
    /// the slot, else one step on toward it from `forward`, else to the group's `place`, and
    /// the long way round a blocked one; with whether the way serves.
    fn find_way(
        &mut self,
        r: &Rounder<'_>,
        (to, forward, place): (BVector, BVector, BVector),
    ) -> Result<(Way, bool), GenieError> {
        let mut way = Way {
            record: Path::default(),
            result: ShortFound::Blocked,
            planned: false,
            goal: to,
        };
        self.round_to_slot(r, to, &mut way)?;
        if way.result != ShortFound::Reached {
            self.round_one_step(r, to, forward, &mut way)?;
            if !way.result.leads() {
                self.round_to_place(r, place, &mut way)?;
            }
        }
        let mut serves = way.result.leads();
        if way.result.fails() {
            serves = self.round_long_way(r, to, &mut way)?;
        }
        Ok((way, serves))
    }

    /// What the way round reads of member `id` ([`Rounder`]).
    ///
    /// # Errors
    ///
    /// A member of a master the slice does not have.
    fn rounder<'d>(
        &self,
        id: ObjectId,
        differentiators: &'d [ObjectId],
    ) -> Result<Rounder<'d>, GenieError> {
        let (p, u) = self.object(id).ok_or_else(|| member_gone(id))?;
        let master = u.master(&self.masters, p)?;
        let mover = u.body.mover().ok_or(NO_MOVER)?;
        let grow = master.radius.x() * ROUND_RADIUS;
        Ok(Rounder {
            id,
            walker: crate::movement::Walker {
                radius: Distance::new(grow),
                ..mover.walker(id)
            },
            computer: self.players.get(p.index()).is_some_and(|q| q.computer),
            at: u.moved_vector(),
            here: u.position(),
            mask: mover.stats.restriction_mask,
            restriction: mover.stats.restriction,
            grow: Distance::new(grow),
            radius: master.radius,
            height: master.height,
            solid: mover.stats.radius.sized(),
            differentiators,
        })
    }

    /// Whether the member could stand at `p` (slot 0x270, [`Spatial::point_clear_among`]: the
    /// records grown by the short pather's radius, the differentiators ignored).
    ///
    /// [`Spatial::point_clear_among`]: crate::map::spatial::Spatial::point_clear_among
    fn round_clear(&mut self, r: &Rounder<'_>, p: BVector) -> Result<bool, GenieError> {
        self.spatial
            .point_clear_among(r.id, p.ground(), r.grow, r.mask, r.differentiators)
    }

    /// The way round's first try ([`World::way_round`]'s 1): to the slot `to` when the member
    /// could stand there, by the short pather when its footprint is near (a path of fewer than
    /// 2 points or no longer than [`EPSILON`] keeps no points and its result is not taken; a
    /// result but blocked or off the map serves), else by the tile planner (planned).
    fn round_to_slot(
        &mut self,
        r: &Rounder<'_>,
        to: BVector,
        way: &mut Way,
    ) -> Result<(), GenieError> {
        if !self.round_clear(r, to)? {
            return Ok(());
        }
        let mut served = false;
        if r.solid && r.near(to.x(), to.y()) {
            let from = r.at.ground();
            let res = self.round_short(
                r.walker,
                r.computer,
                from,
                to,
                r.differentiators,
                &mut way.record,
            )?;
            if way.record.points.len() < 2 || at_most_or_unordered(way.record.length.get(), EPSILON)
            {
                way.record.points.clear();
            } else {
                way.result = res;
                served = !res.fails();
            }
        }
        if !served
            && let Some(found) =
                self.round_plan(r.restriction, r.at.ground(), to, &mut way.record)?
        {
            way.result = planned_as_short(found);
            way.planned = true;
        }
        Ok(())
    }

    /// The way round's second try ([`World::way_round`]'s 2): one tile toward the slot
    /// ([`one_step_toward`]), the goal, when the member could stand there: the long pather from
    /// its place's tile for a member without a footprint and a height (found: reached, its path as
    /// it is), else the short pather; a path not reached, longer than [`ROUND_LENGTH`], of fewer
    /// than 2 points or no longer than [`EPSILON`] is blocked, its points emptied.
    fn round_one_step(
        &mut self,
        r: &Rounder<'_>,
        to: BVector,
        forward: BVector,
        way: &mut Way,
    ) -> Result<(), GenieError> {
        let step = one_step_toward(r, to, forward, self.spatial.map);
        way.goal = step;
        if !self.round_clear(r, step)? {
            return Ok(());
        }
        way.record.points.clear();
        let mut reached = false;
        if r.solid {
            let from = r.at.ground();
            way.result = self.round_short(
                r.walker,
                r.computer,
                from,
                step,
                r.differentiators,
                &mut way.record,
            )?;
        } else {
            let (found, path) = self.round_long(r.id, r.here.ground(), step)?;
            way.record.points = path.points;
            // a long path found is measured once and taken as it is
            way.record.measure();
            if found {
                way.result = ShortFound::Reached;
                reached = true;
            }
        }
        // COMISS with JA (a NaN length passes), then JNC
        if !reached
            && (way.result != ShortFound::Reached
                || ROUND_LENGTH < way.record.length
                || way.record.points.len() < 2
                || at_most_or_unordered(way.record.length.get(), EPSILON))
        {
            way.result = ShortFound::Blocked;
            way.record.points.clear();
        }
        Ok(())
    }

    /// The way round's third try ([`World::way_round`]'s 3): to the group's place, the goal,
    /// when the member could stand there: the short pather when its footprint is near (a path
    /// of fewer than 2 points or no longer than [`EPSILON`] is blocked), else the tile planner
    /// (planned).
    fn round_to_place(
        &mut self,
        r: &Rounder<'_>,
        place: BVector,
        way: &mut Way,
    ) -> Result<(), GenieError> {
        way.goal = place;
        if !self.round_clear(r, place)? {
            return Ok(());
        }
        way.record.points.clear();
        if r.solid && r.near(place.x(), place.y()) {
            let from = r.at.ground();
            way.result = self.round_short(
                r.walker,
                r.computer,
                from,
                place,
                r.differentiators,
                &mut way.record,
            )?;
            if way.record.points.len() < 2 || at_most_or_unordered(way.record.length.get(), EPSILON)
            {
                way.result = ShortFound::Blocked;
                way.record.points.clear();
            }
        } else if let Some(found) =
            self.round_plan(r.restriction, r.at.ground(), place, &mut way.record)?
        {
            way.result = planned_as_short(found);
            way.planned = true;
        }
        Ok(())
    }

    /// The way round's last try ([`World::way_round`]'s 4, blocked or off the map): the long
    /// pather from where the member last moved to toward the slot's tile; a path of more than 2
    /// points serves, its last point the goal. Whether it serves.
    fn round_long_way(
        &self,
        r: &Rounder<'_>,
        to: BVector,
        way: &mut Way,
    ) -> Result<bool, GenieError> {
        way.record.points.clear();
        let (_, path) = self.round_long(r.id, r.at.ground(), to)?;
        way.record.points = path.points;
        if 2 < way.record.points.len()
            && let Some(&last) = way.record.points.last()
        {
            way.goal = last;
            return Ok(true);
        }
        Ok(false)
    }

    /// The way round's short pather ([`World::way_round`], `FUN_140f9e540` with the record) for
    /// `walker` (of a `computer` player or not) from `from` to `to`, range 0, no target, `ignore`
    /// its ignore list; the record measured after (`FUN_140f95a60`). Its result.
    pub(super) fn round_short(
        &mut self,
        walker: crate::movement::Walker,
        computer: bool,
        from: Point,
        to: BVector,
        ignore: &[ObjectId],
        record: &mut Path,
    ) -> Result<ShortFound, GenieError> {
        let mut plan = crate::movement::Planning {
            pathing: &mut self.pathing,
            now: self.world_time,
            computer,
            object: self.streams.get(crate::math::rng::Stream::Object),
            groups: &self.groups,
        };
        let r = crate::movement::short_path_for(
            walker,
            from,
            &mut self.spatial,
            &mut plan,
            (to, Distance::ZERO, None),
            ignore,
            record,
        )?;
        record.measure();
        Ok(r)
    }

    /// The way round's long pather (`FUN_140f9eb70` with the record) for member `id` from the
    /// tile of `from` to the tile of `to`, range 0, no target: whether it reached the goal, and
    /// the path.
    pub(super) fn round_long(
        &self,
        id: ObjectId,
        from: Point,
        to: BVector,
    ) -> Result<(bool, Path), GenieError> {
        let (p, u) = self.object(id).ok_or_else(|| member_gone(id))?;
        let mover = u.body.mover().ok_or(NO_MOVER)?;
        let computer = self.players.get(p.index()).is_some_and(|q| q.computer);
        let walker = crate::movement::LongWalker {
            id,
            position: u.position(),
            radius: mover.stats.radius.ground(),
            mask: mover.stats.restriction_mask,
            moves: mover.stats.speed != Speed::ZERO
                || u.life.master_id == crate::movement::LONG_PATH_MOVER,
            ignore_units: &mover.ignore_units,
            group_units: self.groups.group_units(u.group.id),
        };
        crate::movement::long_path_for(&walker, from, &self.spatial, computer, (to, 0.0), None)
    }

    /// The way round's tile planner (`FUN_140f7cb30` -> `FUN_140f7c530`): the planner of
    /// `restriction` from `from` to `to` with no radii, no range and no unit, appending to the
    /// record; its result, None without a planner.
    pub(super) fn round_plan(
        &mut self,
        restriction: crate::map::RestrictionId,
        from: Point,
        to: BVector,
        record: &mut Path,
    ) -> Result<Option<Found>, GenieError> {
        let tiles = self.spatial.tiles();
        let Some(planner) = self.pathing.planner(restriction, tiles) else {
            return Ok(None);
        };
        record.points.clear();
        let req = crate::pathing::tile_path::Request {
            from,
            to: to.ground(),
            radii: 0.0,
            range: Distance::ZERO,
            closed: Vec::new(),
            full: false,
        };
        let found = planner.plan::<GenieError>(&req, false, record, None)?;
        Ok(Some(found))
    }
}

/// What the way round reads of the member (`way_round`'s locals): its id, the short pather's
/// walker (its radius on x times [`ROUND_RADIUS`]) of a computer player or not, where it last
/// moved to (slot 0x520) and its place, its restriction and mask, the radius the position tests
/// grow the records by, its master's radii and height radius (`+0x5e0`), whether it has a
/// footprint and a height (`FUN_141194060`) and the ignore list.
pub(super) struct Rounder<'d> {
    pub(super) id: ObjectId,
    pub(super) walker: crate::movement::Walker,
    pub(super) computer: bool,
    pub(super) at: BVector,
    pub(super) here: Position,
    pub(super) mask: crate::map::TerrainMask,
    pub(super) restriction: crate::map::RestrictionId,
    pub(super) grow: Distance,
    pub(super) radius: Footprint,
    pub(super) height: Distance,
    pub(super) solid: bool,
    pub(super) differentiators: &'d [ObjectId],
}

impl Rounder<'_> {
    /// `FUN_140a847c0`: the squared gap from the member's footprint (its place) to (`x`, `y`)
    /// is below [`ROUND_REACH`].
    pub(super) fn near(&self, x: f32, y: f32) -> bool {
        let gx = ((x - self.here.x()).abs() - self.radius.x()).max(0.0);
        let gy = ((y - self.here.y()).abs() - self.radius.y()).max(0.0);
        gy * gy + gx * gx < ROUND_REACH
    }
}

/// The tile planner's result as the way round keeps it, in the short pather's integer, read by
/// its code: the goal (1) as reached, the nearest node (2) as partial, nothing (3) as blocked
/// and off the map (0) as in range.
pub(super) const fn planned_as_short(found: Found) -> ShortFound {
    match found {
        Found::OffMap => ShortFound::InRange,
        Found::Goal => ShortFound::Reached,
        Found::Nearest => ShortFound::Partial,
        Found::Nothing => ShortFound::Blocked,
    }
}

/// The way round found so far: the record's path (a new path record, `FUN_14152eea0`), the
/// pather's result (the tile planner's read as the short pather's, [`planned_as_short`]),
/// whether the tile planner made it, and where the member goes.
struct Way {
    record: Path,
    result: ShortFound,
    planned: bool,
    goal: BVector,
}

/// One tile from where the member last moved to toward `to` (the direction on the ground; the
/// group's `forward` when both its axes are within [`ROUND_STILL`]), held inside the map (`map`)
/// by the member's radius on x and, on y, its height radius.
pub(super) fn one_step_toward(
    r: &Rounder<'_>,
    to: BVector,
    forward: BVector,
    map: GridSize,
) -> BVector {
    let d = fmath::sub(to, r.at);
    let mut dir = if d.x().abs() <= ROUND_STILL && d.y().abs() <= ROUND_STILL {
        forward
    } else {
        d
    };
    dir = dir.with_height(0.0);
    let len = fmath::sqrt(dir.x() * dir.x() + 0.0 + dir.y() * dir.y());
    let dir = BVector::new(dir.x() / len, dir.height() / len, dir.y() / len);
    let mut step = fmath::add(r.at, fmath::scale(1.0, dir));
    let (w, h) = (map.width(), map.height());
    let (top_x, top_y) = (w as f32 - r.radius.x(), h as f32 - r.height.get());
    if r.radius.x() > step.x() {
        step = step.with_x(r.radius.x());
    } else if step.x() > top_x {
        step = step.with_x(top_x);
    }
    if r.height.get() > step.y() {
        step = step.with_y(r.height.get());
    } else if step.y() > top_y {
        step = step.with_y(top_y);
    }
    step
}

/// `FUN_140f9bcd0` outside the path (its static point): the way's end lies farther than `gap`
/// from the slot `to` on the ground.
pub(super) fn ends_farther(record: &Path, to: BVector, gap: Distance) -> bool {
    let end = record
        .points
        .last()
        .copied()
        .unwrap_or(crate::movement::OUTSIDE_PATH);
    let mut d = fmath::sub(to, end);
    d = d.with_height(0.0);
    gap < Distance::new(fmath::sqrt(d.x() * d.x() + 0.0 + d.y() * d.y()))
}
