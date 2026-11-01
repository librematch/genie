//! A reached waypoint and what follows it: the next waypoint, a level's end and the moving state.

use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::map::Rect;
use crate::map::spatial::{Spatial, WalkQuery};
use crate::math::fmath::BVector;
use crate::movement::LevelFlags;
use crate::movement::LinkedPathType;
use crate::movement::MoveAlgorithm;
use crate::movement::PathFlags;
use crate::movement::PathState;
use crate::movement::WaypointIndex;
use crate::movement::WaypointLevel;
use crate::movement::{
    CLOSE, EPSILON, Goal, LOOK_AHEAD, Me, Motion, Mover, Next, PathData, RANGE_FLOOR,
    STRAIGHT_GROW, TARGET_SHORT_REACH, drop_edge_waypoints, footprint_gap, gate_gap,
    point_or_zero_of, remove_point,
};
use crate::objects::ObjectId;
use crate::pathing::Pathing;
use crate::world::WorldTime;
use crate::{GenieError, math::fmath};

impl Mover {
    /// The waypoint the level walks to was reached (`LAB_14115136c`, the level's bit 0): with
    /// bit 1 the unit takes the waypoint's place (slot 0x58, the position setter); the level moves
    /// on to its next waypoint, dropping those too near the map's edge for the unit
    /// (`FUN_140f98d50`). Then (`LAB_14115192d`) a level with waypoints left waits for a path to
    /// the next (state 2, not level 0); at its last, a unit not yet in range of where it was going
    /// steps back to the waypoint it left and asks again, else the level arrived (state 4). Level
    /// 0 on a level 1 parent looks ahead first ([`Mover::look_ahead`]). A loop (the path's flags
    /// bit 0) moves on from the waypoint reached instead (`FUN_140f98bc0` with 1,
    /// [`PathData::looping_next`]) and waits for a path to it (state 2): back at its first waypoint the step
    /// ends ([`Mover::loop_end`]), else on to the level's state. A waypoint some object stands on
    /// is not modelled.
    pub(super) fn reached(
        &mut self,
        me: &Me,
        world: &mut Spatial,
        pathing: Option<&mut Pathing>,
        goal: Goal<'_>,
        now: WorldTime,
    ) -> Result<Next, GenieError> {
        let ground_radius = self.stats.radius.ground();
        if self.paths[0].flags.contains(LevelFlags::AT_WAYPOINT) {
            self.take_waypoint_place(me, world, pathing, now)?;
        }
        let level = &mut self.paths[0];
        level.flags &= !LevelFlags::REACHED;
        level.waypoint = level.waypoint.next();
        drop_edge_waypoints(level, self.stats.radius, world.map);
        if level.path_flags.contains(PathFlags::LOOPS) {
            let reached = level.waypoint.previous();
            level.waypoint = level.looping_next(reached);
            level.state = PathState::WantsPath;
            return Ok(if level.waypoint == WaypointIndex::FIRST {
                Next::LoopEnd
            } else {
                Next::Dispatch
            });
        }
        self.drop_held_waypoint(world)?;
        let parent_is_level_1 = self
            .paths
            .get(1)
            .is_some_and(|p| p.level == WaypointLevel::PLANNED && !p.points.is_empty());
        if self.paths[0].level == WaypointLevel::WALKED
            && parent_is_level_1
            && self.look_ahead(me, world)?
        {
            return Ok(Next::Again);
        }
        // LAB_14115192d
        let level = &self.paths[0];
        let left = point_or_zero_of(level, level.waypoint.previous());
        let back = footprint_gap(
            self.position(),
            ground_radius,
            Position::new(left.x(), left.y(), 0.0),
            Footprint::new(0.0, 0.0),
        );
        let range = level.range;
        let mut beyond =
            goal.target
                .map_or((back - range) - Distance::new(RANGE_FLOOR), |(id, t)| {
                    let own = (footprint_gap(self.position(), ground_radius, t.position, t.radius)
                        - range)
                        - Distance::new(RANGE_FLOOR);
                    let near = (self.position(), ground_radius);
                    Distance::new(gate_gap(world, near, (id, t), own, |g| {
                        (g - range.get()) - RANGE_FLOOR
                    }))
                });
        if goal.target.is_some() && level.flags.contains(LevelFlags::MET_TARGET) {
            self.clear_bit_2();
            beyond = Distance::ZERO;
        }
        let level = &mut self.paths[0];
        if level.waypoint.within(level.points.len()) {
            if level.level != WaypointLevel::WALKED {
                level.state = PathState::WantsPath;
            }
            return Ok(Next::Dispatch);
        }
        if EPSILON <= beyond.get()
            && (goal.target.is_some()
                || CLOSE <= beyond.get()
                || level.level == WaypointLevel::WALKED)
        {
            if EPSILON <= back.get() {
                level.waypoint = level.waypoint.previous();
                level.state = PathState::WantsPath;
                return Ok(Next::Dispatch);
            }
            if level.level == WaypointLevel::TOP {
                return self.level_2_short(me, world, goal);
            }
        }
        level.state = PathState::Arrived;
        Ok(Next::Dispatch)
    }

    /// [`Mover::reached`]'s bit 1: the unit takes the waypoint's place (slot 0x58, the position
    /// setter, on the ground), and the bit goes.
    pub(super) fn take_waypoint_place(
        &mut self,
        me: &Me,
        world: &mut Spatial,
        pathing: Option<&mut Pathing>,
        now: WorldTime,
    ) -> Result<(), GenieError> {
        let level = &self.paths[0];
        if level.waypoint.within(level.points.len()) {
            let p = level.point(level.waypoint);
            let at = self.set_position(p.position(), world.map);
            let at = self.grounded(at, world.terrain.as_ref());
            self.moved_to(me, at, world, pathing, now)?;
        }
        self.paths[0].flags &= !LevelFlags::AT_WAYPOINT;
        Ok(())
    }

    /// [`Mover::reached`]'s next waypoint on a level but 0: when the unit lies within
    /// [`TARGET_SHORT_REACH`] (squared) of the one after it, the manager's point query at it
    /// (`FUN_141051900` with flags 0x20; its buffer and ignore list as the last
    /// `FUN_1410562d0` and `FUN_1410562e0` left them; a gate the owner may pass has no record
    /// during its owner's update, [`crate::world::World::open_gates`], so slot 0x678 passes none
    /// of those found) drops it when an object holds it.
    ///
    /// # Errors
    ///
    /// A frozen object the query may meet.
    pub(super) fn drop_held_waypoint(&mut self, world: &Spatial) -> Result<(), GenieError> {
        let level = &self.paths[0];
        let next = level.waypoint.next();
        if level.level == WaypointLevel::WALKED || !next.within(level.points.len()) {
            return Ok(());
        }
        let d = fmath::sub(self.position().vector(), level.points[next.get() as usize]);
        // a NaN distance is not within
        if d.x() * d.x() + d.height() * d.height() + d.y() * d.y() < TARGET_SHORT_REACH {
            let [x, _, y] = (level.points[level.waypoint.get() as usize]).get();
            let ignore = world.manager_ignore();
            if world.object_holds(Point::new(x, y), world.manager_grow(), &ignore, false)? {
                let level = &mut self.paths[0];
                let removed = remove_point(&mut level.points, level.waypoint);
                debug_assert!(removed, "the waypoint lies within the path");
            }
        }
        Ok(())
    }

    /// [`Mover::reached`] at level 2's last waypoint short of its goal (`0x141154f8a`): a walker
    /// without a weapon range (slot 0x230; `UCOMISS`, a NaN has one) beside a target that moved
    /// since the level's path was made (slot 0x5e8, 0 for an object that never moves, against
    /// the path's `+8`; on a tie the target of a higher player number or a higher id) lets its
    /// path go and stops: it arrived (`FUN_140f98e80`, slot 0x5d0; the step returns 2). Else the
    /// level takes state 6 and the unit stops (1). The step's temporary shapes go either way.
    ///
    /// # Errors
    ///
    /// What the stop does not model.
    pub(super) fn level_2_short(
        &mut self,
        me: &Me,
        world: &Spatial,
        goal: Goal<'_>,
    ) -> Result<Next, GenieError> {
        let level = &self.paths[0];
        let created = level.created;
        let own = world.get(me.id).map(|p| p.player);
        let yields = self.stats.weapon_range == Distance::ZERO
            && goal.target.is_some_and(|(target, t)| {
                let moved = world.gait(target).map_or(WorldTime::ZERO, |g| g.last_move);
                moved > created
                    || (moved == created && (own.is_some_and(|o| o < t.player) || me.id < target))
            });
        if yields {
            self.reset_level();
            self.stop()?;
            return Ok(Next::Done(Motion::Arrived));
        }
        self.paths[0].state = PathState::GaveUp;
        self.stop()?;
        Ok(Next::Done(Motion::Done))
    }

    /// Level 0's look ahead on its level 1 parent, at a waypoint it reached (`LAB_14115136c`):
    /// from the parent's fifth waypoint on (at most its last) back toward its current one, the
    /// first within 10 tiles (squared, `DAT_14330ebcc`) that the unit can walk to straight
    /// (`FUN_141142750`) becomes the parent's waypoint; level 0 goes, and the parent asks for a
    /// path (state 2) as the motion step starts again. True when it did.
    pub(super) fn look_ahead(&mut self, me: &Me, world: &mut Spatial) -> Result<bool, GenieError> {
        let target = self.paths[0].target;
        let parent = &self.paths[1];
        // the motion step's place of the unit: +0x824, where it last moved to
        let at = self.moved_at.vector();
        let last = WaypointIndex::last_of(parent.points.len());
        let from = parent.waypoint;
        let mut k = from.ahead(5).min(last);
        while from < k {
            let p = parent.points[k.get() as usize];
            let d = fmath::sub(p, at);
            if d.x() * d.x() + 0.0 + d.y() * d.y() <= LOOK_AHEAD
                && self.straight(me, p, target, world)?
            {
                break;
            }
            k = k.previous();
        }
        if k == from {
            return Ok(false);
        }
        let _ = self.paths.remove(0);
        let parent = &mut self.paths[0];
        parent.waypoint = k;
        parent.state = PathState::WantsPath;
        Ok(true)
    }

    /// `FUN_141142750` with its last argument 1: whether the unit walks straight to `p`. The point
    /// must lie within the map less the unit's radii (the game takes the height radius for the
    /// low edge in y); then the walk's query (`FUN_14114cc60` with the tiles' records, flags
    /// `0x190`, its radius times 0.5) must find nothing but the unit and its target with their
    /// annexes ([`Spatial::query_ignore`]): no tile its
    /// terrain mask lacks, no static record (kinds 2, 11, 13) ([`Spatial::segment_blocked`]).
    /// The game also writes the straight path into a scratch path, which it drops.
    pub(super) fn straight(
        &self,
        me: &Me,
        p: BVector,
        target: Option<ObjectId>,
        world: &mut Spatial,
    ) -> Result<bool, GenieError> {
        let (rx, ry, rz) = (
            self.stats.radius.x(),
            self.stats.radius.y(),
            self.stats.radius.z(),
        );
        let (w, h) = (world.map.width(), world.map.height());
        let (x, y) = (p.x(), p.y());
        if !(rx <= x && rz <= y && x <= w as f32 - rx && y <= h as f32 - ry) {
            return Ok(false);
        }
        let ignore = world.query_ignore(&[Some(me.id), target]);
        // the query starts at +0x824, where the unit last moved to
        let (from, to) = self.query_ends(self.moved_at.vector(), p, world.map);
        let grow = rx * STRAIGHT_GROW;
        world.check_known_records(Rect::new(
            from.x().min(to.x()) - grow,
            from.y().min(to.y()) - grow,
            from.x().max(to.x()) + grow,
            from.y().max(to.y()) + grow,
        ))?;
        let blocked = world.segment_blocked(
            from,
            to,
            Distance::new(grow),
            self.stats.restriction_mask,
            &ignore,
        );
        // FUN_14114cc60 resets the buffer it set for the query (FUN_1410562d0 with 0)
        world.set_manager_buffer(None);
        Ok(!blocked)
    }

    /// `FUN_141142750` with its last argument 0, the close-in path to a waypoint or target nearer
    /// than a tile: the point must lie within the map less the unit's radii (as
    /// [`Mover::straight`]); then the walk's query from the unit to the point (`FUN_14114cc60`
    /// with the tiles' records, flags `0x90`, its radius) must find nothing but the unit and its
    /// target: no tile its terrain mask lacks ([`Spatial::tiles_block`]) and no record the walk
    /// hits ([`Spatial::walk_hits`]). The game writes the straight path into the new level.
    pub(super) fn close_in(
        &self,
        me: &Me,
        p: BVector,
        target: Option<ObjectId>,
        world: &mut Spatial,
    ) -> Result<bool, GenieError> {
        let (rx, ry, rz) = (
            self.stats.radius.x(),
            self.stats.radius.y(),
            self.stats.radius.z(),
        );
        let (w, h) = (world.map.width(), world.map.height());
        let (x, y) = (p.x(), p.y());
        if !(rx <= x && rz <= y && x <= w as f32 - rx && y <= h as f32 - ry) {
            return Ok(false);
        }
        let ignore = world.query_ignore(&[Some(me.id), target]);
        // the query starts at +0x824, where the unit last moved to
        let (from, to) = self.query_ends(self.moved_at.vector(), p, world.map);
        let rect = Rect::spanning(from, to);
        world.check_known_records(rect.grown(Distance::new(rx)))?;
        if world.tiles_block(from, to, Distance::new(rx), self.stats.restriction_mask) {
            // FUN_14114cc60 resets the buffer it set for the query (FUN_1410562d0 with 0)
            world.set_manager_buffer(None);
            return Ok(false);
        }
        let player = world.get(me.id).map(|q| q.player);
        let hits = world.walk_hits(WalkQuery {
            start: from,
            end: to,
            rect,
            grow: Distance::new(rx),
            ally_factor: self.stats.min_collision,
            player,
            ignore: &ignore,
        })?;
        Ok(hits.is_empty())
    }

    /// An arrived level (states 4 and 5, `LAB_141151b6d`): the top level, or one in range of its
    /// target, is reset (`FUN_140f98e80`) and the unit stops (slot 0x5d0): the step returns 2. Any
    /// other hands over to its parent by its link (`mLinkedPathType`): 1 marks the parent's
    /// waypoint reached, 2 has the parent ask again (a looping parent moves on first), 3 has it
    /// ask again; the step starts again with the parent.
    pub(super) fn arrived(&mut self, goal: Goal<'_>) -> Result<Next, GenieError> {
        let level = &self.paths[0];
        if self.paths.len() == 1 || goal.gap <= level.range {
            self.reset_level();
            self.stop()?;
            return Ok(Next::Done(Motion::Arrived));
        }
        match level.linked_type {
            LinkedPathType::REACHES_WAYPOINT => {
                let _ = self.paths.remove(0);
                self.paths[0].flags |= LevelFlags::REACHED;
            }
            LinkedPathType::MOVES_ON => {
                let _ = self.paths.remove(0);
                let parent = &mut self.paths[0];
                parent.state = PathState::WantsPath;
                // a looping parent path moves on (FUN_140f98bc0 with 1)
                if parent.has_path && parent.path_flags.contains(PathFlags::LOOPS) {
                    parent.waypoint = parent.looping_next(parent.waypoint);
                }
            }
            LinkedPathType::ASKS_AGAIN => {
                let _ = self.paths.remove(0);
                self.paths[0].state = PathState::WantsPath;
            }
            // the game's switch has no case: the level stays and the step goes on past the
            // request to the walk (`LAB_141153bd1`)
            _ => return Ok(Next::Dispatch),
        }
        Ok(Next::Again)
    }

    /// `FUN_140f98e80` on the walked level: its path goes, its parents with it, and it holds the
    /// defaults (its tries back to 10, flags clear; its repath time stays).
    pub(crate) fn reset_level(&mut self) {
        self.paths.truncate(1);
        let level = &mut self.paths[0];
        *level = PathData {
            repath_time: level.repath_time,
            created: WorldTime::ZERO,
            has_path: false,
            path_flags: PathFlags::NONE,
            ..PathData::new(WorldTime::ZERO)
        };
    }

    /// The unit shows the sprite `shown` (None: none): its top speed (slot 0x208, the master's
    /// speed times the current sprite's factor, [`crate::objects::sprite::top_speed`]) is the
    /// sprite's. The one derivation of [`MoveStats::speed`] besides [`crate::objects::sprite::new_sprite`].
    pub(crate) fn shows(&mut self, shown: Option<&crate::objects::sprite::Graphic>) {
        self.stats.show(shown);
    }

    /// `FUN_140f9b4d0(level, 2, 0)`: bit 2 goes off the walked level and on for its parents.
    pub(super) fn clear_bit_2(&mut self) {
        self.paths[0].flags &= !LevelFlags::MET_TARGET;
        for parent in &mut self.paths[1..] {
            parent.flags |= LevelFlags::MET_TARGET;
        }
    }

    /// Slot 0x560 (`FUN_1411616f0`): the unit stops ([`Mover::stop`], slot 0x5d0) and its walked
    /// level, if any, resets (`FUN_140f98e80`, [`Mover::reset_level`]).
    ///
    /// # Errors
    ///
    /// As [`Mover::stop`].
    pub(crate) fn stand(&mut self) -> Result<(), GenieError> {
        self.stop()?;
        if !self.paths.is_empty() {
            self.reset_level();
        }
        Ok(())
    }

    /// Slot 0x5d0 (`FUN_14108c2f0` -> `FUN_141161730`): the unit stops: its speed and its
    /// acceleration along its forward vector go to 0 (slots 0x598 and 0x5a8, on velocity data it
    /// has), it stops waiting to move (`+0x7f9`, slot 0x2a0), its own ignore list (`+0x678`)
    /// empties and its animated layer's speed (`+0x7b8`) is 0 at once. A master whose move
    /// algorithm has bit 0 also resets the unit's sprite (`+0x78c`, `+0x958`), which is not
    /// modelled.
    pub(crate) fn stop(&mut self) -> Result<(), GenieError> {
        if self
            .stats
            .move_algorithm
            .contains(MoveAlgorithm::ALTERNATES_SPRITES)
        {
            return Err(GenieError::unsupported(
                "a stop that resets the sprite (move algorithm bit 0)",
            ));
        }
        if let Some(v) = self.velocity.as_mut() {
            v.velocity = v.velocity.with_y(0.0);
            v.acceleration = v.acceleration.with_y(0.0);
        }
        self.waiting = 0;
        self.ignore_units.clear();
        self.animated.speed = 0.0;
        Ok(())
    }

    /// The unit's place changed inside the motion step: the time of the last move
    /// (`FUN_141161950`, `+0x870`), then the place setter, which the world's places and the
    /// mesh follow (`0x1411a58b0` -> `FUN_14118c540`, [`Mover::new_position`]).
    pub(super) fn moved_to(
        &mut self,
        me: &Me,
        at: Position,
        world: &mut Spatial,
        pathing: Option<&mut Pathing>,
        now: WorldTime,
    ) -> Result<(), GenieError> {
        self.mark_move(at, now);
        self.new_position(me.id, at, (world, pathing))?;
        Ok(())
    }

    /// `FUN_140f96050` on the walked level: up its parents (`+0x8`) to the top level, the last of
    /// [`Mover::paths`], whose path has waypoints after its current one
    /// ([`PathData::has_more`]).
    pub(super) fn top_has_more(&self) -> bool {
        self.paths.last().is_some_and(PathData::has_more)
    }

    /// Object slot 185 (`0x1411583d0`): the unit has velocity data whose velocity is longer than 0.
    #[must_use]
    pub fn is_moving(&self) -> bool {
        self.velocity.as_ref().is_some_and(|v| {
            let [x, y, z] = v.velocity.get();
            0.0 < fmath::sqrt(x * x + y * y + z * z)
        })
    }
}
