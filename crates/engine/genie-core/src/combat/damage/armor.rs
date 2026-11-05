//! The armor against melee and against each weapon class, and the damage per weapon
//! (`FUN_141082530`).

use crate::combat::damage::{Armored, Attacker, Victim, charge_damage, charge_works};
use crate::combat::powerup::InfantryCount;
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::DamageShare;
use crate::objects::masters::MasterKind;
use crate::objects::masters::{ClassAmount, DamageAmount, DamageClass};
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::{GenieError, GenieResult};

/// `FUN_141085450`, a combat object's melee armor as its master holds it: its best armor of
/// class 4 above −32768, else its best of any, else its base armor.
pub(super) fn master_melee_armor(armored: &Armored) -> DamageAmount {
    let best = |of: Option<DamageClass>| {
        armored
            .armors
            .iter()
            .filter(|class_amount| of.is_none_or(|damage_class| damage_class == class_amount.class))
            .fold(DamageAmount::LEAST, |damage_amount, class_amount| {
                if damage_amount < class_amount.amount {
                    class_amount.amount
                } else {
                    damage_amount
                }
            })
    };
    let mut armor = best(Some(DamageClass::MELEE));
    if armor == DamageAmount::LEAST {
        armor = best(None);
        if armor == DamageAmount::LEAST {
            armor = armored.base_armor;
        }
    }
    armor
}

/// Slot 0x868 of a combat unit or building (`FUN_141213810`): [`master_melee_armor`] plus its
/// melee armor powerup (`+0xa48`, slot 0x708) and stinger (`+0xb4c`, slot 0x770), each as 16
/// bits (`FUN_141085450`'s adds); a wall foundation (class 27 in state 0) has none.
pub(super) fn melee_armor(
    armored: &Armored,
    own_class: ClassId,
    state: ObjectState,
    roles: &Roles,
) -> DamageAmount {
    let armor = master_melee_armor(armored)
        .plus(armored.armor_powerups.melee_armor())
        .plus(armored.armor_stingers.melee_armor());
    if roles.of_class(own_class).contains(ClassRoles::WALL) && state == ObjectState::Foundation {
        return DamageAmount::ZERO;
    }
    armor
}

/// The armor a combat object (type 70 and up) has against melee (class 4; slot 0x868,
/// [`melee_armor`]) or pierce (class 3; slot 0x870, `FUN_1410857f0`: its best armor of the
/// class, else its base armor and best of any, plus its powerup `+0xa4c` and its stinger
/// `+0xb50`, slots 0x710 and 0x778, each as 16 bits), less the armor removed.
pub(super) fn class_armor(
    armored: &Armored,
    class: OwnSlot,
    own_class: ClassId,
    state: ObjectState,
    roles: &Roles,
) -> DamageAmount {
    match class {
        OwnSlot::Melee => {
            melee_armor(armored, own_class, state, roles).minus(armored.armor_removed.melee)
        }
        OwnSlot::Pierce => master_pierce_armor(&armored.armors, armored.base_armor)
            .plus(armored.armor_powerups.pierce_armor())
            .plus(armored.armor_stingers.pierce_armor())
            .minus(armored.armor_removed.pierce),
    }
}

/// `FUN_1410857f0`'s master part, slot 0x870's armor as a master holds it (`armors`,
/// `base_armor`): its best armor of class 3 from −1 on, else (still −1) its base armor or its
/// best armor of any class. The slot adds the object's pierce armor powerup and stinger (slots
/// 0x710 and 0x778, 0 of type 50 and 60).
#[must_use]
pub(crate) fn master_pierce_armor(
    armors: &[ClassAmount],
    base_armor: DamageAmount,
) -> DamageAmount {
    let best = |from: DamageAmount, of: Option<DamageClass>| {
        armors
            .iter()
            .filter(|class_amount| of.is_none_or(|damage_class| damage_class == class_amount.class))
            .fold(from, |damage_amount, class_amount| {
                if damage_amount < class_amount.amount {
                    class_amount.amount
                } else {
                    damage_amount
                }
            })
    };
    let armor = best(DamageAmount::NO_CLASS, Some(DamageClass::PIERCE));
    if armor == DamageAmount::NO_CLASS {
        best(base_armor, None)
    } else {
        armor
    }
}

/// The two armor classes with their own slots (0x868 and 0x870, [`class_armor`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum OwnSlot {
    /// Melee (class 4), slot 0x868.
    Melee,
    /// Pierce (class 3), slot 0x870.
    Pierce,
}

impl OwnSlot {
    /// The slot of armor class `class`, none for a class without its own.
    const fn of(class: DamageClass) -> Option<Self> {
        match class {
            DamageClass::MELEE => Some(Self::Melee),
            DamageClass::PIERCE => Some(Self::Pierce),
            _ => None,
        }
    }
}

/// [`armor_damage`]'s armor against weapon class `class`: the base armor, else the last entry of
/// that class; an attacker of type 60 and up with [`CombatAbility::IGNORES_ARMOR`] against a melee
/// or pierce entry (3, 4) of a victim without [`CombatAbility::RESISTS_IGNORING`] meets 0; a combat
/// victim's (type 70 and up) melee or pierce armor reads its class's ([`class_armor`]).
pub(super) fn weapon_armor(
    armored: &Armored,
    class: DamageClass,
    attacker: &Attacker,
    kind: MasterKind,
    own_class: ClassId,
    state: ObjectState,
    roles: &Roles,
) -> DamageAmount {
    let mut armor = armored.base_armor;
    for class_amount in &armored.armors {
        if class_amount.class != class {
            continue;
        }
        let own_slot = OwnSlot::of(class_amount.class);
        if MasterKind::MISSILE <= attacker.kind
            && own_slot.is_some()
            && attacker.ability.contains(CombatAbility::IGNORES_ARMOR)
            && !armored.ability.contains(CombatAbility::RESISTS_IGNORING)
        {
            armor = DamageAmount::ZERO;
            continue;
        }
        armor = class_amount.amount;
        if let Some(slot) = own_slot.filter(|_| MasterKind::COMBAT <= kind) {
            armor = class_armor(armored, slot, own_class, state, roles);
        }
    }
    armor
}

/// [`armor_damage`]'s charge test of the attacker's weapon against the victim (its class
/// `own_class`, type `kind`): the charge works on it ([`charge_works`]), its type is 1 or 3 and
/// it is full.
pub(super) fn charges(
    attacker: &Attacker,
    own_class: ClassId,
    kind: MasterKind,
    roles: &Roles,
) -> bool {
    charge_works(
        attacker.charge_target,
        attacker.charge_kind,
        own_class,
        kind,
        roles,
    ) && attacker.charge_kind.adds_to_attack()
        && attacker.max_charge <= attacker.charge
}

/// Slot 0xf0 of a combat object (`FUN_141082530`): per weapon (class, amount) the armor of its
/// class (the last armor entry of the class wins: its amount, or for a type of 70 and up the
/// melee or pierce armor, [`class_armor`]; an attacker of type 60 and up whose combat ability
/// has bit 0 ignores melee and pierce armor unless the target's has bit 1), else the base armor.
/// A weapon of class 11 adds ten per unit of infantry inside the attacker (slot 0x1b8, `+0xad8`
/// of a TRIBE combat object, 0 of type 50 and 60; the product converts as unsigned 64 bits).
/// For an attacker of type 70 and up: the first melee, pierce or class-31 weapon with an attack
/// adds a ready charge of type 1 or 3 that works on the target ([`charge_works`],
/// `FUN_141202580`), and the first such weapon adds the bonus and then the attack stinger and
/// powerup (`+0xb48` plus `+0xa24`, slots 0x768 and 0x6d8). Another class is
/// scaled by 1 less the `bonus_damage_resistance` when it does damage. Each weapon's attack less
/// armor, times the elevation factor and the terrain defense, adds when above 0 (or below 0
/// against an armor below the base that rises above the attack: then at most 0). The damage is
/// the sum, at least 1 (0 under the game's setting `+0x680` at 1). `bonus` is the attacker's
/// bonus damage against `victim` ([`bonus_damage`]), which the first such weapon adds with the
/// powerup.
///
/// [`bonus_damage`]: crate::combat::damage::shown_values::bonus_damage
pub fn armor_damage(
    victim: &Victim,
    weapons: &[ClassAmount],
    factor: f32,
    attacker: &Attacker,
    bonus: f32,
    roles: &Roles,
) -> GenieResult<HitPoints> {
    let armored = victim.armored.as_ref().ok_or(GenieError::unsupported(
        "the armor of an object without combat values",
    ))?;
    let (own_class, kind, state) = (victim.class, victim.kind, victim.state);
    let terrain = if armored.terrain_bonus.restriction().is_none() {
        1.0f32
    } else {
        armored.terrain.ok_or(GenieError::unsupported(
            "a terrain defense bonus without the map's terrain or the restriction",
        ))?
    };
    let mut sum = 0.0f32;
    let mut charged = false;
    let mut bonused = false;
    for &ClassAmount { class, amount } in weapons {
        let armor_f =
            weapon_armor(armored, class, attacker, kind, own_class, state, roles).as_f32();
        let mut attack = amount.as_f32();
        if class == DamageClass::CLASS_11 {
            attack += infantry_attack(attacker.infantry_inside);
        }
        let pierce_or_melee = matches!(
            class,
            DamageClass::PIERCE | DamageClass::MELEE | DamageClass::CLASS_31
        );
        let mut resist = 1.0f32;
        if MasterKind::COMBAT <= attacker.kind {
            let charge = !charged
                && 0.0 < attack
                && pierce_or_melee
                && charges(attacker, own_class, kind, roles);
            let charge_attack = if charge { charge_damage(attacker) } else { 0.0 };
            if 0.0 < charge_attack {
                charged = true;
                attack += charge_attack + 0.0;
            } else {
                attack += 0.0;
            }
            if pierce_or_melee && !bonused {
                // the bonus, then the stinger plus the powerup
                attack += bonus + (attacker.attack_stinger.get() + attacker.attack_powerup.get());
                bonused = true;
            }
        }
        if !pierce_or_melee && armored.resistance != DamageShare::ZERO && 0.0 < attack - armor_f {
            resist = 1.0 - armored.resistance.get();
        }
        let mut armor_f = armor_f;
        let mut below = false;
        if attack < 0.0 && attack < armor_f && armor_f != armored.base_armor.as_f32() {
            armor_f = if armor_f < 0.0 { armor_f } else { 0.0 };
            below = true;
        }
        let class_damage = (attack - armor_f) * factor * terrain * resist;
        if 0.0 < class_damage || below {
            sum += class_damage;
        }
    }
    let least = if attacker.mode_680.is_one() { 0.0 } else { 1.0 };
    Ok(HitPoints::new(if weapons.is_empty() {
        sum
    } else if least > sum {
        least
    } else {
        sum
    }))
}

/// What a weapon of class 11 adds per unit of infantry inside its attacker (`LEA RCX, [RAX +
/// RAX*4]`, `ADD RCX, RCX` at `0x1410827ed`).
const INFANTRY_ATTACK: i64 = 10;

/// What a weapon of class 11 adds for the infantry inside its attacker (`FUN_141082530`, slot
/// 0x1b8): ten per unit, the count widened to 64 bits with its sign and the product converted as
/// unsigned (`CVTSI2SS` of a halved value, doubled, for one at or above 2^63: −1 gives about
/// 1.8e19).
#[must_use]
pub(super) fn infantry_attack(inside: InfantryCount) -> f32 {
    let tens = i64::from(inside.get()).wrapping_mul(INFANTRY_ATTACK);
    tens.cast_unsigned() as f32
}
