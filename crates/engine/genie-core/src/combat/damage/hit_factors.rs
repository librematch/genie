//! A hit's factors: the elevation, the facing offset and the charges (`FUN_1412623d0`).

use crate::GenieError;
use crate::combat::Charge;
use crate::combat::damage::{
    Attacker, ElevationBonus, Facing, HIGHER, LOWER, RELIC_BOOST, SPECIFIC_RELIC_BOOST, Victim,
};
use crate::map::Position;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::objects::masters::master_type::BUILDING;

/// Slot 0x1f0 of a combat object (`FUN_14120d780`) at height `z` whose player has `own` as
/// attributes 211 and 212: a hit from higher up gains 25% and attributes 211 and the target's
/// 272, one from lower down 75% and attributes 212 and 273.
#[must_use]
pub fn elevation_factor(z: f32, own: ElevationBonus, to: &Victim) -> f32 {
    let tz = to.position.z();
    if tz < z {
        own.higher + HIGHER + to.elevation.higher
    } else if z < tz {
        own.lower + LOWER + to.elevation.lower
    } else {
        1.0
    }
}

/// Slot 0x1f0 of a building (`FUN_1411cc890`): the combat object's `factor`
/// ([`elevation_factor`]), times [`SPECIFIC_RELIC_BOOST`] while its `specific_relic_count`
/// (`+0xc24`, `relics[1]`) is above 0, else [`RELIC_BOOST`] while its `relic_count` (`+0xc20`,
/// `relics[0]`) is; a building victim (type 80, `kind`) divides it by the same of its own counts
/// (`victim_relics`). Each count compares signed.
#[must_use]
pub fn building_elevation_factor(
    factor: f32,
    relics: [i32; 2],
    kind: MasterKind,
    victim_relics: [i32; 2],
) -> f32 {
    let mut f = factor;
    if 0 < relics[1] {
        f *= SPECIFIC_RELIC_BOOST;
    } else if 0 < relics[0] {
        f *= RELIC_BOOST;
    }
    if kind == BUILDING {
        if 0 < victim_relics[1] {
            return f / SPECIFIC_RELIC_BOOST;
        }
        if 0 < victim_relics[0] {
            f /= RELIC_BOOST;
        }
    }
    f
}

/// `FUN_14114eaa0` toward another object: how far `other` lies from where an object at `at`
/// faces, in facets (at least 8): the angle to it (`atan2f`, 0 to 2π) less the facing angle,
/// wrapped into −π to π, times the facets over 2π; 0 on the same point or without facets.
#[must_use]
pub fn facing_offset(at: Position, facing: Facing, other: Position) -> f32 {
    if facing.facets <= 0 {
        return 0.0;
    }
    let n = if 7 < facing.facets { facing.facets } else { 8 };
    crate::objects::facing::offset_in_facets(
        other.x() - at.x(),
        other.y() - at.y(),
        facing.angle,
        i32::from(n),
    )
}

/// `FUN_1412623d0`: a charge (its master's `charge_target`, `+0x924`, one bit per group of
/// classes) works on an object of `class` and `kind`: on every object for −1 or charge type −4;
/// for 0, on every object but buildings (type 80) other than classes 51 and 54; else by the bit
/// of the class's group, none for a class in no group.
#[must_use]
pub fn charge_works(
    charge_target: i16,
    charge_type: i16,
    class: ClassId,
    kind: MasterKind,
) -> bool {
    let targets = charge_target as u16;
    if targets == 0xffff
        || charge_type == -4
        || (targets == 0
            && !(BUILDING <= kind && class != class::CLASS_51 && class != class::CLASS_54))
    {
        return true;
    }
    let bit = match class {
        class::CLASS_6
        | class::CLASS_28
        | class::CLASS_45
        | class::CLASS_46
        | class::CLASS_50
        | class::CLASS_56 => 0,
        class::CLASS_12 | class::CLASS_24 | class::CLASS_47 | class::CLASS_57 => 1,
        class::CLASS_0 | class::CLASS_44 => 2,
        class::CLASS_23 | class::CLASS_36 => 3,
        class::CLASS_17 | class::MONK | class::CLASS_43 => 4,
        class::VILLAGER | class::TRADE_CART | class::KING => 5,
        class::TRADE_BOAT
        | class::TRANSPORT
        | class::FISHING_SHIP
        | class::WARSHIP
        | class::CLASS_53 => 6,
        class::SIEGE | class::CLASS_35 | class::CLASS_51 | class::CLASS_54 | class::CLASS_55 => 7,
        class::BUILDING
        | class::WALL
        | class::GATE
        | class::FARM
        | class::TOWER
        | class::CLASS_60 => 8,
        _ => return false,
    };
    targets >> bit & 1 != 0
}

/// Slot 0x300 (`FUN_141203e30`): the charge is ready, at least `max_charge` for charge types 1
/// and 3, else at least 1.
#[must_use]
pub fn charged(a: &Attacker) -> bool {
    if matches!(a.charge_type, 1 | 3) {
        a.max_charge <= a.charge
    } else {
        Charge::new(1.0) <= a.charge
    }
}

/// Slot 0x2f0 of a combat object of type 70 or 80 (`FUN_14120e920`): a charge of type 1 or 3
/// that works on `v` ([`charge_works`]) and is ready.
#[must_use]
pub fn charge_hit(a: &Attacker, v: &Victim) -> bool {
    matches!(a.charge_type, 1 | 3)
        && charge_works(a.charge_target, a.charge_type, v.class, v.kind)
        && charged(a)
}

/// `FUN_1412161b0`: a master of special attack 3 whose charge is ready on its unit AI's (or
/// group's) target in reach; false for other masters. Special attack 3 is not modelled.
pub(super) const fn special_charge_ready(a: &Attacker) -> Result<bool, GenieError> {
    if a.special_attack == 3 {
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
    if a.special_attack == 3 && !special_charge_ready(a)? {
        return Ok(0.0);
    }
    Ok(a.charge.get())
}

/// Slot 0x2f8 of a combat unit or building (`FUN_14120f510`): a charge of type 6 or 7 (slot
/// 0x308, `FUN_141203b50`) that works on `t` ([`charge_works`]) and is ready ([`charged`]).
#[must_use]
pub fn range_charge_hit(a: &Attacker, t: &Victim) -> bool {
    matches!(a.charge_type, 6 | 7)
        && charge_works(a.charge_target, a.charge_type, t.class, t.kind)
        && charged(a)
}
