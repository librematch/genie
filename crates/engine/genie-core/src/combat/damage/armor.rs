//! The armor against melee and against each weapon class, and the damage per weapon
//! (`FUN_141082530`).

use crate::GenieError;
use crate::combat::damage::{Armored, Attacker, Victim, charge_damage, charge_works};
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::masters::master_type::MISSILE;
use crate::objects::masters::{ClassAmount, DamageClass};

/// `FUN_141085450`, a combat object's melee armor as its master holds it: its best armor of
/// class 4 above −32768, else its best of any, else its base armor.
pub(super) fn master_melee_armor(a: &Armored) -> i16 {
    let best = |of: Option<DamageClass>| {
        a.armors
            .iter()
            .filter(|e| of.is_none_or(|c| c == e.class))
            .fold(i16::MIN, |m, e| if m < e.amount { e.amount } else { m })
    };
    let mut armor = best(Some(DamageClass::MELEE));
    if armor == i16::MIN {
        armor = best(None);
        if armor == i16::MIN {
            armor = a.base_armor;
        }
    }
    armor
}

/// Slot 0x868 of a combat unit or building (`FUN_141213810`): [`master_melee_armor`] plus its
/// melee armor powerup (`+0xa48`, slot 0x708) and stinger (`+0xb4c`, slot 0x770, 0 as
/// [`Victim::most_hp`] says), each as 16 bits; a wall foundation (class 27 in state 0) has none.
pub(super) fn melee_armor(a: &Armored, own_class: ClassId, state: ObjectState) -> i16 {
    let armor = master_melee_armor(a).wrapping_add(a.armor_powerups.melee);
    if own_class == class::WALL && state == ObjectState::Foundation {
        return 0;
    }
    armor
}

/// The armor a combat object (type 70 and up) has against melee (class 4; slot 0x868,
/// [`melee_armor`]) or pierce (class 3; slot 0x870, `FUN_1410857f0`: its best armor of the
/// class, else its base armor and best of any, plus its powerup; the stinger `+0xb50` is 0, as
/// [`Victim::most_hp`] says), less the armor removed.
pub(super) fn class_armor(
    a: &Armored,
    class: OwnSlot,
    own_class: ClassId,
    state: ObjectState,
) -> i16 {
    let best = |c: DamageClass, from: i16| {
        a.armors
            .iter()
            .filter(|e| e.class == c)
            .fold(from, |m, e| if m < e.amount { e.amount } else { m })
    };
    let any = |from: i16| {
        a.armors
            .iter()
            .fold(from, |m, e| if m < e.amount { e.amount } else { m })
    };
    match class {
        OwnSlot::Melee => melee_armor(a, own_class, state).wrapping_sub(a.armor_removed.melee),
        OwnSlot::Pierce => {
            let mut armor = best(DamageClass::PIERCE, -1);
            if armor == -1 {
                armor = any(a.base_armor);
            }
            armor
                .wrapping_add(a.armor_powerups.pierce)
                .wrapping_sub(a.armor_removed.pierce)
        }
    }
}

/// The two armor classes with their own slots (0x868 and 0x870, [`class_armor`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    a: &Armored,
    class: DamageClass,
    attacker: &Attacker,
    kind: MasterKind,
    own_class: ClassId,
    state: ObjectState,
) -> i16 {
    let mut armor = a.base_armor;
    for e in &a.armors {
        if e.class != class {
            continue;
        }
        let own_slot = OwnSlot::of(e.class);
        if MISSILE <= attacker.kind
            && own_slot.is_some()
            && attacker.ability.contains(CombatAbility::IGNORES_ARMOR)
            && !a.ability.contains(CombatAbility::RESISTS_IGNORING)
        {
            armor = 0;
            continue;
        }
        armor = e.amount;
        if let Some(slot) = own_slot.filter(|_| COMBAT <= kind) {
            armor = class_armor(a, slot, own_class, state);
        }
    }
    armor
}

/// [`armor_damage`]'s charge test of the attacker's weapon against the victim (its class
/// `own_class`, type `kind`): the charge works on it ([`charge_works`]), its type is 1 or 3 and
/// it is full.
pub(super) fn charges(attacker: &Attacker, own_class: ClassId, kind: MasterKind) -> bool {
    charge_works(
        attacker.charge_target,
        attacker.charge_type,
        own_class,
        kind,
    ) && matches!(attacker.charge_type, 1 | 3)
        && attacker.max_charge <= attacker.charge
}

/// Slot 0xf0 of a combat object (`FUN_141082530`): per weapon (class, amount) the armor of its
/// class (the last armor entry of the class wins: its amount, or for a type of 70 and up the
/// melee or pierce armor, [`class_armor`]; an attacker of type 60 and up whose combat ability
/// has bit 0 ignores melee and pierce armor unless the target's has bit 1), else the base armor.
/// For an attacker of type 70 and up: the first melee, pierce or class-31 weapon with an attack
/// adds a ready charge of type 1 or 3 that works on the target ([`charge_works`],
/// `FUN_141202580`), and the first such weapon adds the attack powerup (the stinger `+0xb48`,
/// unsaved and never written here, is 0); a weapon of class 11 adds ten per `+0xad8` (unsaved,
/// written only by `FUN_141209d60`, which no update of this recording runs). Another class is
/// scaled by 1 less the `bonus_damage_resistance` when it does damage. Each weapon's attack less
/// armor, times the elevation factor and the terrain defense, adds when above 0 (or below 0
/// against an armor below the base that rises above the attack: then at most 0). The damage is
/// the sum, at least 1 (0 under the game's setting `+0x680` at 1). `bonus` is the attacker's
/// bonus damage against `v` ([`bonus_damage`]), which the first such weapon adds with the
/// powerup.
///
/// [`bonus_damage`]: crate::combat::damage::shown_values::bonus_damage
pub fn armor_damage(
    v: &Victim,
    weapons: &[ClassAmount],
    factor: f32,
    attacker: &Attacker,
    bonus: f32,
) -> Result<HitPoints, GenieError> {
    let a = v.armored.as_ref().ok_or(GenieError::unsupported(
        "the armor of an object without combat values",
    ))?;
    let (own_class, kind, state) = (v.class, v.kind, v.state);
    let terrain = if a.terrain_bonus < 0 {
        1.0f32
    } else {
        a.terrain.ok_or(GenieError::unsupported(
            "a terrain defense bonus without the map's terrain or the restriction",
        ))?
    };
    let mut sum = 0.0f32;
    let mut charged = false;
    let mut bonused = false;
    for &ClassAmount { class, amount } in weapons {
        let armor_f = f32::from(weapon_armor(a, class, attacker, kind, own_class, state));
        let mut attack = f32::from(amount);
        let pierce_or_melee = matches!(
            class,
            DamageClass::PIERCE | DamageClass::MELEE | DamageClass::CLASS_31
        );
        let mut resist = 1.0f32;
        if COMBAT <= attacker.kind {
            let charge =
                !charged && 0.0 < attack && pierce_or_melee && charges(attacker, own_class, kind);
            let c = if charge {
                charge_damage(attacker)?
            } else {
                0.0
            };
            if 0.0 < c {
                charged = true;
                attack += c + 0.0;
            } else {
                attack += 0.0;
            }
            if pierce_or_melee && !bonused {
                // the bonus, then the stinger plus the powerup
                attack += bonus + (0.0 + attacker.attack_powerup);
                bonused = true;
            }
        }
        if !pierce_or_melee && a.resistance != 0.0 && 0.0 < attack - armor_f {
            resist = 1.0 - a.resistance;
        }
        let mut armor_f = armor_f;
        let mut below = false;
        if attack < 0.0 && attack < armor_f && armor_f != f32::from(a.base_armor) {
            armor_f = if armor_f < 0.0 { armor_f } else { 0.0 };
            below = true;
        }
        let v = (attack - armor_f) * factor * terrain * resist;
        if 0.0 < v || below {
            sum += v;
        }
    }
    let least = if attacker.mode_680 == 1 { 0.0 } else { 1.0 };
    Ok(HitPoints::new(if weapons.is_empty() {
        sum
    } else if least > sum {
        least
    } else {
        sum
    }))
}
