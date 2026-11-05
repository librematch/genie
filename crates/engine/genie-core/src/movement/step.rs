//! The velocity step, a new object's setup, the position setter and the motion step's loop.

use crate::objects::layers::Speedup;

use crate::combat::missile::grounded as missile_grounded;
use crate::groups::UnitGroups;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Rect;
use crate::map::spatial::{Placed, RecordId, Spatial, WalkQuery};
use crate::map::terrain::Terrain;
use crate::math::Angle;
use crate::math::fmath::BVector;
use crate::math::fmath::at_most_or_unordered;
use crate::movement::LevelFlags;
use crate::movement::LevelsExt;
use crate::movement::MoveState;
use crate::movement::PathState;
use crate::movement::Speed;
use crate::movement::StepPlans;
use crate::movement::TopSpeed;
use crate::movement::{
    AfterRequest, EPSILON, FAR, Goal, Me, Motion, MoveStats, Mover, Next, Planning, Reach, Reached,
    footprint_gap, gate_gap, ground, include_kept, moved_far, pause_holds, point_or_zero_of, reach,
};
use crate::movement::{SubstituteCount, WaitDelays};
use crate::objects::ObjectId;
use crate::objects::facing::SpriteFacets;
use crate::objects::layers::Animated;
use crate::objects::layers::Static;
use crate::objects::sprite::Graphic;
use crate::pathing::Pathing;
use crate::players::PlayerKind;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::layer::Roles;
use crate::world::layer::generated::MovementTuning;
use crate::world::layer::generated::SpawnTuning;
use crate::world::layer::generated::VisibilityTuning;
use crate::world::objects_ref::ObjectsRef;
use crate::{GenieError, GenieResult};

mod ends;

/// [`Mover::level_gaps`]'s measure: the target with its place (none for a level without one or
/// a target that is gone), the gap to it, and the gap to it, else to where the top level's path
/// ends.
type LevelGaps = (Option<(ObjectId, Placed)>, Distance, Distance);

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
    /// No own move (reason −1): a speed below `MovementTuning::uncapped_speed` is not capped by the top
    /// speed.
    Free,
    /// An own move of another reason: the group's top speed (`+0xbc`) while the unit's walked
    /// level walks (state 3) unpaused, else the unit stands.
    OwnMove(Speed),
}

/// What the walk along the level's path takes of the step ([`Mover::walk`]'s arguments after
/// the unit).
#[derive(Debug)]
pub(super) struct WalkStep<'world> {
    /// The step's time.
    pub(super) time_step: Seconds,
    /// The world's spatial index.
    pub(super) world: &'world mut Spatial,
    /// The world's unit groups (none for a walk without the world's planners).
    pub(super) groups: Option<&'world UnitGroups>,
    /// The world's pathers (none likewise).
    pub(super) pathing: Option<&'world mut Pathing>,
    /// The goal the level walks to.
    pub(super) goal: Goal<'world>,
    /// The world's time.
    pub(super) now: WorldTime,
    /// The unit's player's type.
    pub(super) player_kind: PlayerKind,
    /// The objects the old pather tests even moving.
    pub(super) include: &'world mut Vec<ObjectId>,
    /// The waypoint once it is not left behind (`local_1860`).
    pub(super) walked: &'world mut BVector,
    /// The direction the unit turns to (slot 0x610).
    pub(super) heading: &'world mut BVector,
}

impl Mover {
    /// Slot 0x5e0 (`FUN_14114f300`): the forward speed the velocity step takes toward `wanted`
    /// (the acceleration times dt plus the speed). A unit its group steers (`group`) takes the
    /// group's answer ([`GroupSpeed`]; a free one below `MovementTuning::uncapped_speed`, `COMISS` then `JA`,
    /// only the floor). Any other is capped by its top speed `top` (slot 0x208; `COMISS` then
    /// `JA`: a NaN is not above it), then floored at 0 (a NaN stays).
    #[must_use]
    pub fn next_speed(
        &self,
        wanted: Speed,
        group: Option<GroupSpeed>,
        top: Speed,
        movement_tuning: &MovementTuning,
    ) -> Speed {
        match group {
            Some(GroupSpeed::Aside) => return Speed::ZERO,
            Some(GroupSpeed::OwnMove(speed)) => {
                // the walked level walks (state 3) and its pause is not above 0 (`COMISS`
                // then `JA`: a NaN pause walks)
                let walks = self.paths.first().is_some_and(|path_data| {
                    path_data.state == PathState::Walking
                        && at_most_or_unordered(path_data.pause_time.get(), 0.0)
                });
                return if walks { speed } else { Speed::ZERO };
            }
            Some(GroupSpeed::Free) if wanted.get() < movement_tuning.uncapped_speed => {}
            Some(GroupSpeed::Free) | None => {
                if top.get() < wanted.get() {
                    return top;
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
    /// the unit's group's steer, `top` what its top speed reads at `position`), then the unit
    /// moves `dt × speed` along its forward vector (slot 182, `0x141158490`) at the world's time
    /// `now`. Returns the position after it.
    pub fn step_velocity(
        &mut self,
        position: Position,
        dt: Seconds,
        map: GridSize,
        terrain: Option<&Terrain>,
        (now, group, top): (WorldTime, Option<GroupSpeed>, TopSpeed),
        movement_tuning: &MovementTuning,
    ) -> Position {
        let Some(velocity) = self.velocity.as_ref() else {
            return position;
        };
        let top = self.top_speed(top, position.ground());
        let speed = self.next_speed(
            Speed::new(velocity.acceleration.y() * dt + velocity.velocity.y()),
            group,
            top,
            movement_tuning,
        );
        if let Some(velocity) = self.velocity.as_mut() {
            velocity.velocity = velocity.velocity.with_y(speed.get());
        }
        // the position setter runs only for a distance it moves
        let Some(at) = self.move_forward(
            Distance::new(dt.get() * speed.get()),
            top,
            map,
            movement_tuning,
        ) else {
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
    pub fn grounded(&self, at: Position, terrain: Option<&Terrain>) -> Position {
        let height = at.z();
        let z = terrain
            .and_then(|terrain| terrain.ground(at.ground()).ok())
            .map_or(height, |ground| {
                missile_grounded(at.z(), ground, self.stats.fly_mode)
            });
        at.with_z(z)
    }

    /// The moving setup (`FUN_1411580a0`) of a new object of `stats` at `at`: it faces x (its
    /// right −z), does not wait, has no path and moveForward's factor 1; the position setter
    /// (`FUN_141161950`) makes its place a move at `now`. Its top speed is the one of the sprite
    /// it holds, `shown` (its standing one, slot 0x798: [`MoveStats::show`]), not the master's
    /// speed with its running sprite. Its angle and its sprite's facets are the caller's. Every
    /// moving object (types 30 and up) gets it, a building too, though it never moves: the walk
    /// reads the last move time of a building it meets (slot 0x5e8, `FUN_14106d2d0`).
    pub(crate) fn new_object(
        mut stats: MoveStats,
        shown: Option<&Graphic>,
        at: Position,
        now: WorldTime,
        angle: Angle,
        sprite_facets: Option<SpriteFacets>,
        spawn_tuning: &SpawnTuning,
    ) -> Self {
        stats.show(shown);
        Self {
            // the animated object's constructor (FUN_141070cf0): animation speed (+0x7b8) 0
            animated: Animated::new(Speed::ZERO),
            forward: BVector::new(1.0, 0.0, 0.0),
            right: BVector::new(0.0, 0.0, spawn_tuning.start_right_z),
            velocity: None,
            paths: Vec::new(),
            factor: 1.0,
            stats,
            waiting: 0,
            wait_delays: WaitDelays::NONE,
            attack_moving: false,
            last_move: now,
            moved_at: at,
            substitute_count: SubstituteCount::NONE,
            speedup: Speedup::NONE,
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
    /// setter (the unit's top speed `top`); a distance below 1e-7 moves nothing.
    pub(super) fn move_forward(
        &self,
        distance: Distance,
        top: Speed,
        map: GridSize,
        movement_tuning: &MovementTuning,
    ) -> Option<Position> {
        let distance = distance * self.factor;
        if distance.get().abs() < EPSILON {
            return None;
        }
        let to = self.moved_at.vector() + distance.get() * self.forward;
        Some(self.set_position(to.position(), top, map, movement_tuning))
    }

    /// The unit's place setter from its moving layer: its place `place` (the row's static
    /// layer) moves to `at` ([`crate::objects::layers::Static::new_position`]), its shape going
    /// from the mesh while it moves (slot 185, [`Mover::is_moving`]).
    pub(crate) fn new_position(
        &self,
        place: &mut Static,
        id: ObjectId,
        at: Position,
        places: (&mut Spatial, Option<&mut Pathing>),
        roles: &Roles,
        visibility_tuning: VisibilityTuning,
    ) -> GenieResult<()> {
        place.new_position(
            id,
            at,
            MoveState::of_mover(Some(self)),
            places,
            roles,
            visibility_tuning,
        )
    }

    /// The position setter (slot 11 -> `FUN_141161950` -> `0x1411a58b0`): a unit that can move
    /// (its top speed `top`, slot 0x208 where it stands, above 1e-7) keeps its footprint 0.01
    /// inside the map, then any position stays 0.001 below the map's size.
    pub(crate) fn set_position(
        &self,
        at: Position,
        top: Speed,
        map: GridSize,
        movement_tuning: &MovementTuning,
    ) -> Position {
        let (x, y) = (at.x(), at.y());
        let inside = |coordinate: f32, radius: f32, size: i32| {
            let low = radius + movement_tuning.edge_gap;
            if coordinate < low {
                low
            } else if size as f32 - low < coordinate {
                (size as f32 - radius) - movement_tuning.edge_gap
            } else {
                coordinate
            }
        };
        // both clamps read the map's +0x18, its width (0x14116220d for y): on a map that is not
        // square the y clamp takes the width
        let (x, y) = if EPSILON < top.get() {
            (
                inside(x, self.stats.radius.x(), map.width()),
                inside(y, self.stats.radius.y(), map.width()),
            )
        } else {
            (x, y)
        };
        let on_map = |raw: f32, size: i32| {
            let coordinate = raw.max(0.0);
            if size as f32 <= coordinate {
                size as f32 - movement_tuning.edge_position_gap
            } else {
                coordinate
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
        place: &mut Static,
        me: &Me,
        dt: Seconds,
        world: &mut Spatial,
        mut plan: Option<&mut Planning<'_>>,
        by_id: ObjectsRef<'_>,
    ) -> GenieResult<Motion> {
        let result = self.motion_passes(place, me, dt, world, plan.as_deref_mut(), by_id);
        // every exit of the step removes the kept hits' temporary shapes (FUN_141055970)
        if let Some(planning) = plan {
            planning.pathing.drop_temporary_shapes();
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
        at: Position,
        world: &Spatial,
        target_id: Option<ObjectId>,
        ground_radius: Footprint,
        by_id: ObjectsRef<'_>,
    ) -> GenieResult<LevelGaps> {
        let movement_tuning = &by_id.tuning().movement;
        let roles = by_id.roles();
        // FUN_1415417a0 on the level's target: none for -1 and for an object that is gone,
        // which then measures as a walk to a point (LAB_1411512df)
        let target = match target_id {
            None => None,
            Some(id) => world.get_known(id)?.map(|&placed| (id, placed)),
        };
        // the gap to the target, else to where the top level's path ends; a gate's is the
        // least over it and its first two annexes (slot 0x680), each annex's less 0.1
        // (`MINSS`, 0x1411512b8)
        let gap = target.as_ref().map_or(Distance::new(FAR), |(id, placed)| {
            let own = footprint_gap(at, ground_radius, placed.position, placed.radius);
            let near = (at, ground_radius);
            gate_gap(
                world,
                near,
                (*id, placed),
                own,
                |distance| distance - Distance::new(movement_tuning.range_floor),
                by_id,
                roles,
            )
        });
        let to_end = match (&target, self.paths.len()) {
            (Some(_), _) => gap,
            (None, 1) => Distance::new(FAR),
            (None, _) => {
                self.top_path()
                    .and_then(|path| path.last())
                    .map_or(Distance::new(FAR), |&end| {
                        footprint_gap(
                            at,
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
        (at, me): (Position, &Me),
        (world, by_id): (&mut Spatial, ObjectsRef<'_>),
        plan: Option<&mut Planning<'_>>,
        goal: Goal<'_>,
        plans: &mut StepPlans,
        include: &[ObjectId],
    ) -> GenieResult<Next> {
        match self.paths.walked()?.state {
            PathState::Arrived | PathState::Unplanned => self.arrived(goal),
            PathState::WantsPath => {
                let plan = plan.ok_or(GenieError::unsupported(
                    "a path request without the world's planners",
                ))?;
                Ok(
                    match self.request((at, me), (world, by_id), plan, goal, plans, include)? {
                        AfterRequest::Again => Next::Again,
                        AfterRequest::Walk => Next::Dispatch,
                        AfterRequest::Paused => Next::Done(Motion::Moving),
                        AfterRequest::GaveUp => Next::Done(Motion::Done),
                    },
                )
            }
            PathState::Idle | PathState::Walking | PathState::GaveUp => Ok(Next::Dispatch),
        }
    }

    /// [`Mover::motion`]'s passes.
    pub(super) fn motion_passes(
        &mut self,
        place: &mut Static,
        me: &Me,
        dt: Seconds,
        world: &mut Spatial,
        mut plan: Option<&mut Planning<'_>>,
        by_id: ObjectsRef<'_>,
    ) -> GenieResult<Motion> {
        let movement_tuning = &by_id.tuning().movement;
        // FUN_141194060 with 2: a top speed of 0 (slot 0x208) ends the step
        if self.top_speed(me.top_speed, place.position().ground()) == Speed::ZERO {
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
        let mut plans = StepPlans::ZERO;
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
            let Some(level) = self.paths.first().filter(|path_data| path_data.has_path) else {
                return Ok(Motion::Done);
            };
            let (target, gap, to_end) =
                self.level_gaps(place.position(), world, level.target, ground_radius, by_id)?;
            let goal = Goal {
                target: target.as_ref().map(|(id, placed)| (*id, placed)),
                gap,
                to_end,
            };
            if self.paths.walked()?.flags.contains(LevelFlags::REACHED) {
                let now = plan
                    .as_deref()
                    .map_or(WorldTime::ZERO, |planning| planning.now);
                let pathing = plan.as_deref_mut().map(|planning| &mut *planning.pathing);
                match self.reached((place, me), world, pathing, goal, now, by_id)? {
                    Reached::Pass(Next::Again) => continue,
                    Reached::Pass(Next::Dispatch) => {}
                    Reached::Pass(Next::Done(motion)) => return Ok(motion),
                    Reached::LoopEnd => {
                        let top = self.top_speed(me.top_speed, place.position().ground());
                        return self.loop_end(dt, walked, heading, top, movement_tuning);
                    }
                }
            }
            match self.level_state(
                (place.position(), me),
                (world, by_id),
                plan.as_deref_mut(),
                goal,
                &mut plans,
                &include,
            )? {
                Next::Done(motion) => return Ok(motion),
                Next::Again => continue,
                // a link of another type: on past the request (state 4 or 5) to the walk
                Next::Dispatch => {}
            }
            let (now, player_kind) = plan
                .as_deref()
                .map_or((WorldTime::ZERO, PlayerKind::Phantom), |planning| {
                    (planning.now, planning.player_kind)
                });
            let groups = plan.as_deref().map(|planning| planning.groups);
            let pathing = plan.as_deref_mut().map(|planning| &mut *planning.pathing);
            let step = self.walk(
                place.position(),
                me,
                WalkStep {
                    time_step: dt,
                    world,
                    groups,
                    pathing,
                    goal,
                    now,
                    player_kind,
                    include: &mut include,
                    walked: &mut walked,
                    heading: &mut heading,
                },
                by_id,
            )?;
            if let Next::Done(motion) = step {
                return Ok(motion);
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
    pub(super) fn walk(
        &mut self,
        at: Position,
        me: &Me,
        step: WalkStep<'_>,
        by_id: ObjectsRef<'_>,
    ) -> GenieResult<Next> {
        let movement_tuning = &by_id.tuning().movement;
        let WalkStep {
            time_step: dt,
            world,
            groups,
            pathing,
            goal,
            now,
            player_kind,
            include,
            walked,
            heading,
        } = step;
        // LAB_141153bd1: the waypoint's distance, the direction (slot 0x610), the step's end
        // (slot 0x638) and its query (FUN_14114cc60) start at +0x824, where the unit last moved to
        let position = self.moved_at.vector();
        let level = self.paths.walked()?;
        let waypoint = point_or_zero_of(level, level.waypoint);
        if ground(position, waypoint).get() < EPSILON {
            self.paths.walked_mut()?.flags |= LevelFlags::REACHED | LevelFlags::AT_WAYPOINT;
            return Ok(Next::Again);
        }
        let direction = self.direction_to(position, waypoint);
        *heading = direction;
        // slot 0x208 from the unit's place, which the walk leaves as it is
        let top = self.top_speed(me.top_speed, at.ground());
        let projected = self.projected(direction, dt, top);
        match reach(waypoint, position, projected) {
            Reach::Away => {
                self.paths.walked_mut()?.flags |= LevelFlags::REACHED | LevelFlags::AT_WAYPOINT;
                return Ok(Next::Again);
            }
            Reach::Reached => {
                self.paths.walked_mut()?.flags |= LevelFlags::REACHED | LevelFlags::AT_WAYPOINT;
            }
            Reach::Short if 2 < self.substitute_count.get() => {
                self.paths.walked_mut()?.flags |= LevelFlags::REACHED | LevelFlags::AT_WAYPOINT;
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
            let player = world.get(me.id).map(|placed| placed.player);
            let hits = world.walk_hits(WalkQuery {
                start: from,
                end: to,
                rect,
                grow: Distance::new(grow),
                ally_factor: self.stats.min_collision,
                player,
                ignore: &[RecordId::from(me.id)],
                objects: by_id,
            })?;
            if !hits.is_empty() {
                let avoided = self.avoid(
                    (at, me),
                    (world, groups, by_id),
                    &hits,
                    projected,
                    dt,
                    (now, player_kind),
                )?;
                if !avoided.kept.is_empty() {
                    include_kept(world, pathing, &avoided.kept, include)?;
                }
                if avoided.at_target {
                    // FUN_140f9b4d0 three times: bit 0 on and bit 1 off on the walked level, bit 2
                    // on for it and its parents; then back to the top of the step
                    let level = self.paths.walked_mut()?;
                    level.flags = (level.flags | LevelFlags::REACHED) & !LevelFlags::AT_WAYPOINT;
                    for path_data in &mut self.paths {
                        path_data.flags |= LevelFlags::MET_TARGET;
                    }
                    return Ok(Next::Again);
                }
                self.pauses.extend(avoided.pauses);
                if !avoided.kept.is_empty() {
                    return self.go_round(me, world, &hits, player_kind, by_id);
                }
            }
        }
        if let Some(next) = self.walk_ends(at, world, goal, by_id)? {
            return Ok(next);
        }
        let clipped = self.steer(direction, dt, movement_tuning)?;
        self.control_speed(waypoint, clipped, top);
        Ok(Next::Done(Motion::Moving))
    }
}
