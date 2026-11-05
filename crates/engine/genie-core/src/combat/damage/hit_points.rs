//! The shield, the invulnerability floor, and the hit points' loss (slot 0xb0) and gain
//! (slot 0xb8).

use crate::combat::Charge;
use crate::combat::damage::{Armored, Victim};
use crate::combat::stingers::credit;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered};
use crate::objects::HitPoints;
use crate::objects::HpPowerup;
use crate::objects::MasterHitPoints;
use crate::objects::masters::ChargeKind;
use crate::objects::masters::MasterKind;

/// What lands a hit: a swing, or a missile (a shield of charge type 5 blocks the first, 4 the
/// second: [`shield`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HitKind {
    /// A melee swing.
    Melee,
    /// A missile.
    Missile,
}

/// Whether a shield blocked a hit ([`shield`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum ShieldBlock {
    /// It spent a charge on the hit.
    Blocked,
    /// The hit gets through.
    Passes,
}

/// `FUN_14108d8a0`: a shield blocks the hit: a target whose master's charge type is 5 against a
/// melee hit (4 against a missile) spends a charge of 1 when it has one (`COMISS 1.0, charge`,
/// `JA` past: a NaN charge blocks too). The type-5 shield's local-player statistics
/// (`FUN_140a497f0`) change no simulation state (FD-037-D20). A target without combat values has
/// charge type 0.
pub fn shield(victim: &mut Victim, hit: HitKind) -> ShieldBlock {
    let kind = match hit {
        HitKind::Melee => ChargeKind::MELEE_SHIELD,
        HitKind::Missile => ChargeKind::MISSILE_SHIELD,
    };
    if victim.source.charge_kind == kind && at_most_or_unordered(1.0, victim.source.charge.get()) {
        victim.source.charge -= Charge::new(1.0);
        return ShieldBlock::Blocked;
    }
    ShieldBlock::Passes
}

/// `FUN_141202a80`: the hit points the master's `invulnerability_level` keeps (a share of its hit
/// points when above 0, the level itself when below), 0 for none.
pub(super) fn hp_floor(armored: &Armored, master_hp: MasterHitPoints) -> HitPoints {
    armored.invulnerability.floor(master_hp)
}

/// Slot 0xc0: the hit points with the powerups' room.
pub(super) fn effective(victim: &Victim) -> HitPoints {
    let pu = victim
        .armored
        .as_ref()
        .map_or(HpPowerup::NONE, |armored| armored.hp_powerup);
    pu.room() + victim.hp
}

/// Slot 0xb0, the loss of `hit_points` hit points: a combat object of type 70 or 80
/// (`FUN_141205c80`) takes it on its hit point powerups' room first, and its hit points stay above
/// its `invulnerability_level` floor; left below 1, its stingers credit its kill ([`credit`]).
/// Another object (`FUN_141189160`) just loses it. Not ported yet (MISSING.md): a surviving
/// object's hit point transforms (`+0xb20`, `FUN_141204450`).
pub fn lose(victim: &mut Victim, damage: HitPoints) {
    let unit = matches!(victim.kind, MasterKind::COMBAT | MasterKind::BUILDING);
    let Some(armored) = victim.armored.as_mut().filter(|_| unit) else {
        victim.hp -= damage;
        return;
    };
    let Some(hit_points) = armored.hp_powerup.absorb(damage) else {
        return;
    };
    let floor = hp_floor(armored, victim.master_hp);
    let left = victim.hp - hit_points;
    victim.hp = if floor == HitPoints::ZERO || floor <= left {
        left
    } else {
        floor
    };
    credit(victim);
}

/// Slot 0xc8 (200) of a combat unit or building (`FUN_141202bb0`), its most hit points: its hit
/// point stinger `stinger` (`+0xb44`, slot 0x760) plus its master's hit points `master_hp`, plus
/// its most hit points powerup (`+0xa3c`, `powerup`'s room).
#[must_use]
pub fn most_hp(stinger: HitPoints, master_hp: MasterHitPoints, powerup: HpPowerup) -> HitPoints {
    HitPoints::new(stinger.get() + HitPoints::of_master(master_hp).get() + powerup.max.get())
}

/// Slot 0xb8 of a combat object (`FUN_141203be0`) on hit points `hp` with the powerups
/// `hp_powerups` (`+0xa38`, its room `+0xa3c`) of a master of `master_hp` and the hit point
/// stinger `stinger`: `amount` added up to its most (slot 200, [`most_hp`]), an overflow taken
/// off the powerup (never below 0); a powerup above its room first takes the room. The hit point
/// transforms of `FUN_141204450`, empty in the recordings, are not held.
pub fn gain_hp(
    hp: &mut HitPoints,
    powerup: &mut HpPowerup,
    (master_hp, stinger): (MasterHitPoints, HitPoints),
    amount: HitPoints,
) {
    let (mut up, room) = (powerup.taken, powerup.max);
    let most = most_hp(stinger, master_hp, *powerup);
    if room < up {
        up = room;
    }
    if most <= *hp {
        if HitPoints::ZERO < up {
            up = HitPoints::new((up - amount).get().max(0.0));
        }
    } else {
        let healed = *hp + amount;
        *hp = healed;
        if most < healed {
            *hp = most;
            if HitPoints::ZERO < up {
                up = HitPoints::new((up - (healed - most)).get().max(0.0));
            }
        }
    }
    powerup.taken = up;
}

/// Whether a heal left its object at its full hit points ([`heal_hp`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum Heal {
    /// It holds exactly its master's hit points.
    Full,
    /// It holds fewer.
    Partial,
}

/// `FUN_141193b30`: slot 0xb8 adds `amount` ([`gain_hp`]); a loss (`amount` below 0) leaves at
/// least 1 hit point; healed to its full hit points (slot 0xc0 at least slot 200, [`most_hp`];
/// `COMISS` with `JC`: a NaN is not), it holds exactly its master's ([`Heal::Full`], else
/// [`Heal::Partial`]).
pub fn heal_hp(
    hp: &mut HitPoints,
    powerup: &mut HpPowerup,
    (master_hp, stinger): (MasterHitPoints, HitPoints),
    amount: HitPoints,
) -> Heal {
    gain_hp(hp, powerup, (master_hp, stinger), amount);
    if amount < HitPoints::ZERO && *hp < HitPoints::new(1.0) {
        *hp = HitPoints::new(1.0);
    }
    let most = most_hp(stinger, master_hp, *powerup);
    if below_or_unordered((powerup.room() + *hp).get(), most.get()) {
        return Heal::Partial;
    }
    *hp = HitPoints::of_master(master_hp);
    Heal::Full
}

/// `FUN_141193b30` on an object below type 70 or a tree, slot 0x110 of its class: its slot 0xb8
/// (`FUN_141188ca0`) adds `amount` as it is, and its slot 200 (`FUN_1411889a0`) is its master's
/// hit points, `master_hp`. A loss leaves at least 1 hit point; at its master's hit points
/// (`COMISS` with `JC`: a NaN is not) it holds exactly them ([`Heal::Full`], else
/// [`Heal::Partial`]).
pub fn heal_base_hp(hp: &mut HitPoints, master_hp: MasterHitPoints, amount: HitPoints) -> Heal {
    *hp += amount;
    if amount < HitPoints::ZERO && *hp < HitPoints::new(1.0) {
        *hp = HitPoints::new(1.0);
    }
    let most = HitPoints::of_master(master_hp);
    if below_or_unordered(hp.get(), most.get()) {
        return Heal::Partial;
    }
    *hp = most;
    Heal::Full
}
