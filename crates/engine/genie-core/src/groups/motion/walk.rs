//! The walk toward the walked level's waypoint, and `BMotion`'s update with what it reads of
//! the world.

use crate::GenieError;
use crate::groups::GroupId;
use crate::groups::Motion;
use crate::groups::PathBudget;
use crate::groups::motion::FlagChange;
use crate::groups::motion::YawLimit;
use crate::groups::motion::{
    EPSILON, GroupView, Lead, Pass, Passing, StepWorld, ground, hold, passes_waypoint, path_step,
    point, reset_level, set_flag,
};
use crate::map::Footprint;
use crate::map::Position;
use crate::map::Tile;
use crate::map::spatial::{Placed, RecordId, Spatial};
use crate::math::fmath::BVector;
use crate::math::fmath::sqrt;
use crate::math::rng::Stream;
use crate::movement::LevelFlags;
use crate::movement::PathFlags;
use crate::movement::PathState;
use crate::movement::WaypointIndex;
use crate::movement::{Planning, footprint_gap};
use crate::objects::ObjectKey;
use crate::players::PlayerKind;
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::index::ObjectsRef;
use crate::world::layer::generated::GroupTuning;
use crate::world::slots::CheckedIndex;
use std::mem::take;

/// The walk of the walked level (`LAB_14104b6b4`): see [`path_step`]. A group on its waypoint
/// counts it reached ([`crate::movement::LevelFlags::AT_WAYPOINT`]); else the step along the ground
/// toward it (this tick's speed times dt) passes it (reached, `AT_WAYPOINT`) or ends on it
/// (`AT_WAYPOINT` and `REACHED`, the walk goes on). Then what stands in the way ([`way_blocked`]),
/// the walk's end in range ([`walk_ends`]), the turn toward the waypoint ([`turn_toward`]) and the
/// members' pull ([`pull_to_members`]); the forward speed takes the top speed.
pub(super) fn steer(
    motion: &mut Motion,
    group: &GroupView,
    lead: &Lead,
    world: &mut StepWorld<'_, '_>,
    target: Option<&Placed>,
) -> Result<Pass, GenieError> {
    let group_tuning = &world.by_id.tuning().group;
    let by_id = world.by_id;
    let dt = world.dt;
    let level = motion.paths.checked(0)?;
    let waypoint = point(level, level.waypoint);
    let at = motion.position[0];
    if ground(at, waypoint).get() < EPSILON {
        set_flag(&mut motion.paths, LevelFlags::AT_WAYPOINT, FlagChange::Set);
        return Ok(Pass::Reached);
    }
    // FUN_141046080: the direction on the ground
    let mut direction = waypoint - at;
    direction = direction.with_height(0.0);
    let l = sqrt(direction.x() * direction.x() + 0.0 + direction.y() * direction.y());
    direction = BVector::new(direction.x() / l, direction.height() / l, direction.y() / l);
    let step = (dt.get() * motion.acceleration.y() + motion.velocity.y()) * dt.get();
    let next = at + step * direction;
    match passes_waypoint(waypoint, at, next, group_tuning) {
        Passing::Passes => {
            set_flag(&mut motion.paths, LevelFlags::AT_WAYPOINT, FlagChange::Set);
            return Ok(Pass::Reached);
        }
        Passing::Reaches => {
            set_flag(&mut motion.paths, LevelFlags::AT_WAYPOINT, FlagChange::Set);
            set_flag(&mut motion.paths, LevelFlags::REACHED, FlagChange::Set);
        }
        Passing::Short => {}
    }
    if let Some(pass) = way_blocked(motion, lead, world.spatial, at, next, by_id)? {
        return Ok(pass);
    }
    if let Some(pass) = walk_ends(motion, at, target, group_tuning)? {
        return Ok(pass);
    }
    turn_toward(motion, direction, dt);
    pull_to_members(motion, group, group_tuning);
    motion.velocity = motion
        .velocity
        .with_y(motion.maximum_velocity_forward.get());
    Ok(Pass::Done(true))
}

/// What stands in the way of the step from `at` to `next` (`FUN_1411570c0`, both held inside
/// the map, grown by the lead's radius; the lead's group's differentiator units, else the lead,
/// ignored; it sets the manager's ignore list to them, `FUN_1410562e0`): the walked level's
/// target alone sets bits 0 and 2 (the next pass counts the waypoint reached); a hit moved after
/// the level was made sends the walk back to the parent ([`back_to_parent`]). None when the walk
/// goes on.
fn way_blocked(
    motion: &mut Motion,
    lead: &Lead,
    spatial: &mut Spatial,
    at: BVector,
    next: BVector,
    by_id: ObjectsRef<'_>,
) -> Result<Option<Pass>, GenieError> {
    let group_tuning = &by_id.tuning().group;
    let ignore: Vec<RecordId> = match lead.group.as_ref() {
        Some(g) if !g.differentiator_units.is_empty() => g.differentiator_units.clone(),
        _ => vec![lead.walker.id],
    }
    .into_iter()
    .map(RecordId::from)
    .collect();
    let hits = spatial.line_query(
        hold(at, spatial.map, group_tuning),
        hold(next, spatial.map, group_tuning),
        lead.walker.radius,
        None,
        &ignore,
    )?;
    if hits.is_empty() {
        return Ok(None);
    }
    let (on_target, newer) = weigh_hits(motion, spatial, &hits, by_id)?;
    if hits.len() == 1 && on_target {
        set_flag(&mut motion.paths, LevelFlags::REACHED, FlagChange::Set);
        set_flag(&mut motion.paths, LevelFlags::MET_TARGET, FlagChange::Set);
        return Ok(Some(Pass::Next));
    }
    if 0 < newer {
        return Ok(Some(back_to_parent(motion, lead, spatial)?));
    }
    Ok(None)
}

/// The hits the world places, but those the walked level or the top level ignores: whether
/// one is the walked level's target, and how many moved (their gait's last move) after the
/// level was made.
fn weigh_hits(
    motion: &Motion,
    spatial: &Spatial,
    hits: &[RecordId],
    by_id: ObjectsRef<'_>,
) -> Result<(bool, u32), GenieError> {
    let level = motion.paths.checked(0)?;
    let top_ignore = motion
        .paths
        .last()
        .map(|t| t.ignore.as_slice())
        .unwrap_or_default();
    let mut on_target = false;
    let mut newer = 0;
    for &hit in hits {
        let Some(id) = hit.key().filter(|&id| spatial.get(id).is_some()) else {
            continue;
        };
        if level.ignore.contains(&id) || top_ignore.contains(&id) {
            continue;
        }
        if level.targets(id) {
            on_target = true;
        }
        let last_move = by_id.gait(id).map_or(WorldTime::ZERO, |g| g.last_move);
        if level.created < last_move {
            newer += 1;
        }
    }
    Ok((on_target, newer))
}

/// The walk goes back to the parent level (it asks again, state 2), and past one whose waypoint
/// the lead cannot stand on to that one's parent; a level without a parent resets and the group
/// stands (the step ends with 1).
fn back_to_parent(motion: &mut Motion, lead: &Lead, spatial: &Spatial) -> Result<Pass, GenieError> {
    for _ in 0..2 {
        if motion.paths.len() < 2 {
            reset_level(&mut motion.paths)?;
            motion.stand();
            return Ok(Pass::Done(true));
        }
        motion.paths.checked_mut(1)?.state = PathState::WantsPath;
        let _ = motion.paths.remove(0);
        let parent = motion.paths.checked(0)?;
        if !parent.has_path {
            break;
        }
        let p = point(parent, parent.waypoint);
        if spatial.standable(Tile::new(p.x() as i32, p.y() as i32), lead.walker.mask) {
            break;
        }
    }
    Ok(Pass::Next)
}

/// `LAB_14104bcb6`: in range of where the walk ends, it ends. Without a target, a walked level
/// that does not loop with a range above `GroupTuning::near` under a top level with a path: a group within
/// the range (less [`EPSILON`]) of the top's last point (the 3D distance) arrived (the level
/// resets, the group stands); with a target, a group whose footprint gap to it is the range
/// (within [`EPSILON`]) counts the waypoint reached. None when the walk goes on.
fn walk_ends(
    motion: &mut Motion,
    at: BVector,
    target: Option<&Placed>,
    group_tuning: &GroupTuning,
) -> Result<Option<Pass>, GenieError> {
    let level = motion.paths.checked(0)?;
    match target {
        None => {
            let top = motion.paths.last().filter(|t| t.has_path);
            if !level.path_flags.contains(PathFlags::LOOPS)
                && group_tuning.near < level.range.get()
                && let Some(top) = top
            {
                let end = point(top, WaypointIndex::last_of(top.points.len()));
                let d = at - end;
                let gap = sqrt(d.height() * d.height() + d.x() * d.x() + d.y() * d.y());
                if gap - level.range.get() < EPSILON {
                    reset_level(&mut motion.paths)?;
                    motion.stand();
                    return Ok(Some(Pass::Done(true)));
                }
            }
        }
        Some(t) => {
            let gap = footprint_gap(
                t.position,
                t.radius,
                Position::new(at.x(), at.y(), 0.0),
                Footprint::new(0.0, 0.0),
            );
            if (gap - level.range).get().abs() < EPSILON {
                return Ok(Some(Pass::Reached));
            }
        }
    }
    Ok(None)
}

/// The turn toward the waypoint (`direction`, on the ground) from the forward on the ground, by
/// the yaw the turn rate allows in `dt` ([`Motion::calculate_yaw_angle`], [`Motion::turn`]).
fn turn_toward(motion: &mut Motion, direction: BVector, dt: Seconds) {
    let forward = motion.position[1].with_height(0.0);
    let f = sqrt(forward.x() * forward.x() + 0.0 + forward.y() * forward.y());
    let forward = BVector::new(forward.x() / f, forward.height() / f, forward.y() / f);
    let yaw = motion.calculate_yaw_angle(forward, direction, dt, YawLimit::Clamped);
    motion.turn(yaw);
}

/// The members' centre pulls the group while every member acts on a target: the members'
/// mean place (where each last moved to, summed in order; none for no members) on the ground,
/// at least `GroupTuning::pull_distance` (squared) from the group's, takes `GroupTuning::pull_share` of the new place,
/// which keeps `GroupTuning::pull_keep` of its own.
fn pull_to_members(motion: &mut Motion, group: &GroupView, group_tuning: &GroupTuning) {
    let all_acting = group
        .members
        .iter()
        .all(|m| m.is_some_and(|(target, _)| target.is_some()));
    if !all_acting {
        return;
    }
    let n = group.members.len();
    let mut centre = BVector::ZERO;
    if n != 0 {
        for &(_, p) in group.members.iter().flatten() {
            centre = BVector::new(
                centre.x() + p.x(),
                p.height() + centre.height(),
                p.y() + centre.y(),
            );
        }
        let n = n as f32;
        centre = BVector::new(centre.x() / n, centre.height() / n, centre.y() / n);
    }
    let c = centre.with_height(0.0);
    let here = motion.position[0].with_height(0.0);
    let d = here - c;
    if group_tuning.pull_distance <= d.x() * d.x() + d.height() * d.height() + d.y() * d.y() {
        motion.new_position[0] =
            group_tuning.pull_share * c + group_tuning.pull_keep * motion.new_position[0];
    }
}

impl World {
    /// `BMotion`'s update (`FUN_14104a090`) of group `group`, `budget` its `param_2`: the path
    /// step for the group's lead ([`path_step`]), then the velocity along the vectors
    /// ([`Motion::integrate`]). The group update calls it only with a lead the world holds, a
    /// moving unit; without one, or for a group the world does not hold, nothing changes.
    ///
    /// # Errors
    ///
    /// What the path step does not model.
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, from a
    /// caller that holds it.
    pub fn motion_update(&mut self, group: GroupId, budget: PathBudget) -> Result<(), GenieError> {
        let Some(g) = self.groups.get(group) else {
            return Ok(());
        };
        let Some(lead) = self.lead_view(g.motion.lead) else {
            return Ok(());
        };
        let view = GroupView {
            player_kind: g
                .owner
                .and_then(|o| self.players.get(o.index()))
                .map_or(PlayerKind::Phantom, |p| p.kind),
            members: g
                .members
                .iter()
                .map(|m| {
                    self.object(m.id).map(|(_, u)| {
                        (
                            self.top_target_object(u.id),
                            u.moved_vector(self.parts.mover_of(u.id)),
                        )
                    })
                })
                .collect(),
        };
        let g = self.groups.held_mut(group)?;
        let mut motion = take(&mut g.motion);
        let result = {
            let mut plan = Planning {
                pathing: &mut self.pathing,
                now: self.world_time,
                player_kind: lead.player_kind,
                object: self.streams.get(Stream::Object),
                groups: &self.groups,
            };
            let mut world = StepWorld {
                spatial: &mut self.spatial,
                plan: &mut plan,
                dt: self.dt_seconds,
                by_id: ObjectsRef::new(
                    &self.players,
                    &self.index,
                    &self.model.masters,
                    &self.parts,
                    &self.model.roles,
                    &self.model.tuning,
                ),
            };
            path_step(&mut motion, &view, &lead, &mut world, budget)
        };
        if result.is_ok() {
            motion.integrate(self.dt_seconds)?;
        }
        self.groups.held_mut(group)?.motion = motion;
        result.map(|_| ())
    }
}

/// The ignore list the short pather and the walk's query take of the lead's group, at most this
/// many (`FUN_14100bbe0` with `0x80`; the request's own loop stops past `0x7f`).
pub(super) const SHORT_IGNORED: usize = 128;
