//! The reach test, a `MoveTo`'s path end that follows its target, the top path and the pauses.

use crate::groups::MoveReason;
use crate::map::Distance;
use crate::map::Tile;
use crate::movement::LevelFlags;
use crate::movement::LevelsExt;
use crate::movement::PathFlags;
use crate::movement::PathState;
use crate::movement::Speed;
use crate::movement::WaypointLevel;
use crate::objects::{ObjectId, ObjectKey};
use crate::players::PlayerKind;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WorldTime;

use crate::construction::MainBuildingId;
use crate::groups::UnitGroups;
use crate::map::DistanceSquared;
use crate::map::Point;
use crate::map::Position;
use crate::map::spatial::{Placed, RecordId, Spatial};
use crate::math::conversions::cvttss2si;
use crate::math::fmath::BVector;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered, sqrt};
use crate::movement::WaypointIndex;
use crate::movement::{
    Avoided, EPSILON, LongWalker, Me, Mover, PathAsk, PathData, Planning, ShortAsk, footprint_gap,
    footprint_gap_squared, lowered_units, overlap, pause_rank, point_or_zero, point_or_zero_of,
};
use crate::objects::masters::MasterId;
use crate::pathing::long_path::{LongRequest, LongTarget, gate_lets_pass, reaches};
use crate::pathing::tile_path::{Path, Request};
use crate::players::PlayerId;
use crate::players::PlayerSet;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::generated::MovementTuning;
use crate::world::objects_ref::ObjectsRef;
use crate::{GenieError, GenieResult};

/// What a reach test asks of the unit ([`Mover::reach_test`]'s arguments after the world).
#[derive(Clone, Copy, Debug)]
pub(super) struct ReachAsk {
    /// The goal (x, y on the map).
    pub(super) goal: Point,
    /// The target object's id.
    pub(super) target: Option<ObjectId>,
    /// How near the goal counts.
    pub(super) range: Distance,
    /// The unit's master's id.
    pub(super) master: MasterId,
}

impl Mover {
    /// `FUN_140fa0370`: both ends lie on the map. With a planner for its restriction, the tile
    /// planner (`FUN_140f7c530` without the tile search, closed tiles or smoothing; the radii are
    /// the unit's and the target's, the range 3 more for a target of master 45 or class 33) must
    /// reach the goal or its range (result 0 or 1). Then a unit with a footprint and a height
    /// (`FUN_141194060` with 1) no more than 10 tiles from the goal asks the short pather
    /// (`FUN_140f9e540` with its ignore list, `FUN_14114ece0`, no include list, and its argument
    /// 12 the test's 11th: 0 from slot 0x3d8, the caller's from slot 0x3d0, which genie's callers
    /// give as 0; the old pather would keep its last area and hulls for 1), whose path to the
    /// goal or into the range (a result below 2) answers yes. Otherwise a goal at least a tile from the unit's
    /// footprint (the squared gap, to the target's footprint for a target, at least 1) is the long
    /// pather's to answer ([`crate::pathing::long_path::reaches`], from the unit's tile to the goal's, the
    /// planner's other arguments unset: no path, the reach within a tile, no player's objects); a
    /// nearer one is not reached. `master` is the unit's master's id (42 moves for the long
    /// pather without a speed).
    pub(super) fn reach_test(
        &self,
        at: Position,
        me: &Me,
        world: &mut Spatial,
        plan: &mut Planning<'_>,
        ask: ReachAsk,
        by_id: ObjectsRef<'_>,
    ) -> GenieResult<bool> {
        let long_path_tuning = &by_id.tuning().long_path;
        let tile_path_tuning = &by_id.tuning().tile_path;
        let movement_tuning = &by_id.tuning().movement;
        let roles = by_id.roles();
        let ReachAsk {
            goal: to,
            target,
            range,
            master,
        } = ask;
        // FUN_14114fea0 and FUN_14114ffc0 hand the unit's +0x824 (x, height, y) as the start
        let (x, y, height_at) = (self.moved_at.x(), self.moved_at.y(), self.moved_at.z());
        let (map_width, map_height) = (world.map.width(), world.map.height());
        // COMISS 0, v; JA and COMISS v, size; JNC (0x140fa03f1..0x140fa0459): a NaN passes
        let on_map = |px: f32, py: f32| {
            at_most_or_unordered(0.0, px)
                && at_most_or_unordered(0.0, py)
                && below_or_unordered(px, map_width as f32)
                && below_or_unordered(py, map_height as f32)
        };
        if !on_map(x, y) || !on_map(to.x(), to.y()) {
            return Ok(false);
        }
        let radius = self.stats.radius;
        let ground = radius.ground();
        // the target is read where it is tested: the short path below is handed the spatial
        // index, which it writes
        let placed = target_placed(world, target)?;
        let target_radius = placed.map_or(0.0, |placed| placed.radius.x());
        let bonus = placed.is_some_and(|placed| {
            roles.of_master(placed.master).contains(MasterRoles::DOCK)
                || roles
                    .of_class(placed.class)
                    .contains(ClassRoles::WIDE_REACH)
        });
        let mut path = Path::default();
        if let Some(planner) = plan.pathing.planner(self.stats.restriction, world.tiles()) {
            let req = Request {
                from: Point::new(x, y),
                to,
                radii: Distance::new(radius.x() + target_radius),
                range: if bonus {
                    range + Distance::new(movement_tuning.wide_range)
                } else {
                    range
                },
                closed: Vec::new(),
                full: false,
            };
            let found =
                planner.plan::<GenieError>(&req, false, &mut path, None, tile_path_tuning)?;
            if !found.reaches() {
                return Ok(false);
            }
        }
        let solid = 0.0 < radius.z() && 0.0 < radius.x() && 0.0 < radius.y();
        let (dx, dy) = (to.x() - x, to.y() - y);
        if solid && sqrt(dx * dx + 0.0 + dy * dy) <= movement_tuning.reach_short {
            let goal = BVector::new(to.x(), 0.0, to.y());
            // FUN_140fa0370 hands the start test both vectors' first two floats: x and the
            // height, the unit's (`+0x824`) and the target's (slot 0x520), not x and y
            let height = placed.map_or(0.0, |placed| placed.position.z());
            let line = (Point::new(x, height_at), Point::new(to.x(), height));
            let ignore = self.ignore_list(me, world, plan, target.zip(placed), line, by_id)?;
            let ask = ShortAsk {
                goal,
                range,
                target,
                ignore: &ignore,
                include: &[],
            };
            if self
                .short_path(me, world, plan, ask, &mut path, by_id)?
                .reaches()
            {
                return Ok(true);
            }
        }
        let long_target = if let Some(placed) = target_placed(world, target)? {
            let gap2 = footprint_gap_squared(at, ground, placed.position, placed.radius);
            if gap2 < DistanceSquared::new(movement_tuning.reach_near) {
                return Ok(false);
            }
            let gap = footprint_gap(at, ground, placed.position, placed.radius);
            Some(LongTarget::new(
                placed.position,
                placed.radius,
                gap,
                gap2,
                *long_path_tuning,
            ))
        } else {
            let gap = |delta: f32, radius: f32| {
                let beyond = delta.abs() - radius;
                if 0.0 <= beyond { beyond } else { 0.0 }
            };
            // the gap from the unit's place (its decoded x and y), not +0x824
            let (px, py) = (at.x(), at.y());
            let (gx, gy) = (gap(to.x() - px, radius.x()), gap(to.y() - py, radius.y()));
            if gx * gx + gy * gy < DistanceSquared::new(movement_tuning.reach_near).get() {
                return Ok(false);
            }
            None
        };
        // FUN_140f9eb70 reads slot 0x208 once: a unit whose top speed is not 0, or a master of
        // class 0x2a
        let moves = self.top_speed(me.top_speed, at.ground()) != Speed::ZERO
            || roles
                .of_master(master)
                .contains(MasterRoles::LONG_PATH_MOVER);
        // FUN_140f9eb70 lowers the tall counts under its list (FUN_140f60440): the unit and its
        // ignore units that wait or move, or its group's differentiator units, then the target
        let walker = LongWalker {
            id: me.id,
            position: at,
            radius: ground,
            mask: self.stats.restriction_mask,
            moves,
            ignore_units: &self.ignore_units,
            group_units: plan.groups.group_units(me.group),
        };
        let lowered = lowered_units(&walker, world, target, by_id)
            .into_iter()
            .filter_map(|id| world.get(id))
            .filter(|placed| placed.tall)
            .map(|placed| Tile::new(placed.position.x() as i32, placed.position.y() as i32))
            .collect();
        let req = LongRequest {
            from: Tile::new(cvttss2si(x), cvttss2si(y)),
            to: Tile::new(cvttss2si(to.x()), cvttss2si(to.y())),
            range,
            target: long_target,
            moves,
            mask: self.stats.restriction_mask,
            player_kind: plan.player_kind,
            lowered,
            within_range: false,
            near_player: false,
            allies: world
                .get(me.id)
                .map_or(PlayerSet::NONE, |placed| world.allies_of(placed.player)),
        };
        let player = world.get(me.id).map(|placed| placed.player);
        let gates = |tile| {
            player.map_or(Ok(false), |player_id| {
                gate_lets_pass(world, by_id, player_id, tile)
            })
        };
        reaches(world.tiles(), world.map, &req, &gates, *long_path_tuning)
    }

    /// The test before a non-quiet `MoveTo`'s path end follows its moved target (`0x140aa2a70` in
    /// state 4): the top path (`FUN_140f960f0`) and the walked level's own path, that level being
    /// level 0, are both held, and the level's current waypoint (`FUN_140f9bcd0`) is the top
    /// path's last point (`FUN_140f95fb0`, compared x, y, z). A top path of one point, where the
    /// game compares with its static point, is not modelled.
    pub fn walks_to_path_end(&self) -> GenieResult<bool> {
        let Some(level) = self.paths.first().filter(|path_data| {
            path_data.level == WaypointLevel::Walked && !path_data.points.is_empty()
        }) else {
            return Ok(false);
        };
        let Some(top) = self.top_path() else {
            return Ok(false);
        };
        let [.., _, last] = top.as_slice() else {
            return Err(GenieError::unsupported(
                "a top path of one point (FUN_140f95fb0)",
            ));
        };
        Ok(point_or_zero_of(level, level.waypoint) == *last)
    }

    /// A non-quiet `MoveTo`'s path end follows its target to `point` (`0x140aa2a70` after the line
    /// test): the walked level's current waypoint (`FUN_140f9b890` at its index) and the top path's
    /// last point take it (the paths' layout, [`Position::vector`]); bits 1 and 0 go off the walked
    /// level (`FUN_140f9b4d0` twice).
    pub fn follow_path_end(&mut self, point: Position) {
        let point = point.vector();
        if let Some(level) = self.paths.first_mut() {
            if let Some(waypoint) = level
                .waypoint
                .slot()
                .and_then(|index| level.points.get_mut(index))
            {
                *waypoint = point;
            }
            level.flags &= !(LevelFlags::REACHED | LevelFlags::AT_WAYPOINT);
        }
        if let Some(last) = self
            .paths
            .last_mut()
            .and_then(|path_data| path_data.points.last_mut())
        {
            *last = point;
        }
    }

    /// The path of the top level (`FUN_140f960f0`): the last level's, or the walked level's when
    /// it has no parent; None when that level holds no path.
    pub(super) fn top_path(&self) -> Option<&Vec<BVector>> {
        self.paths
            .last()
            .map(|path_data| &path_data.points)
            .filter(|path| !path.is_empty())
    }

    /// The path request of a `MoveTo` without its unit AI's say ([`Mover::request_path_asked`]
    /// with [`PathAsk::default`]).
    pub fn request_path(
        &mut self,
        from: Position,
        goal: Position,
        range: Distance,
        target: Option<ObjectId>,
        now: WorldTime,
    ) {
        self.request_path_asked(from, goal, range, target, now, PathAsk::default());
    }

    /// Slot 0x540 (`FUN_14114e0f0`) with a path record, no target and range 0: the future path
    /// data (`+0x860`, none held) and the path data (`+0x858`) reset (`FUN_140f98e80`: its parents
    /// go, its repath time stays), then the record's path, its first waypoint 1: `planned` as
    /// level 2 to plan (state 2, `FUN_140f9b800`), else as level 0 walking it (state 3,
    /// `FUN_140f9b7c0`). The path keeps a new record's flags (6), its `length` and its creation
    /// time `now`.
    pub(crate) fn take_record_path(
        &mut self,
        points: Vec<BVector>,
        length: Distance,
        now: WorldTime,
        planned: bool,
    ) {
        let repath_time = self
            .paths
            .first()
            .map_or(Seconds::ZERO, |path_data| path_data.repath_time);
        self.paths = vec![PathData {
            level: if planned {
                WaypointLevel::Top
            } else {
                WaypointLevel::Walked
            },
            points,
            has_path: true,
            path_flags: PathFlags::BACK_AND_FORTH | PathFlags::FORWARD,
            length,
            created: now,
            waypoint: WaypointIndex::SECOND,
            state: if planned {
                PathState::WantsPath
            } else {
                PathState::Walking
            },
            repath_time,
            ..PathData::without_path()
        }];
    }

    /// The path request of a `MoveTo` (`FUN_14114e410`): one level of path data, the top (level
    /// 2), in state 2: the next motion step plans the path to walk. Its path (a new one, flags 6,
    /// made `now`) runs from the unit (`from`, `x, y, z` on the map; `FUN_140f95a50`) through the
    /// unit's own waypoints, which go (slot 0x450), to the goal unless that is less than 1e-7
    /// from the last point on the ground (`FUN_140f95a10` each, which leaves out a point equal to
    /// the one before). A saved patrol's waypoint ([`PathAsk::resume`]) leaves out the unit's
    /// place and the goal and the level walks on from that waypoint; a path still empty then
    /// runs from the unit to the goal on the ground (height 0). The path loops with
    /// [`PathAsk::patrol`] (its flags' bit 0), goes back and forth with
    /// [`PathAsk::back_and_forth`] (bit 1); an attack move does not loop and sets `+0x7fb`, which
    /// none clears ([`PathAsk::attack_move`]). A unit with future path data (`+0x860`) goes
    /// another way, which the slice does not model (it holds none).
    pub fn request_path_asked(
        &mut self,
        from: Position,
        goal: Position,
        range: Distance,
        target: Option<ObjectId>,
        now: WorldTime,
        ask: PathAsk,
    ) {
        let start = from.vector();
        let mut path = Path::default();
        if ask.resume == WaypointIndex::NONE {
            let _ = path.insert(0, start);
        }
        for &waypoint in &self.user_waypoints {
            let _ = path.push(waypoint);
        }
        self.clear_user_waypoints();
        if ask.resume == WaypointIndex::NONE {
            let end = goal.vector();
            let last = point_or_zero(&path, WaypointIndex::last_of(path.points.len()));
            let mut offset = end - last;
            offset = offset.with_height(0.0);
            if EPSILON < sqrt(offset.x() * offset.x() + 0.0 + offset.y() * offset.y()) {
                let _ = path.push(end);
            }
        }
        if path.points.is_empty() {
            let _ = path.insert(0, start);
            let _ = path.push(BVector::new(goal.x(), 0.0, goal.y()));
        }
        // a new path's flags are 6 (FUN_14152eea0); bit 0 by the patrol, cleared again by an
        // attack move, then bit 1 by the back and forth. The walked level, if any, is reset for
        // it (FUN_140f98e80: its repath time stays)
        let mut path_flags = if ask.patrol {
            PathFlags::NEW | PathFlags::LOOPS
        } else {
            PathFlags::NEW & !PathFlags::LOOPS
        };
        match ask.attack_move {
            Some(true) => {
                path_flags &= !PathFlags::LOOPS;
                self.attack_moving = true;
            }
            Some(false) => self.attack_moving = false,
            None => {}
        }
        path_flags = if ask.back_and_forth {
            path_flags | PathFlags::BACK_AND_FORTH
        } else {
            path_flags & !PathFlags::BACK_AND_FORTH
        };
        let repath_time = self
            .paths
            .first()
            .map_or(Seconds::ZERO, |path_data| path_data.repath_time);
        self.paths = vec![PathData {
            level: WaypointLevel::Top,
            points: path.points,
            path_flags,
            waypoint: if ask.resume == WaypointIndex::NONE {
                WaypointIndex::SECOND
            } else {
                ask.resume
            },
            state: PathState::WantsPath,
            range,
            target,
            repath_time,
            ..PathData::new(now)
        }];
    }

    /// Slot 0x448 (`FUN_14114e920`): a waypoint of the unit's own at `at` (x, y and height z),
    /// held as (x, height, y); an x or y less than 1e-7 above a whole number (`floorf`, `COMISS`,
    /// `JBE`: not a NaN) moves to that tile's middle. Its array's growth (5, then twice) fails only
    /// without memory.
    pub fn add_user_waypoint(&mut self, at: Position, movement_tuning: &MovementTuning) {
        let snap = |value: f32| {
            let whole = libm::floorf(value);
            if (value - whole).abs() < EPSILON {
                whole + movement_tuning.user_waypoint_middle
            } else {
                value
            }
        };
        self.user_waypoints
            .push(BVector::new(snap(at.x()), at.z(), snap(at.y())));
    }

    /// Slot 0x450 (`FUN_141070ce0`): the unit's own waypoints go (their count `+0x7fc` 0).
    pub fn clear_user_waypoints(&mut self) {
        self.user_waypoints.clear();
    }

    /// The unit vector on the ground from `from` to `to` (slot 194, `0x14114f210`); the forward
    /// vector when the two are less than 1e-7 apart or on the same point.
    pub(crate) fn direction_to(&self, from: BVector, to: BVector) -> BVector {
        if from.x() != to.x() || from.y() != to.y() {
            let x = to.x() - from.x();
            let z = to.y() - from.y();
            let length = sqrt(x * x + 0.0 + z * z);
            // COMISS eps, length; JBE at 0x14114f2aa: a NaN length divides too
            if at_most_or_unordered(EPSILON, length) {
                return BVector::new(x / length, 0.0, z / length);
            }
        }
        self.forward
    }

    /// Where this tick's speed would take the unit along `direction` (slot 199, `0x141161860`)
    /// from where it last moved to (`+0x824`, [`Mover::moved_at`]): the forward speed after one
    /// more step of acceleration, capped by its top speed `top` (slot 0x208; COMISS/JA, then JC:
    /// a NaN speed stays).
    pub(crate) fn projected(&self, direction: BVector, dt: Seconds, top: Speed) -> BVector {
        let position = self.moved_at.vector();
        let Some(velocity) = &self.velocity else {
            return position;
        };
        let mut next = velocity.velocity + dt.get() * velocity.acceleration;
        if top < Speed::new(next.y()) || next.y() <= 0.0 {
            next = next.with_y(top.get());
        }
        position + dt.get() * next.y() * direction
    }

    /// [`Mover::avoid`]'s first loop's test of the hit `id` (`other` its record): kept unless
    /// the step shrinks its overlap with the unit's box (`FUN_1411615a0`); then when the walked
    /// path ignores it and it moved before the top path was made (or the top path is this
    /// update's and the step overlaps it more than `MovementTuning::pushed` of the radius), or it moved after
    /// the walked path was made (or at the same time and outranks the unit: a higher player, or
    /// the same and a higher id).
    pub(super) fn keeps_hit(
        &self,
        (at, me): (Position, &Me),
        player: PlayerId,
        (id, other): (ObjectId, &Placed),
        projected: BVector,
        now: WorldTime,
        by_id: ObjectsRef<'_>,
    ) -> GenieResult<bool> {
        let movement_tuning = &by_id.tuning().movement;
        let level = self.paths.walked()?;
        let top = self
            .paths
            .last()
            .filter(|path_data| !path_data.points.is_empty());
        let ground = self.stats.radius.ground();
        let vector = other.position.vector();
        let now_overlap = overlap(at.vector(), ground, vector, other.radius);
        let next_overlap = overlap(projected, ground, vector, other.radius);
        let last_move = by_id
            .gait(id)
            .map_or(WorldTime::ZERO, |gait| gait.last_move);
        let mut pushed = false;
        if let Some(top) = top.filter(|_| now_overlap <= next_overlap) {
            if top.created == now
                && ground.x() * movement_tuning.pushed < next_overlap
                && level.ignored.contains(&id)
            {
                pushed = true;
            }
            if top.created < now && level.ignored.contains(&id) {
                pushed = last_move < top.created;
            }
        }
        let outranks = player < other.player || (player == other.player && me.id < id);
        let outranks = level.created < last_move || (outranks && level.created == last_move);
        Ok(!(next_overlap < now_overlap || (!pushed && !outranks)))
    }

    /// The walk's avoidance of what its step hits (`0x141150f00`, after `FUN_14114cc60`). The first
    /// loop keeps a hit the unit must go round: none but a building, wall or gate
    /// ([`ClassRoles::KEPT_BY_MEMBERS`]) for a member of a unit group the world holds whose own
    /// move there has reason 0 (`FUN_141017590`); none whose overlap with the unit's box the step
    /// shrinks (`FUN_1411615a0`); else one in the walked path's ignore units (`FUN_140f98d20`) that
    /// moved before the top path was made, or one that moved after the walked path was made (or at
    /// the same time and outranks the unit: a higher player, or the same and a higher id). A step
    /// whose one hit is the walked level's target counts its waypoint reached (the caller's,
    /// [`Avoided::at_target`]), that hit kept or not. A kept hit (its place in the include list and
    /// its temporary shape `FUN_141054fe0`, [`include_kept`]; the step's slide `FUN_141159220` and
    /// the re-plan, [`Mover::go_round`]) is the caller's. Then, on the walked level's first two
    /// waypoints (or the third of a path more than 500 old), each hit the unit outranks pauses for
    /// `dt` when its own pause is shorter: a human walker outranks another player's unit, or one of
    /// its own ordered at another time, whose last human order (`+0x650`) is older; else, between
    /// two members of one unit group (`+0x6d0`, not −1), the earlier member (`FUN_1410175f0`'s
    /// index in the group's list, −1 for none) outranks, and a group the world does not hold
    /// (`FUN_14152ff40`, `groups`) pauses neither; else the lower id (slot 0x5f8) outranks. A Gaia
    /// unit is outranked by any non-Gaia walker; the pause itself (slot 0x5f0) is the caller's. The
    /// paths' hash sets (`+0x18`, `FUN_140f96120`) are empty in this game (the probe
    /// `path_sets.py`).
    ///
    /// [`include_kept`]: crate::movement::long_walk::include_kept
    pub(super) fn avoid(
        &self,
        (at, me): (Position, &Me),
        (world, groups, by_id): (&Spatial, Option<&UnitGroups>, ObjectsRef<'_>),
        hits: &[RecordId],
        projected: BVector,
        dt: Seconds,
        (now, player_kind): (WorldTime, PlayerKind),
    ) -> GenieResult<Avoided> {
        let roles = by_id.roles();
        let level = self.paths.walked()?;
        let player = world
            .get(me.id)
            .ok_or(GenieError::unsupported("a walker without a place"))?
            .player;
        // a hit the world holds no object of (a map edge's record) is passed over
        let met = |hit: RecordId| -> GenieResult<Option<(ObjectId, &Placed)>> {
            let Some(id) = by_id
                .annex_of(hit)
                .map(MainBuildingId::id)
                .or_else(|| hit.key())
            else {
                return Ok(None);
            };
            Ok(world.get_known(id)?.map(|placed| (id, placed)))
        };
        // the walker's group (FUN_14152ff40 on +0x6d0) and its own move's reason there
        // (FUN_141017590): a member on one of reason 0 keeps buildings, walls and gates alone
        let own_move = groups
            .and_then(|gs| gs.get(me.group?))
            .is_some_and(|group| group.member_reason(me.id) == MoveReason::Detour);
        let mut target_hit = false;
        let mut kept = Vec::new();
        for &hit in hits {
            let Some((id, other)) = met(hit)? else {
                continue;
            };
            if level.targets(id) {
                target_hit = true;
            }
            if own_move
                && !roles
                    .of_class(other.class)
                    .contains(ClassRoles::KEPT_BY_MEMBERS)
            {
                continue;
            }
            if self.keeps_hit((at, me), player, (id, other), projected, now, by_id)? {
                kept.push(id);
            }
        }
        // FUN_141159220 slides the projected step out of the hits of no speed of classes 3, 27,
        // 39 and 52 (buildings, walls, gates): only the kept hits' re-plan reads the slid point,
        // and with no kept hit the slide changes nothing
        if hits.len() == 1 && target_hit {
            return Ok(Avoided {
                kept,
                pauses: Vec::new(),
                at_target: true,
            });
        }
        if !kept.is_empty() {
            return Ok(Avoided {
                kept,
                pauses: Vec::new(),
                at_target: false,
            });
        }
        let mut pauses = Vec::new();
        if level.waypoint < WaypointIndex::of_data(2)
            || (level.waypoint < WaypointIndex::of_data(3)
                && level.created.after(Millis::new(500)) < now)
        {
            for &hit in hits {
                let Some((id, other)) = met(hit)? else {
                    continue;
                };
                if level.targets(id) {
                    continue;
                }
                if (player == PlayerId::GAIA || other.player != PlayerId::GAIA)
                    && pause_rank(me, (player, player_kind), (id, other), groups, by_id)
                        != Some(true)
                {
                    continue;
                }
                // slot 0x528's walked level, paused for dt more when its pause is shorter
                if let Some(seconds) = self.pause_of(id, by_id).filter(|&seconds| seconds < dt) {
                    pauses.push((id, seconds + dt));
                }
            }
        }
        Ok(Avoided {
            kept,
            pauses,
            at_target: false,
        })
    }

    /// The pause of unit `id`'s walked level as slot 0x5f0 leaves it: the last this walk gave it
    /// (the game's call writes it at once, a later pass of the same step reads it), else the
    /// unit's own (`by_id`, [`crate::map::spatial::Gait::pause`]).
    pub(super) fn pause_of(&self, id: ObjectId, by_id: ObjectsRef<'_>) -> Option<Seconds> {
        self.pauses
            .iter()
            .rev()
            .find(|&&(object_id, _)| object_id == id)
            .map(|&(_, seconds)| seconds)
            .or_else(|| by_id.gait(id).and_then(|gait| gait.pause))
    }
}

/// The place of a reach test's target in the spatial index, None without a target. The caller
/// found the target there, so a missing one is a broken invariant.
fn target_placed(world: &Spatial, target: Option<ObjectId>) -> GenieResult<Option<&Placed>> {
    target
        .map(|id| {
            world.get(id).ok_or_else(|| {
                GenieError::unreachable("a reach test's target left the spatial index")
                    .with("object", id)
            })
        })
        .transpose()
}
