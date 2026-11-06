//! The missile (`RGE_Missile_Object`, type 60) and its action (`RGE_Action_Missile`, type 8): a
//! ranged attacker's swing makes one at its weapon offset (`FUN_141081020`, [`spawn_point`]), its
//! action aims at the target (`FUN_140a9c600`, [`aim`]; `set_state` 3, `FUN_140a9de70`,
//! [`MissileAction::launch`]) and flies it each update (`FUN_140a9e6e0`,
//! [`MissileAction::fly`]) until it hits the target (the shooter's hit, slot 0x860) or the ground.
//!
//! 158041: `RGE_Action_Missile` vtable `0x142ed5d68` (start slot 5 `0x140a9c5b0`, update slot 6
//! `0x140a9e6e0`, `set_state` slot 16 `0x140a9de70`); `RGE_Missile_Object` vtable `0x142f57fe8`
//! (update `0x14113e520`).

use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Radii;
use crate::map::{GroundOffset, Offset, Position};
use crate::math::Angle;
use crate::math::fmath::{atan2, cos, sin, sqrt};
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::FlyMode;
use crate::objects::masters::HitChance;
use crate::objects::masters::MasterId;
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::{GenieError, GenieResult};
use genie_diff::Diff;

use genie_model::combat::missile::arc_ratio;
use genie_model::combat::missile::hit_mode;
use genie_model::combat::missile::miss_spread;
use genie_model::combat::missile::missile_modes;
mod landing;
mod missile_state;
use genie_model::combat::missile::missile_stats;
mod missile_target;
use genie_model::combat::missile::projectile_kind;
mod shooter_id;
mod strike_result;
pub(crate) mod unit;

pub use self::arc_ratio::ArcRatio;
pub use self::hit_mode::{HitMode, TargetOwner};
pub use self::miss_spread::MissSpread;
pub use self::missile_modes::{SmartMode, VanishMode};
pub use self::missile_state::MissileState;
pub use self::missile_stats::MissileStats;
pub use self::missile_target::{MissileTarget, MissileTargetId};
pub use self::projectile_kind::ProjectileKind;
pub use self::shooter_id::{Shooter, ShooterId};
pub use self::strike_result::StrikeResult;
use crate::combat::{AttackCount, Firepower, MissileCount};
use crate::math::image::missile::{
    DRAW_SCALE as MISSILE_DRAW_SCALE, HALF as MISSILE_HALF,
    VOLLEY_EPSILON as MISSILE_VOLLEY_EPSILON, VOLLEY_HALF as MISSILE_VOLLEY_HALF,
    VOLLEY_SCALE as MISSILE_VOLLEY_SCALE,
};
use crate::movement::Acceleration;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::MissileTuning;

/// The missile action's type, which word 9 adds when it is the top action.
pub const MISSILE: ActionKind = ActionKind::named(8);

/// The hit chance's draw scale (`DAT_142eb69cc`) and a missed missile's half spread
/// (`DAT_142e7f384`).
const DRAW_SCALE: f32 = MISSILE_DRAW_SCALE;
const HALF: f32 = MISSILE_HALF;
/// A volley's count takes the firepower past this hair (`FUN_141203140`).
const VOLLEY_EPSILON: f32 = MISSILE_VOLLEY_EPSILON;
/// A volley's scatter: the draw's scale and the half spread (`FUN_14120c0a0`).
const VOLLEY_SCALE: f32 = MISSILE_VOLLEY_SCALE;
const VOLLEY_HALF: f32 = MISSILE_VOLLEY_HALF;

/// Where a missile came from (`+0x9c`, `mProjectileData`, `FUN_140aa86e0`): the firing object's
/// place, the missile master's radii on the ground, the firing object's master and owner (−1
/// both without one, `FUN_140aa8720`). The hit's elevation reads its height.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Origin {
    /// The firing object's place.
    pub(crate) position: Position,
    /// The missile master's radii on the ground.
    pub(crate) radius: Footprint,
    /// The firing object's master (−1 without one).
    pub(crate) master: Option<MasterId>,
    /// Its owner (−1 without one).
    pub(crate) owner: Option<PlayerId>,
}

impl Origin {
    /// `FUN_140aa8720`: no origin.
    pub const NONE: Self = Self {
        position: Position::ORIGIN,
        radius: Footprint::ZERO,
        master: None,
        owner: None,
    };

    /// The save's 28 bytes: seven 32-bit words.
    #[must_use]
    pub fn from_bytes(raw: &[u8; 28]) -> Self {
        let mut words = [0u32; 7];
        for (word, bytes) in words.iter_mut().zip(raw.as_chunks::<4>().0) {
            *word = u32::from_le_bytes(*bytes);
        }
        let [x, y, z, rx, ry, master, owner] = words;
        Self {
            position: Position::new(f32::from_bits(x), f32::from_bits(y), f32::from_bits(z)),
            radius: Footprint::new(f32::from_bits(rx), f32::from_bits(ry)),
            // −1 (all 32 bits) for none; a master's id is a short, a player's a byte
            master: i16::try_from(master.cast_signed())
                .ok()
                .and_then(MasterId::of_data),
            owner: u8::try_from(owner).ok().map(PlayerId::from),
        }
    }
}

/// The missile action (type 8).
#[derive(Clone, Debug, PartialEq, Diff)]
pub struct MissileAction {
    /// `+0x18`.
    pub(crate) state: MissileState,
    /// The target (`+0x20`, its id `+0x30`), −1 for none.
    pub(crate) target: Option<MissileTargetId>,
    /// The object it was fired from (`+0x28`, its id `+0x34`).
    pub(crate) fired_from: Option<ShooterId>,
    /// `+0x38`..`+0x40`: the target's place when fired.
    pub(crate) point: Position,
    /// `+0x44`: the way flown so far (a straight missile's).
    pub(crate) timer: Distance,
    /// `+0x68`..`+0x70`: the direction, a unit vector for a straight missile.
    pub(crate) direction: Offset,
    /// `+0x74`, `+0x78`: the arc's vertical speed and acceleration.
    pub(crate) ballistic_velocity: Speed,
    /// `+0x78`: the arc's vertical acceleration.
    pub(crate) ballistic_acceleration: Acceleration,
    /// `+0x7c`: the way left to the aimed point.
    pub(crate) distance: Distance,
    /// `+0x80`: the master's hit mode, 1 after a miss.
    pub(crate) hit_mode: HitMode,
    /// `+0x98`: the last hit's result.
    pub(crate) result: StrikeResult,
    /// `+0x9c`.
    pub(crate) origin: Origin,
    /// `+0x88`: the objects the last step met (not saved: none after a load).
    #[diff(unsaved)]
    pub(crate) met: Vec<ObjectId>,
}

/// A missile object's own part.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Missile {
    /// `+0x990`: how far it flies at most (`max_range`).
    pub(crate) max_range: Distance,
    /// `+0x998` (`fired_from_id`) and `+0x99c` (`mIsChargeProjectile`).
    pub(crate) fired_from: Option<ShooterId>,
    /// `+0x99c` (`mIsChargeProjectile`): a charge's projectile.
    pub(crate) charge: bool,
    /// `+0x994`: farther than `MissileTuning::landing` from its point; the ground does not stop it.
    /// Not saved: the load's constructor sets 1.
    #[diff(unsaved)]
    pub(crate) flying: bool,
    /// `+0x7dc`..`+0x7e4`: this update's step (the moving object's `velocity_x`..`_z`).
    pub(crate) step: Offset,
}

/// `FUN_141081020`'s place for a new missile: the firing object's place plus its master's weapon
/// offset (`+0x844`..`+0x84c`) turned by its facing (`+0x7e8`): x by cos·oy − sin·ox, y by
/// cos·ox + sin·oy, the height by oz.
#[must_use]
pub fn spawn_point(at: Position, angle: Angle, offset: Offset) -> Position {
    let (cosine, sine) = (cos(angle.get()), sin(angle.get()));
    let (ox, oy) = (offset.x(), offset.y());
    let y = cosine * ox + sine * oy + at.y();
    let x = (cosine * oy - sine * ox) + at.x();
    Position::new(x, y, at.z() + offset.z())
}

/// `FUN_141081020`'s reach: the firing master's radii's length plus its range plus 2.
#[must_use]
pub fn reach(radius: Footprint, range: Distance, missile_tuning: &MissileTuning) -> Distance {
    Distance::new(
        sqrt(radius.x() * radius.x() + radius.y() * radius.y())
            + range.get()
            + missile_tuning.reach_margin,
    )
}

/// `FUN_141203140` (no charge): a volley's missiles, the object's firepower (`+0xa00`, in whole
/// missiles past a hair) up to its master's `max_attacks_in_volley`.
#[must_use]
pub fn volley_count(firepower: Firepower, max_volley: MissileCount) -> i32 {
    let count = (firepower.get() + VOLLEY_EPSILON) as i64 as i32;
    count.min(max_volley.counted())
}

/// `FUN_14120c0a0`'s scatter of a volley's extra missile along a spread: an Object-stream draw
/// over 32767 of the spread, less half of it.
#[must_use]
pub fn scatter(draw: u32, spread: Distance) -> f32 {
    let spread = spread.get();
    (draw as i32 as f32 * spread) / VOLLEY_SCALE - spread * VOLLEY_HALF
}

/// `FUN_141226690`'s place for a volley's extra missile, scattered by `scatter` (x, y): the
/// weapon offset, plus the scatter over the master's spread adjustment when that is above 0
/// (its height by 0 over it), turned by the facing from the object's place
/// ([`spawn_point`]); a building (class 3) whose target `target` lies nearer than its weapon
/// offset's length on the ground fires from the target's x and y.
#[must_use]
pub fn volley_spawn_point(
    at: Position,
    angle: Angle,
    offset: Offset,
    adjustment: Distance,
    scatter: GroundOffset,
    building_at: Option<Point>,
) -> Position {
    let adjustment = adjustment.get();
    let (ox, oy) = (offset.x(), offset.y());
    let offset = if 0.0 < adjustment {
        Offset::new(
            scatter.x() / adjustment + ox,
            scatter.y() / adjustment + oy,
            0.0 / adjustment + offset.z(),
        )
    } else {
        offset
    };
    let mut position = spawn_point(at, angle, offset);
    if let Some(point) = building_at {
        let (dx, dy) = (point.x() - at.x(), point.y() - at.y());
        let gap = sqrt(dx * dx + dy * dy);
        if gap < sqrt(ox * ox + oy * oy) {
            let (tx, ty) = (point.x(), point.y());
            position = position.with_x(tx).with_y(ty);
        }
    }
    position
}

/// `FUN_141226690`'s reach: the firing master's radii's length plus its range plus 6.
#[must_use]
pub fn volley_reach(
    radius: Footprint,
    range: Distance,
    missile_tuning: &MissileTuning,
) -> Distance {
    Distance::new(
        sqrt(radius.x() * radius.x() + radius.y() * radius.y())
            + range.get()
            + missile_tuning.volley_reach_margin,
    )
}

/// What the aim reads of the firing object (`+0x28`, else the missile): its master's
/// `base_hit_chance` (`+0x83e`) and `missed_missile_spread` (`+0x860`), its class and the
/// target's master's radius on x and class for the class-54 rule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aimer {
    /// The firing object's master's `base_hit_chance` (`+0x83e`).
    pub(crate) hit_chance: HitChance,
    /// Its `missed_missile_spread` (`+0x860`).
    pub(crate) spread: MissSpread,
    /// Its class.
    pub(crate) class: ClassId,
}

/// What a missile's aim takes besides the Action stream's draw ([`aim`]'s arguments).
#[derive(Debug)]
pub struct AimAt<'missile> {
    /// The point it aims for.
    pub(crate) point: Position,
    /// Where the missile is.
    pub(crate) from: Position,
    /// The target's radii and class, none without a target.
    pub(crate) target: Option<(Radii, ClassId)>,
    /// The firing object's hit chance, spread and class.
    pub(crate) aimer: Aimer,
    /// The missile's master's id.
    pub(crate) master_id: MasterId,
    /// The missile's attack count (`+0x900`).
    pub(crate) attack_count: AttackCount,
    /// The hit mode, which a miss sets to 1.
    pub(crate) hit_mode: &'missile mut HitMode,
}

/// `FUN_140a9c600`: where a missile at `from` aims for its `point`: a target's height rises by a
/// third of its master's height radius (`target`: its radii x and z and class; None without a
/// target); the Action stream's draw ("intercept") over 100 against the hit chance (80 for a
/// class-54 shooter at a target narrower than a tile of class 27, 52 or 3) misses above it: the
/// hit mode becomes 1 and the point strays by up to the ground distance times the spread, two
/// more draws, half each way; a missile of master 528 with an attack count (`+0x900`) strays by
/// that share of it. The class-54 rule is the firing object's (`+0x28`): the caller's `aimer` is
/// the missile's own master for a volley's extra missile, which has none.
pub fn aim(
    at: AimAt<'_>,
    draw: &mut dyn FnMut() -> u32,
    roles: &Roles,
    missile_tuning: &MissileTuning,
) -> GenieResult<Position> {
    let AimAt {
        point,
        from,
        target,
        aimer,
        master_id,
        attack_count,
        hit_mode,
    } = at;
    let (mut x, mut y, mut z) = (point.x(), point.y(), point.z());
    if let Some((radius, _)) = target {
        z += radius.z() / missile_tuning.height_share;
    }
    let dx = x - from.x();
    let mut gap = sqrt((y - from.y()) * (y - from.y()) + dx * dx);
    let mut chance = aimer.hit_chance;
    if let Some((radius, class)) = target
        && roles
            .of_class(aimer.class)
            .contains(ClassRoles::UNPACKED_TREBUCHET)
        && radius.x() < 1.0
        && roles
            .of_class(class)
            .intersects(ClassRoles::WALL | ClassRoles::TOWER | ClassRoles::BUILDING)
    {
        chance = HitChance::of_data(0x50);
    }
    let roll = draw();
    if i32::from(chance.get()) < (roll as i32 * 100) / 0x7fff {
        if *hit_mode == HitMode::TARGET_ALONE {
            *hit_mode = HitMode::MISSED;
        }
        gap *= aimer.spread.get();
        // the missile's attack count (`+0x900`, its firer's when fired): master 528 strays by its
        // share, 527 and the others by all of it
        // DIVSS hands on a NaN distance quieted, which it is already: its bits stay on any host
        if AttackCount::ZERO < attack_count
            && roles
                .of_master(master_id)
                .contains(MasterRoles::STRAYING_MISS)
            && !gap.is_nan()
        {
            gap /= attack_count.as_f32();
        }
        let gap = gap + 0.0;
        let half = gap * HALF;
        x += (draw() as i32 as f32 * gap) / DRAW_SCALE - half;
        y += (draw() as i32 as f32 * gap) / DRAW_SCALE - half;
    }
    Ok(Position::new(x, y, z))
}

impl MissileAction {
    /// `set_state(3)` (`FUN_140a9de70` case 3) from `from` toward the aimed point: the way left
    /// (its length), a straight missile's unit direction (projectile type 0; type 3 keeps the
    /// way itself) with the object's angle on it (slot 0x158: `atan2f` of y over x, returned),
    /// the timer back to 0, the arc's acceleration and vertical speed (`arc` not 0: twice the
    /// arc's rise, `−arc × way × way` or `−arc × way` above 0, over the half flight time squared;
    /// the speed the opposite of the acceleration times that time); then `set_state(4)`. A smart
    /// missile leading a moving target and the projectile types 1 and 2 are not modelled; the
    /// sprite's pitch facet is not held.
    pub fn launch(
        &mut self,
        from: Position,
        aimed: Position,
        stats: &MissileStats,
    ) -> GenieResult<Option<Angle>> {
        let dx = aimed.x() - from.x();
        let dy = aimed.y() - from.y();
        let dz = aimed.z() - from.z();
        let dz2 = dz * dz;
        let way = sqrt(dy * dy + dx * dx + dz2);
        self.distance = Distance::new(way);
        if stats.smart_mode.contains(SmartMode::LEADS_TARGET) && self.target.is_some() {
            return Err(GenieError::unsupported(
                "a smart missile leading its target",
            ));
        }
        let angle = match stats.projectile_kind {
            ProjectileKind::STRAIGHT => {
                self.direction = if way <= 0.0 {
                    Offset::ZERO
                } else {
                    Offset::new(dx / way, dy / way, dz / way)
                };
                self.timer = Distance::ZERO;
                Some(Angle::new(atan2(self.direction.y(), self.direction.x())))
            }
            ProjectileKind::WHOLE_WAY => {
                self.direction = Offset::new(dx, dy, dz);
                Some(Angle::new(atan2(dy, dx)))
            }
            _ => {
                return Err(GenieError::unsupported(
                    "a missile of projectile type 1 or 2",
                ));
            }
        };
        let (acceleration, velocity) = if way <= 0.0 || stats.arc.is_flat() {
            (0.0, 0.0)
        } else {
            let flight_time = if stats.speed <= Speed::ZERO {
                0.0
            } else {
                way / stats.speed.get()
            };
            let apex_time = flight_time * HALF;
            let rise = if stats.arc <= ArcRatio::FLAT {
                way * stats.arc.get() * way
            } else {
                -(way * stats.arc.get())
            };
            let acceleration = (rise + rise) / (apex_time * apex_time);
            (acceleration, -acceleration * apex_time)
        };
        self.ballistic_acceleration = Acceleration::new(acceleration);
        self.ballistic_velocity = Speed::new(velocity);
        self.state = MissileState::Flying;
        Ok(angle)
    }

    /// One update's flight in state 4 (`FUN_140a9e6e0`), before the step's hits: a straight
    /// missile steps its speed times the world's dt along its direction, its timer counting the
    /// way and its distance shrinking; past its reach and 2 beyond its point it ends (`set_state`
    /// 1). An arc rises by `(dt × acceleration × 0.5 + vertical speed) × dt`, its vertical speed
    /// gaining `dt × acceleration`. Returns the step and whether the missile flies on (farther
    /// than 0.1 from its point, `+0x994`).
    pub fn fly(
        &mut self,
        dt: Seconds,
        max_range: Distance,
        stats: &MissileStats,
        missile_tuning: &MissileTuning,
    ) -> GenieResult<(Offset, bool)> {
        let mut step = match stats.projectile_kind {
            ProjectileKind::STRAIGHT => {
                let way = stats.speed * dt;
                let offset = way.get() * self.direction;
                self.timer += way;
                self.distance -= way;
                if max_range <= self.timer && self.distance < Distance::new(missile_tuning.beyond) {
                    // State 1: the missile ends.
                    self.state = MissileState::Done;
                }
                offset
            }
            ProjectileKind::WHOLE_WAY => {
                self.distance = Distance::ZERO;
                self.direction
            }
            _ => {
                return Err(GenieError::unsupported(
                    "a missile of projectile type 1 or 2",
                ));
            }
        };
        if !stats.arc.is_flat() {
            step = step.with_z(
                step.z()
                    + ((dt * self.ballistic_acceleration) * HALF + self.ballistic_velocity).get()
                        * dt.get(),
            );
            self.ballistic_velocity += dt * self.ballistic_acceleration;
        }
        Ok((step, missile_tuning.landing < self.distance.get()))
    }
}

/// `FUN_14113f740`'s height test: an object at height `z` whose master's height radius is
/// `height` meets a missile at `missile_z` whose master's is `missile_height` when it lies at
/// most `height` below the missile and at most `missile_height` above.
#[must_use]
pub fn meets_height(z: f32, height: f32, missile_z: f32, missile_height: f32) -> bool {
    let above = z - missile_z;
    -height <= above && above <= missile_height
}

/// The ground hit's box test (`FUN_140a9e6e0`): a missile at `at` with radii `radius` lies on
/// the target at `target` with radii `target_radius` when it is nearer on each axis than the
/// two radii together.
#[must_use]
pub fn on_target(at: Position, radius: Radii, target: Position, target_radius: Footprint) -> bool {
    let dx = (at.x() - target.x()).abs();
    let dy = (at.y() - target.y()).abs();
    dx < target_radius.x() + radius.x() && dy < target_radius.y() + radius.y()
}

/// The height a place takes on the ground (`FUN_141193a10`, with the ground's height) by its
/// master's fly mode: 0 puts it on the ground, 1 lifts it to the ground when it is below, others
/// keep it. A missile still flying (`+0x994`) keeps its height before this (slot 0x60,
/// `FUN_14113eb60`, from the static `new_position`).
#[must_use]
pub fn grounded(z: f32, ground: f32, fly_mode: FlyMode) -> f32 {
    match fly_mode {
        FlyMode::GROUND => ground,
        FlyMode::ABOVE if z < ground => ground,
        _ => z,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::Firepower;
    use crate::combat::MissileCount;
    use crate::combat::missile::MissileState;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::testing::class;
    use crate::testing::ids;
    use crate::testing::roles;
    use crate::testing::tuning;
    #[cfg(test)]
    use std::iter::once;

    /// MarvinBlue/426048731's spear -28 (master 509: speed 7, arc −0.06), fired at the deer 9655
    /// in the update that made the save after 3137: 2.80657 to go when `set_state(3)` ran, and its
    /// arc's acceleration −23.52 and vertical speed 4.71503 as the game held them.
    #[test]
    fn spear_arcs_to_its_point() {
        let stats = MissileStats {
            projectile_kind: ProjectileKind::STRAIGHT,
            arc: ArcRatio::new(-0.06),
            speed: Speed::new(7.0),
            radius: Radii::new(0.1, 0.1, 0.0),
            ..MissileStats::default()
        };
        let mut missile_action = MissileAction {
            state: MissileState::Launch,
            target: Some(MissileTargetId::new(object_id(9655))),
            fired_from: Some(ShooterId::new(object_id(9171))),
            point: Position::ORIGIN,
            timer: Distance::ZERO,
            direction: Offset::ZERO,
            ballistic_velocity: Speed::ZERO,
            ballistic_acceleration: Acceleration::ZERO,
            distance: Distance::ZERO,
            hit_mode: HitMode::TARGET_ALONE,
            result: StrikeResult::None,
            origin: Origin::NONE,
            met: Vec::new(),
        };
        // a way of 2.80657 along x
        let angle = missile_action
            .launch(
                Position::new(0.0, 0.0, 0.0),
                Position::new(2.80657, 0.0, 0.0),
                &stats,
            )
            .unwrap();
        assert_eq!(angle, Some(Angle::ZERO));
        assert_eq!(missile_action.state, MissileState::Flying);
        assert!((missile_action.ballistic_acceleration.get() - -23.52).abs() < 1e-3);
        assert!((missile_action.ballistic_velocity.get() - 4.71503).abs() < 1e-4);
        // one update of 52 ms: 0.364 of the way
        let (step, flying) = missile_action
            .fly(
                Seconds::new(0.052),
                Distance::new(5.282_842_6),
                &stats,
                &tuning().missile,
            )
            .unwrap();
        assert!((missile_action.timer.get() - 0.364).abs() < 1e-6);
        assert!(
            (missile_action.distance - Distance::new(2.44257))
                .get()
                .abs()
                < 1e-5
        );
        assert!((step.x() - 0.364).abs() < 1e-6);
        assert!(flying);
    }

    /// `FUN_140a9c600`: the Action stream's draw over 100 against the hit chance; a hit keeps the
    /// point (a target's height a third of its height radius up), a miss turns the hit mode to 1
    /// and strays by up to the ground distance times the spread, half each way. A missed missile
    /// of master 528 with an attack count strays by that share; master 527 and the others by all.
    #[test]
    fn missile_misses_by_its_spread_and_master_528_by_its_attack_count_share() {
        let aimer = Aimer {
            hit_chance: HitChance::of_data(70),
            spread: MissSpread::new(0.5),
            class: class::CLASS_0,
        };
        // the target 3 east and 4 north: a ground distance of 5
        let point = Position::new(13.0, 14.0, 0.0);
        let from = Position::new(10.0, 10.0, 1.0);
        let target = Some((Radii::new(0.2, 0.2, 1.5), class::CLASS_6));
        let aim_with = |draws: [u32; 3], master: MasterId, count: i32, mode: &mut HitMode| {
            let mut drawn = draws.into_iter();
            aim(
                AimAt {
                    point,
                    from,
                    target,
                    aimer,
                    master_id: master,
                    attack_count: AttackCount::of_data(count),
                    hit_mode: mode,
                },
                &mut || drawn.next().unwrap(),
                &roles(),
                &tuning().missile,
            )
        };
        // a draw of 70% or less hits: the target's height rises by 1.5 / 3
        let mut mode = HitMode::TARGET_ALONE;
        assert_eq!(
            aim_with([22937, 0, 0], master_id(4), 0, &mut mode),
            Ok(Position::new(13.0, 14.0, 0.5))
        );
        assert_eq!(mode, HitMode::TARGET_ALONE);
        // a miss: 5 × 0.5 = 2.5; the draws 32767 and 0 stray 1.25 one way and the other
        let missed = aim_with([32767, 32767, 0], master_id(4), 3, &mut mode).unwrap();
        assert_eq!(mode, HitMode::MISSED);
        assert_eq!(missed, Position::new(13.0 + 1.25, 14.0 - 1.25, 0.5));
        // master 527 keeps the distance with an attack count; 528 divides it by the count
        let mut mode = HitMode::of_data(2);
        assert_eq!(
            aim_with([32767, 32767, 0], ids::ID_527, 3, &mut mode),
            Ok(missed)
        );
        assert_eq!(mode, HitMode::of_data(2));
        let share = 2.5f32 / 3.0 + 0.0;
        let expected = Position::new(
            (32767.0 * share) / 32767.0 - share * 0.5 + 13.0,
            (0.0 * share) / 32767.0 - share * 0.5 + 14.0,
            0.5,
        );
        assert_eq!(
            aim_with([32767, 32767, 0], ids::ID_528, 3, &mut mode),
            Ok(expected)
        );
        // without an attack count master 528 strays by all of it
        assert_eq!(
            aim_with([32767, 32767, 0], ids::ID_528, 0, &mut mode),
            Ok(missed)
        );
        // a class-54 shooter at a narrow building hits at 80
        let siege = Aimer {
            class: class::CLASS_54,
            ..aimer
        };
        let mut drawn = once(26213u32);
        let narrow = Some((Radii::new(0.5, 0.5, 0.0), class::BUILDING));
        assert_eq!(
            aim(
                AimAt {
                    point,
                    from,
                    target: narrow,
                    aimer: siege,
                    master_id: master_id(4),
                    attack_count: AttackCount::of_data(0),
                    hit_mode: &mut mode
                },
                &mut || drawn.next().unwrap(),
                &roles(),
                &tuning().missile
            ),
            Ok(point)
        );
    }

    #[test]
    fn shot_turns_its_offset_by_the_facing() {
        // facing along x: the offset's y is ahead, its x to the side
        let position = spawn_point(
            Position::new(10.0, 20.0, 0.0),
            Angle::ZERO,
            Offset::new(0.0, 0.5, 1.5),
        );
        assert_eq!(position, Position::new(10.5, 20.0, 1.5));
        assert_eq!(
            reach(
                Footprint::new(0.2, 0.2),
                Distance::new(3.0),
                &tuning().missile
            ),
            Distance::new(0.282_842_73 + 3.0 + 2.0)
        );
    }

    /// MarvinBlue/426048731's spears (fly mode 1) in the update that made the save after 3148:
    /// -30, 0.27 past its point, keeps its height 0.3496 above the flat ground; -29 would sink to
    /// −0.3602 and is lifted onto it (the ground hit: its height is the ground's).
    #[test]
    fn spear_past_its_point_takes_the_ground_by_its_fly_mode() {
        assert_eq!(grounded(0.349_624_8, 0.0, FlyMode::ABOVE), 0.349_624_8);
        assert_eq!(grounded(-0.360_166_4, 0.0, FlyMode::ABOVE), 0.0);
        // still flying it keeps any height; fly mode 0 always takes the ground
        assert_eq!(grounded(0.349_624_8, 0.0, FlyMode::GROUND), 0.0);
    }

    #[test]
    fn missile_meets_an_object_at_its_height_and_lands_on_its_box() {
        // an object half a tile high meets a missile of no height from its foot to its top
        assert!(meets_height(0.0, 0.5, 0.3, 0.0));
        assert!(!meets_height(0.0, 0.5, 0.6, 0.0));
        assert!(!meets_height(0.2, 0.5, 0.0, 0.0));
        // the box test: nearer than the radii together on both axes
        assert!(on_target(
            Position::new(10.0, 10.0, 0.0),
            Radii::new(0.1, 0.1, 0.0),
            Position::new(10.25, 9.8, 0.0),
            Footprint::new(0.2, 0.2)
        ));
        assert!(!on_target(
            Position::new(10.0, 10.0, 0.0),
            Radii::new(0.1, 0.1, 0.0),
            Position::new(10.3, 9.8, 0.0),
            Footprint::new(0.2, 0.2)
        ));
    }

    #[test]
    fn volley_fires_whole_missiles_up_to_its_master_most_scattered_about_its_point() {
        // a town center with 7 villagers inside fires 8 a volley, 11 at most
        assert_eq!(
            volley_count(Firepower::new(8.0), MissileCount::of_data(11)),
            8
        );
        assert_eq!(
            volley_count(Firepower::new(12.0), MissileCount::of_data(11)),
            11
        );
        assert_eq!(
            volley_count(Firepower::new(0.999_999_9), MissileCount::of_data(11)),
            1
        );
        // a spread of 0.5: from a quarter short to a quarter past
        assert_eq!(scatter(0, Distance::new(0.5)), -0.25);
        assert_eq!(scatter(0x7fff, Distance::new(0.5)), 0.25);
        // the reach adds 6 to the range
        assert_eq!(
            volley_reach(
                Footprint::new(2.0, 0.0),
                Distance::new(6.0),
                &tuning().missile
            ),
            Distance::new(14.0)
        );
    }

    #[test]
    fn building_volley_missile_starts_at_its_target_when_the_target_is_nearer_than_its_offset() {
        let at = Position::new(10.0, 10.0, 0.0);
        // facing x (angle 0): the offset's x goes along -y, its y along x
        let position = volley_spawn_point(
            at,
            Angle::ZERO,
            Offset::new(0.0, 2.0, 1.0),
            Distance::new(0.0),
            GroundOffset::ZERO,
            None,
        );
        assert_eq!(position, Position::new(12.0, 10.0, 1.0));
        // the scatter over the adjustment adds to the offset
        let position = volley_spawn_point(
            at,
            Angle::ZERO,
            Offset::new(0.0, 2.0, 1.0),
            Distance::new(0.5),
            GroundOffset::new(0.0, 0.25),
            None,
        );
        assert_eq!(position, Position::new(12.5, 10.0, 1.0));
        // a building's target a tile off: nearer than the offset's 2
        let position = volley_spawn_point(
            at,
            Angle::ZERO,
            Offset::new(0.0, 2.0, 1.0),
            Distance::new(0.0),
            GroundOffset::ZERO,
            Some(Point::new(11.0, 10.0)),
        );
        assert_eq!(position, Position::new(11.0, 10.0, 1.0));
        let position = volley_spawn_point(
            at,
            Angle::ZERO,
            Offset::new(0.0, 2.0, 1.0),
            Distance::new(0.0),
            GroundOffset::ZERO,
            Some(Point::new(13.0, 10.0)),
        );
        assert_eq!(position, Position::new(12.0, 10.0, 1.0));
    }

    /// `FUN_141226690`'s `COMISS length, gap; JBE` (`0x141226c23`) skips the target's place when
    /// the gap is NaN (unordered), and the inlined sqrt (`UCOMISS 0, x; JA`) takes `SQRTSS` for
    /// a NaN, which gives the NaN back.
    #[test]
    fn building_volley_missile_keeps_its_offset_when_the_gap_to_its_target_is_nan() {
        let position = volley_spawn_point(
            Position::new(10.0, 10.0, 0.0),
            Angle::ZERO,
            Offset::new(0.0, 2.0, 1.0),
            Distance::new(0.0),
            GroundOffset::ZERO,
            Some(Point::new(f32::NAN, 10.0)),
        );
        assert_eq!(position, Position::new(12.0, 10.0, 1.0));
    }
}
