//! The walk: its kept hits, the obstruction query's ends, the facets, the steering and the speed
//! control.

use crate::map::GridSize;
use crate::map::Point;
use crate::map::spatial::{RecordId, Spatial};
use crate::math::Angle;
use crate::math::fmath::BVector;
use crate::movement::PathState;
use crate::movement::Speed;
use crate::movement::{
    ALLY_PAUSE, EPSILON, FAST_TURN, Me, Motion, Mover, Next, OTHER_PAUSE, UP, Velocity,
};
use crate::objects::ObjectKey;
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::{GenieError, math::fmath};

impl Mover {
    /// The walk's kept hits (`0x141150f00` after its first hit loop, which took them into the
    /// include list with their temporary shapes, [`include_kept`]): each hit but the walked
    /// level's target, a building's annex standing for it: one the walker outranks (as the
    /// pause loop of [`Mover::avoid`] ranks them; any Gaia hit of a walker of a player) pauses
    /// (slot 0x5f0: 0.1 for one of a player the walker's counts an ally, else 0.5), and one of
    /// the target's master that neither player counts an ally notifies the walker's unit AI
    /// (0x215, [`Mover::in_the_way`]). Then the walked level goes and its parent asks for a path again
    /// (state 2), which plans around the temporary shapes; without a parent the unit stops (slot
    /// 0x5d0) and the step gives up (1).
    ///
    /// [`include_kept`]: crate::movement::long_walk::include_kept
    pub(super) fn go_round(
        &mut self,
        me: &Me,
        world: &Spatial,
        hits: &[RecordId],
        computer: bool,
    ) -> Result<Next, GenieError> {
        let level = &self.paths[0];
        let player = world
            .get(me.id)
            .ok_or(GenieError::unsupported("a walker without a place"))?
            .player;
        for &hit in hits {
            let Some(id) = world.annex_of(hit).or_else(|| hit.key()) else {
                continue;
            };
            if level.targets(id) {
                continue;
            }
            let Some(other) = world.get_known(id)? else {
                continue;
            };
            let outranks = if player == PlayerId::GAIA || other.player != PlayerId::GAIA {
                let mine = world.gait(me.id).map_or(WorldTime::ZERO, |g| g.human_order);
                let theirs = world.gait(id).map_or(WorldTime::ZERO, |g| g.human_order);
                if !computer && (other.player != player || theirs != mine) {
                    theirs < mine
                } else {
                    me.id < id
                }
            } else {
                true
            };
            // slot 0x5f0 with the walker's player's view of the hit's (FUN_141663d90 with the
            // ally flag): an ally pauses 0.1, anyone else 0.5 (Gaia counts for no one)
            let allied = world.allied_with(player, other.player);
            if outranks && world.gait(id).is_some_and(|g| g.pause.is_some()) {
                let pause = if allied { ALLY_PAUSE } else { OTHER_PAUSE };
                self.pauses.push((id, pause));
            }
            // a hit that neither counts the other an ally, of the walked level's target's
            // master, notifies the walker's unit AI (0x215)
            if !allied
                && !world.allied_with(other.player, player)
                && let Some(t) = level.target.filter(|t| !t.is_recyclable())
            {
                let target = world.get(t).map(|t| t.master);
                if target.is_some_and(|m| m == other.master) {
                    self.in_the_way.push(id);
                }
            }
        }
        // without a parent (`+8`) the unit stops (slot 0x5d0) and the step returns 1
        if self.paths.len() < 2 {
            self.stop()?;
            return Ok(Next::Done(Motion::Done));
        }
        let _ = self.paths.remove(0);
        self.paths[0].state = PathState::WantsPath;
        Ok(Next::Again)
    }

    /// The obstruction query's ends (`FUN_14114cc60`): the step's two ends, each held within 0.1
    /// of the map's far edges (`FUN_141051900` orders them into its rectangle).
    pub(super) fn query_ends(&self, from: BVector, to: BVector, map: GridSize) -> (Point, Point) {
        let far = |size: i32| size as f32 - 0.1;
        let hold = |v: f32, max: f32| if v < 0.0 { 0.0 } else { v.min(max) };
        (
            Point::new(
                hold(from.x(), far(map.width())),
                hold(from.y(), far(map.height())),
            ),
            Point::new(
                hold(to.x(), far(map.width())),
                hold(to.y(), far(map.height())),
            ),
        )
    }

    /// The facets of the unit's current sprite ([`Mover::sprite_facets`]), else its master's when
    /// every sprite it shows has the same.
    #[must_use]
    pub fn facets(&self) -> Option<crate::objects::facing::SpriteFacets> {
        self.sprite_facets.or(self.stats.facets)
    }

    /// Turn toward `direction` (slot 195, [`Mover::turn_toward`], from the flat forward vector
    /// renormalised, with [`Mover::turn_dt`]), then the yaw by the turn ([`Mover::yaw`]) unless
    /// the walked level's flags have bit 0 and the turn is 0. Returns whether the turn fell short
    /// (the angle was above the most), which brakes the unit ([`Mover::control_speed`]).
    pub(super) fn steer(&mut self, direction: BVector, dt: Seconds) -> Result<bool, GenieError> {
        let mut flat = self.forward;
        flat = flat.with_height(0.0);
        let flat = fmath::normalize(flat);
        let (turn, clipped) = self.turn_toward(flat, direction, self.turn_dt(dt));
        if self.paths[0].flags & 1 != 0 && turn == Angle::ZERO {
            return Ok(clipped);
        }
        self.yaw(turn, self.facets())?;
        Ok(clipped)
    }

    /// The dt a turn takes: a hundred times `dt` for a master whose object flags have bit 1
    /// ([`MoveStats::fast_turn`]), as the motion step (`0x141150f00`) and a group member's
    /// steering (`FUN_141025fb0` at `0x141027887`) multiply it before slot 195.
    ///
    /// [`MoveStats::fast_turn`]: crate::movement::MoveStats::fast_turn
    pub(crate) fn turn_dt(&self, dt: Seconds) -> Seconds {
        if self.stats.fast_turn {
            dt * FAST_TURN
        } else {
            dt
        }
    }

    /// Slot 195 (`FUN_14114f630`): the turn from `forward` toward `direction` in `dt`, and
    /// whether it fell short. The angle between them (`FUN_14087e650`) turns whole when at most
    /// the master's turn rate (its slot 0x90) times `dt` (`COMISS` then `JBE` at `0x14114f6d6`:
    /// either NaN turns whole, unclipped); else the most, negated for a negative angle, and
    /// clipped. The turn is negated when the cross product of the direction and `forward` has a
    /// height above 0.
    pub(crate) fn turn_toward(
        &self,
        forward: BVector,
        direction: BVector,
        dt: Seconds,
    ) -> (Angle, bool) {
        let angle = fmath::angle_between(direction, forward);
        let most = self.stats.turn_rate * dt;
        let (mut turn, clipped) = if fmath::at_most_or_unordered(angle, most.get()) {
            (Angle::new(angle), false)
        } else if angle < 0.0 {
            (-most, true)
        } else {
            (most, true)
        };
        if 0.0 < fmath::cross(direction, forward).height() {
            turn = -turn;
        }
        (turn, clipped)
    }

    /// Slot 184 (`FUN_141163140`): the yaw by `turn` about the up axis: the forward vector turns
    /// and is renormalised, then takes its place with the up axis (slot 173,
    /// [`Mover::set_orientation`], which takes the forward vector's facet, `FUN_14119d0c0`); the
    /// angle becomes the facet's (`FUN_14114d780`, [`crate::objects::facing`]) of `facets`, the current
    /// sprite's (the right vector the routine turns too, slot 173 rebuilds).
    ///
    /// # Errors
    ///
    /// Without the current sprite's facets: a unit whose sprites differ in their facets.
    pub(crate) fn yaw(
        &mut self,
        turn: Angle,
        facets: Option<crate::objects::facing::SpriteFacets>,
    ) -> Result<(), GenieError> {
        let m = fmath::rotation(turn.get(), UP);
        let forward = fmath::normalize(fmath::rotate(&m, self.forward));
        self.set_orientation(forward, UP);
        let facets = facets.ok_or(GenieError::unsupported(
            "the yaw of a unit whose sprites differ in their facets",
        ))?;
        let facet = crate::objects::facing::facet_of_forward(forward, facets);
        self.angle = crate::objects::facing::angle_of_facet(facet, facets);
        Ok(())
    }

    /// Slot 173 (`FUN_14115d150`): the unit faces `forward` (`+0x830`), its right vector
    /// (`+0x83c`) the cross product of `up` and it, renormalised. The facet it takes of the
    /// forward vector (`+0xe`, `FUN_14119d0c0`) is not held: the slice derives the facet from the
    /// angle (`+0x7e8`), which this slot leaves.
    pub(crate) fn set_orientation(&mut self, forward: BVector, up: BVector) {
        self.forward = forward;
        self.right = fmath::normalize(fmath::cross(up, forward));
    }

    /// Slot 0x598 (`FUN_14115d050`): the unit's speed along its forward vector (its velocity's
    /// `+8`, `FUN_140f9b880`) becomes `speed`; a unit without velocity data gets them, zeroed
    /// (`FUN_140f8f5b0`), unless `speed` is 0 (`UCOMISS` with `JP`: a NaN gets them).
    pub(crate) fn set_speed(&mut self, speed: Speed) {
        if self.velocity.is_none() {
            if speed == Speed::ZERO {
                return;
            }
            self.velocity = Some(Velocity {
                velocity: BVector::ZERO,
                acceleration: BVector::ZERO,
            });
        }
        if let Some(v) = self.velocity.as_mut() {
            v.velocity = v.velocity.with_y(speed.get());
        }
    }

    /// Slot 0x600 (`FUN_14114d3b0`): the unit has a walked level whose pause (`+0x34`) is above
    /// 0.
    #[must_use]
    pub fn paused(&self) -> bool {
        self.paths
            .first()
            .is_some_and(|l| Seconds::ZERO < l.pause_time)
    }

    /// The speed control toward the waypoint (slot 198, `0x141150a80`): a unit whose turn fell
    /// short stops (its speed 0, slot 0x598) and brakes; else it accelerates to the top speed
    /// unless the waypoint lies inside the turning circle, where it brakes. The way to the
    /// waypoint and the circle start where the unit last moved to (`+0x824`,
    /// [`Mover::moved_at`]).
    pub(super) fn control_speed(&mut self, waypoint: BVector, clipped: bool) {
        let position = self.moved_at.vector();
        let Some(v) = self.velocity.as_mut() else {
            self.velocity = Some(Velocity {
                velocity: BVector::ZERO,
                acceleration: BVector::new(0.0, 0.0, f32::MAX),
            });
            return;
        };
        if clipped {
            v.acceleration = v.acceleration.with_y(-f32::MAX);
            v.velocity = v.velocity.with_y(0.0);
            return;
        }
        let d = fmath::sub(waypoint, position);
        let length = fmath::sqrt(d.height() * d.height() + d.x() * d.x() + d.y() * d.y());
        let toward = BVector::new(d.x() / length, d.height() / length, d.y() / length);
        let angle = fmath::angle_between(self.forward, toward);
        let radius =
            ((self.stats.turn_radius * v.velocity.y()) / self.stats.turn_radius_speed.get()).get();
        if 0.0 < v.velocity.y() && EPSILON < angle.abs() {
            let mut side = self.right;
            if 0.0 < fmath::cross(toward, self.forward).height() {
                side = -side;
            }
            let centre = BVector::new(
                side.x() * radius + position.x(),
                side.height() * radius + position.height(),
                side.y() * radius + position.y(),
            );
            if fmath::length(fmath::sub(waypoint, centre)) < radius {
                v.acceleration = v.acceleration.with_y(-f32::MAX);
                return;
            }
        }
        if self.stats.speed <= Speed::new(v.velocity.y()) {
            return;
        }
        v.acceleration = v.acceleration.with_y(f32::MAX);
    }
}
