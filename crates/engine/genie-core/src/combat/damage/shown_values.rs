//! The bonus damage tasks (`FUN_14107ce60`) and the attack, armor and range a combat object
//! shows.

use crate::GenieError;
use crate::combat::damage::{
    EPSILON, ONE, Victim, charge_hit, charged, master_melee_armor, melee_armor, range_charge_hit,
};
use crate::map::Distance;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered, maxss, minss};
use crate::objects::HitPoints;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ChargeEvent;
use crate::objects::masters::ChargeTarget;
use crate::objects::masters::DamageAmount;
use crate::objects::masters::MasterKind;
use crate::objects::masters::{ClassAmount, DamageClass};
use crate::players::attribute::AttributeId;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;

/// The action type of a bonus damage task (`FUN_14107ce60`).
pub const BONUS_DAMAGE: ActionKind = ActionKind::named(0xa0);

impl Victim {
    /// A combat unit or building (types 70 and 80, `TRIBE_Combat_Object` and
    /// `TRIBE_Building_Object`): its powerups and charges count; a combat object of type 50 or
    /// 60 (`RGE_Combat_Object`) answers 0 for them.
    pub(super) const fn tribe(&self) -> bool {
        matches!(self.kind, MasterKind::COMBAT | MasterKind::BUILDING)
    }

    /// Slot 0xc0, its hit points as a share reads them: a combat unit's or building's
    /// (`FUN_141202990`) most hit points powerup less its hit points powerup (`+0xa3c` less
    /// `+0xa38`), plus its hit points; any other object's its hit points (`FUN_141188830`).
    #[must_use]
    pub fn shown_hp(&self) -> HitPoints {
        self.armored
            .as_ref()
            .filter(|_| self.tribe())
            .map_or(self.hp, |a| a.hp_powerup.room() + self.hp)
    }

    /// Slot 0xc8, its most hit points: a combat unit's or building's (`FUN_141202bb0`) hit point
    /// stinger (`+0xb44`, slot 0x760) plus its master's hit points plus its most hit points
    /// powerup (`+0xa3c`); any other object's its master's hit points (`FUN_1411889a0`). The
    /// stingers (`+0xb38`..`+0xb50`) are 0: `TRIBE_Combat_Object::update` zeroes them and rebuilds
    /// them from the object's stinger list (`FUN_1412092e0`), which the slice holds empty.
    #[must_use]
    pub fn most_hp(&self) -> HitPoints {
        let hp = HitPoints::of_master(self.master_hp);
        self.armored
            .as_ref()
            .filter(|_| self.tribe())
            .map_or(hp, |a| hp + a.hp_powerup.max)
    }

    /// `FUN_141188880`: the share of its most hit points it has (slot 0xc0, at most slot 0xc8 by
    /// `MINSS`, over slot 0xc8), with `lacking` the share it lacks (1 less that); 0 when its most
    /// hit points are not above 0 (`COMISS 0, most` with `JNC`: a NaN divides).
    #[must_use]
    pub fn hp_share(&self, lacking: bool) -> f32 {
        let most = self.most_hp();
        let least = minss(most.get(), self.shown_hp().get());
        if !below_or_unordered(0.0, most.get()) {
            return 0.0;
        }
        let share = least / most.get();
        if lacking { ONE - share } else { share }
    }
}

/// `FUN_14107ce60`: the damage `attacker`'s bonus damage tasks ([`Victim::bonus_tasks`]) add
/// against `target`. A task for the target's master (`object_id`) or class (`object_group`), or
/// for neither (both −1) on a target of class 51 or 54 or of a type below 80, adds its
/// `work_val1` for each whole `work_val2` of a hit point share ([`Victim::hp_share`], rounded
/// down after [`EPSILON`] is added): the target's with a `work_range` above 0, else the
/// attacker's (`COMISS` with `CMOVBE`: a NaN reads the attacker's), the share lacking with
/// `attribute_type4` not 0. A `work_val2` not above 0, or a target without hit points above 0
/// (slot 0xc0), adds nothing (`COMISS` with `JBE`). The sum is at least 0 (`MAXSS`).
#[must_use]
pub fn bonus_damage(attacker: &Victim, target: &Victim, roles: &Roles) -> f32 {
    let mut sum = 0.0f32;
    for t in attacker
        .bonus_tasks
        .iter()
        .filter(|t| t.action_kind == BONUS_DAMAGE)
    {
        let any = t.object_id.is_none()
            && t.object_group.is_none()
            && (roles
                .of_class(target.class)
                .intersects(ClassRoles::PACKED_TREBUCHET | ClassRoles::UNPACKED_TREBUCHET)
                || target.kind < MasterKind::BUILDING);
        if !(any || t.object_id == Some(target.master_id) || t.for_class(target.class)) {
            continue;
        }
        let whose = if at_most_or_unordered(t.work_range.get(), 0.0) {
            attacker
        } else {
            target
        };
        // the task's `+0x26` read as a flag: other than 0 (none, −1, is other too)
        let share = whose.hp_share(AttributeId::field(t.attribute_kind4) != 0);
        if at_most_or_unordered(t.work_val2.get(), 0.0)
            || at_most_or_unordered(target.shown_hp().get(), 0.0)
        {
            continue;
        }
        sum += libm::floorf(share / t.work_val2.get() + EPSILON) * t.work_val1.get();
    }
    maxss(sum, 0.0)
}

/// What the attack and the range a combat object shows (slots 0x878 and 0x880) read besides the
/// object as a hit reads it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Shown<'a> {
    /// The target of its top action when that attacks (action type 9 or 7, `FUN_14107cc30`).
    pub target: Option<&'a Victim>,
    /// The weapons (class, amount) of its master's charge projectile (`+0x928`) when its player
    /// holds that master (`FUN_141659d20`).
    pub projectile: Option<&'a [ClassAmount]>,
    /// Its master's `charge_event` (`+0x920`), which a ready charge of type 6 or 7 adds to its
    /// range.
    pub charge_event: ChargeEvent,
    /// Its max range powerup (`+0xa40`, slot 0x700).
    pub range_powerup: Distance,
}

/// Slot 0x320 of a combat unit or building (`FUN_141211440`): its charge is ready (slot 0x300,
/// [`charged`]) and, unless its master's `charge_target` is −1, works on `target` (slot 0x2f0,
/// [`charge_hit`], or slot 0x2f8, [`range_charge_hit`]); with a `charge_target` but no target,
/// not. A combat object of type 50 or 60 has no charge (`FUN_14106c580`).
pub(super) fn charge_shown(v: &Victim, target: Option<&Victim>, roles: &Roles) -> bool {
    let a = &v.source;
    v.tribe()
        && charged(a)
        && (a.charge_target == ChargeTarget::EVERY
            || target.is_some_and(|t| charge_hit(a, t, roles) || range_charge_hit(a, t, roles)))
}

/// The best amount of `weapons` above 0 (the first of a tie), of a melee, pierce or class-31
/// weapon with `fighting`.
pub(super) fn best_attack(weapons: &[ClassAmount], fighting: bool) -> DamageAmount {
    weapons
        .iter()
        .filter(|w| {
            !fighting
                || matches!(
                    w.class,
                    DamageClass::PIERCE | DamageClass::MELEE | DamageClass::CLASS_31
                )
        })
        .fold(
            DamageAmount::ZERO,
            |m, w| {
                if m < w.amount { w.amount } else { m }
            },
        )
}

/// Slot 0x878 of a combat object, the attack it shows (`FUN_141085c70`; a combat unit's or
/// building's `FUN_141213990`). With a ready charge of type 6 or 7 ([`charge_shown`]) and its
/// charge projectile's master among its player's, that master's best melee, pierce or class-31
/// attack (`FUN_1410d5740`). Else its best melee, pierce or class-31 attack, without one its best
/// attack (from 0), plus its attack powerup (`+0xa24`, slot 0x6d8) and stinger (`+0xb48`, slot
/// 0x768, 0 as [`Victim::most_hp`] says), each as a whole number ([`DamageAmount::of_effect`]), plus the bonus
/// damage of its tasks against its attack's target ([`bonus_damage`]); a ready charge of type 1
/// or 3 adds the charge (`+0x9e0`). A combat object of type 50 or 60 has no powerups (its slots
/// answer 0). The sums wrap at 16 bits.
#[must_use]
pub fn shown_attack(v: &Victim, s: &Shown<'_>, roles: &Roles) -> DamageAmount {
    let a = &v.source;
    let charge = charge_shown(v, s.target, roles);
    if charge
        && a.charge_kind.ranged()
        && let Some(weapons) = s.projectile
    {
        return best_attack(weapons, true);
    }
    let mut attack = best_attack(&v.weapons, true);
    if attack == DamageAmount::ZERO {
        attack = best_attack(&v.weapons, false);
    }
    let powerup = if v.tribe() {
        a.attack_powerup.get()
    } else {
        0.0
    };
    attack = attack
        .plus(DamageAmount::of_effect(powerup))
        .plus(DamageAmount::of_effect(0.0));
    if let Some(t) = s.target {
        attack = attack.plus(DamageAmount::of_effect(bonus_damage(v, t, roles)));
    }
    if charge && a.charge_kind.adds_to_attack() {
        attack = attack.plus(DamageAmount::of_effect(a.charge.get()));
    }
    attack
}

/// Slot 0x868 of a combat object, the melee armor it shows: a combat unit's or building's
/// [`melee_armor`], a type 50's or 60's [`master_melee_armor`] (`FUN_141085450`, its slots 0x708
/// and 0x770 answer 0).
///
/// # Errors
///
/// An object without combat values (a type below 50).
pub fn shown_melee_armor(v: &Victim, roles: &Roles) -> Result<DamageAmount, GenieError> {
    let a = v.armored.as_ref().ok_or(GenieError::unreachable(
        "the melee armor of an object without combat values",
    ))?;
    Ok(if v.tribe() {
        melee_armor(a, v.class, v.state, roles)
    } else {
        master_melee_armor(a)
    })
}

/// Slot 0x880 of a combat object (`FUN_1410861a0`), the range it shows: its master's weapon
/// range (`+0x828`), to which a ready charge of type 6 or 7 ([`charge_shown`]) adds its master's
/// `charge_event`, plus its max range powerup (`+0xa40`, slot 0x700; 0 for a type 50 or 60).
#[must_use]
pub fn shown_range(v: &Victim, s: &Shown<'_>, roles: &Roles) -> Distance {
    let a = &v.source;
    let mut range = a.weapon_range;
    if a.charge_kind.ranged() && charge_shown(v, s.target, roles) {
        range += s.charge_event.range();
    }
    let powerup = if v.tribe() {
        s.range_powerup
    } else {
        Distance::ZERO
    };
    powerup + range
}
