//! A new level's path: no path, its link under the walked level, the short pather's ignore list
//! and a blocked start.

use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Point;
use crate::map::Position;
use crate::map::Rect;
use crate::map::Tile;
use crate::map::spatial::{Placed, RecordId, Spatial};
use crate::math::conversions::cvttss2si_64;
use crate::movement::LevelFlags;
use crate::movement::Levels;
use crate::movement::LinkedPathType;
use crate::movement::PathFlags;
use crate::movement::PathState;
use crate::movement::PlanTries;
use crate::movement::WaypointIndex;
use crate::movement::WaypointLevel;
use crate::movement::request_plans::NewPath;
use crate::movement::{
    AfterRequest, LONG_PATHER, Me, Mover, PathData, Planning, ReachAsk, SHORT_PATHER, ShortAsk,
    TileAsk, Walker, footprint_gap_squared, owner_number, short_path_including, split,
    tile_plan_for,
};
use crate::objects::ObjectId;
use crate::objects::masters::MasterId;
use crate::pathing::FixedPoint;
use crate::pathing::ShortFound;
use crate::pathing::hull_path::HullPather;
use crate::pathing::tile_path::{Found, Path, Request};
use crate::players::NO_RELATIONS;
use crate::unit_ai::task_finder::allied as task_finder_allied;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::index::ObjectsRef;
use crate::world::layer::ClassRoles;
use crate::world::layer::generated::MovementTuning;
use crate::world::layer::generated::TilePathTuning;
use crate::world::slots::CheckedIndex;
use crate::{GenieError, math::fmath, pathing::short_path};

impl Mover {
    /// The new level's path is no path (`0x141150f00` after the plans, fewer than two points or
    /// no length): the level marks its waypoint reached and the step starts again, or it waits at
    /// the end of its path (state 5); at its last waypoint while it may still try (its continue
    /// counter, `FUN_140f95bf0` with 0) the unit pauses for `STUCK_PAUSE` and stops; the exits
    /// that stop the unit otherwise are not modelled.
    pub(super) fn no_path(
        &mut self,
        found: bool,
        target: Option<&Placed>,
        to_end: Distance,
        movement_tuning: &MovementTuning,
    ) -> Result<AfterRequest, GenieError> {
        let top_has_more = self.top_has_more();
        if target.is_some() && self.paths.walked()?.flags.contains(LevelFlags::MET_TARGET) {
            // FUN_140f9b4d0(level, 2, 0): bit 2 goes off the level and on for its parents
            self.paths.walked_mut()?.flags &= !LevelFlags::MET_TARGET;
            for parent in self.paths.iter_mut().skip(1) {
                parent.flags |= LevelFlags::MET_TARGET;
            }
        }
        let level = self.paths.walked_mut()?;
        if !found || level.range < to_end {
            if level.waypoint.at_or_past_last(level.points.len()) {
                if level.continue_counter.left() {
                    // the unit pauses itself (slot 0x5f0, the walked level's `+0x34`) and stops
                    // (slot 0x5d0); the step returns 0
                    level.pause_time = Seconds::new(movement_tuning.stuck_pause);
                    self.stop()?;
                    return Ok(AfterRequest::Paused);
                }
                if level.path_flags.contains(PathFlags::LOOPS) || self.attack_moving {
                    return Err(GenieError::unsupported(
                        "a looping level without a path pauses the unit",
                    ));
                }
                if WaypointLevel::PLANNED < level.level || !top_has_more {
                    return Err(GenieError::unsupported(
                        "a level without a path stops the unit",
                    ));
                }
                level.state = PathState::Unplanned;
                return Ok(AfterRequest::Again);
            }
        } else if !level.path_flags.contains(PathFlags::LOOPS) {
            return Err(GenieError::unsupported(
                "a found path without length stops the unit",
            ));
        }
        level.flags |= LevelFlags::REACHED;
        Ok(AfterRequest::Again)
    }

    /// The new level goes under the walked one (`0x141150f00` from `0x141153b0c`): the walked
    /// level waits (state 0, its tries back to 10). A top level's plan becomes a level of its own
    /// in state 2 (`FUN_140f9b800`, level 1), whose stretches the step serves next; any other path
    /// is walked at once (`FUN_140f9b7c0`, level 0, state 3) after its long stretches got a
    /// midpoint each (`FUN_140f9b8f0`), linked to the walked level as found (1) or not (2).
    pub(super) fn link(
        &mut self,
        np: NewPath,
        solid: bool,
        movement_tuning: &MovementTuning,
    ) -> Result<AfterRequest, GenieError> {
        let NewPath {
            mut new,
            mut path,
            found,
            partial,
            planned,
            ..
        } = np;
        let level = self.paths.walked_mut()?;
        level.state = PathState::Idle;
        level.continue_counter = PlanTries::FULL;
        new.range = level.range;
        new.target = level.target;
        new.waypoint = WaypointIndex::SECOND;
        if level.level == WaypointLevel::TOP && planned {
            new.length = path.length;
            new.points = path.points;
            new.state = PathState::WantsPath;
            new.level = WaypointLevel::PLANNED;
            new.linked_type = if found {
                LinkedPathType::REACHES_WAYPOINT
            } else {
                LinkedPathType::MOVES_ON
            };
            self.paths.insert(0, new);
            return Ok(if SHORT_PATHER || LONG_PATHER {
                AfterRequest::Again
            } else {
                AfterRequest::Walk
            });
        }
        let mut i = path.points.len() as i32 - 1;
        while 0 < i {
            let (a, b) = (
                *path.points.checked(i as usize)?,
                *path.points.checked(i as usize - 1)?,
            );
            let mut d = fmath::sub(a, b);
            d = d.with_height(0.0);
            if movement_tuning.split < d.x() * d.x() + 0.0 + d.y() * d.y() {
                split(&mut path, i - 1);
            }
            i -= 1;
        }
        new.length = path.length;
        new.points = path.points;
        new.state = PathState::Walking;
        new.level = WaypointLevel::WALKED;
        new.linked_type = if !found || partial {
            LinkedPathType::MOVES_ON
        } else {
            LinkedPathType::REACHES_WAYPOINT
        };
        if !solid {
            // the walked level lets its path go (FUN_140f95b70): it holds none (path id -1),
            // which reads as no points, flags, length or time
            let level = self.paths.walked_mut()?;
            level.has_path = false;
            level.points.clear();
            level.path_flags = PathFlags::NONE;
            level.length = Distance::ZERO;
            level.created = WorldTime::ZERO;
        }
        self.paths.insert(0, new);
        Ok(AfterRequest::Walk)
    }

    /// The objects the short pather ignores (`FUN_14114ece0`, its path from the unit, `line` the two
    /// points its arguments 3 to 6 give the start test): the
    /// unit, its target, the unit's own list but itself when its top path was made this update
    /// (`+0x858`'s path at `+0x8`), and, for a standing unit with a target whose action is in
    /// state 8 (a drop-off), the workers standing within its radius and a tenth (the records
    /// `FUN_141051900` finds; one whose current path level's path was made this update is
    /// passed, [`crate::map::spatial::Gait::path_made`]) of the classes 2, 4,
    /// 19 and 21 (or whose slot 0x4f0 says so: 0 for a combat object, a task of action type 0x99
    /// on the master for a building of the classes 3, 27, 39, 49 and 52), when the
    /// unit's start is blocked (`FUN_140fa0110`: [`short_path::Pather::start_blocked`] along
    /// `line` with its radius, its weapon range and the target's radii, no shape ignored), allied with the unit and farther from the target than the unit's radius
    /// squared.
    pub(super) fn ignore_list(
        &self,
        me: &Me,
        world: &Spatial,
        plan: &mut Planning<'_>,
        target: Option<(ObjectId, &Placed)>,
        line: (Point, Point),
        by_id: ObjectsRef<'_>,
    ) -> Result<Vec<ObjectId>, GenieError> {
        let movement_tuning = &by_id.tuning().movement;
        let roles = by_id.roles();
        let now = plan.now;
        let mut list = vec![me.id];
        if let Some((id, _)) = target {
            list.push(id);
        }
        // the unit's own list but itself, when the walked level's top path (`FUN_140f960f0`: a
        // level holding a path) was made this update; before the drop-off workers
        if self
            .paths
            .last()
            .is_some_and(|top| top.has_path && top.created == now)
        {
            list.extend(self.ignore_units.iter().filter(|&&id| id != me.id));
        }
        // the query runs for a unit that does not move (slot 0x5c8); a hit counts only with a
        // target and the unit's action in state 8
        if let Some((target_id, t)) = target
            && !self.is_moving()
            && me.action_state.get() == 8
        {
            let grow = self.stats.radius.x() + movement_tuning.range_floor;
            // the query's point: slot 0x520, +0x824
            let (x, y) = (self.moved_at.x(), self.moved_at.y());
            let rect = Rect::new(x, y, x, y);
            world.check_known_records(rect.grown(Distance::new(grow)))?;
            let owner = world.get(me.id).map(|p| p.player);
            let r = self.stats.radius.x();
            let skip = [RecordId::from(me.id), RecordId::from(target_id)];
            for id in world.obstruction_candidates(rect, Distance::new(grow), &skip) {
                let Some(p) = world.get(id) else { continue };
                let gait = by_id.gait(id);
                if gait.is_some_and(|g| g.moving) {
                    continue;
                }
                // a worker whose path is this update's walks: it is passed
                if gait.and_then(|g| g.path_made) == Some(now) {
                    continue;
                }
                // slot 0x4f0: a building's master with a task of action type 0x99
                // (FUN_1415c60b0), 0 for anything else
                let worker_building = roles.of_class(p.class).intersects(
                    ClassRoles::BUILDING
                        | ClassRoles::WALL
                        | ClassRoles::GATE
                        | ClassRoles::FARM
                        | ClassRoles::TOWER,
                ) && world
                    .task_0x99
                    .get(p.player.index())
                    .is_some_and(|m| m.contains(&p.master));
                if !roles.of_class(p.class).intersects(
                    ClassRoles::TRADE_BOAT
                        | ClassRoles::VILLAGER
                        | ClassRoles::TRADE_CART
                        | ClassRoles::FISHING_SHIP,
                ) && !worker_building
                {
                    continue;
                }
                // FUN_140fa0110 runs for each such hit: its temporary shapes leave the mesh's
                // free list reversed each time, so it is not run once for all
                if !self.start_blocked(world, plan, t, line)? {
                    continue;
                }
                let allied = owner.is_some_and(|o| {
                    let relations = world.relations.get(p.player).unwrap_or(&NO_RELATIONS);
                    task_finder_allied(relations, p.player, o, true)
                });
                let gap = footprint_gap_squared(p.position, p.radius, t.position, t.radius);
                if !allied || gap <= DistanceSquared::new(r * r) {
                    continue;
                }
                list.push(id);
            }
        }
        Ok(list)
    }

    /// `FUN_140fa0110`: whether the unit's start is blocked on its way along `line` (its arguments
    /// 3 to 6, in fixed point by truncation), with its radius, its weapon range (slot 0x230) and
    /// the target's radii (`FUN_140faa170`, [`short_path::Pather::start_blocked`]).
    pub(super) fn start_blocked(
        &self,
        world: &Spatial,
        plan: &mut Planning<'_>,
        target: &Placed,
        (from, to): (Point, Point),
    ) -> Result<bool, GenieError> {
        // CVTTSS2SI into a 64-bit register, its low 32 bits kept
        let fixed = |f: f32| cvttss2si_64(f * short_path::TO_FIXED) as i32;
        let query = short_path::Query {
            from: FixedPoint::new(fixed(from.x()), fixed(from.y())),
            to: FixedPoint::new(fixed(to.x()), fixed(to.y())),
            radius: Distance::new(self.stats.radius.x()),
            target_size: target.radius,
            range: self.stats.weapon_range,
            blocked: self.stats.restriction_mask.blocked(),
        };
        let tiles = world.tiles();
        let words = |x: u32, y: u32| tiles.word(Tile::new(x as i32, y as i32)).unwrap_or(0);
        let (mesh, pather) = plan
            .pathing
            .mesh_and_pather()
            .ok_or(GenieError::unsupported(
                "a start test without the game's mesh",
            ))?;
        pather.start_blocked(mesh, &words, &query)
    }

    /// After a short path (`FUN_140f9e540`) for the new level: when the top path (`FUN_140f960f0`
    /// of the walked level) was made this tick, the new path takes the unit's ignore units
    /// (`FUN_140f9b7a0`).
    pub(super) fn give_ignored(&self, new: &mut PathData, now: WorldTime) {
        if self.paths.last().is_some_and(|t| t.created == now) {
            new.ignored.clone_from(&self.ignore_units);
        }
    }

    /// The short pather (`FUN_140f9e540`) from the unit to `to` with the ignore and include
    /// lists `lists`, the owner's number for the player ([`short_path_including`]), into `out`;
    /// returns its result.
    pub(super) fn short_path(
        &self,
        me: &Me,
        world: &mut Spatial,
        plan: &mut Planning<'_>,
        ask: ShortAsk<'_>,
        out: &mut Path,
        by_id: ObjectsRef<'_>,
    ) -> Result<ShortFound, GenieError> {
        let walker = self.walker(me.id);
        let owner = owner_number(world, me.id);
        // from +0x824: the motion step's `local_18c8`, the reach test's start (FUN_14114fea0)
        short_path_including(
            walker,
            self.moved_at.ground(),
            (world, by_id),
            plan,
            ask,
            owner,
            out,
        )
    }

    /// What the planners read of this unit ([`Walker`]).
    pub(crate) const fn walker(&self, id: ObjectId) -> Walker {
        Walker {
            id,
            radius: Distance::new(self.stats.radius.x()),
            restriction: self.stats.restriction,
            mask: self.stats.restriction_mask,
        }
    }
    /// The tile pathfinder (`FUN_140f7c530`) from the unit to `to`, appended to `out`, smoothed
    /// with the obstruction manager's segment test for the unit (the records of the smoothing's
    /// ignore list ignored, [`Pathing::smoothing_ignore`]); None without a planner. The planner reads the first byte of the target's record
    /// (`+0x658`): 3, a box, for the obstruction types 2, 10, 11 and 13 (`FUN_141056460`).
    ///
    /// [`Pathing::smoothing_ignore`]: crate::pathing::Pathing::smoothing_ignore
    pub(super) fn tile_plan(
        &self,
        me: &Me,
        world: &mut Spatial,
        plan: &mut Planning<'_>,
        ask: TileAsk<'_>,
        out: &mut Path,
        by_id: ObjectsRef<'_>,
    ) -> Result<Option<Found>, GenieError> {
        // the smoothing's ignore list: the unit and the level's target object (the planner's
        // argument 12, the object the world holds)
        let level_target = self
            .paths
            .first()
            .and_then(|l| l.target)
            .filter(|&t| world.get(t).is_some());
        // from +0x824, the motion step's `local_18c8`
        tile_plan_for(
            self.walker(me.id),
            self.moved_at.ground(),
            level_target,
            (world, by_id),
            plan,
            ask,
            out,
        )
    }

    /// Slot 0x3d8 of the unit (`FUN_14114ffc0` -> `FUN_140fa0370` without a target object):
    /// whether it can walk to within `range` of the point `to` (x, y on the map), as
    /// [`Mover::reach_test`] decides. `master` is the unit's master's id.
    pub fn reaches(
        &self,
        (at, me): (Position, &Me),
        (world, by_id): (&mut Spatial, ObjectsRef<'_>),
        plan: &mut Planning<'_>,
        to: Point,
        range: Distance,
        master: MasterId,
    ) -> Result<bool, GenieError> {
        let ask = ReachAsk {
            goal: to,
            target: None,
            range,
            master,
        };
        self.reach_test(at, me, world, plan, ask, by_id)
    }

    /// Slot 0x3d0 of the unit (`FUN_14114fea0` -> `FUN_140fa0370` with the target object): whether
    /// it can walk to within `range` of object `target` (its place for the target's slot 0x520),
    /// as [`Mover::reach_test`] decides; a target that is gone is not reached.
    pub fn reaches_object(
        &self,
        (at, me): (Position, &Me),
        (world, by_id): (&mut Spatial, ObjectsRef<'_>),
        plan: &mut Planning<'_>,
        target: ObjectId,
        range: Distance,
        master: MasterId,
    ) -> Result<bool, GenieError> {
        let Some(t) = world.get_known(target)? else {
            return Ok(false);
        };
        let to = t.position.ground();
        let ask = ReachAsk {
            goal: to,
            target: Some(target),
            range,
            master,
        };
        self.reach_test(at, me, world, plan, ask, by_id)
    }

    /// Slot 0x3e8 of the unit (`FUN_14114f790`): the length of its short path to object `target`
    /// within `range`: the obstruction manager's old short pather ([`HullPather::find`], with the
    /// unit's place, radius and terrain mask, ignoring the unit and the target) when it finds a
    /// path within range or none is needed (its result below 2); None for another result or a
    /// target that is gone. The manager's temporary tile records go after
    /// ([`Spatial::clear_tile_records`]).
    pub fn short_reach(
        &self,
        at: Position,
        me: &Me,
        world: &mut Spatial,
        target: ObjectId,
        range: Distance,
        by_id: ObjectsRef<'_>,
    ) -> Result<Option<Distance>, GenieError> {
        let Some(t) = world.get_known(target)? else {
            return Ok(None);
        };
        let to = t.position.ground();
        let mask = self.stats.restriction_mask;
        let mut pather = HullPather::new(
            (world, by_id),
            me.id,
            Some(target),
            mask,
            Distance::new(self.stats.radius.x()),
        );
        let (result, path) = pather.find(at.ground(), to, Some(target), range)?;
        // the slot removes the manager's temporary tile records after (FUN_1410561f0)
        world.clear_tile_records();
        Ok(result.reaches().then(|| path.length()))
    }

    /// The length of the tile planner's path (`FUN_140f7c530` with the radii `radii`, range 0, no
    /// closed tiles, the tile search for a human player, no smoothing) from the unit to `to`, and
    /// the planner's result; None without a planner for the unit's restriction.
    pub fn plan_length(
        &self,
        at: Position,
        world: &Spatial,
        plan: &mut Planning<'_>,
        to: Point,
        radii: Distance,
        tile_path_tuning: &TilePathTuning,
    ) -> Result<Option<(Found, Distance)>, GenieError> {
        let full = !plan.player_type.is_computer();
        let Some(planner) = plan.pathing.planner(self.stats.restriction, world.tiles()) else {
            return Ok(None);
        };
        let req = Request {
            from: at.ground(),
            to,
            radii,
            range: Distance::ZERO,
            closed: Vec::new(),
            full,
        };
        let mut path = Path::default();
        let found = planner.plan::<GenieError>(&req, false, &mut path, None, tile_path_tuning)?;
        path.measure();
        Ok(Some((found, path.length)))
    }
}
