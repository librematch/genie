//! A unit's attack rate and firepower, and its share of a building's volleys
//! (`FUN_141213520`).

use crate::GenieError;
use crate::combat::Firepower;
use crate::garrison::DamageRate;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::{at_most_or_unordered, maxss};
use crate::math::image::garrison::{
    EPSILON as GARRISON_EPSILON, ONE_MISSILE as GARRISON_ONE_MISSILE,
    RATE_RANGE as GARRISON_RATE_RANGE,
};
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::class;
use crate::objects::masters::{ClassAmount, DamageClass};
use crate::objects::masters::{Master, Masters};
use crate::world::Seconds;

/// A combat master's attack rate needs a weapon range above this (`FUN_14120dc10`).
pub(super) const RATE_RANGE: f32 = GARRISON_RATE_RANGE;
/// A unit's firepower counts its volley missiles beyond this many (`FUN_14120e1a0`).
pub(super) const ONE_MISSILE: f32 = GARRISON_ONE_MISSILE;
/// A reload at most this long gives no firepower (`FUN_14120e1a0`).
pub(super) const EPSILON: f32 = GARRISON_EPSILON;
/// The class whose garrison share is 1 whatever its firepower (`FUN_141213520`, `-0xe82` raw);
/// UNLOAD's flag 0 leaves it inside (`FUN_14118a610`).
pub(super) const WHOLE_SHARE_CLASS: ClassId = class::RELIC;

/// The amounts of `weapons` (class, amount) whose class `counts`.
pub(super) fn weapons_sum(weapons: &[ClassAmount], counts: impl Fn(DamageClass) -> bool) -> f32 {
    weapons
        .iter()
        .filter(|w| counts(w.class))
        .fold(0.0, |sum, w| sum + w.amount.as_f32())
}

/// `FUN_14120dc10`: the attack rate of an object of `master` (its reload powerup `+0xa28`
/// taken off its reload): none without a reload or a range above 1 (an unordered one too);
/// else the pierce and class 38 amounts of its volley missile (a master its player has; not for
/// classes 4 and 6), else its own melee, pierce, class 31 and 38 amounts, over the reload.
#[must_use]
pub fn attack_rate(master: &Master, masters: &Masters, reload_powerup: Seconds) -> DamageRate {
    // COMISS reload, 0 and COMISS range, 1.0, each with JBE
    if at_most_or_unordered(master.reload.get(), 0.0)
        || at_most_or_unordered(master.weapon_range.get(), RATE_RANGE)
    {
        return DamageRate::ZERO;
    }
    // the game reads the slot of an id below the player's master count without testing it: a
    // null slot there would crash it, so a master the player has is the only case it runs
    let volley = master
        .volley_missile
        .and_then(|id| masters.get(id))
        .filter(|_| !matches!(master.class, class::VILLAGER | class::CLASS_6));
    let sum = volley.map_or_else(
        || {
            weapons_sum(&master.weapons, |c| {
                matches!(
                    c,
                    DamageClass::MELEE
                        | DamageClass::PIERCE
                        | DamageClass::CLASS_31
                        | DamageClass::CLASS_38
                )
            })
        },
        |v| {
            weapons_sum(&v.weapons, |c| {
                matches!(c, DamageClass::PIERCE | DamageClass::CLASS_38)
            })
        },
    );
    DamageRate::new(sum / (master.reload - reload_powerup).get())
}

/// Whether the player's master table has a slot for `id` (`0 <= id < player+0x20`), as far as
/// [`Masters`] knows it: up to its last master. The game's count can run past the last master
/// with null slots, which [`Masters`] does not keep.
pub(super) fn in_table(masters: &Masters, id: MasterId) -> bool {
    masters.iter().any(|m| id <= m.id)
}

/// `FUN_14120e1a0`: what a unit of `master` (its reload powerup `reload_powerup`) fires inside a
/// building. A master's garrison firepower at most 0 is its magnitude; else the firepower times
/// its rate: the pierce and class 38 amounts of its volley missile (else its missile; the slot
/// the player's table holds, nothing for a null one) times its volley fire past one missile (in
/// whole missiles, `CVTTSS2SI`, the products wrapping), plus its own melee, pierce, class 31 and
/// 38 amounts, over its reload (none at most 1e-7).
#[must_use]
pub fn firepower(master: &Master, masters: &Masters, reload_powerup: Seconds) -> DamageRate {
    let g = master.garrison_firepower.get();
    // COMISS 0, g; JC: an unordered firepower goes on
    if g <= 0.0 {
        return DamageRate::new(g.abs());
    }
    let mut sum = 0.0f32;
    // COMISS volley, 1.0; JBE: an unordered volley fire adds no missiles
    if ONE_MISSILE < master.volley_fire.get() {
        // the volley missile's slot when the table has one, else the missile's (an id out of
        // the table gives none); a null slot or one without weapons adds nothing
        let missile = master
            .volley_missile
            .filter(|&id| in_table(masters, id))
            .or_else(|| master.missile.master())
            .and_then(|id| masters.get(id));
        if let Some(m) = missile {
            let extra = cvttss2si(master.volley_fire.get() - ONE_MISSILE);
            for &ClassAmount { class, amount } in &m.weapons {
                if matches!(class, DamageClass::PIERCE | DamageClass::CLASS_38) {
                    // IMUL EAX, R8D then CVTDQ2PS
                    sum += i32::from(amount.get()).wrapping_mul(extra) as f32;
                }
            }
        }
    }
    // one sum, the missile's amounts first
    for &ClassAmount { class, amount } in &master.weapons {
        if matches!(
            class,
            DamageClass::MELEE
                | DamageClass::PIERCE
                | DamageClass::CLASS_31
                | DamageClass::CLASS_38
        ) {
            sum += amount.as_f32();
        }
    }
    let reload = (master.reload - reload_powerup).get();
    if reload <= EPSILON {
        return DamageRate::ZERO;
    }
    DamageRate::new(sum / reload * g)
}

/// `FUN_141213520`: the share of a building's volleys a unit of `master` inside it adds, against
/// the building's attack rate `rate` ([`attack_rate`]): none against a rate of ±0 (`_fdclass`);
/// 1 for class 42; else its [`firepower`] over the rate, at least the floor (`MAXSS`): 0, or for
/// a master with object flag 3 the firepower over the rate of the master the flags' upper half
/// names (`+0x656`, the `.dat`'s trait piece) when that one is of type 70.
pub fn share(
    master: &Master,
    masters: &Masters,
    reload_powerup: Seconds,
    rate: DamageRate,
) -> Result<Firepower, GenieError> {
    if rate.get() == 0.0 {
        return Ok(Firepower::ZERO);
    }
    if master.class == WHOLE_SHARE_CLASS {
        return Ok(Firepower::new(1.0));
    }
    let mut floor = 0.0;
    if master.object_flags.contains(ObjectFlags::TRANSFORMS) {
        // MOVZX of the ushort at +0x656; the game reads the slot without a test, so a null one
        // would crash it: a master the player has is the only case it runs
        if let Some(m) = master
            .object_flags
            .piece()
            .and_then(|id| masters.get(id))
            // `SEEK_SHELTER`'s villagers: class 4 of type 70.
            // The type of a combat unit: the master a transforming master names (`+0x656`), and a unit a
            // transport takes in (`FUN_14120eb10`).
            && m.kind == MasterKind::COMBAT
        {
            floor = firepower(m, masters, reload_powerup).over(rate);
        }
    }
    Ok(Firepower::new(maxss(
        firepower(master, masters, reload_powerup).over(rate),
        floor,
    )))
}
