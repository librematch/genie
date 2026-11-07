//! A target's slot 0xe8: a building's, a static object's and a combat unit's, with the
//! unit's facing, its retaliation and the hit's end.

use crate::combat::KillState;
use crate::combat::damage::{
    Armored, Attacker, Building, EPSILON, KillCount, ONE, Razed, Victim, charge_hit, damage,
    effective, elevation_factor, facing_offset, hp_floor, lose, special_charge_ready,
};
use crate::combat::stingers::hand_off;
use crate::world::slots::CheckedIndexExt;
use crate::{GenieError, GenieResult};

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
pub(super) struct Strike<'world> {
    /// The attacker's attacks by armour class.
    weapons: &'world [ClassAmount],
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
    victim: &mut Victim,
    src: &mut Victim,
    others: &mut [&mut Victim],
    weapons: &[ClassAmount],
    factor: f32,
    blast_taken: HitPoints,
    layer: HitLayer<'_>,
) -> GenieResult<()> {
    let HitLayer { roles, .. } = layer;
    match victim.kind {
        MasterKind::COMBAT => take_unit(victim, src, others, weapons, factor, blast_taken, layer),
        MasterKind::RGE_COMBAT | MasterKind::MISSILE => {
            damage(victim, src, weapons, factor, blast_taken, roles)
        }
        MasterKind::BUILDING => {
            let strike = Strike {
                weapons,
                factor,
                blast_taken,
            };
            take_building(victim, src, others, strike, HitRoute::Dealt, layer).map(|_| ())
        }
        _ => {
            take_static(victim, &src.source, weapons, factor, roles);
            Ok(())
        }
    }
}

/// A razing's counts for the attacker's player `ap` and the owner `player_id` (16-bit adds of the
/// players' ids, `ADD DX, word ptr [.. + 0x718]`): the razings (43), the kill values (170, 152),
/// the razings of the other (155) and the per-player tables (350, 375, 425).
///
/// # Errors
///
/// A per-player table past the attributes.
fn razing_counts(
    (ap, player_id): (PlayerId, PlayerId),
    cost: Amount,
    slots: Slots,
) -> GenieResult<[(PlayerId, AttributeId, Amount); 7]> {
    Ok([
        (ap, slots.attribute.razings, ONE_COUNT),
        (ap, slots.attribute.kills_value, cost),
        (player_id, slots.attribute.kills_value_other, cost),
        (player_id, slots.attribute.razings_other, ONE_COUNT),
        (
            ap,
            slots.attribute.razings_of.offset(player_id.index())?,
            ONE_COUNT,
        ),
        (
            player_id,
            slots.attribute.razed_by.offset(ap.index())?,
            ONE_COUNT,
        ),
        (
            ap,
            slots.attribute.razings_value_of.offset(player_id.index())?,
            cost,
        ),
    ])
}

/// A trebuchet's kill counts (its damage did not count the kill) for the attacker's player `ap` and
/// the owner `player_id`: the kills (20), the kill values (170, 152), the kills of the other (154)
/// and the per-player tables (300, 325, 400).
///
/// # Errors
///
/// A per-player table past the attributes.
fn kill_counts(
    (ap, player_id): (PlayerId, PlayerId),
    cost: Amount,
    slots: Slots,
) -> GenieResult<[(PlayerId, AttributeId, Amount); 7]> {
    Ok([
        (ap, slots.attribute.kills, ONE_COUNT),
        (ap, slots.attribute.kills_value, cost),
        (player_id, slots.attribute.kills_other, ONE_COUNT),
        (player_id, slots.attribute.kills_value_other, cost),
        (
            ap,
            slots.attribute.kills_of.offset(player_id.index())?,
            ONE_COUNT,
        ),
        (
            player_id,
            slots.attribute.killed_by.offset(ap.index())?,
            ONE_COUNT,
        ),
        (
            ap,
            slots.attribute.kills_value_of.offset(player_id.index())?,
            cost,
        ),
    ])
}

/// How a hit reaches a building's slot 0xe8 (`FUN_1411cea90`): as dealt (its flag set, which
/// every caller sets) or forwarded by an annex to its main building (the flag clear).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum HitRoute {
    /// As dealt.
    Dealt,
    /// Forwarded by an annex.
    Forwarded,
}

/// A building's slot 0xe8 (`FUN_1411cea90`), its flag the hit's `route` ([`HitRoute`]). Its wake
/// from semi-asleep (`+0xc98`: the flag cleared, `FUN_14119deb0` with 0, the owner's `+0x80` list,
/// `FUN_14107db90`) never runs here: only the building's load sets that flag (`FUN_1411df160` ->
/// `FUN_1411ddd20` with the save's `temp_semi_asleep`), and no saved building of the 158041
/// recordings has it (1473 headers read); the slice does not hold it. A hit [`HitRoute::Dealt`] to
/// an annex (`+0xbb8`) gives the hit to its main building among `others` (whose slot 0xe8 runs with
/// the flag clear and the same `blast_taken`) and takes its hit points when that hit got past its
/// hit point test. Else at 0 hit points or fewer (`COMISS 0, hp` / `JNC`: a NaN goes on) nothing
/// happens. The damage ([`damage`]); without a blast's damage so far (`UCOMISS blast_taken, 0`:
/// exactly 0), a hit that took less than its least ([`ONE`]; `DamageTuning::least_class_hit` from
/// the [`ClassRoles::LEAST_HIT`](crate::world::layer::ClassRoles::LEAST_HIT) under the game's
/// setting `+0x680` at 1) takes the least again (slot 0xb0 with it; `COMISS least, lost` / `JBE`
/// past). Not held: the statistics (`FUN_1417783d0`) and the local player's telemetry
/// (`FUN_140a4b540`). A player's building (`+0x718` above 0) left below 1 hit point (`COMISS 1, hp`
/// / `JBE` past) is razed ([`Razed`]): a castle's or a wonder's history event; but for an annex or
/// a wall (class 27), the razing counts, or a trebuchet's kill counts unless its kill is counted
/// (`+0x954`, which its damage's slot 0x8b0 sets, [`count_kill`], or its stingers' credits,
/// [`crate::combat::stingers::credit`]), then the kill bonus ([`kill_bonus`]) and the razing
/// values (172, 153). Then `damaged_lately_timer` (`+0x30d`) is 60 and its annexes take its hit
/// points (the write-back gives them to those `others` does not hold, [`Building::struck`]); a
/// dealt hit's attacker of type 50 and up leaves a building of 1 hit point or more (`COMISS hp, 1`
/// / `JC` past) to its stingers ([`hand_off`]). [`BuildingHit::Past`] when the hit got past
/// the hit point test.
pub(super) fn take_building(
    target: &mut Victim,
    src: &mut Victim,
    others: &mut [&mut Victim],
    strike: Strike<'_>,
    route: HitRoute,
    layer: HitLayer<'_>,
) -> GenieResult<BuildingHit> {
    let HitLayer {
        roles,
        slots,
        damage_tuning,
    } = layer;
    let Strike {
        weapons,
        factor,
        blast_taken,
    } = strike;
    let before = target.hp;
    let main = target.building.as_ref().and_then(|building| building.main);
    if route == HitRoute::Dealt
        && let Some(main) = main
    {
        forward_to_main(target, src, others, strike, main, layer)?;
        return Ok(BuildingHit::Stopped);
    }
    if HitPoints::ZERO >= target.hp {
        return Ok(BuildingHit::Stopped);
    }
    damage(target, src, weapons, factor, blast_taken, roles)?;
    // +0x954 after the damage: its slot 0x8b0 or its stingers' credits counted the kill
    let counted = target.kill_state.is_counted();
    if blast_taken == HitPoints::ZERO {
        let least = if src.source.mode_680.is_one()
            && roles
                .of_class(src.source.class)
                .contains(ClassRoles::LEAST_HIT)
        {
            damage_tuning.least_class_hit
        } else {
            ONE
        };
        if least > (before - target.hp).get() {
            lose(target, HitPoints::new(least));
        }
    }
    let building = target.building.get_or_insert_with(Building::default);
    if target.player != PlayerId::GAIA && ONE > target.hp.get() {
        let (ap, player_id) = (src.player, target.player);
        let event = {
            let held = roles.of_master(target.master_id);
            if held.contains(MasterRoles::CASTLE) {
                Some(EventKind::CastleDestroyed)
            } else if held.contains(MasterRoles::WONDER) {
                Some(EventKind::WonderDestroyed)
            } else {
                None
            }
        };
        let mut adds = Vec::new();
        if building.main.is_none() && !roles.of_class(target.class).contains(ClassRoles::WALL) {
            let cost = building.cost;
            if !building.trebuchet {
                adds.extend(razing_counts((ap, player_id), cost, *slots)?);
            } else if !counted {
                adds.extend(kill_counts((ap, player_id), cost, *slots)?);
            }
            kill_bonus(&src.source)?;
            adds.extend([
                (ap, slots.attribute.razings_value, cost),
                (player_id, slots.attribute.razings_value_other, cost),
            ]);
        }
        building.razed.push(Razed {
            attacker_player: ap,
            owner: player_id,
            event,
            adds,
        });
    }
    building.damaged_lately = DamagedTimer::HIT;
    building.struck = true;
    let annexes = building.annexes.clone();
    for victim in others
        .iter_mut()
        .filter(|victim| annexes.contains(&victim.id))
    {
        victim.hp = target.hp;
    }
    if route == HitRoute::Dealt
        && MasterKind::RGE_COMBAT <= src.kind
        && target.hp >= HitPoints::new(ONE)
    {
        hand_off(src, target);
    }
    Ok(BuildingHit::Past)
}

/// Whether a building's hit got past its hit point test ([`take_building`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub(super) enum BuildingHit {
    /// It did: the building took it.
    Past,
    /// The building had no hit points, or the hit went to its main building.
    Stopped,
}

/// [`take_building`]'s forward of an annex `target`'s hit to its main building `main` among
/// `others` (out of the slice while it takes the hit, not forwarded again): the annex takes the
/// main building's hit points when the hit got past its test and the main building lists it.
/// Its caller answers [`BuildingHit::Stopped`].
fn forward_to_main(
    target: &mut Victim,
    src: &mut Victim,
    others: &mut [&mut Victim],
    strike: Strike<'_>,
    main: MainBuildingId,
    layer: HitLayer<'_>,
) -> GenieResult<()> {
    let main_index = others
        .iter()
        .position(|victim| victim.id == main.id())
        .ok_or(NOT_HELD)?;
    let mut victim = replace(
        &mut **others.checked_mut(main_index)?,
        Victim::placeholder(),
    );
    let struck = take_building(&mut victim, src, others, strike, HitRoute::Forwarded, layer);
    let annexed = victim
        .building
        .as_ref()
        .is_some_and(|building| building.annexes.contains(&target.id));
    if matches!(struck, Ok(BuildingHit::Past)) && annexed {
        target.hp = victim.hp;
    }
    **others.checked_mut(main_index)? = victim;
    struck.map(|_| ())
}

/// Slot 0x8b0 of `victim`, whose kill is not counted yet (`+0x954`), for the attacker's player
/// `killer`: a TRIBE combat object's (types 70 and 80, `FUN_141205880`) counts the kill for
/// `killer` (`FUN_1412005b0`, [`KillCount::Killer`]), then pays its master's kill rewards (each
/// task of action type 0xa1 with a `work_range` above 1e-7, `FUN_14120b5f0`); a combat object's
/// of type 50 or 60 (`FUN_14107da00`) pays nothing. Both then count its kill (`+0x954`) and tell
/// the local player's event log (`FUN_140a4d670`, not the simulation's).
///
/// # Errors
///
/// A kill reward task, which is not modelled.
pub(super) fn count_kill(victim: &mut Victim, killer: PlayerId) -> GenieResult<()> {
    if matches!(victim.kind, MasterKind::COMBAT | MasterKind::BUILDING) {
        victim.kill_counts.push(KillCount::Killer(killer));
        if victim
            .armored
            .as_ref()
            .is_some_and(|armored| armored.kill_tasks)
        {
            return Err(GenieError::unsupported(
                "a kill reward task (action type 0xa1)",
            ));
        }
    }
    victim.kill_state = KillState::Counted;
    Ok(())
}

/// `FUN_1415c6510`, the kill bonus of the attacker `src` (type 40 and up) for an object it
/// killed: its master's tasks of action type 0x9a pay their owner's attributes, research
/// technologies and run effects (not modelled); an attacker without one does nothing.
pub(super) fn kill_bonus(src: &Attacker) -> GenieResult<()> {
    if MasterKind::ACTION <= src.kind && src.kill_bonus_tasks {
        return Err(GenieError::unsupported(
            "an attacker's kill bonus task (action type 0x9a)",
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
    victim: &mut Victim,
    src: &Attacker,
    weapons: &[ClassAmount],
    factor: f32,
    roles: &Roles,
) {
    // slot 0x2f0 of a TRIBE attacker (types 70, 80; [`charge_hit`]) starts the sum at its raw
    // charge (slot 0x310, `+0x9e0`) plus 0
    let mut damage = if MasterKind::COMBAT <= src.kind && charge_hit(src, victim, roles) {
        src.charge.get() + 0.0
    } else {
        0.0f32
    };
    for class_amount in weapons {
        damage += class_amount.amount.as_f32() * factor;
    }
    victim.hp -= HitPoints::new(damage);
    if victim.hp < HitPoints::new(1.0) {
        victim.hp = HitPoints::ZERO;
        victim.goto_sleep = false;
        // The villager class (4): its hits leave what a static target holds.
        if !roles.of_class(src.class).contains(ClassRoles::VILLAGER) {
            victim.amount = Amount::ZERO;
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
/// handling). Left below 1 hit point, a player's target whose kill is not counted (`+0x954`)
/// runs slot 0x8b0 ([`count_kill`]): a hit's least loss can kill what its damage left alive. Not
/// held: the statistics (`FUN_1417783d0`); not modelled: a kill bonus task of the attacker
/// (action type 0x9a, `FUN_1415c6510`). A target left at 1 hit point or more takes the
/// attacker's stingers ([`hand_off`], `FUN_14107bdf0`).
pub(super) fn take_unit(
    victim: &mut Victim,
    src: &mut Victim,
    others: &mut [&mut Victim],
    weapons: &[ClassAmount],
    factor: f32,
    blast_taken: HitPoints,
    layer: HitLayer<'_>,
) -> GenieResult<()> {
    let HitLayer {
        roles,
        damage_tuning,
        ..
    } = layer;
    let before = effective(victim);
    if victim.hp < HitPoints::new(1.0) {
        return Ok(());
    }
    let armored = victim.armored.clone().ok_or(GenieError::unsupported(
        "a combat object without its combat values",
    ))?;
    let mut factor = factor;
    if armored.special_attack == SpecialAttack::FRONT_DEFENCE && !armored.busy {
        let off = facing_to(&armored, victim.position, src)?;
        if damage_tuning.minus_facing < off && off < damage_tuning.facing {
            factor *= damage_tuning.special;
            if armored.special_sprite.is_some() {
                return Err(GenieError::unsupported(
                    "a special attack's sprite (master +0x8e8)",
                ));
            }
        }
    }
    if retaliates(victim, src, others, &armored, layer)? {
        factor = 0.0;
    }
    damage(victim, src, weapons, factor, blast_taken, roles)?;
    after_hit(victim, src, &armored, before)
}

/// [`take_unit`]'s facing of a target (`armored`, at `at`) to its attacker `src`
/// ([`facing_offset`]).
///
/// # Errors
///
/// A target without its facing (`+0x7e8`).
pub(super) fn facing_to(armored: &Armored, at: Position, src: &Victim) -> GenieResult<f32> {
    let facing = armored.facing.ok_or(GenieError::unsupported(
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
    victim: &mut Victim,
    src: &mut Victim,
    others: &mut [&mut Victim],
    armored: &Armored,
    layer: HitLayer<'_>,
) -> GenieResult<bool> {
    let HitLayer { damage_tuning, .. } = layer;
    let reach = src.source.weapon_range;
    let melee = reach <= Distance::new(damage_tuning.retaliation_range)
        && damage_tuning.retaliation_range != reach.get();
    // the conditions are pure: the ones the slice holds decide first
    if !(armored.special_attack == SpecialAttack::RETALIATION
        && armored.unit_ai
        && !armored.busy
        && melee)
    {
        return Ok(false);
    }
    let idle = armored.alert_idle.ok_or(GenieError::unsupported(
        "the alert mode and attackers of a retaliating target's unit AI (FUN_140c42b30)",
    ))?;
    if !idle {
        return Ok(false);
    }
    let off = facing_to(armored, victim.position, src)?;
    if !(damage_tuning.minus_facing < off && off < damage_tuning.facing) {
        return Ok(false);
    }
    let mut elevation_factor = elevation_factor(
        victim.position.z(),
        victim.attack_elevation,
        src,
        damage_tuning,
    );
    if src.kind == MasterKind::COMBAT && special_charge_ready(&src.source) {
        elevation_factor *= damage_tuning.retaliation_scale;
    }
    let class_amounts = victim.weapons.clone();
    take(
        src,
        victim,
        others,
        &class_amounts,
        elevation_factor,
        HitPoints::ZERO,
        layer,
    )?;
    Ok(src.hp <= HitPoints::new(1.0))
}

/// [`take_unit`]'s end after the damage: a hit takes at least one hit point of the `before`
/// effective ones (but past the hit point floor; a direct write, no slot 0xb0); a target with
/// `damage_reflection` gives its attacker that share of the loss, at least 1 (slot 0xb0); below 1
/// hit point a player's target whose kill is not counted runs slot 0x8b0 ([`count_kill`]) and the
/// attacker's kill bonus follows, else its stingers ([`hand_off`]).
///
/// # Errors
///
/// A kill bonus task and the combat ability's effects, which are not modelled.
pub(super) fn after_hit(
    victim: &mut Victim,
    src: &mut Victim,
    armored: &Armored,
    before: HitPoints,
) -> GenieResult<()> {
    // a hit takes at least one hit point
    let mut lost = before - effective(victim);
    let floor = hp_floor(armored, victim.master_hp);
    if lost < HitPoints::new(1.0)
        && (floor == HitPoints::ZERO || floor + HitPoints::new(1.0) <= before)
    {
        victim.hp = before - HitPoints::new(1.0);
        lost = HitPoints::new(1.0);
    }
    if EPSILON < armored.reflection.get() {
        let hit_points = armored.reflection.get() * lost;
        lose(
            src,
            if HitPoints::new(1.0) <= hit_points {
                hit_points
            } else {
                HitPoints::new(1.0)
            },
        );
    }
    // COMISS 1, hp / JBE past: below 1 (not NaN) slot 0x8b0 for a player's victim whose kill
    // is not counted, then the kill bonus; COMISS hp, 1 / JC past: at 1 or more (not NaN) the
    // attacker's effects
    if ONE > victim.hp.get() {
        if victim.player != PlayerId::GAIA && !victim.kill_state.is_counted() {
            count_kill(victim, src.player)?;
        }
        kill_bonus(&src.source)?;
    }
    if MasterKind::RGE_COMBAT <= src.kind && victim.hp >= HitPoints::new(ONE) {
        hand_off(src, victim);
    }
    Ok(())
}
