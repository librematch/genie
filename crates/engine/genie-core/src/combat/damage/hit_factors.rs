//! A hit's factors: the elevation, the facing offset and the charges (`FUN_1412623d0`).

use crate::GenieError;
use crate::combat::Charge;
use crate::combat::damage::{Attacker, ElevationBonus, Facing, Victim};
use crate::map::Position;
use crate::objects::RelicCount;
use crate::objects::Relics;
use crate::objects::facing::offset_in_facets;
use crate::objects::masters::ChargeKind;
use crate::objects::masters::ChargeTarget;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::SpecialAttack;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::DamageTuning;

/// Slot 0x1f0 of a combat object (`FUN_14120d780`) at height `z` whose player has `own` as
/// attributes 211 and 212: a hit from higher up gains 25% and attributes 211 and the target's
/// 272, one from lower down 75% and attributes 212 and 273.
#[must_use]
pub fn elevation_factor(
    z: f32,
    own: ElevationBonus,
    to: &Victim,
    damage_tuning: &DamageTuning,
) -> f32 {
    let tz = to.position.z();
    if tz < z {
        own.higher + damage_tuning.higher + to.elevation.higher
    } else if z < tz {
        own.lower + damage_tuning.lower + to.elevation.lower
    } else {
        1.0
    }
}

/// Slot 0x1f0 of a building (`FUN_1411cc890`): the combat object's `factor`
/// ([`elevation_factor`]), times `DamageTuning::specific_relic_boost` while its `specific_relic_count`
/// (`+0xc24`, [`Relics::specific`]) is above 0, else `DamageTuning::relic_boost` while its `relic_count`
/// (`+0xc20`, [`Relics::count`]) is; a building victim (type 80, `kind`) divides it by the same of its own counts
/// (`victim_relics`). Each count compares signed.
#[must_use]
pub fn building_elevation_factor(
    factor: f32,
    relics: Relics,
    kind: MasterKind,
    victim_relics: Relics,
    damage_tuning: &DamageTuning,
) -> f32 {
    let mut f = factor;
    if RelicCount::ZERO < relics.specific {
        f *= damage_tuning.specific_relic_boost;
    } else if RelicCount::ZERO < relics.count {
        f *= damage_tuning.relic_boost;
    }
    if kind == MasterKind::BUILDING {
        if RelicCount::ZERO < victim_relics.specific {
            return f / damage_tuning.specific_relic_boost;
        }
        if RelicCount::ZERO < victim_relics.count {
            f /= damage_tuning.relic_boost;
        }
    }
    f
}

/// `FUN_14114eaa0` toward another object: how far `other` lies from where an object at `at`
/// faces, in facets (at least 8): the angle to it (`atan2f`, 0 to 2π) less the facing angle,
/// wrapped into −π to π, times the facets over 2π; 0 on the same point or without facets.
#[must_use]
pub fn facing_offset(at: Position, facing: Facing, other: Position) -> f32 {
    if !facing.facets.any() {
        return 0.0;
    }
    offset_in_facets(
        other.x() - at.x(),
        other.y() - at.y(),
        facing.angle,
        facing.facets.at_least_eight(),
    )
}

/// `FUN_1412623d0`: a charge (its master's `charge_target`, `+0x924`, one bit per group of
/// classes) works on an object of `class` and `kind`: on every object for −1 or charge type −4;
/// for 0, on every object but buildings (type 80) other than classes 51 and 54; else by the bit
/// of the class's group, none for a class in no group.
#[must_use]
pub fn charge_works(
    charge_target: ChargeTarget,
    charge_type: ChargeKind,
    class: ClassId,
    kind: MasterKind,
    roles: &Roles,
) -> bool {
    if charge_target == ChargeTarget::EVERY
        || charge_type == ChargeKind::EVERY_TARGET
        || (charge_target == ChargeTarget::ALL_BUT_BUILDINGS
            && !(MasterKind::BUILDING <= kind
                && !roles.of_class(class).contains(ClassRoles::PACKED_TREBUCHET)
                && !roles
                    .of_class(class)
                    .contains(ClassRoles::UNPACKED_TREBUCHET)))
    {
        return true;
    }
    let bit = {
        let held = roles.of_class(class);
        if held.intersects(
            ClassRoles::INFANTRY
                | ClassRoles::PHALANX
                | ClassRoles::TWO_HANDED_SWORDSMAN
                | ClassRoles::PIKEMAN
                | ClassRoles::SPEARMAN
                | ClassRoles::RAIDER,
        ) {
            0
        } else if held.intersects(
            ClassRoles::CAVALRY
                | ClassRoles::WAR_ELEPHANT
                | ClassRoles::SCOUT_CAVALRY
                | ClassRoles::CAVALRY_RAIDER,
        ) {
            1
        } else if held.intersects(ClassRoles::ARCHERY | ClassRoles::ARCHERY_CANNON) {
            2
        } else if held.intersects(ClassRoles::CAVALRY_CANNON | ClassRoles::CAVALRY_ARCHER) {
            3
        } else if held
            .intersects(ClassRoles::HEALER | ClassRoles::MONK | ClassRoles::MONK_WITH_RELIC)
        {
            4
        } else if held.intersects(ClassRoles::VILLAGER | ClassRoles::TRADE_CART | ClassRoles::KING)
        {
            5
        } else if held.intersects(
            ClassRoles::TRADE_BOAT
                | ClassRoles::TRANSPORT
                | ClassRoles::FISHING_SHIP
                | ClassRoles::WARSHIP
                | ClassRoles::BOARDING_SHIP,
        ) {
            6
        } else if held.intersects(
            ClassRoles::SIEGE
                | ClassRoles::PETARD
                | ClassRoles::PACKED_TREBUCHET
                | ClassRoles::UNPACKED_TREBUCHET
                | ClassRoles::SCORPION,
        ) {
            7
        } else if held.intersects(
            ClassRoles::BUILDING
                | ClassRoles::WALL
                | ClassRoles::GATE
                | ClassRoles::FARM
                | ClassRoles::TOWER
                | ClassRoles::MISC_BUILDING,
        ) {
            8
        } else {
            return false;
        }
    };
    charge_target.holds(bit)
}

/// Slot 0x300 (`FUN_141203e30`): the charge is ready, at least `max_charge` for charge types 1
/// and 3, else at least 1.
#[must_use]
pub fn charged(a: &Attacker) -> bool {
    if a.charge_kind.adds_to_attack() {
        a.max_charge <= a.charge
    } else {
        Charge::new(1.0) <= a.charge
    }
}

/// Slot 0x2f0 of a combat object of type 70 or 80 (`FUN_14120e920`): a charge of type 1 or 3
/// that works on `v` ([`charge_works`]) and is ready.
#[must_use]
pub fn charge_hit(a: &Attacker, v: &Victim, roles: &Roles) -> bool {
    a.charge_kind.adds_to_attack()
        && charge_works(a.charge_target, a.charge_kind, v.class, v.kind, roles)
        && charged(a)
}

/// `FUN_1412161b0`: a master of special attack 3 whose charge is ready on its unit AI's (or
/// group's) target in reach; false for other masters. Special attack 3 is not modelled.
pub(super) const fn special_charge_ready(a: &Attacker) -> Result<bool, GenieError> {
    if a.special_attack.get() == SpecialAttack::CHARGE.get() {
        return Err(SPECIAL_CHARGE_GAP);
    }
    Ok(false)
}

/// The gap of [`special_charge_ready`], which a unit's speed (slot 0x208, `FUN_141202c70`) reads
/// too.
pub(crate) const SPECIAL_CHARGE_GAP: GenieError =
    GenieError::unsupported("a charge of special attack 3 (FUN_1412161b0)");

/// `FUN_141202580`: the damage a ready charge adds, the charge itself; a master of special
/// attack 3 adds it only while [`special_charge_ready`] holds.
pub(super) fn charge_damage(a: &Attacker) -> Result<f32, GenieError> {
    if a.special_attack == SpecialAttack::CHARGE && !special_charge_ready(a)? {
        return Ok(0.0);
    }
    Ok(a.charge.get())
}

/// Slot 0x2f8 of a combat unit or building (`FUN_14120f510`): a charge of type 6 or 7 (slot
/// 0x308, `FUN_141203b50`) that works on `t` ([`charge_works`]) and is ready ([`charged`]).
#[must_use]
pub fn range_charge_hit(a: &Attacker, t: &Victim, roles: &Roles) -> bool {
    a.charge_kind.ranged()
        && charge_works(a.charge_target, a.charge_kind, t.class, t.kind, roles)
        && charged(a)
}
