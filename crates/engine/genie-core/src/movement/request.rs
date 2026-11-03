//! The motion step's path request: the long pather's plan, the walled-off stretches and the
//! goal's reach.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Position;
use crate::map::Rect;
use crate::map::spatial::{Placed, Spatial, WalkQuery};
use crate::math::fmath::BVector;
use crate::movement::LevelFlags;
use crate::movement::Levels;
use crate::movement::PathFlags;
use crate::movement::PathState;
use crate::movement::PlanTries;
use crate::movement::Speed;
use crate::movement::StepPlans;
use crate::movement::WaitDelays;
use crate::movement::WaypointIndex;
use crate::movement::WaypointLevel;
use crate::movement::request_plans::{NewPath, Way};
use crate::movement::{
    AfterRequest, EPSILON, Goal, LongWalker, Me, Mover, Planning, cut_path, footprint_gap, ground,
    long_path_for, may_plan, point_or_zero,
};
use crate::objects::ObjectId;
use crate::pathing::tile_path::Path;
use crate::world::index::ObjectsRef;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::generated::MovementTuning;

/// What the long pather is asked for a far waypoint ([`Mover::long_plan`]'s arguments after the
/// planning state).
#[derive(Clone, Copy, Debug)]
pub(super) struct LongAsk<'a> {
    /// The waypoint (x, height, y).
    pub(super) waypoint: BVector,
    /// The unit's ground distance to it.
    pub(super) way: Distance,
    /// How near the waypoint counts.
    pub(super) range: Distance,
    /// The target object, its id and record.
    pub(super) target: Option<(ObjectId, &'a Placed)>,
}

impl Mover {
    /// The motion step's path request for the level it walks, in state 2 (`0x141150f00`, its
    /// `_Memory[10] == 2` block): unless the unit is in range of its target (state 4) or on its
    /// waypoint, the level spends one of its tries and a new level gets a path to the waypoint:
    /// the short pather's (`FUN_140f9e540`) for a near waypoint, else or when that fails the tile
    /// pathfinder's (`FUN_140f7c530`; a top level's plan becomes a level of its own, whose
    /// stretches the next pass serves; the short pather takes the walk's kept hits, `include`, as
    /// its include list). The new level goes under the walked one, which waits
    /// (state 0). A waypoint or target nearer than a tile takes the close-in path when the short
    /// pather found none ([`Mover::close_in`]); a far waypoint takes the long pather's path
    /// ([`Mover::long_plan`]). The escape from a game type 12 tile, a trade cart's budget and the
    /// other exits that stop the unit (slot 0x5d0) are not modelled.
    pub(super) fn request(
        &mut self,
        (at, me): (Position, &Me),
        (world, by_id): (&mut Spatial, ObjectsRef<'_>),
        plan: &mut Planning<'_>,
        goal: Goal<'_>,
        plans: &mut StepPlans,
        include: &[ObjectId],
    ) -> Result<AfterRequest, GenieError> {
        let movement_tuning = &by_id.tuning().movement;
        let roles = by_id.roles();
        // the request measures and plans from +0x824 (`pfVar14`, `local_18c8`), where the unit
        // last moved to
        let position = self.moved_at.vector();
        if let Some(step) = self.in_reach(at, goal, position, movement_tuning)? {
            return Ok(step);
        }
        let solid = self.stats.radius.sized();
        let trade_cart = world
            .get(me.id)
            // The trade cart's class (19), whose path requests the owner's budget limits (`FUN_14165d6f0`).
            // The classes a long path's stretch may not cross (`0x141150f00` after `FUN_140f9eb70`, raw
            // `-0xe91`, `-0xe78`, `-0xea9`): walls, towers, and buildings that do not move; gates
            // ([`GATE`]) too. A villager's (`-0xea8`) path crosses them.
            .is_some_and(|p| roles.of_class(p.class).contains(ClassRoles::TRADE_CART));
        if solid && !self.plan_allowed(me, world, plan, trade_cart, plans)? {
            return Ok(AfterRequest::Paused);
        }
        if let Some(step) = self.spend_try()? {
            return Ok(step);
        }
        // the unit stops waiting to move (slot 0x2a0); a new level with a new path
        self.waiting = 0;
        let mut np = NewPath::new(plan.now);
        if solid {
            let level = self.paths.walked()?;
            let waypoint = level.point(level.waypoint);
            let w = Way {
                goal,
                include,
                position,
                waypoint,
                distance: ground(waypoint, position),
                radius: Distance::new(self.stats.radius.x()),
                level: level.level,
                range: level.range,
                target_id: level.target,
                trade_cart,
            };
            if let Some(step) = self.plan_solid((at, me), world, plan, &w, &mut np, by_id)? {
                return Ok(step);
            }
        } else {
            self.straight_way(position, &mut np)?;
        }
        if np.path.points.len() < 2 || np.path.length.get() < EPSILON {
            return self.no_path(
                np.found,
                goal.target.map(|(_, t)| t),
                goal.to_end,
                movement_tuning,
            );
        }
        self.link(np, solid, movement_tuning)
    }

    /// The request's first exits: a target within the level's range (at least
    /// `MovementTuning::range_floor`) puts the level in state 4; a unit on its waypoint marks the level
    /// (flag 1, and 2 when away from the target's footprint by more than [`EPSILON`] or
    /// without one). Both go again.
    pub(super) fn in_reach(
        &mut self,
        at: Position,
        goal: Goal<'_>,
        position: BVector,
        movement_tuning: &MovementTuning,
    ) -> Result<Option<AfterRequest>, GenieError> {
        let ground_radius = self.stats.radius.ground();
        let here = at;
        let level = self.paths.walked_mut()?;
        let reach = if level.range <= Distance::ZERO {
            movement_tuning.range_floor
        } else {
            level.range.get()
        };
        if goal.target.is_some() && goal.gap <= Distance::new(reach) {
            level.state = PathState::Arrived;
            return Ok(Some(AfterRequest::Again));
        }
        let waypoint = level.point(level.waypoint);
        if ground(position, waypoint) != Distance::ZERO {
            return Ok(None);
        }
        let away = goal.target.is_none_or(|(_, t)| {
            EPSILON < footprint_gap(here, ground_radius, t.position, t.radius).get()
        });
        if away {
            level.flags |= LevelFlags::AT_WAYPOINT;
        }
        level.flags |= LevelFlags::REACHED;
        Ok(Some(AfterRequest::Again))
    }

    /// Whether a solid unit may plan: a trade cart plans only within its owner's budget
    /// (`FUN_14165d6f0` on its wait delays); a fourth plan in one step, or no budget, stops the
    /// unit (slot 0x5d0) and counts a wait delay (`+0x7fa`), and the step returns 0. A plan
    /// counts for the owner (player `+0x67c`, `FUN_141663c50`) and for the step, and the wait
    /// delays start again.
    fn plan_allowed(
        &mut self,
        me: &Me,
        world: &Spatial,
        plan: &mut Planning<'_>,
        trade_cart: bool,
        plans: &mut StepPlans,
    ) -> Result<bool, GenieError> {
        let owner = world.get(me.id).map(|p| p.player);
        let budget = owner.map(|o| plan.pathing.budget(o));
        if !may_plan(trade_cart, budget, self.wait_delays, *plans) {
            self.stop()?;
            self.wait_delays = self.wait_delays.plus_one();
            return Ok(false);
        }
        if let Some(o) = owner {
            plan.pathing.count_plan(o);
        }
        *plans = plans.plus_one();
        self.wait_delays = WaitDelays::NONE;
        Ok(true)
    }

    /// The level spends a try unless its top path loops or the unit moves to attack. Out of
    /// tries: a level 2 short of its last waypoint gets its tries again and goes on (flag 1); a
    /// lower level, or a top without more points, resets and gives up (`FUN_140f9b4b0`:
    /// `FUN_140f98e80`, state 6; the unit stops, slot 0x5d0); else the level takes state 5.
    pub(super) fn spend_try(&mut self) -> Result<Option<AfterRequest>, GenieError> {
        let top_loops = self
            .paths
            .last()
            .is_some_and(|t| !t.points.is_empty() && t.path_flags.contains(PathFlags::LOOPS));
        if top_loops || self.attack_moving {
            return Ok(None);
        }
        let top_has_more = self.top_has_more();
        let level = self.paths.walked_mut()?;
        level.continue_counter = level.continue_counter.spent();
        if level.continue_counter.left() {
            return Ok(None);
        }
        if WaypointLevel::PLANNED < level.level || !top_has_more {
            if level.level == WaypointLevel::TOP
                && !level.waypoint.at_or_past_last(level.points.len())
            {
                level.flags |= LevelFlags::REACHED;
                level.continue_counter = PlanTries::FULL;
                return Ok(Some(AfterRequest::Again));
            }
            self.reset_level()?;
            self.paths.walked_mut()?.state = PathState::GaveUp;
            self.stop()?;
            return Ok(Some(AfterRequest::GaveUp));
        }
        level.state = PathState::Unplanned;
        Ok(Some(AfterRequest::Again))
    }

    /// A unit without a footprint: the straight way to its waypoint (to where it stands past
    /// the level's last).
    pub(super) fn straight_way(
        &self,
        position: BVector,
        np: &mut NewPath,
    ) -> Result<(), GenieError> {
        let level = self.paths.walked()?;
        let _ = np.path.insert(0, position);
        let to = if level.waypoint.within(level.points.len()) {
            level.point(level.waypoint)
        } else {
            position
        };
        let _ = np.path.push(to);
        np.path.measure();
        np.found = true;
        Ok(())
    }

    /// The long pather's plan for a far waypoint (`0x141150f00` at `LAB_141152f9c`): the new path
    /// empties and the long pather writes it (`FUN_140f9eb70` from the unit's tile to the
    /// waypoint's, the level's range and target; [`crate::pathing::long_path::path`]), made now. Its tall
    /// counts leave out the unit, its ignored units that wait to move or move (slots 0x2a8 and
    /// 0x5c8; not the target) and the target (`FUN_140f60440`). A path shorter than `EPSILON`
    /// stops the unit (slot 0x5d0; the new level and its path go, `FUN_14152f540` and
    /// `FUN_140f8f7f0`) and the step returns 0: None. A path ending farther from the waypoint
    /// than the unit is (on the ground) empties, its end's gap to the waypoint returned; else
    /// its stretches are checked for walls ([`Mover::walled_off`]). Returns whether it reached
    /// the goal and that gap.
    pub(super) fn long_plan(
        &mut self,
        (at, me): (Position, &Me),
        world: &mut Spatial,
        plan: &Planning<'_>,
        ask: LongAsk<'_>,
        path: &mut Path,
        by_id: ObjectsRef<'_>,
    ) -> Result<Option<(bool, Option<Distance>)>, GenieError> {
        let roles = by_id.roles();
        let LongAsk {
            waypoint,
            way,
            range,
            target,
        } = ask;
        let master = world.get(me.id).map(|p| p.master);
        let walker = LongWalker {
            id: me.id,
            position: at,
            radius: self.stats.radius.ground(),
            mask: self.stats.restriction_mask,
            moves: self.stats.speed != Speed::ZERO
                || roles
                    .of_some_master(master)
                    .contains(MasterRoles::LONG_PATH_MOVER),
            ignore_units: &self.ignore_units,
            group_units: plan.groups.group_units(me.group),
        };
        let target_id = target.map(|(id, _)| id);
        // from +0x824, the motion step's `local_18c8`
        let (mut found, long) = long_path_for(
            &walker,
            self.moved_at.ground(),
            world,
            plan.player_type,
            (waypoint, range.get()),
            target,
            by_id,
        )?;
        *path = long;
        path.measure();
        if path.length.get() < EPSILON {
            self.stop()?;
            return Ok(None);
        }
        let end = ground(
            waypoint,
            point_or_zero(path, WaypointIndex::last_of(path.points.len())),
        );
        if way < end {
            path.points.clear();
            path.length = Distance::ZERO;
            return Ok(Some((found, Some(end))));
        }
        if !self.walled_off(me, world, target_id, path, by_id)? {
            found = false;
        }
        Ok(Some((found, None)))
    }

    /// The long path's stretches (`0x141150f00` after `FUN_140f9eb70`, for a unit with a
    /// footprint and a height, `FUN_141194060` with 1): each stretch's obstruction query
    /// (`FUN_14114cc60` as the walk's, grown by the radius times `MovementTuning::long_path_grow`; the unit and
    /// the target ignored with their annexes, `FUN_14114e8a0`) that meets a wall, a gate, a tower
    /// or a building that does not move (classes 27, 39, 52, and 3 at top speed 0, slot 0x208:
    /// [`crate::map::spatial::Placed::moves`]) ends
    /// the path before that stretch (`FUN_140f98dd0`), or, on the first one, empties it (its
    /// length kept) and false; a villager's path is never cut.
    pub(super) fn walled_off(
        &self,
        me: &Me,
        world: &mut Spatial,
        target_id: Option<ObjectId>,
        path: &mut Path,
        by_id: ObjectsRef<'_>,
    ) -> Result<bool, GenieError> {
        let movement_tuning = &by_id.tuning().movement;
        let roles = by_id.roles();
        let me_at = world.get(me.id);
        if me_at.is_some_and(|p| roles.of_class(p.class).contains(ClassRoles::VILLAGER)) {
            return Ok(true);
        }
        let player = me_at.map(|p| p.player);
        let ignore = by_id.query_ignore(&[Some(me.id), target_id]);
        let grow = self.stats.radius.x() * movement_tuning.long_path_grow;
        let stretches = path.points.iter().zip(path.points.iter().skip(1));
        for (i, (&a, &b)) in stretches.enumerate() {
            let (from, to) = self.query_ends(a, b, world.map);
            let rect = Rect::spanning(from, to);
            world.check_known_records(rect.grown(Distance::new(grow)))?;
            let hits = world.walk_hits(WalkQuery {
                start: from,
                end: to,
                rect,
                grow: Distance::new(grow),
                ally_factor: self.stats.min_collision,
                player,
                ignore: &ignore,
                objects: by_id,
            })?;
            let walled = hits.iter().any(|&h| {
                world.get(h).is_some_and(|p| {
                    roles
                        .of_class(p.class)
                        .intersects(ClassRoles::WALL | ClassRoles::GATE | ClassRoles::TOWER)
                        || (roles.of_class(p.class).contains(ClassRoles::BUILDING) && !p.moves)
                })
            });
            if walled {
                if i == 0 {
                    path.points.clear();
                    return Ok(false);
                }
                // i + 1 lies inside the path: the cut always happens
                let _ = cut_path(path, i as i32 + 1);
                return Ok(true);
            }
        }
        Ok(true)
    }

    /// Whether the unit's restriction has a planner (`FUN_140f7cb30`); the slice builds one for
    /// any restriction its data has.
    pub(super) fn planner_exists(&self, world: &Spatial, plan: &mut Planning<'_>) -> bool {
        plan.pathing
            .planner(self.stats.restriction, world.tiles())
            .is_some()
    }

    /// `FUN_14114d3d0`: the walked level's goal is in reach: its target's gap (else where its top
    /// path ends) within its range, or a target's last waypoint at most 16 tiles beyond its
    /// range on a top level.
    pub(super) fn near_goal(
        &self,
        way: Distance,
        gap: Distance,
        to_end: Distance,
        movement_tuning: &MovementTuning,
    ) -> Result<bool, GenieError> {
        let level = self.paths.walked()?;
        let d = if level.target.is_none() { to_end } else { gap };
        if (d - level.range).get() < EPSILON {
            return Ok(true);
        }
        Ok(level.target.is_some()
            && level.waypoint == WaypointIndex::last_of(level.points.len())
            && way - level.range <= Distance::new(movement_tuning.short_reach)
            && level.level == WaypointLevel::TOP)
    }
}
