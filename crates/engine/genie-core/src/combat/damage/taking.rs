//! A target's slot 0xe8: a building's, a static object's and a combat unit's, with the
//! unit's facing, its retaliation and the hit's end.

use crate::GenieError;
use crate::combat::damage::{
    Armored, Attacker, Building, EPSILON, FACING, LEAST_CLASS_HIT, MINUS_FACING, ONE,
    RETALIATION_RANGE, RETALIATION_SCALE, Razed, SPECIAL, Victim, charge_hit, damage, effective,
    elevation_factor, facing_offset, hp_floor, lose, special_charge_ready,
};
use crate::world::slots::CheckedIndex;

/// One kill or razing, as the counts add it to an attribute.
const ONE_COUNT: Amount = Amount::new(ONE);
use crate::combat::damage::HitLayer;
use crate::construction::MainBuildingId;
use crate::construction::build::NOT_HELD;
use crate::map::Distance;
use crate::map::Position;
use crate::objects::DamagedTimer;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::MasterKind;
use crate::objects::masters::SpecialAttack;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::players::history::EventKind;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use std::mem::replace;

/// One hit as a building takes it ([`take_building`]): the attacker's weapons, the factor they
/// take and the blast damage the target took already.
#[derive(Clone, Copy, Debug)]
pub(super) struct Strike<'a> {
    /// The attacker's attacks by armour class.
    weapons: &'a [ClassAmount],
    /// The factor its attacks take.
    factor: f32,
    /// The blast damage the target took already.
    blast_taken: HitPoints,
}

/// The target's slot 0xe8 with the source's `weapons` at `factor`, less the damage a blast has
/// already done to it (`blast_taken`, a building's `+0xb90`): a static, animated, doppelganger,
/// moving or action object, or a tree (types below 50 and 90: `FUN_14118efd0`,
/// [`take_static`]), a combat object of type 50 or 60 ([`damage`]), of type 70
/// ([`take_unit`]) or a building ([`take_building`]). `others` are the objects besides the two
/// the hit may reach: an annex's main building, which takes a hit on the annex, and the annexes
/// that take a main building's hit points (every caller passes the flag that forwards an
/// annex's hit, 1).
///
/// # Errors
///
/// A part of the hit the slice does not model.
pub fn take(
    t: &mut Victim,
    src: &mut Victim,
    others: &mut [&mut Victim],
    weapons: &[ClassAmount],
    factor: f32,
    blast_taken: HitPoints,
    layer: HitLayer<'_>,
) -> Result<(), GenieError> {
    let HitLayer { roles, slots } = layer;
    match t.kind {
        MasterKind::COMBAT => take_unit(t, src, others, weapons, factor, blast_taken, layer),
        MasterKind::RGE_COMBAT | MasterKind::MISSILE => {
            damage(t, src, weapons, factor, blast_taken, roles)
        }
        MasterKind::BUILDING => {
            let strike = Strike {
                weapons,
                factor,
                blast_taken,
            };
            take_building(t, src, others, strike, true, roles, slots).map(|_| ())
        }
        _ => {
            take_static(t, &src.source, weapons, factor, roles);
            Ok(())
        }
    }
}

/// A razing's counts for the attacker's player `ap` and the owner `q` (16-bit adds of the
/// players' ids, `ADD DX, word ptr [.. + 0x718]`): the razings (43), the kill values (170, 152),
/// the razings of the other (155) and the per-player tables (350, 375, 425).
///
/// # Errors
///
/// A per-player table past the attributes.
fn razing_counts(
    (ap, q): (PlayerId, PlayerId),
    cost: Amount,
    slots: Slots,
) -> Result<[(PlayerId, AttributeId, Amount); 7], GenieError> {
    Ok([
        (ap, slots.attribute.razings, ONE_COUNT),
        (ap, slots.attribute.kills_value, cost),
        (q, slots.attribute.kills_value_other, cost),
        (q, slots.attribute.razings_other, ONE_COUNT),
        (ap, slots.attribute.razings_of.offset(q.index())?, ONE_COUNT),
        (q, slots.attribute.razed_by.offset(ap.index())?, ONE_COUNT),
        (
            ap,
            slots.attribute.razings_value_of.offset(q.index())?,
            cost,
        ),
    ])
}

/// A trebuchet's kill counts (its damage did not count the kill) for the attacker's player `ap`
/// and the owner `q`: the kills (20), the kill values (170, 152), the kills of the other (154)
/// and the per-player tables (300, 325, 400).
///
/// # Errors
///
/// A per-player table past the attributes.
fn kill_counts(
    (ap, q): (PlayerId, PlayerId),
    cost: Amount,
    slots: Slots,
) -> Result<[(PlayerId, AttributeId, Amount); 7], GenieError> {
    Ok([
        (ap, slots.attribute.kills, ONE_COUNT),
        (ap, slots.attribute.kills_value, cost),
        (q, slots.attribute.kills_other, ONE_COUNT),
        (q, slots.attribute.kills_value_other, cost),
        (ap, slots.attribute.kills_of.offset(q.index())?, ONE_COUNT),
        (q, slots.attribute.killed_by.offset(ap.index())?, ONE_COUNT),
        (ap, slots.attribute.kills_value_of.offset(q.index())?, cost),
    ])
}

/// A building's slot 0xe8 (`FUN_1411cea90`), `forward` the flag every caller sets (clear for
/// the main building's hit an annex forwards). Its wake from semi-asleep (`+0xc98`: the flag
/// cleared, `FUN_14119deb0` with 0, the owner's `+0x80` list, `FUN_14107db90`) never runs here:
/// only the building's load sets that flag (`FUN_1411df160` -> `FUN_1411ddd20` with the save's
/// `temp_semi_asleep`), and no saved building of the 158041 recordings has it (1473 headers
/// read); the slice does not hold it. With `forward`, an annex (`+0xbb8`) gives the hit to its
/// main building among `others` (whose slot 0xe8 runs with the flag clear and the same
/// `blast_taken`) and takes its hit points when that hit got past its hit point test. Else at 0
/// hit points or fewer (`COMISS 0, hp` / `JNC`: a NaN goes on) nothing happens. The damage
/// ([`damage`]); without a blast's damage so far (`UCOMISS blast_taken, 0`: exactly 0), a hit
/// that took less than its least ([`ONE`]; [`LEAST_CLASS_HIT`] from the
/// [`LEAST_HIT_CLASSES`] under the game's setting `+0x680` at 1) takes the least again (slot
/// 0xb0 with it; `COMISS least, lost` / `JBE` past). Not held: the statistics (`FUN_1417783d0`)
/// and the local player's telemetry (`FUN_140a4b540`). A player's building (`+0x718` above 0)
/// left below 1 hit point (`COMISS 1, hp` / `JBE` past) is razed ([`Razed`]): a castle's or a
/// wonder's history event; but for an annex or a wall (class 27), the razing counts, or a
/// trebuchet's kill counts unless its kill is counted (`+0x954`, which its damage's kill sets:
/// slot 0x8b0's `FUN_14107da00`; a building at 0 hit points takes no hit, so no earlier one
/// did), then the kill bonus ([`kill_bonus`]) and the razing values (172, 153). Then
/// `damaged_lately_timer` (`+0x30d`) is 60 and its annexes take its hit points (the write-back
/// gives them to those `others` does not hold, [`Building::struck`]); with `forward`, an
/// attacker of type 50 and up leaves a building of 1 hit point or more (`COMISS hp, 1` / `JC`
/// past) to its effects ([`attack_effects`]). Answers whether the hit got past the hit point
/// test.
pub(super) fn take_building(
    t: &mut Victim,
    src: &mut Victim,
    others: &mut [&mut Victim],
    strike: Strike<'_>,
    forward: bool,
    roles: &Roles,
    slots: Slots,
) -> Result<bool, GenieError> {
    let Strike {
        weapons,
        factor,
        blast_taken,
    } = strike;
    let before = t.hp;
    let main = t.building.as_ref().and_then(|b| b.main);
    if forward && let Some(main) = main {
        return forward_to_main(t, src, others, strike, main, roles, slots);
    }
    if HitPoints::ZERO >= t.hp {
        return Ok(false);
    }
    damage(t, src, weapons, factor, blast_taken, roles)?;
    // the kill of its damage ran slot 0x8b0, which counts it (+0x954)
    let counted = t.killed.is_some();
    if blast_taken == HitPoints::ZERO {
        let least = if src.source.mode_680.is_one()
            && roles
                .of_class(src.source.class)
                .contains(ClassRoles::LEAST_HIT)
        {
            LEAST_CLASS_HIT
        } else {
            ONE
        };
        if least > (before - t.hp).get() {
            lose(t, HitPoints::new(least));
        }
    }
    let b = t.building.get_or_insert_with(Building::default);
    if t.player != PlayerId::GAIA && ONE > t.hp.get() {
        let (ap, q) = (src.player, t.player);
        let event = {
            let held = roles.of_master(t.master_id);
            if held.contains(MasterRoles::CASTLE) {
                Some(EventKind::CASTLE_DESTROYED)
            } else if held.contains(MasterRoles::WONDER) {
                Some(EventKind::WONDER_DESTROYED)
            } else {
                None
            }
        };
        let mut adds = Vec::new();
        if b.main.is_none() && !roles.of_class(t.class).contains(ClassRoles::WALL) {
            let cost = b.cost;
            if !b.trebuchet {
                adds.extend(razing_counts((ap, q), cost, slots)?);
            } else if !counted {
                adds.extend(kill_counts((ap, q), cost, slots)?);
            }
            kill_bonus(&src.source)?;
            adds.extend([
                (ap, slots.attribute.razings_value, cost),
                (q, slots.attribute.razings_value_other, cost),
            ]);
        }
        b.razed.push(Razed {
            attacker_player: ap,
            owner: q,
            event,
            adds,
        });
    }
    b.damaged_lately = DamagedTimer::HIT;
    b.struck = true;
    let annexes = b.annexes.clone();
    for a in others.iter_mut().filter(|a| annexes.contains(&a.id)) {
        a.hp = t.hp;
    }
    if forward && MasterKind::RGE_COMBAT <= src.kind && t.hp >= HitPoints::new(ONE) {
        attack_effects(&src.source, t)?;
    }
    Ok(true)
}

/// [`take_building`]'s forward of an annex `t`'s hit to its main building `main` among
/// `others` (out of the slice while it takes the hit, not forwarded again): the annex takes the
/// main building's hit points when the hit got past its test and the main building lists it.
/// Answers false.
fn forward_to_main(
    t: &mut Victim,
    src: &mut Victim,
    others: &mut [&mut Victim],
    strike: Strike<'_>,
    main: MainBuildingId,
    roles: &Roles,
    slots: Slots,
) -> Result<bool, GenieError> {
    let k = others
        .iter()
        .position(|v| v.id == main.id())
        .ok_or(NOT_HELD)?;
    let mut m = replace(&mut **others.checked_mut(k)?, Victim::placeholder());
    let struck = take_building(&mut m, src, others, strike, false, roles, slots);
    let annexed = m
        .building
        .as_ref()
        .is_some_and(|b| b.annexes.contains(&t.id));
    if matches!(struck, Ok(true)) && annexed {
        t.hp = m.hp;
    }
    **others.checked_mut(k)? = m;
    struck.map(|_| false)
}

/// `FUN_1415c6510`, the kill bonus of the attacker `src` (type 40 and up) for an object it
/// killed: its master's tasks of action type 0x9a pay their owner's attributes, research
/// technologies and run effects (not modelled); an attacker without one does nothing.
pub(super) fn kill_bonus(src: &Attacker) -> Result<(), GenieError> {
    if MasterKind::ACTION <= src.kind && src.kill_bonus_tasks {
        return Err(GenieError::unsupported(
            "an attacker's kill bonus task (action type 0x9a)",
        ));
    }
    Ok(())
}

/// `FUN_14107bdf0`, the attacker `src`'s combat ability's effects on its target `t`: only a combat
/// ability with [`CombatAbility::HIT_EFFECTS`], against a target whose player is not allied with
/// the attacker's both ways (`FUN_141663d90` with the ally flag, [`Victim::allied`]), does anything
/// (its master's effects on the target, not modelled).
pub(super) const fn attack_effects(src: &Attacker, t: &Victim) -> Result<(), GenieError> {
    if src.ability.contains(CombatAbility::HIT_EFFECTS) && !t.allied.both() {
        return Err(GenieError::unsupported(
            "an attacker whose hits apply effects (FUN_14107bdf0)",
        ));
    }
    Ok(())
}

impl Victim {
    /// What stands in a slice of objects for one a hit has taken out for the time it changes it:
    /// no object's id ([`ObjectId::TAKEN_OUT`]).
    pub(super) fn placeholder() -> Self {
        Self {
            id: ObjectId::TAKEN_OUT,
            ..Self::default()
        }
    }
}

/// A static object's slot 0xe8 (`FUN_14118efd0`), also the animated, doppelganger, moving and
/// action objects' and a tree's: it has no armor (slot 0xf0 `FUN_14118cab0`: a TRIBE attacker's
/// ready charge on it, then the weapons times the factor) and loses the damage (slot 0xb0,
/// `FUN_141189160`); below 1 hit point (`COMISS`/`JBE`: a NaN stays) it has 0 and wakes
/// (`FUN_14119deb0` with 0: `goto_sleep` clears; an object asleep in its player's sleeping list
/// moves to the live list, the caller's), and but for a villager's hit it loses what it holds.
/// The routine's test for no attacker cannot be reached: slot 0xf0 has called the attacker's
/// slot 0x2f0 before it. Its last call (slot 0x4c0, `FUN_141196a00` or `FUN_141158bb0`) plays
/// the master's damage sound (`+0x700`), whose draws are the Unsynced stream's: no simulation
/// state.
pub(super) fn take_static(
    t: &mut Victim,
    src: &Attacker,
    weapons: &[ClassAmount],
    factor: f32,
    roles: &Roles,
) {
    // slot 0x2f0 of a TRIBE attacker (types 70, 80; [`charge_hit`]) starts the sum at its raw
    // charge (slot 0x310, `+0x9e0`) plus 0
    let mut damage = if MasterKind::COMBAT <= src.kind && charge_hit(src, t, roles) {
        src.charge.get() + 0.0
    } else {
        0.0f32
    };
    for w in weapons {
        damage += w.amount.as_f32() * factor;
    }
    t.hp -= HitPoints::new(damage);
    if t.hp < HitPoints::new(1.0) {
        t.hp = HitPoints::ZERO;
        t.goto_sleep = false;
        // The villager class (4): its hits leave what a static target holds.
        if !roles.of_class(src.class).contains(ClassRoles::VILLAGER) {
            t.amount = Amount::ZERO;
        }
    }
}

/// A combat object's slot 0xe8 (`FUN_1412108e0`): none on an object below 1 hit point. The special
/// attack (`special_attack_flag` 1) of an idle target facing its attacker takes the damage to 0.66
/// (its sprite, master `+0x8e8`, is not modelled); the retaliation (flag 2) of an idle target with
/// a unit AI facing an attacker of weapon range below 0.2 strikes the attacker first with its own
/// weapons (four times over a unit whose charge is ready, [`special_charge_ready`]), and an
/// attacker left with at most 1 hit point takes the hit to nothing. Then the damage ([`damage`]); a
/// hit takes at least one hit point (but past the hit point floor); a target with
/// `damage_reflection` gives its attacker that share of the loss, at least 1 (slot 0xb0, no kill
/// handling). Not held: the statistics (`FUN_1417783d0`); not modelled: a kill bonus task of the
/// attacker (action type 0x9a, `FUN_1415c6510`) and its combat ability's effects
/// ([`CombatAbility::HIT_EFFECTS`], `FUN_14107bdf0`).
pub(super) fn take_unit(
    t: &mut Victim,
    src: &mut Victim,
    others: &mut [&mut Victim],
    weapons: &[ClassAmount],
    factor: f32,
    blast_taken: HitPoints,
    layer: HitLayer<'_>,
) -> Result<(), GenieError> {
    let HitLayer { roles, slots } = layer;
    let before = effective(t);
    if t.hp < HitPoints::new(1.0) {
        return Ok(());
    }
    let a = t.armored.clone().ok_or(GenieError::unsupported(
        "a combat object without its combat values",
    ))?;
    let mut factor = factor;
    if a.special_attack == SpecialAttack::FRONT_DEFENCE && !a.busy {
        let off = facing_to(&a, t.position, src)?;
        if MINUS_FACING < off && off < FACING {
            factor *= SPECIAL;
            if a.special_graphic {
                return Err(GenieError::unsupported(
                    "a special attack's sprite (master +0x8e8)",
                ));
            }
        }
    }
    if retaliates(t, src, others, &a, roles, slots)? {
        factor = 0.0;
    }
    damage(t, src, weapons, factor, blast_taken, roles)?;
    after_hit(t, src, &a, before)
}

/// [`take_unit`]'s facing of a target (`a`, at `at`) to its attacker `src`
/// ([`facing_offset`]).
///
/// # Errors
///
/// A target without its facing (`+0x7e8`).
pub(super) fn facing_to(a: &Armored, at: Position, src: &Victim) -> Result<f32, GenieError> {
    let facing = a.facing.ok_or(GenieError::unsupported(
        "the facing of a target with a special attack or a retaliation (+0x7e8)",
    ))?;
    Ok(facing_offset(at, facing, src.position))
}

/// [`take_unit`]'s retaliation (`special_attack_flag` 2): an idle target with a unit AI facing
/// an attacker of weapon range below 0.2 strikes it first with its own weapons (four times over a
/// unit whose charge is ready, [`special_charge_ready`]); true when the attacker is left with at
/// most 1 hit point, which takes the hit to nothing.
///
/// # Errors
///
/// A retaliating target without its unit AI's alert mode, and what the strike does not model.
pub(super) fn retaliates(
    t: &mut Victim,
    src: &mut Victim,
    others: &mut [&mut Victim],
    a: &Armored,
    roles: &Roles,
    slots: Slots,
) -> Result<bool, GenieError> {
    let reach = src.source.weapon_range;
    let melee = reach <= Distance::new(RETALIATION_RANGE) && RETALIATION_RANGE != reach.get();
    // the conditions are pure: the ones the slice holds decide first
    if !(a.special_attack == SpecialAttack::RETALIATION && a.unit_ai && !a.busy && melee) {
        return Ok(false);
    }
    let idle = a.alert_idle.ok_or(GenieError::unsupported(
        "the alert mode and attackers of a retaliating target's unit AI (FUN_140c42b30)",
    ))?;
    if !idle {
        return Ok(false);
    }
    let off = facing_to(a, t.position, src)?;
    if !(MINUS_FACING < off && off < FACING) {
        return Ok(false);
    }
    let mut f = elevation_factor(t.position.z(), t.attack_elevation, src);
    if src.kind == MasterKind::COMBAT && special_charge_ready(&src.source)? {
        f *= RETALIATION_SCALE;
    }
    let w = t.weapons.clone();
    take(
        src,
        t,
        others,
        &w,
        f,
        HitPoints::ZERO,
        HitLayer { roles, slots },
    )?;
    Ok(src.hp <= HitPoints::new(1.0))
}

/// [`take_unit`]'s end after the damage: a hit takes at least one hit point of the `before`
/// effective ones (but past the hit point floor); a target with `damage_reflection` gives its
/// attacker that share of the loss, at least 1; below 1 hit point the attacker's kill bonus,
/// else its combat ability's effects.
///
/// # Errors
///
/// A kill bonus task and the combat ability's effects, which are not modelled.
pub(super) fn after_hit(
    t: &mut Victim,
    src: &mut Victim,
    a: &Armored,
    before: HitPoints,
) -> Result<(), GenieError> {
    // a hit takes at least one hit point
    let mut lost = before - effective(t);
    let floor = hp_floor(a, t.master_hp);
    if lost < HitPoints::new(1.0)
        && (floor == HitPoints::ZERO || floor + HitPoints::new(1.0) <= before)
    {
        t.hp = before - HitPoints::new(1.0);
        lost = HitPoints::new(1.0);
    }
    if EPSILON < a.reflection.get() {
        let d = a.reflection.get() * lost;
        lose(
            src,
            if HitPoints::new(1.0) <= d {
                d
            } else {
                HitPoints::new(1.0)
            },
        );
    }
    // COMISS 1, hp / JBE past: below 1 (not NaN) the kill bonus; COMISS hp, 1 / JC past: at 1
    // or more (not NaN) the attacker's effects
    if ONE > t.hp.get() {
        kill_bonus(&src.source)?;
    }
    if MasterKind::RGE_COMBAT <= src.kind && t.hp >= HitPoints::new(ONE) {
        attack_effects(&src.source, t)?;
    }
    Ok(())
}
