//! The velocity step, a new object's setup, the position setter and the motion step's loop.

use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Rect;
use crate::map::spatial::{Placed, RecordId, Spatial, WalkQuery};
use crate::math::Angle;
use crate::math::fmath::BVector;
use crate::movement::LevelFlags;
use crate::movement::PathFlags;
use crate::movement::PathState;
use crate::movement::Speed;
use crate::movement::{
    ANNEX_GAP, AfterRequest, EDGE_GAP, EDGE_POSITION_GAP, EPSILON, FAR, Goal, Me, Motion,
    MoveStats, Mover, Next, Planning, RANGE_FLOOR, Reach, footprint_gap, gate_gap, ground,
    include_kept, moved_far, pause_holds, point_or_zero_of, reach,
};
use crate::objects::ObjectId;
use crate::pathing::Pathing;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::{GenieError, math::fmath};

/// [`Mover::level_gaps`]'s measure: the target with its place (none for a level without one or
/// a target that is gone), the gap to it, and the gap to it, else to where the top level's path
/// ends.
type LevelGaps = (Option<(ObjectId, Placed)>, Distance, Distance);

/// The speed below which a free group member's speed is not capped by its top speed (slot
/// 0x5e0).
const UNCAPPED_SPEED: f32 = crate::math::image::movement::UNCAPPED_SPEED;

/// What slot 0x5e0 (`FUN_14114f300`) reads of a unit its unit group steers: the unit is in a
/// group the world holds (`+0x6d0`, `FUN_14152ff40`), the group's motion moves
/// (`FUN_141046600`) or the unit is not in its place ([`UnitGroup::slot_in_place`]), and the
/// unit is free for the group's moves (`FUN_141017f00`). Its own move's reason
/// (`FUN_141017590`) decides.
///
/// [`UnitGroup::slot_in_place`]: crate::groups::UnitGroup::slot_in_place
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GroupSpeed {
    /// Moved aside (reason 2): the unit stands.
    Aside,
    /// No own move (reason −1): a speed below [`UNCAPPED_SPEED`] is not capped by the top
    /// speed.
    Free,
    /// An own move of another reason: the group's top speed (`+0xbc`) while the unit's walked
    /// level walks (state 3) unpaused, else the unit stands.
    OwnMove(Speed),
}

/// What the walk along the level's path takes of the step ([`Mover::walk`]'s arguments after
/// the unit).
#[derive(Debug)]
pub(super) struct WalkStep<'a> {
    /// The step's time.
    pub(super) time_step: Seconds,
    /// The world's spatial index.
    pub(super) world: &'a mut Spatial,
    /// The world's unit groups (none for a walk without the world's planners).
    pub(super) groups: Option<&'a crate::groups::UnitGroups>,
    /// The world's pathers (none likewise).
    pub(super) pathing: Option<&'a mut Pathing>,
    /// The goal the level walks to.
    pub(super) goal: Goal<'a>,
    /// The world's time.
    pub(super) now: WorldTime,
    /// Whether the unit's player is a computer.
    pub(super) computer: bool,
    /// The objects the old pather tests even moving.
    pub(super) include: &'a mut Vec<ObjectId>,
    /// The waypoint once it is not left behind (`local_1860`).
    pub(super) walked: &'a mut BVector,
    /// The direction the unit turns to (slot 0x610).
    pub(super) heading: &'a mut BVector,
}

impl Mover {
    /// Slot 0x5e0 (`FUN_14114f300`): the forward speed the velocity step takes toward `wanted`
    /// (the acceleration times dt plus the speed). A unit its group steers (`group`) takes the
    /// group's answer ([`GroupSpeed`]; a free one below [`UNCAPPED_SPEED`], `COMISS` then `JA`,
    /// only the floor). Any other is capped by the top speed (slot 0x208; `COMISS` then `JA`: a
    /// NaN is not above it), then floored at 0 (a NaN stays).
    #[must_use]
    pub fn next_speed(&self, wanted: Speed, group: Option<GroupSpeed>) -> Speed {
        match group {
            Some(GroupSpeed::Aside) => return Speed::ZERO,
            Some(GroupSpeed::OwnMove(speed)) => {
                // the walked level walks (state 3) and its pause is not above 0 (`COMISS`
                // then `JA`: a NaN pause walks)
                let walks = self.paths.first().is_some_and(|l| {
                    l.state == PathState::Walking
                        && fmath::at_most_or_unordered(l.pause_time.get(), 0.0)
                });
                return if walks { speed } else { Speed::ZERO };
            }
            Some(GroupSpeed::Free) if wanted.get() < UNCAPPED_SPEED => {}
            Some(GroupSpeed::Free) | None => {
                if self.stats.speed.get() < wanted.get() {
                    return self.stats.speed;
                }
            }
        }
        if wanted.get() < 0.0 {
            Speed::ZERO
        } else {
            wanted
        }
    }

    /// The velocity part of `RGE_Moving_Object::update` (`FUN_140f9ba80`): the forward speed moves
    /// toward the acceleration's target, as slot 0x5e0 takes it ([`Mover::next_speed`], `group`
    /// the unit's group's steer), then the unit moves `dt × speed` along its forward vector (slot
    /// 182, `0x141158490`) at the world's time `now`. Returns the position after it.
    pub fn step_velocity(
        &mut self,
        position: Position,
        dt: Seconds,
        map: GridSize,
        terrain: Option<&crate::map::terrain::Terrain>,
        (now, group): (WorldTime, Option<GroupSpeed>),
    ) -> Position {
        let Some(v) = self.velocity.as_ref() else {
            return position;
        };
        let speed = self.next_speed(Speed::new(v.acceleration.y() * dt + v.velocity.y()), group);
        if let Some(v) = self.velocity.as_mut() {
            v.velocity = v.velocity.with_y(speed.get());
        }
        // the position setter runs only for a distance it moves
        let Some(at) = self.move_forward(Distance::new(dt.get() * speed.get()), map) else {
            return position;
        };
        let at = self.grounded(at, terrain);
        self.mark_move(at, now);
        at
    }

    /// The static `new_position`'s end (`0x1411a58b0`, slot 0x60: `FUN_141158000` ->
    /// `FUN_141193a10`): the place takes the ground's height ([`crate::map::terrain::Terrain::ground`])
    /// by the master's fly mode: 0 puts it on the ground, 1 lifts it to the ground when below,
    /// others keep it. Without the map's elevations the height stays; `+0x7f8` (the place is on
    /// the ground) is not held.
    #[must_use]
    pub fn grounded(
        &self,
        at: Position,
        terrain: Option<&crate::map::terrain::Terrain>,
    ) -> Position {
        let height = at.z();
        let z = terrain
            .and_then(|t| t.ground(at.ground()).ok())
            .map_or(height, |ground| {
                crate::combat::missile::grounded(at.z(), ground, false, self.stats.fly_mode)
            });
        at.with_z(z)
    }

    /// The moving setup (`FUN_1411580a0`) of a new object of `stats` at `at`: it faces x (its
    /// right −z), does not wait, has no path and moveForward's factor 1; the position setter
    /// (`FUN_141161950`) makes its place a move at `now`. Its angle and its sprite's facets are
    /// the caller's. Every moving object (types 30 and up) gets it, a building too, though it
    /// never moves: the walk reads the last move time of a building it meets (slot 0x5e8,
    /// `FUN_14106d2d0`).
    pub(crate) const fn new_object(
        stats: MoveStats,
        at: Position,
        now: WorldTime,
        angle: Angle,
        sprite_facets: Option<crate::objects::facing::SpriteFacets>,
    ) -> Self {
        Self {
            // the animated object's constructor (FUN_141070cf0): animation speed (+0x7b8) 0
            animated: crate::objects::layers::Animated::at(Speed::ZERO, at),
            forward: BVector::new(1.0, 0.0, 0.0),
            right: BVector::new(0.0, 0.0, crate::production::spawn::START_RIGHT_Z),
            velocity: None,
            paths: Vec::new(),
            factor: 1.0,
            stats,
            waiting: 0,
            wait_delays: crate::movement::WaitDelays::NONE,
            attack_moving: false,
            last_move: now,
            moved_at: at,
            substitute_count: crate::movement::SubstituteCount::NONE,
            ignore_units: Vec::new(),
            user_waypoints: Vec::new(),
            last_turn: Seconds::ZERO,
            angle,
            pauses: Vec::new(),
            in_the_way: Vec::new(),
            sprite_facets,
        }
    }

    /// The position setter's end (`FUN_141161950`): a place `at` more than 1e-7 off where it
    /// last moved to on any axis is a move at `now` (`+0x870`), and where it last moved to.
    pub(crate) fn mark_move(&mut self, at: Position, now: WorldTime) {
        if moved_far(self.moved_at, at) {
            self.last_move = now;
            self.moved_at = at;
        }
    }

    /// `moveForward` (slot 182, `FUN_141158490`): `distance × factor` along the forward vector
    /// from where the unit last moved to (`+0x824`, [`Mover::moved_at`]), through the position
    /// setter; a distance below 1e-7 moves nothing.
    pub(super) fn move_forward(&self, distance: Distance, map: GridSize) -> Option<Position> {
        let d = distance * self.factor;
        if d.get().abs() < EPSILON {
            return None;
        }
        let to = fmath::add(self.moved_at.vector(), fmath::scale(d.get(), self.forward));
        Some(self.set_position(to.position(), map))
    }

    /// x, y, z on the map: the place of the static layer below it
    /// ([`crate::objects::layers::Static`]).
    #[must_use]
    pub const fn position(&self) -> Position {
        self.animated.base().position()
    }

    /// A test's fixture puts the unit at `at` without the world's index, which the test sets
    /// itself.
    #[cfg(test)]
    pub(crate) const fn place_for_test(&mut self, at: Position) {
        self.animated.base_mut().place_for_test(at);
    }

    /// The same mover at `at`, for a test's fixture.
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn placed(mut self, at: Position) -> Self {
        self.place_for_test(at);
        self
    }

    /// The unit's place setter from its moving layer ([`crate::objects::layers::Static::new_position`]):
    /// its shape goes from the mesh while it moves (slot 185, [`Mover::is_moving`]).
    pub(crate) fn new_position(
        &mut self,
        id: ObjectId,
        at: Position,
        places: (&mut Spatial, Option<&mut Pathing>),
    ) -> Result<(), GenieError> {
        let moving = self.is_moving();
        self.animated
            .base_mut()
            .new_position(id, at, moving, places)?;
        Ok(())
    }

    /// The position setter (slot 11 -> `FUN_141161950` -> `0x1411a58b0`): a unit that can move
    /// keeps its footprint 0.01 inside the map, then any position stays 0.001 below the map's size.
    pub(crate) fn set_position(&self, at: Position, map: GridSize) -> Position {
        let (x, y) = (at.x(), at.y());
        let inside = |v: f32, radius: f32, size: i32| {
            let low = radius + EDGE_GAP;
            if v < low {
                low
            } else if size as f32 - low < v {
                (size as f32 - radius) - EDGE_GAP
            } else {
                v
            }
        };
        // both clamps read the map's +0x18, its width (0x14116220d for y): on a map that is not
        // square the y clamp takes the width
        let (x, y) = if EPSILON < self.stats.speed.get() {
            (
                inside(x, self.stats.radius.x(), map.width()),
                inside(y, self.stats.radius.y(), map.width()),
            )
        } else {
            (x, y)
        };
        let on_map = |v: f32, size: i32| {
            let v = v.max(0.0);
            if size as f32 <= v {
                size as f32 - EDGE_POSITION_GAP
            } else {
                v
            }
        };
        Position::new(on_map(x, map.width()), on_map(y, map.height()), at.z())
    }

    /// The motion step (object slot 193 -> `0x141150f00`, `doIntelligentMovementUpdate`) along the
    /// path a walking unit follows, as the game's loop: each pass looks at the level the unit
    /// walks. A level whose waypoint was reached moves on to the next one ([`Mover::reached`]), an
    /// arrived level hands over to its parent or ends the walk ([`Mover::arrived`]), a level that
    /// wants a path gets one ([`Mover::request`]); a walking level steers the unit toward its
    /// waypoint ([`Mover::walk`]). A pass that changes the levels starts the step again. Unit
    /// groups and obstruction avoidance are not modelled. `plan` is None where the caller has no
    /// planners: a path request then stops the slice. The unit's place may change (it takes a
    /// reached waypoint's), in `me` and `world`.
    pub fn motion(
        &mut self,
        me: &Me,
        dt: Seconds,
        world: &mut Spatial,
        mut plan: Option<&mut Planning<'_>>,
    ) -> Result<Motion, GenieError> {
        let result = self.motion_passes(me, dt, world, plan.as_deref_mut());
        // every exit of the step removes the kept hits' temporary shapes (FUN_141055970)
        if let Some(p) = plan {
            p.pathing.drop_temporary_shapes();
        }
        result
    }

    /// [`Mover::motion_passes`]'s measure of the level walked toward `target_id`: the target's
    /// place (`FUN_1415417a0`: none for -1 and for an object that is gone, which then measures as
    /// a walk to a point, `LAB_1411512df`), the gap to it, and the gap to it, else to where the
    /// top level's path ends.
    ///
    /// # Errors
    ///
    /// A target whose place the slice does not know.
    pub(super) fn level_gaps(
        &self,
        world: &Spatial,
        target_id: Option<ObjectId>,
        ground_radius: Footprint,
    ) -> Result<LevelGaps, GenieError> {
        // FUN_1415417a0 on the level's target: none for -1 and for an object that is gone,
        // which then measures as a walk to a point (LAB_1411512df)
        let target = match target_id {
            None => None,
            Some(id) => world.get_known(id)?.map(|&t| (id, t)),
        };
        // the gap to the target, else to where the top level's path ends; a gate's is the
        // least over it and its first two annexes (slot 0x680), each annex's less 0.1
        // (`MINSS`, 0x1411512b8)
        let gap = target.as_ref().map_or(Distance::new(FAR), |(id, t)| {
            let own = footprint_gap(self.position(), ground_radius, t.position, t.radius);
            let near = (self.position(), ground_radius);
            gate_gap(world, near, (*id, t), own, |g| g - ANNEX_GAP)
        });
        let to_end = match (&target, self.paths.len()) {
            (Some(_), _) => gap,
            (None, 1) => Distance::new(FAR),
            (None, _) => {
                self.top_path()
                    .and_then(|p| p.last())
                    .map_or(Distance::new(FAR), |&end| {
                        footprint_gap(
                            self.position(),
                            ground_radius,
                            end.position().with_z(0.0),
                            Footprint::new(0.0, 0.0),
                        )
                    })
            }
        };
        Ok((target, gap, to_end))
    }

    /// [`Mover::motion_passes`]'s dispatch on the level's state toward `goal`: an arrived level (4,
    /// 5) hands over ([`Mover::arrived`]), a level that wants a path (2) gets one
    /// ([`Mover::request`]: again, on to the walk, a pause that keeps the unit moving, or a
    /// request given up); any other goes on to the walk.
    ///
    /// # Errors
    ///
    /// A path request without the world's planners, and what the hand-over and the request do not
    /// model.
    pub(super) fn level_state(
        &mut self,
        me: &Me,
        world: &mut Spatial,
        plan: Option<&mut Planning<'_>>,
        goal: Goal<'_>,
        plans: &mut u32,
        include: &[ObjectId],
    ) -> Result<Next, GenieError> {
        match self.paths[0].state {
            PathState::Arrived | PathState::Unplanned => self.arrived(goal),
            PathState::WantsPath => {
                let plan = plan.ok_or(GenieError::unsupported(
                    "a path request without the world's planners",
                ))?;
                Ok(match self.request(me, world, plan, goal, plans, include)? {
                    AfterRequest::Again => Next::Again,
                    AfterRequest::Walk => Next::Dispatch,
                    AfterRequest::Paused => Next::Done(Motion::Moving),
                    AfterRequest::GaveUp => Next::Done(Motion::Done),
                })
            }
            PathState::Idle | PathState::Walking | PathState::GaveUp => Ok(Next::Dispatch),
        }
    }

    /// [`Mover::motion`]'s passes.
    pub(super) fn motion_passes(
        &mut self,
        me: &Me,
        dt: Seconds,
        world: &mut Spatial,
        mut plan: Option<&mut Planning<'_>>,
    ) -> Result<Motion, GenieError> {
        if self.stats.speed == Speed::ZERO {
            return Ok(Motion::Done);
        }
        let Some(level) = self.paths.first_mut() else {
            return Ok(Motion::Done);
        };
        // a paused walk stands (slot 0x5d0) until its pause ran out
        if pause_holds(level, dt) {
            self.stop()?;
            return Ok(Motion::Moving);
        }
        let ground_radius = self.stats.radius.ground();
        let mut plans = 0;
        // the walk's kept hits, which the step's short paths include (`DAT_143f4a000`, its count
        // `local_19bc`, 0 at the step's start)
        let mut include = Vec::new();
        // the waypoint the step's walk last went for (`local_1860`), zero before one
        let mut walked = BVector::ZERO;
        // the direction the step's walk last took (slot 0x610 into `[RBP+0x148]`), zero before
        // one: a loop's end turns toward it
        let mut heading = BVector::ZERO;
        loop {
            // a level without a path ends the step (`LAB_14115582c`), as no level does
            let Some(level) = self.paths.first().filter(|l| l.has_path) else {
                return Ok(Motion::Done);
            };
            let (target, gap, to_end) = self.level_gaps(world, level.target, ground_radius)?;
            let goal = Goal {
                target: target.as_ref().map(|(id, t)| (*id, t)),
                gap,
                to_end,
            };
            if self.paths[0].flags.contains(LevelFlags::REACHED) {
                let now = plan.as_deref().map_or(WorldTime::ZERO, |p| p.now);
                let pathing = plan.as_deref_mut().map(|p| &mut *p.pathing);
                match self.reached(me, world, pathing, goal, now)? {
                    Next::Again => continue,
                    Next::Dispatch => {}
                    Next::Done(m) => return Ok(m),
                    Next::LoopEnd => return self.loop_end(dt, walked, heading),
                }
            }
            match self.level_state(me, world, plan.as_deref_mut(), goal, &mut plans, &include)? {
                Next::Done(m) => return Ok(m),
                Next::Again => continue,
                // a link of another type: on past the request (state 4 or 5) to the walk
                Next::Dispatch => {}
                Next::LoopEnd => unreachable!("only a reached waypoint ends a loop's step"),
            }
            let (now, computer) = plan
                .as_deref()
                .map_or((WorldTime::ZERO, false), |p| (p.now, p.computer));
            let groups = plan.as_deref().map(|p| p.groups);
            let pathing = plan.as_deref_mut().map(|p| &mut *p.pathing);
            let step = self.walk(
                me,
                WalkStep {
                    time_step: dt,
                    world,
                    groups,
                    pathing,
                    goal,
                    now,
                    computer,
                    include: &mut include,
                    walked: &mut walked,
                    heading: &mut heading,
                },
            )?;
            if let Next::Done(m) = step {
                return Ok(m);
            }
        }
    }

    /// The walk along the level's path (`0x141150f00` from `LAB_141153bd1`): toward the waypoint
    /// in a straight line, turning to face it. On the waypoint, or when the step would lead away
    /// from it, the waypoint counts as reached and the motion step starts again; a step that
    /// reaches it marks it reached and the walk goes on. A walk to a target ends in range of it
    /// (the waypoint reached); a walk to a point with a range ends where its top path ends (the
    /// unit stops, the step returns 2). A walk that may meet an obstruction is not modelled.
    /// `walked` takes the waypoint once it is not left behind (`local_1860`). `groups` are the
    /// world's unit groups (none for a walk without the world's planners).
    pub(super) fn walk(&mut self, me: &Me, step: WalkStep<'_>) -> Result<Next, GenieError> {
        let WalkStep {
            time_step: dt,
            world,
            groups,
            pathing,
            goal,
            now,
            computer,
            include,
            walked,
            heading,
        } = step;
        let ground_radius = self.stats.radius.ground();
        // LAB_141153bd1: the waypoint's distance, the direction (slot 0x610), the step's end
        // (slot 0x638) and its query (FUN_14114cc60) start at +0x824, where the unit last moved to
        let position = self.moved_at.vector();
        let level = &self.paths[0];
        let waypoint = point_or_zero_of(level, level.waypoint);
        if ground(position, waypoint).get() < EPSILON {
            self.paths[0].flags |= LevelFlags::REACHED | LevelFlags::AT_WAYPOINT;
            return Ok(Next::Again);
        }
        let direction = self.direction_to(position, waypoint);
        *heading = direction;
        let projected = self.projected(direction, dt);
        match reach(waypoint, position, projected) {
            Reach::Away => {
                self.paths[0].flags |= LevelFlags::REACHED | LevelFlags::AT_WAYPOINT;
                return Ok(Next::Again);
            }
            Reach::Reached => self.paths[0].flags |= LevelFlags::REACHED | LevelFlags::AT_WAYPOINT,
            Reach::Short if 2 < self.substitute_count.get() => {
                self.paths[0].flags |= LevelFlags::REACHED | LevelFlags::AT_WAYPOINT;
            }
            Reach::Short => {}
        }
        *walked = waypoint;
        if self.stats.radius.sized() {
            // FUN_14114e8a0 ignores the unit itself (and a building's annexes), then
            // FUN_14114cc60 asks for the step's hits, grown by the unit's radius times 1
            let (from, to) = self.query_ends(position, projected, world.map);
            let rect = Rect::spanning(from, to);
            let grow = self.stats.radius.x();
            world.check_known_records(rect.grown(Distance::new(grow)))?;
            let player = world.get(me.id).map(|p| p.player);
            let hits = world.walk_hits(WalkQuery {
                start: from,
                end: to,
                rect,
                grow: Distance::new(grow),
                ally_factor: self.stats.min_collision,
                player,
                ignore: &[RecordId::from(me.id)],
            })?;
            if !hits.is_empty() {
                let avoided =
                    self.avoid(me, (world, groups), &hits, projected, dt, (now, computer))?;
                if !avoided.kept.is_empty() {
                    include_kept(world, pathing, &avoided.kept, include)?;
                }
                if avoided.at_target {
                    // FUN_140f9b4d0 three times: bit 0 on and bit 1 off on the walked level, bit 2
                    // on for it and its parents; then back to the top of the step
                    let level = &mut self.paths[0];
                    level.flags = (level.flags | LevelFlags::REACHED) & !LevelFlags::AT_WAYPOINT;
                    for l in &mut self.paths {
                        l.flags |= LevelFlags::MET_TARGET;
                    }
                    return Ok(Next::Again);
                }
                self.pauses.extend(avoided.pauses);
                if !avoided.kept.is_empty() {
                    return self.go_round(me, world, &hits, computer);
                }
            }
        }
        let level = &self.paths[0];
        match goal.target {
            Some((id, t)) => {
                let range = level.range;
                let own =
                    footprint_gap(self.position(), ground_radius, t.position, t.radius) - range;
                let near = (self.position(), ground_radius);
                let gap = gate_gap(world, near, (id, t), own, |g| g - range);
                if gap.get() < EPSILON {
                    // in range of the target: the waypoint counts as reached
                    let level = &mut self.paths[0];
                    level.flags &= !LevelFlags::AT_WAYPOINT;
                    level.flags |= LevelFlags::REACHED;
                    return Ok(Next::Again);
                }
            }
            // a walk to a point with a range ends where its top level's path ends
            None if !level.path_flags.contains(PathFlags::LOOPS)
                && RANGE_FLOOR < level.range.get() =>
            {
                if let Some(&end) = self.top_path().and_then(|p| p.last()) {
                    let gap = footprint_gap(
                        self.position(),
                        ground_radius,
                        end.position().with_z(0.0),
                        Footprint::new(0.0, 0.0),
                    );
                    if gap - level.range < Distance::new(EPSILON) {
                        self.reset_level();
                        self.stop()?;
                        return Ok(Next::Done(Motion::Arrived));
                    }
                }
            }
            None => {}
        }
        let clipped = self.steer(direction, dt)?;
        self.control_speed(waypoint, clipped);
        Ok(Next::Done(Motion::Moving))
    }

    /// The motion step's end on a loop back at its first waypoint (`LAB_141155637`): the unit
    /// turns from its forward vector on the ground (`FUN_141194060` with 3 holds, which zeroes
    /// the height; `FUN_141027d00` divides by the length) toward `heading`, the direction the
    /// step's walk last took (`[RBP+0x148]`, zero when no walk ran in this step: the angle to it
    /// is then π/2, [`fmath::angle_between`]) ([`Mover::steer`]), controls its speed toward
    /// `walked` (slot 0x630, [`Mover::control_speed`]) and, its level waiting for a path (state
    /// 2), stops (slot 0x5d0).
    pub(super) fn loop_end(
        &mut self,
        dt: Seconds,
        walked: BVector,
        heading: BVector,
    ) -> Result<Motion, GenieError> {
        let clipped = self.steer(heading, dt)?;
        self.control_speed(walked, clipped);
        if self
            .paths
            .first()
            .is_some_and(|l| l.state == PathState::WantsPath)
        {
            self.stop()?;
        }
        Ok(Motion::Moving)
    }
}
