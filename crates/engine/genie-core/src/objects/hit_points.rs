//! A combat object's hit points, a unit's or a building's (`TRIBE_Combat_Object`, types 70 and
//! 80): the heal, the gain and the loss, the regeneration and the powerup's clamp (slots 0x110,
//! 0xb8 and 0x260, and the update's checks). Whatever heals (a monk's heal action, a building
//! that heals around it) calls the healed object's heal here.

use crate::combat::damage::ArmorPair;
use crate::combat::damage::KillCount;
use crate::combat::damage::TransformTest;
use crate::combat::damage::{gain_hp as damage_gain_hp, heal_hp, most_hp};
use crate::combat::stingers::credit_combat;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered, maxss};
use crate::math::image::turn::{
    REGENERATION_EPSILON as TURN_REGENERATION_EPSILON,
    REGENERATION_MINUTE as TURN_REGENERATION_MINUTE,
};
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectState;
use crate::objects::Regeneration;
use crate::objects::layers::Combat;
use crate::objects::masters::ClassId;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::Master;
use crate::objects::unit::Unit;
use crate::world::Seconds;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::TurnTuning;
use crate::{GenieError, GenieResult};
use genie_diff::Diff;

/// The regeneration's minute (`DAT_14330ebc8`).
pub(crate) const REGENERATION_MINUTE: f32 = TURN_REGENERATION_MINUTE;
/// The size below which a rate or a period is none (`DAT_142f49458`).
pub(crate) const REGENERATION_EPSILON: f32 = TURN_REGENERATION_EPSILON;

/// A combat object's hit point powerup, which takes a loss before its hit points: what losses
/// took of it (`+0xa38`, `mHitpointsPowerup`) and its most (`+0xa3c`, `mHitpointsMaxPowerup`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct HpPowerup {
    /// `+0xa38`: what losses took of it.
    pub(crate) taken: HitPoints,
    /// `+0xa3c`: its most.
    pub(crate) max: HitPoints,
}

impl HpPowerup {
    /// No powerup: none taken of none.
    pub const NONE: Self = Self {
        taken: HitPoints::ZERO,
        max: HitPoints::ZERO,
    };

    /// What it can still take: its most less what it took (`max - taken`).
    #[must_use]
    pub fn room(self) -> HitPoints {
        self.max - self.taken
    }

    /// Slot 0xb0's first part: below its most, it takes the loss `hit_points` up to its room. The
    /// rest of the loss, or None when it took all of it.
    pub fn absorb(&mut self, hit_points: HitPoints) -> Option<HitPoints> {
        let (taken, max) = (self.taken, self.max);
        if taken < max {
            let room = max - taken;
            if hit_points <= room {
                self.taken = taken + hit_points;
                return None;
            }
            self.taken = taken + room;
            return Some(hit_points - room);
        }
        Some(hit_points)
    }
}

/// Slot 0x260 of a combat object (`FUN_1412033a0`), its regeneration rate in hit points a
/// minute: its stinger (`+0xb3c`, slot 0x750) plus (its powerup (`+0xa34`, slot 0x6f0) plus (a
/// hero's `TurnTuning::hero_regeneration` plus its master's `regeneration_rate`, `+0x8b4`)), in the
/// assembly's order. A hero is a master whose `hero_flag` (`+0x8d9`) grants
/// [`HeroFlags::REGENERATES`].
#[must_use]
pub(crate) fn regeneration_rate(
    master: &Master,
    combat: &Combat,
    turn_tuning: &TurnTuning,
) -> Regeneration {
    let hero = master
        .creatable_part()
        .hero_flag
        .grants(HeroFlags::REGENERATES);
    let bonus = if hero {
        Regeneration::new(turn_tuning.hero_regeneration)
    } else {
        Regeneration::ZERO
    };
    combat.regen_stinger + (combat.regen_powerup + (bonus + master.creatable_part().regeneration))
}

/// What a regeneration step ([`Unit::regenerate`]) leaves to its caller.
#[derive(Clone, Debug, PartialEq)]
#[must_use]
pub(crate) struct Regenerated {
    /// Its loss's or gain's change.
    pub(crate) change: HpChange,
    /// The most hit points it read before them (slot 200), none where it took no whole point.
    pub(crate) most: Option<HitPoints>,
}

impl Regenerated {
    /// A step that took no whole point.
    const NONE: Self = Self {
        change: HpChange::Settled,
        most: None,
    };
}

/// What a combat object's hit point change through its slots 0xb0 and 0xb8 leaves to its caller,
/// which reads the world.
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub(crate) enum HpChange {
    /// Nothing more: no change, the powerups' room took the loss, or the gain found the object
    /// at its most.
    Settled,
    /// Its hit point transforms' test runs (`FUN_141204450`, [`TransformTest::Runs`]).
    TransformTest,
    /// A loss left it below 1 hit point: its stingers' kill credits, if any (slot 22's).
    Fell(Option<KillCount>),
}

impl Unit {
    /// Slot 0x110 of a combat object (`FUN_141213b10`) of its master's `class` and
    /// `hit_points`: the classes 2, 20, 21, 22 and 53 repair at a cost (not modelled); any other
    /// heals (`FUN_141193b30`, [`crate::combat::damage::heal_hp`]), whose slot 0xb8 may run its
    /// hit point transforms' test (the caller's: it reads the world).
    pub(crate) fn heal(
        &mut self,
        combat: &mut Combat,
        class: ClassId,
        hit_points: MasterHitPoints,
        amount: HitPoints,
        roles: &Roles,
    ) -> GenieResult<TransformTest> {
        if roles.of_class(class).intersects(
            ClassRoles::TRADE_BOAT
                | ClassRoles::TRANSPORT
                | ClassRoles::FISHING_SHIP
                | ClassRoles::WARSHIP
                | ClassRoles::BOARDING_SHIP,
        ) {
            return Err(GenieError::unsupported(
                "a heal at a cost (FUN_141213b10's classes)",
            ));
        }
        Ok(heal_hp(
            &mut self.life.hp,
            &mut combat.hp_powerup,
            hit_points,
            combat.hp_stinger,
            amount,
        )
        .test)
    }

    /// Slot 0xb8 of a combat object (`FUN_141203be0`): `amount` added up to its hit points with
    /// its powerups' room, an overflow taken off the powerup (`+0xa38`, never below 0); below its
    /// most its hit point transforms' test follows ([`HpChange::TransformTest`], the caller's).
    pub(crate) fn gain_hp(
        &mut self,
        combat: &mut Combat,
        master: &Master,
        amount: HitPoints,
    ) -> HpChange {
        match damage_gain_hp(
            &mut self.life.hp,
            &mut combat.hp_powerup,
            master.hit_points,
            combat.hp_stinger,
            amount,
        ) {
            TransformTest::Runs => HpChange::TransformTest,
            TransformTest::Skipped => HpChange::Settled,
        }
    }

    /// Slot 0xb0 of a combat object (`FUN_141205c80`), the loss of `d` hit points: the hit point
    /// powerups' room (`+0xa38` up to `+0xa3c`) takes it first, the rest comes off the hit points,
    /// which stay above the master's `invulnerability_level` floor (`FUN_141202a80`). Left below
    /// 1 (`COMISS` with 1, `JC`: a NaN too), its stingers credit its kill: the credits for the
    /// caller to count ([`credit_combat`]); at 1 or more its hit point transforms' test follows
    /// ([`HpChange::TransformTest`], the caller's). [`crate::combat::damage::lose`] is the same
    /// loss on a hit's victim.
    pub(crate) fn lose_hp(
        &mut self,
        combat: &mut Combat,
        master: &Master,
        loss: HitPoints,
    ) -> HpChange {
        let Some(hit_points) = combat.hp_powerup.absorb(loss) else {
            return HpChange::Settled;
        };
        let floor = master
            .creatable_part()
            .invulnerability
            .floor(master.hit_points);
        let left = self.life.hp - hit_points;
        self.life.hp = if floor == HitPoints::ZERO || floor <= left {
            left
        } else {
            floor
        };
        if below_or_unordered(self.life.hp.get(), 1.0) {
            HpChange::Fell(credit_combat(combat, self.life.hp))
        } else {
            HpChange::TransformTest
        }
    }

    /// `TRIBE_Combat_Object::update`'s regeneration (`0x141208440` after
    /// `RGE_Combat_Object::update`): a living object (at least 1 hit point, `COMISS`/`JC`: a NaN
    /// is not; in state 2) whose rate ([`regeneration_rate`], slot 0x260) is above
    /// [`REGENERATION_EPSILON`] in size (a NaN is) earns a whole hit point per 60 / |rate|
    /// seconds: the world's dt plus the time held back (`berserker_timer`, `+0xa10`) over that
    /// period (none for a period of at most [`REGENERATION_EPSILON`]), its fraction (`fmodf`)
    /// held back as time. Whole points above 0 heal (a positive rate: slot 0xb8,
    /// [`Unit::gain_hp`]) at most the room below its most (slot 200,
    /// [`most_hp`]), or
    /// come off (slot 0xb0, [`Unit::lose_hp`]) at most its hit points with its powerups' room
    /// (slot 0xc0). Returns what the loss or the gain leaves to the caller ([`HpChange`]: the
    /// kill credits of a loss that left it below 1 hit point, or the hit point transforms' test)
    /// and the most it read before them (slot 200), with which the caller then gives back the
    /// armor its charge's hits took ([`Unit::restore_charged_armor`]); none where it took no
    /// whole point.
    pub(crate) fn regenerate(
        &mut self,
        combat: &mut Combat,
        master: &Master,
        dt_seconds: Seconds,
        turn_tuning: &TurnTuning,
    ) -> Regenerated {
        let rate = regeneration_rate(master, combat, turn_tuning).get();
        if below_or_unordered(self.life.hp.get(), turn_tuning.regeneration_least_hp)
            || self.life.state != ObjectState::Alive
        {
            return Regenerated::NONE;
        }
        if !below_or_unordered(REGENERATION_EPSILON, rate.abs()) {
            return Regenerated::NONE;
        }
        let held = dt_seconds + combat.regen_timer;
        let per = (REGENERATION_MINUTE / rate).abs();
        let points = if below_or_unordered(REGENERATION_EPSILON, per.abs()) {
            held.get() / per
        } else {
            0.0
        };
        let fraction = points % turn_tuning.regeneration_point;
        let whole = points - fraction;
        combat.regen_timer = Seconds::new(fraction * per);
        if at_most_or_unordered(whole, 0.0) {
            return Regenerated::NONE;
        }
        let most = most_hp(combat.hp_stinger, master.hit_points, combat.hp_powerup).get();
        let mut change = HpChange::Settled;
        let now = (combat.hp_powerup.room() + self.life.hp).get();
        if at_most_or_unordered(rate, 0.0) {
            let amount = if at_most_or_unordered(whole, now) {
                whole
            } else {
                now
            };
            if !at_most_or_unordered(amount, 0.0) {
                change = self.lose_hp(combat, master, HitPoints::new(amount));
            }
        } else {
            let left = most - now;
            let amount = if at_most_or_unordered(whole, left) {
                whole
            } else {
                left
            };
            if !at_most_or_unordered(amount, 0.0) {
                change = self.gain_hp(combat, master, HitPoints::new(amount));
            }
        }
        Regenerated {
            change,
            most: Some(HitPoints::new(most)),
        }
    }

    /// The regeneration's end (`0x141208440`, after slot 0xb0 or 0xb8 and its transforms' test):
    /// at its most hit points `most` (read before, slot 200; slot 0xc0 now at least it, `COMISS`:
    /// a NaN is not) the armor its charge's hits took (`+0x9e4`, `+0x9e6`) comes back.
    pub(crate) fn restore_charged_armor(&self, combat: &mut Combat, most: HitPoints) {
        if most <= combat.hp_powerup.room() + self.life.hp {
            combat.armor_removed = ArmorPair::ZERO;
        }
    }

    /// `TRIBE_Combat_Object::update`'s hit point powerup check (after its abilities): a powerup
    /// (`+0xa38`) above its room (`+0xa3c`; `COMISS`/`JBE`, a NaN is not) comes off the hit points
    /// (`MAXSS` with 1: not below 1, a NaN becomes 1) and the powerup takes the room.
    pub(crate) fn clamp_hp_powerup(&mut self, combat: &mut Combat, turn_tuning: &TurnTuning) {
        let hp_powerup = combat.hp_powerup;
        if at_most_or_unordered(hp_powerup.taken.get(), hp_powerup.max.get()) {
            return;
        }
        self.life.hp = HitPoints::new(maxss(
            self.life.hp.get() - hp_powerup.room().get().abs(),
            turn_tuning.hp_powerup_floor,
        ));
        combat.hp_powerup.taken = hp_powerup.max;
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::HitPoints;
    use crate::objects::HpPowerup;

    /// A powerup's room is its most less what it took; it takes a loss up to its room and
    /// passes the rest on, and passes all of it at or above its most (a NaN most too).
    #[test]
    fn powerup_takes_loss_up_to_its_room_and_passes_the_rest() {
        let mut hp_powerup = HpPowerup {
            taken: HitPoints::new(2.0),
            max: HitPoints::new(5.0),
        };
        assert_eq!(hp_powerup.room(), HitPoints::new(3.0));
        assert_eq!(hp_powerup.absorb(HitPoints::new(1.0)), None);
        assert_eq!(hp_powerup.taken, HitPoints::new(3.0));
        assert_eq!(
            hp_powerup.absorb(HitPoints::new(4.0)),
            Some(HitPoints::new(2.0))
        );
        assert_eq!(
            (hp_powerup.taken, hp_powerup.room()),
            (HitPoints::new(5.0), HitPoints::ZERO)
        );
        assert_eq!(
            hp_powerup.absorb(HitPoints::new(1.5)),
            Some(HitPoints::new(1.5))
        );
        let mut nan = HpPowerup {
            taken: HitPoints::ZERO,
            max: HitPoints::new(f32::NAN),
        };
        assert_eq!(nan.absorb(HitPoints::new(1.0)), Some(HitPoints::new(1.0)));
        assert_eq!(nan.taken, HitPoints::ZERO);
        assert_eq!(HpPowerup::NONE, HpPowerup::default());
    }
}
