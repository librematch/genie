//! The damage beyond what a blast has done (`FUN_141082e80`) and the blast itself (slot
//! 0x848).

use crate::combat::Charge;
use crate::combat::damage::HitKind;
use crate::combat::damage::HitLayer;
use crate::combat::damage::ShieldBlock;
use crate::combat::damage::{
    Attacker, Kill, Victim, armor_damage, bonus_damage, charge_hit, elevation_factor,
    facing_offset, kill_bonus, lose, shield, take,
};
use crate::combat::missile::ShooterId;
use crate::construction::build::NOT_HELD;
use crate::map::Distance;
use crate::map::Position;
use crate::map::Tile;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered, sqrt};
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::masters::BlastDefense;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::MasterKind;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::DamageTuning;
use crate::world::slots::CheckedIndexExt;
use crate::{GenieError, GenieResult};
use std::mem::replace;

/// `FUN_141082e80`: the damage ([`armor_damage`]) beyond what a blast has done: an attacker of
/// type 70 and up records whether the hit is its charge's (slot 0x2f0, `+0x8a8`), which empties
/// a charge without an area effect (slot 0x318 with 0); the target loses it (slot 0xb0). The
/// target's unit AI gets notify 500 (but from an attacker of class 64) with the attacker, the hit
/// points left and its master's, sent by the attacker (a missile, type 60: by what it was fired
/// from, `+0x998`). Below 1 hit point it has 0 and is killed: Gaia takes a player's livestock
/// (type 70). Not modelled: an attacker of type 60 and up whose combat ability marks its target
/// (bit 2, `FUN_141206250`) and a kill reward task (action type 0xa1). Not held: the damaged flag (`+0x683`), the last damage
/// time (`+0x648`), the alerts of another owner (slot 0x2e8), the killer (`+0x690`, `+0x694`),
/// the local player's statistics, the damage sprites (slot 0x4c0) and the trigger events.
pub(super) fn damage(
    victim: &mut Victim,
    src: &mut Victim,
    weapons: &[ClassAmount],
    factor: f32,
    blast_taken: HitPoints,
    roles: &Roles,
) -> GenieResult<()> {
    let hit_points = armor_damage(
        victim,
        weapons,
        factor,
        &src.source,
        bonus_damage(src, victim, roles),
        roles,
    )?;
    if blast_taken < hit_points {
        if MasterKind::COMBAT <= src.kind {
            let hit = charge_hit(&src.source, victim, roles);
            src.source.charge_hit = hit;
            if hit && src.source.area_effect <= Distance::ZERO {
                src.source.charge = Charge::ZERO;
            }
        }
        lose(victim, hit_points - blast_taken);
    }
    let marks = victim.kind == MasterKind::COMBAT
        || (victim.kind == MasterKind::BUILDING
            && roles
                .of_class(victim.class)
                .intersects(ClassRoles::PACKED_TREBUCHET | ClassRoles::UNPACKED_TREBUCHET));
    if MasterKind::MISSILE <= src.kind
        && src.source.ability.contains(CombatAbility::MARKS_TARGET)
        && marks
    {
        return Err(GenieError::unsupported(
            "an attacker whose combat ability marks its target (FUN_141206250)",
        ));
    }
    let master_hp = i32::from(victim.master_hp.get());
    if !roles
        .of_class(src.source.class)
        .contains(ClassRoles::SELF_ALERTING)
    {
        let sender = if src.kind == MasterKind::MISSILE {
            src.source.fired_from.map(ShooterId::id)
        } else {
            Some(src.id)
        };
        // the notify's p1 is the attacker's id (the game's record)
        victim
            .damaged
            .push((sender, [src.id.get(), victim.hp.get() as i32, master_hp]));
    }
    if victim.hp < HitPoints::new(1.0) {
        victim.hp = HitPoints::ZERO;
        if victim
            .armored
            .as_ref()
            .is_some_and(|armored| armored.kill_tasks)
        {
            return Err(GenieError::unsupported(
                "a kill reward task (action type 0xa1)",
            ));
        }
        victim.killed = Some(Kill {
            attacker: src.id,
            attacker_player: src.player,
            attacker_class: src.class,
            // The livestock class, which goes to Gaia when killed.
            // The artifact class (1), which no blast reaches.
            to_gaia: victim.kind == MasterKind::COMBAT
                && roles.of_class(victim.class).contains(ClassRoles::LIVESTOCK)
                && victim.player != PlayerId::new(0),
        });
    }
    Ok(())
}

/// The blast's rectangle of tiles about `center` for radius `distance` (`FUN_14107f690`): the tiles
/// from `(int)(coordinate - distance) - 6` (at least 0) to before `(int)(coordinate + distance) +
/// 6` (at most the map's edge, which no object passes), each `CVTTSS2SI` into 32 bits kept as a
/// short.
pub(super) fn blast_tiles(center: Position, distance: Distance) -> (Tile, Tile) {
    let low = |coordinate: f32| {
        let tile = (cvttss2si(coordinate - distance.get()) as i16).wrapping_sub(6);
        i32::from(tile.max(0))
    };
    let high = |coordinate: f32| {
        i32::from((cvttss2si(coordinate + distance.get()) as i16).wrapping_add(6))
    };
    (
        Tile::new(low(center.x()), low(center.y())),
        Tile::new(high(center.x()), high(center.y())),
    )
}

/// Slot 0x848 (`FUN_14107f690`), the blast of `attacker` about `center` with factor `factor`, over
/// the objects the tiles of [`blast_tiles`] hold, row by row and a tile's objects in its list's
/// order ([`Victim::tile_place`]): the attacker's hit is first marked as no charge's (`+0x8a8`). An
/// object is passed when the blast spares its friends (a weapon range of at most 1 or
/// [`BlastLevel::SPARES_FRIENDS`]: `FUN_1410bfcb0`) and its player is the attacker's or allied,
/// when it is `exclude`, the attacker or an artifact, when its `area_effect_object_level` is at
/// most 1 and below the blast's level, when a ram's blast meets neither a building nor `main`, when
/// it lies beyond the radius (the gap past its radii) or its shield blocks the blast. Its damage
/// factor is the attacker's elevation factor, times its `friendly_fire_damage` when it is of type
/// 70 and up and its player counts the source's as its own or allied; times the falloff `(r - d) /
/// r` (a melee attacker's only with [`BlastLevel::FALLS_OFF`], and not on `main`; a missile's
/// everywhere, at distance 0 on a `main` building). An object other than `main` lies within the
/// cone of a cone blast ([`BlastLevel::CONE`]: within half a facet, or 1.67 for a wide one,
/// [`BlastLevel::WIDE_CONE`]) and takes the attacker's `area_damage` share; a fixed damage
/// (`area_damage` below 0) takes its hit points alone (slot 0xb0; below 1 it has 0 and holds
/// nothing, `FUN_14119d320`; a combat unit of a player that had 1 is counted killed,
/// `FUN_1412005b0`). Then the object's slot 0xe8 with the attacker's weapons ([`take`]), and the
/// attacker's hit is its charge's if any hit was (slot 0x2f0). Every building on the tiles starts
/// the blast with nothing taken (`+0xb90`, a first pass over the tiles); a building's hit leaves
/// out what the blast has done to it so far, its main building's for an annex (`+0xbb8`, which also
/// gives the annex its `area_damage` share and its blast level, the main building's master's), and
/// adds its loss to it; the main building of an annex it reaches is among `targets`. The compares
/// are the assembly's: a NaN gap past a radius is 0 (`COMISS`/`JBE`), a NaN square distance is
/// beyond the radius (`JBE`), a NaN facet offset stays in a cone (`JA` past, both the narrow and
/// the wide one), a NaN `area_damage` leaves the factor as it is. Neither caller reads the game's
/// answer (whether the radius is above 0, or NaN: `COMISS 0, distance`/`JC`): the port has none.
pub fn blast(
    attacker: &mut Victim,
    center: Position,
    exclude: Option<ObjectId>,
    main: Option<ObjectId>,
    targets: &mut [&mut Victim],
    factor: f32,
    layer: HitLayer<'_>,
) -> GenieResult<()> {
    let source = attacker.source;
    let distance = source.area_effect;
    if !below_or_unordered(0.0, distance.get()) {
        return Ok(());
    }
    let wide = if source.blast_level.contains(BlastLevel::CONE) {
        distance
    } else {
        Distance::ZERO
    };
    let blast = Blast {
        attacker: source,
        center,
        radius: distance,
        wide,
        tiles: blast_tiles(center, distance),
        spares_friends: source.blast_level.spares_friends(source.weapon_range),
        exclude,
        main,
        factor,
    };
    targets.sort_by_key(|victim| {
        (
            victim.position.y() as i32,
            victim.position.x() as i32,
            victim.tile_place,
        )
    });
    // every building on the tiles starts the blast with nothing taken (+0xb90, first pass)
    for victim in targets
        .iter_mut()
        // The building type (`'P'`): its slot 0xe8 is `FUN_1411cea90` ([`take_building`]).
        // The wall class (27): its razing counts nothing.
        .filter(|victim| victim.kind == MasterKind::BUILDING && blast.on_tiles(victim))
    {
        if let Some(building) = victim.building.as_mut() {
            building.blast_taken = HitPoints::ZERO;
        }
    }
    attacker.source.charge_hit = false;
    let weapons = attacker.weapons.clone();
    for index in 0..targets.len() {
        blast_one(attacker, targets, index, (&blast, &weapons), layer)?;
    }
    Ok(())
}

/// One blast as each object reads it ([`blast`]): the attacker's values at its start, its
/// center, radius and wide cone's radius (0 without one), its tiles ([`blast_tiles`]), whether
/// it spares the attacker's friends, the object it leaves out, its `main` object and its factor.
#[derive(Clone, Copy, Debug)]
pub(super) struct Blast {
    pub(super) attacker: Attacker,
    pub(super) center: Position,
    pub(super) radius: Distance,
    pub(super) wide: Distance,
    pub(super) tiles: (Tile, Tile),
    pub(super) spares_friends: bool,
    pub(super) exclude: Option<ObjectId>,
    pub(super) main: Option<ObjectId>,
    pub(super) factor: f32,
}

impl Blast {
    /// Whether the object stands on the blast's tiles (its place truncated).
    pub(super) const fn on_tiles(&self, victim: &Victim) -> bool {
        let (from, to) = self.tiles;
        let (x, y) = (victim.position.x() as i32, victim.position.y() as i32);
        from.x() <= x && x < to.x() && from.y() <= y && y < to.y()
    }
}

/// The blast on object `index` of `targets` with the attacker's `weapons` ([`blast`]): passed off
/// its tiles, as a friend it spares, the object it leaves out, the attacker, an artifact, an
/// object of a blast level it defends, a ram's non-building other than `main`, one beyond the
/// radius, one whose shield blocks it, and one outside a cone; a fixed damage takes its size
/// ([`fixed_blast`]); any other takes the hit ([`blast_hit`]).
pub(super) fn blast_one(
    attacker: &mut Victim,
    targets: &mut [&mut Victim],
    index: usize,
    (blast, weapons): (&Blast, &[ClassAmount]),
    layer: HitLayer<'_>,
) -> GenieResult<()> {
    let HitLayer {
        roles,
        damage_tuning,
        ..
    } = layer;
    let source = &blast.attacker;
    let victim = &**targets.checked(index)?;
    if !blast.on_tiles(victim) {
        return Ok(());
    }
    if victim.uncertain {
        return Err(GenieError::unsupported(
            "a blast over an object whose player's update is not modelled",
        )
        .with("blasted", victim.id));
    }
    if (blast.spares_friends && victim.allied.by_attacker)
        || Some(victim.id) == blast.exclude
        || victim.id == attacker.id
        || roles.of_class(victim.class).contains(ClassRoles::ARTIFACT)
    {
        return Ok(());
    }
    let hit = if Some(victim.id) == blast.main {
        BlastHit::Main
    } else {
        BlastHit::Splash
    };
    let ledger = blast_ledger(targets, index)?;
    let (share, defense, blast_taken) = ledger_values(targets, index, ledger, source)?;
    if !(BlastDefense::ALWAYS_REACHED <= defense || source.blast_level.reaches(defense)) {
        return Ok(());
    }
    let victim = &mut **targets.checked_mut(index)?;
    if roles
        .of_master(source.master_id)
        .contains(MasterRoles::BUILDINGS_ONLY_BLAST)
        && victim.kind != MasterKind::BUILDING
        && hit == BlastHit::Splash
    {
        return Ok(());
    }
    let Some(d2) = blast_distance2(victim, blast.center, blast.radius.get()) else {
        return Ok(());
    };
    if MasterKind::COMBAT <= victim.kind && shield(victim, HitKind::Melee) == ShieldBlock::Blocked {
        return Ok(());
    }
    let mut blast_falloff = blast_falloff(
        attacker,
        source,
        victim,
        (blast.radius.get(), d2),
        hit,
        damage_tuning,
    );
    if hit == BlastHit::Splash {
        if outside_cone(
            attacker,
            source,
            victim,
            (d2, blast.wide.get()),
            damage_tuning,
        )? {
            return Ok(());
        }
        // COMISS share, 0 / JC, then COMISS 0, share / JBE: a share below 0 is a fixed
        // damage, a NaN one goes on to the hit with the factor as it is
        if share < 0.0 {
            return fixed_blast(attacker, source, victim, share);
        }
        if !share.is_nan() {
            blast_falloff *= share;
        }
    }
    blast_hit(
        attacker,
        targets,
        index,
        (weapons, blast_falloff * blast.factor),
        (ledger, blast_taken),
        layer,
    )
}

/// The object a building's blast damage so far is kept on (`+0xbb8`): its main building's
/// place among `targets` for an annex, its own for another building; None for an object that
/// is not a building.
pub(super) fn blast_ledger(targets: &[&mut Victim], index: usize) -> GenieResult<Option<usize>> {
    let target = &**targets.checked(index)?;
    if target.kind != MasterKind::BUILDING {
        return Ok(None);
    }
    target
        .building
        .as_ref()
        .and_then(|building| building.main)
        .map_or_else(
            || Ok(Some(index)),
            |role_id| {
                targets
                    .iter()
                    .position(|victim| victim.id == role_id.id())
                    .map(Some)
                    .ok_or(NOT_HELD)
            },
        )
}

/// What the blast reads of object `index` through its `ledger`: the `area_damage` share, the blast
/// level it defends to and the blast's damage so far; an annex reads its main building's.
pub(super) fn ledger_values(
    targets: &[&mut Victim],
    index: usize,
    ledger: Option<usize>,
    attacker: &Attacker,
) -> GenieResult<(f32, BlastDefense, HitPoints)> {
    let taken = |victim: &Victim| {
        victim
            .building
            .as_ref()
            .map_or(HitPoints::ZERO, |building| building.blast_taken)
    };
    let target = &**targets.checked(index)?;
    Ok(match ledger {
        Some(ledger) if ledger != index => {
            let victim = &**targets.checked(ledger)?;
            (
                victim.source.area_damage.get(),
                victim.blast_defense,
                taken(victim),
            )
        }
        Some(_) => (
            attacker.area_damage.get(),
            target.blast_defense,
            taken(target),
        ),
        None => (
            attacker.area_damage.get(),
            target.blast_defense,
            HitPoints::ZERO,
        ),
    })
}

/// The square distance from the blast's `center` to the object's box (the gap past each
/// radius, a NaN gap 0: `COMISS`/`JBE`); None beyond the radius `reach` (a NaN square distance is
/// beyond it: `JBE`). The hit's slack (its info's `+0xc`, `+0x10`) is 0 for a melee hit.
pub(super) fn blast_distance2(victim: &Victim, center: Position, reach: f32) -> Option<f32> {
    let gap = |distance: f32, radius: Distance| {
        if at_most_or_unordered(distance, radius.get()) {
            0.0
        } else {
            distance - radius.get()
        }
    };
    let dx = gap(
        (victim.position.x() - center.x()).abs(),
        Distance::new(victim.radius.x()),
    );
    let dy = gap(
        (victim.position.y() - center.y()).abs(),
        Distance::new(victim.radius.y()),
    );
    let d2 = dy * dy + dx * dx;
    if at_most_or_unordered(reach * reach, d2) {
        None
    } else {
        Some(d2)
    }
}

/// Whom a blast hits: its main target (the missile's or the attack's) or another in its radius.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum BlastHit {
    /// The main target.
    Main,
    /// Another.
    Splash,
}

/// The blast's damage factor on `victim` at square distance `d2` within radius `radius`: the
/// attacker's elevation factor, times `victim`'s `friendly_fire_damage` when it is of type 70 and
/// up and its player is the attacker's or allied, times the falloff `(radius - distance) / radius`
/// (a melee attacker's only with [`BlastLevel::FALLS_OFF`] and not on the main target; a missile's
/// everywhere, at distance 0 on the main target's building, [`BlastHit`]).
pub(super) fn blast_falloff(
    attacker: &Victim,
    source: &Attacker,
    victim: &Victim,
    (radius, d2): (f32, f32),
    hit: BlastHit,
    damage_tuning: &DamageTuning,
) -> f32 {
    let mut elevation_factor = elevation_factor(
        attacker.position.z(),
        attacker.attack_elevation,
        victim,
        damage_tuning,
    );
    if MasterKind::COMBAT <= victim.kind
        && (victim.player == attacker.player || victim.allied.by_target)
    {
        elevation_factor *= victim
            .armored
            .as_ref()
            .map_or(1.0, |x| x.friendly_fire.get());
    }
    if source.missile.is_none() {
        if source.blast_level.contains(BlastLevel::FALLS_OFF) && hit == BlastHit::Splash {
            elevation_factor *= (radius - sqrt(d2)) / radius;
        }
    } else {
        let distance = if hit == BlastHit::Splash || victim.kind != MasterKind::BUILDING {
            sqrt(d2)
        } else {
            0.0
        };
        elevation_factor *= (radius - distance) / radius;
    }
    elevation_factor
}

/// Whether `victim` lies outside the cone of a cone blast ([`BlastLevel::CONE`]): beyond half a
/// facet of the attacker's facing, or for a wide one ([`BlastLevel::WIDE_CONE`]) beyond the wide
/// radius or 1.67 facets; a NaN facet offset stays in either. A blast of no cone has none to leave.
pub(super) fn outside_cone(
    attacker: &Victim,
    source: &Attacker,
    victim: &Victim,
    (d2, wide): (f32, f32),
    damage_tuning: &DamageTuning,
) -> GenieResult<bool> {
    if !source.blast_level.contains(BlastLevel::CONE) {
        return Ok(false);
    }
    let facing =
        attacker
            .armored
            .as_ref()
            .and_then(|x| x.facing)
            .ok_or(GenieError::unsupported(
                "the facing of an attacker with a cone blast (+0x7e8)",
            ))?;
    if !source.blast_level.contains(BlastLevel::WIDE_CONE) {
        let off = facing_offset(attacker.position, facing, victim.position);
        return Ok(damage_tuning.cone < off || off < damage_tuning.minus_cone);
    }
    if wide * wide < d2 {
        return Ok(true);
    }
    let off = facing_offset(attacker.position, facing, victim.position).abs();
    Ok(damage_tuning.wide_cone < off)
}

/// A fixed blast damage (an `area_damage` `share` below 0) on `victim`: it loses the share's size
/// alone (slot 0xb0); below 1 it has 0 hit points and holds nothing (`FUN_14119d320`); a combat
/// unit of a player that had 1 is counted killed (`FUN_1412005b0`).
pub(super) fn fixed_blast(
    attacker: &Victim,
    source: &Attacker,
    victim: &mut Victim,
    share: f32,
) -> GenieResult<()> {
    let before = victim.hp;
    lose(victim, HitPoints::new(share.abs()));
    if victim.hp < HitPoints::new(1.0) {
        victim.hp = HitPoints::ZERO;
        victim.amount = Amount::ZERO;
        if Amount::of_master(victim.capacity) < victim.amount {
            victim.amount = Amount::of_master(victim.capacity);
        }
        if HitPoints::new(1.0) <= before
            && victim.player != PlayerId::GAIA
            && victim.kind == MasterKind::COMBAT
        {
            victim.killed = Some(Kill {
                attacker: attacker.id,
                attacker_player: attacker.player,
                attacker_class: attacker.class,
                to_gaia: false,
            });
            kill_bonus(source)?;
        }
    }
    Ok(())
}

/// The blast's hit on object `index` with the attacker's `weapons` and `factor` ([`take`]): the
/// object leaves the slice while its hit may reach the others (its main building); a hit marks
/// the attacker's as its charge's, and its loss goes onto the `ledger` past `blast_taken`
/// (`ADDSS`, commutative).
pub(super) fn blast_hit(
    attacker: &mut Victim,
    targets: &mut [&mut Victim],
    index: usize,
    (weapons, factor): (&[ClassAmount], f32),
    (ledger, blast_taken): (Option<usize>, HitPoints),
    layer: HitLayer<'_>,
) -> GenieResult<()> {
    let HitLayer { roles, .. } = layer;
    let before = targets.checked(index)?.hp;
    let mut victim = replace(&mut **targets.checked_mut(index)?, Victim::placeholder());
    let hit = take(
        &mut victim,
        attacker,
        targets,
        weapons,
        factor,
        blast_taken,
        layer,
    );
    if hit.is_ok() {
        if !attacker.source.charge_hit {
            attacker.source.charge_hit = charge_hit(&attacker.source, &victim, roles);
        }
        let lost = before - victim.hp;
        let ledger = match ledger {
            Some(ledger_index) if ledger_index == index => victim.building.as_mut(),
            Some(ledger) => targets.checked_mut(ledger)?.building.as_mut(),
            None => None,
        };
        if let Some(building) = ledger {
            building.blast_taken += lost;
        }
    }
    **targets.checked_mut(index)? = victim;
    hit
}
