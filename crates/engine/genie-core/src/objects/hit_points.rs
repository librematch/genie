//! A combat object's hit points, a unit's or a building's (`TRIBE_Combat_Object`, types 70 and
//! 80): the heal, the gain and the loss, the regeneration and the powerup's clamp (slots 0x110,
//! 0xb8 and 0x260, and the update's checks). Whatever heals (a monk's heal action, a building
//! that heals around it) calls the healed object's heal here.

use crate::GenieError;
use crate::combat::damage::ArmorPair;
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::Master;
use crate::objects::masters::class;
use crate::objects::unit::Unit;
use crate::world::Seconds;
use genie_diff::Diff;

/// A hero's regeneration (`DAT_142ed06a8`, a minute's hit points).
pub(crate) const HERO_REGENERATION: f32 = crate::math::image::turn::HERO_REGENERATION;
/// The regeneration's minute (`DAT_14330ebc8`).
pub(crate) const REGENERATION_MINUTE: f32 = crate::math::image::turn::REGENERATION_MINUTE;
/// The size below which a rate or a period is none (`DAT_142f49458`).
pub(crate) const REGENERATION_EPSILON: f32 = crate::math::image::turn::REGENERATION_EPSILON;
/// The hit points below which nothing regenerates (`DAT_14330eb8c`).
pub(crate) const REGENERATION_LEAST_HP: f32 = crate::math::image::turn::REGENERATION_LEAST_HP;
/// A whole hit point, the `fmodf` divisor (`XMM11`, `DAT_14330eb8c`).
pub(crate) const REGENERATION_POINT: f32 = crate::math::image::turn::REGENERATION_POINT;
/// The hit points a powerup's check leaves at least (`MAXSS` with `DAT_14330eb8c`).
pub(crate) const HP_POWERUP_FLOOR: f32 = crate::math::image::turn::HP_POWERUP_FLOOR;

/// A combat object's hit point powerup, which takes a loss before its hit points: what losses
/// took of it (`+0xa38`, `mHitpointsPowerup`) and its most (`+0xa3c`, `mHitpointsMaxPowerup`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct HpPowerup {
    /// `+0xa38`: what losses took of it.
    pub taken: HitPoints,
    /// `+0xa3c`: its most.
    pub max: HitPoints,
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

    /// Slot 0xb0's first part: below its most, it takes the loss `d` up to its room. The rest
    /// of the loss, or None when it took all of it.
    pub fn absorb(&mut self, d: HitPoints) -> Option<HitPoints> {
        let (taken, max) = (self.taken, self.max);
        if taken < max {
            let room = max - taken;
            if d <= room {
                self.taken = taken + d;
                return None;
            }
            self.taken = taken + room;
            return Some(d - room);
        }
        Some(d)
    }
}

/// Slot 0x260 of a combat object (`FUN_1412033a0`), its regeneration rate in hit points a
/// minute: its stinger (`+0xb3c`, slot 0x750) plus (its powerup (`+0xa34`, slot 0x6f0) plus (a
/// hero's [`HERO_REGENERATION`] plus its master's `regeneration_rate`, `+0x8b4`)), in the
/// assembly's order. A hero is a master whose `hero_flag` (`+0x8d9`) grants
/// [`HeroFlags::REGENERATES`].
#[must_use]
pub(crate) fn regeneration_rate(master: &Master, unit: &Unit) -> f32 {
    let hero = master.hero_flag.grants(HeroFlags::REGENERATES);
    let bonus = if hero { HERO_REGENERATION } else { 0.0 };
    unit.regen_stinger + (unit.regen_powerup + (bonus + master.regeneration))
}

impl Unit {
    /// Slot 0x110 of a combat object (`FUN_141213b10`) of its master's `class` and
    /// `hit_points`: the classes 2, 20, 21, 22 and 53 repair at a cost (not modelled); any other
    /// heals (`FUN_141193b30`, [`crate::combat::damage::heal_hp`]).
    pub(crate) fn heal(
        &mut self,
        class: ClassId,
        hit_points: i16,
        amount: HitPoints,
    ) -> Result<(), GenieError> {
        if matches!(
            class,
            class::TRADE_BOAT
                | class::TRANSPORT
                | class::FISHING_SHIP
                | class::WARSHIP
                | class::CLASS_53
        ) {
            return Err(GenieError::unsupported(
                "a heal at a cost (FUN_141213b10's classes)",
            ));
        }
        let _ = crate::combat::damage::heal_hp(
            &mut self.life.hp,
            &mut self.hp_powerup,
            hit_points,
            amount,
        );
        Ok(())
    }

    /// Slot 0xb8 of a combat object (`FUN_141203be0`): `amount` added up to its hit points with
    /// its powerups' room, an overflow taken off the powerup (`+0xa38`, never below 0; the hit
    /// point transforms of `FUN_141204450`, empty in the recordings, are not held).
    pub(crate) fn gain_hp(&mut self, master: &Master, amount: HitPoints) {
        crate::combat::damage::gain_hp(
            &mut self.life.hp,
            &mut self.hp_powerup,
            master.hit_points,
            amount,
        );
    }

    /// Slot 0xb0 of a combat object (`FUN_141205c80`), the loss of `d` hit points: the hit point
    /// powerups' room (`+0xa38` up to `+0xa3c`) takes it first, the rest comes off the hit points,
    /// which stay above the master's `invulnerability_level` floor (`FUN_141202a80`). Not held
    /// (no update of the recordings fills either list): a dying object's kill credits of its
    /// active stingers (`+0xb58`, `+0x954`) and a surviving one's hit point transforms
    /// (`FUN_141204450`); [`crate::combat::damage::lose`] is the same loss on a hit's victim.
    pub(crate) fn lose_hp(&mut self, master: &Master, d: HitPoints) {
        let Some(d) = self.hp_powerup.absorb(d) else {
            return;
        };
        let floor =
            crate::combat::damage::invulnerability_floor(master.invulnerability, master.hit_points);
        let left = self.life.hp - d;
        self.life.hp = if floor == HitPoints::ZERO || floor <= left {
            left
        } else {
            floor
        };
    }

    /// `TRIBE_Combat_Object::update`'s regeneration (`0x141208440` after
    /// `RGE_Combat_Object::update`): a living object (at least 1 hit point, `COMISS`/`JC`: a NaN
    /// is not; in state 2) whose rate ([`regeneration_rate`], slot 0x260) is above
    /// [`REGENERATION_EPSILON`] in size (a NaN is) earns a whole hit point per 60 / |rate|
    /// seconds: the world's dt plus the time held back (`berserker_timer`, `+0xa10`) over that
    /// period (none for a period of at most [`REGENERATION_EPSILON`]), its fraction (`fmodf`)
    /// held back as time. Whole points above 0 heal (a positive rate: slot 0xb8,
    /// [`Unit::gain_hp`]) at most the room below its most (slot 200: its master's hit points plus
    /// its powerups' room; the hit point stinger `+0xb44`, written only by auras, is not held), or
    /// come off (slot 0xb0, [`Unit::lose_hp`]) at most its hit points with its powerups' room
    /// (slot 0xc0). Then an object at its most (slot 0xc0 at least slot 200) has the armor its
    /// charge's hits took back (`+0x9e4` and `+0x9e6`, one dword, zeroed).
    pub(crate) fn regenerate(&mut self, master: &Master, dt_seconds: Seconds) {
        use crate::math::fmath::{at_most_or_unordered, below_or_unordered};
        let rate = regeneration_rate(master, self);
        if below_or_unordered(self.life.hp.get(), REGENERATION_LEAST_HP)
            || self.life.state != ObjectState::Alive
        {
            return;
        }
        if !below_or_unordered(REGENERATION_EPSILON, rate.abs()) {
            return;
        }
        let held = dt_seconds + self.regen_timer;
        let per = (REGENERATION_MINUTE / rate).abs();
        let points = if below_or_unordered(REGENERATION_EPSILON, per.abs()) {
            held.get() / per
        } else {
            0.0
        };
        let fraction = points % REGENERATION_POINT;
        let whole = points - fraction;
        self.regen_timer = Seconds::new(fraction * per);
        if at_most_or_unordered(whole, 0.0) {
            return;
        }
        let most = f32::from(master.hit_points) + self.hp_powerup.max.get();
        let now = (self.hp_powerup.room() + self.life.hp).get();
        if at_most_or_unordered(rate, 0.0) {
            let amount = if at_most_or_unordered(whole, now) {
                whole
            } else {
                now
            };
            if !at_most_or_unordered(amount, 0.0) {
                self.lose_hp(master, HitPoints::new(amount));
            }
        } else {
            let left = most - now;
            let amount = if at_most_or_unordered(whole, left) {
                whole
            } else {
                left
            };
            if !at_most_or_unordered(amount, 0.0) {
                self.gain_hp(master, HitPoints::new(amount));
            }
        }
        // at its most hit points (slot 0xc0 at least slot 200, `COMISS`: a NaN is not) the
        // armor its charge's hits took (`+0x9e4`, `+0x9e6`) comes back
        if most <= (self.hp_powerup.room() + self.life.hp).get() {
            self.armor_removed = ArmorPair::ZERO;
        }
    }

    /// `TRIBE_Combat_Object::update`'s hit point powerup check (after its abilities): a powerup
    /// (`+0xa38`) above its room (`+0xa3c`; `COMISS`/`JBE`, a NaN is not) comes off the hit points
    /// (`MAXSS` with 1: not below 1, a NaN becomes 1) and the powerup takes the room.
    pub(crate) fn clamp_hp_powerup(&mut self) {
        let p = self.hp_powerup;
        if crate::math::fmath::at_most_or_unordered(p.taken.get(), p.max.get()) {
            return;
        }
        self.life.hp = HitPoints::new(crate::math::fmath::maxss(
            self.life.hp.get() - p.room().get().abs(),
            HP_POWERUP_FLOOR,
        ));
        self.hp_powerup.taken = p.max;
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::HitPoints;
    use crate::objects::HpPowerup;

    /// A powerup's room is its most less what it took; it takes a loss up to its room and
    /// passes the rest on, and passes all of it at or above its most (a NaN most too).
    #[test]
    fn a_powerup_takes_a_loss_up_to_its_room_and_passes_the_rest() {
        let mut p = HpPowerup {
            taken: HitPoints::new(2.0),
            max: HitPoints::new(5.0),
        };
        assert_eq!(p.room(), HitPoints::new(3.0));
        assert_eq!(p.absorb(HitPoints::new(1.0)), None);
        assert_eq!(p.taken, HitPoints::new(3.0));
        assert_eq!(p.absorb(HitPoints::new(4.0)), Some(HitPoints::new(2.0)));
        assert_eq!((p.taken, p.room()), (HitPoints::new(5.0), HitPoints::ZERO));
        assert_eq!(p.absorb(HitPoints::new(1.5)), Some(HitPoints::new(1.5)));
        let mut nan = HpPowerup {
            taken: HitPoints::ZERO,
            max: HitPoints::new(f32::NAN),
        };
        assert_eq!(nan.absorb(HitPoints::new(1.0)), Some(HitPoints::new(1.0)));
        assert_eq!(nan.taken, HitPoints::ZERO);
        assert_eq!(HpPowerup::NONE, HpPowerup::default());
    }
}
