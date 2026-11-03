//! The walk's ends before its steer and the motion step's end on a loop.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::math::fmath::BVector;
use crate::movement::LevelFlags;
use crate::movement::Levels;
use crate::movement::PathFlags;
use crate::movement::PathState;
use crate::movement::{EPSILON, Goal, Motion, Mover, Next, footprint_gap, gate_gap};
use crate::world::Seconds;
use crate::world::index::ObjectsRef;
use crate::world::layer::generated::MovementTuning;

impl Mover {
    /// [`Mover::walk`]'s ends before its steer: a walk to a target in range of it counts the
    /// waypoint reached and the motion step starts again; a walk to a point with a range ends
    /// where its top level's path ends (the unit stops, the step returns 2). None walks on.
    ///
    /// # Errors
    ///
    /// Path data without the walked level, and what the stop does not model.
    pub(super) fn walk_ends(
        &mut self,
        at: Position,
        world: &Spatial,
        goal: Goal<'_>,
        by_id: ObjectsRef<'_>,
    ) -> Result<Option<Next>, GenieError> {
        let movement_tuning = &by_id.tuning().movement;
        let roles = by_id.roles();
        let ground_radius = self.stats.radius.ground();
        let level = self.paths.walked()?;
        match goal.target {
            Some((id, t)) => {
                let range = level.range;
                let own = footprint_gap(at, ground_radius, t.position, t.radius) - range;
                let near = (at, ground_radius);
                let gap = gate_gap(world, near, (id, t), own, |g| g - range, by_id, roles);
                if gap.get() < EPSILON {
                    // in range of the target: the waypoint counts as reached
                    let level = self.paths.walked_mut()?;
                    level.flags &= !LevelFlags::AT_WAYPOINT;
                    level.flags |= LevelFlags::REACHED;
                    return Ok(Some(Next::Again));
                }
            }
            // a walk to a point with a range ends where its top level's path ends
            None if !level.path_flags.contains(PathFlags::LOOPS)
                && movement_tuning.range_floor < level.range.get() =>
            {
                if let Some(&end) = self.top_path().and_then(|p| p.last()) {
                    let gap = footprint_gap(
                        at,
                        ground_radius,
                        end.position().with_z(0.0),
                        Footprint::new(0.0, 0.0),
                    );
                    if gap - level.range < Distance::new(EPSILON) {
                        self.reset_level()?;
                        self.stop()?;
                        return Ok(Some(Next::Done(Motion::Arrived)));
                    }
                }
            }
            None => {}
        }
        Ok(None)
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
        movement_tuning: &MovementTuning,
    ) -> Result<Motion, GenieError> {
        let clipped = self.steer(heading, dt, movement_tuning)?;
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
