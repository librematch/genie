//! The damage beyond what a blast has done (`FUN_141082e80`) and the blast itself (slot
//! 0x848).

use crate::GenieError;
use crate::combat::Charge;
use crate::combat::damage::HitLayer;
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
use crate::world::slots::CheckedIndex;
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
    t: &mut Victim,
    src: &mut Victim,
    weapons: &[ClassAmount],
    factor: f32,
    blast_taken: HitPoints,
    roles: &Roles,
) -> Result<(), GenieError> {
    let d = armor_damage(
        t,
        weapons,
        factor,
        &src.source,
        bonus_damage(src, t, roles),
        roles,
    )?;
    if blast_taken < d {
        if MasterKind::COMBAT <= src.kind {
            let hit = charge_hit(&src.source, t, roles);
            src.source.charge_hit = hit;
            if hit && src.source.area_effect <= Distance::ZERO {
                src.source.charge = Charge::ZERO;
            }
        }
        lose(t, d - blast_taken);
    }
    let marks = t.kind == MasterKind::COMBAT
        || (t.kind == MasterKind::BUILDING
            && roles
                .of_class(t.class)
                .intersects(ClassRoles::PACKED_TREBUCHET | ClassRoles::UNPACKED_TREBUCHET));
    if MasterKind::MISSILE <= src.kind
        && src.source.ability.contains(CombatAbility::MARKS_TARGET)
        && marks
    {
        return Err(GenieError::unsupported(
            "an attacker whose combat ability marks its target (FUN_141206250)",
        ));
    }
    let master_hp = i32::from(t.master_hp.get());
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
        t.damaged
            .push((sender, [src.id.get(), t.hp.get() as i32, master_hp]));
    }
    if t.hp < HitPoints::new(1.0) {
        t.hp = HitPoints::ZERO;
        if t.armored.as_ref().is_some_and(|a| a.kill_tasks) {
            return Err(GenieError::unsupported(
                "a kill reward task (action type 0xa1)",
            ));
        }
        t.killed = Some(Kill {
            attacker: src.id,
            attacker_player: src.player,
            attacker_class: src.class,
            // The livestock class, which goes to Gaia when killed.
            // The artifact class (1), which no blast reaches.
            to_gaia: t.kind == MasterKind::COMBAT
                && roles.of_class(t.class).contains(ClassRoles::LIVESTOCK)
                && t.player != PlayerId::new(0),
        });
    }
    Ok(())
}

/// The blast's rectangle of tiles about `center` for radius `r` (`FUN_14107f690`): the tiles
/// from `(int)(c - r) - 6` (at least 0) to before `(int)(c + r) + 6` (at most the map's edge,
/// which no object passes), each `CVTTSS2SI` into 32 bits kept as a short.
pub(super) fn blast_tiles(center: Position, r: Distance) -> (Tile, Tile) {
    let low = |c: f32| {
        let t = (cvttss2si(c - r.get()) as i16).wrapping_sub(6);
        i32::from(t.max(0))
    };
    let high = |c: f32| i32::from((cvttss2si(c + r.get()) as i16).wrapping_add(6));
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
/// the wide one), a NaN `area_damage` leaves the factor as it is. Answers whether the radius is
/// above 0 (or NaN: `COMISS 0, r`/`JC`), which neither caller reads.
pub fn blast(
    attacker: &mut Victim,
    center: Position,
    exclude: Option<ObjectId>,
    main: Option<ObjectId>,
    targets: &mut [&mut Victim],
    factor: f32,
    layer: HitLayer<'_>,
) -> Result<bool, GenieError> {
    let a = attacker.source;
    let r = a.area_effect;
    if !below_or_unordered(0.0, r.get()) {
        return Ok(false);
    }
    let wide = if a.blast_level.contains(BlastLevel::CONE) {
        r
    } else {
        Distance::ZERO
    };
    let blast = Blast {
        a,
        center,
        r,
        wide,
        tiles: blast_tiles(center, r),
        spares_friends: a.blast_level.spares_friends(a.weapon_range),
        exclude,
        main,
        factor,
    };
    targets.sort_by_key(|t| (t.position.y() as i32, t.position.x() as i32, t.tile_place));
    // every building on the tiles starts the blast with nothing taken (+0xb90, first pass)
    for t in targets
        .iter_mut()
        // The building type (`'P'`): its slot 0xe8 is `FUN_1411cea90` ([`take_building`]).
        // The wall class (27): its razing counts nothing.
        .filter(|t| t.kind == MasterKind::BUILDING && blast.on_tiles(t))
    {
        if let Some(b) = t.building.as_mut() {
            b.blast_taken = HitPoints::ZERO;
        }
    }
    attacker.source.charge_hit = false;
    let weapons = attacker.weapons.clone();
    for i in 0..targets.len() {
        blast_one(attacker, targets, i, (&blast, &weapons), layer)?;
    }
    Ok(true)
}

/// One blast as each object reads it ([`blast`]): the attacker's values at its start, its
/// center, radius and wide cone's radius (0 without one), its tiles ([`blast_tiles`]), whether
/// it spares the attacker's friends, the object it leaves out, its `main` object and its factor.
#[derive(Clone, Copy, Debug)]
pub(super) struct Blast {
    pub(super) a: Attacker,
    pub(super) center: Position,
    pub(super) r: Distance,
    pub(super) wide: Distance,
    pub(super) tiles: (Tile, Tile),
    pub(super) spares_friends: bool,
    pub(super) exclude: Option<ObjectId>,
    pub(super) main: Option<ObjectId>,
    pub(super) factor: f32,
}

impl Blast {
    /// Whether the object stands on the blast's tiles (its place truncated).
    pub(super) const fn on_tiles(&self, t: &Victim) -> bool {
        let (from, to) = self.tiles;
        let (x, y) = (t.position.x() as i32, t.position.y() as i32);
        from.x() <= x && x < to.x() && from.y() <= y && y < to.y()
    }
}

/// The blast on object `i` of `targets` with the attacker's `weapons` ([`blast`]): passed off
/// its tiles, as a friend it spares, the object it leaves out, the attacker, an artifact, an
/// object of a blast level it defends, a ram's non-building other than `main`, one beyond the
/// radius, one whose shield blocks it, and one outside a cone; a fixed damage takes its size
/// ([`fixed_blast`]); any other takes the hit ([`blast_hit`]).
pub(super) fn blast_one(
    attacker: &mut Victim,
    targets: &mut [&mut Victim],
    i: usize,
    (b, weapons): (&Blast, &[ClassAmount]),
    layer: HitLayer<'_>,
) -> Result<(), GenieError> {
    let HitLayer {
        roles,
        damage_tuning,
        ..
    } = layer;
    let a = &b.a;
    let t = &**targets.checked(i)?;
    if !b.on_tiles(t) {
        return Ok(());
    }
    if t.uncertain {
        return Err(GenieError::unsupported(
            "a blast over an object whose player's update is not modelled",
        )
        .with("blasted", t.id));
    }
    if (b.spares_friends && t.allied.by_attacker)
        || Some(t.id) == b.exclude
        || t.id == attacker.id
        || roles.of_class(t.class).contains(ClassRoles::ARTIFACT)
    {
        return Ok(());
    }
    let is_main = Some(t.id) == b.main;
    let ledger = blast_ledger(targets, i)?;
    let (share, defense, blast_taken) = ledger_values(targets, i, ledger, a)?;
    if !(BlastDefense::ALWAYS_REACHED <= defense || a.blast_level.reaches(defense)) {
        return Ok(());
    }
    let t = &mut **targets.checked_mut(i)?;
    if roles
        .of_master(a.master_id)
        .contains(MasterRoles::BUILDINGS_ONLY_BLAST)
        && t.kind != MasterKind::BUILDING
        && !is_main
    {
        return Ok(());
    }
    let Some(d2) = blast_distance2(t, b.center, b.r.get()) else {
        return Ok(());
    };
    if MasterKind::COMBAT <= t.kind && shield(t, false) {
        return Ok(());
    }
    let mut f = blast_falloff(attacker, a, t, (b.r.get(), d2), is_main, damage_tuning);
    if !is_main {
        if outside_cone(attacker, a, t, (d2, b.wide.get()), damage_tuning)? {
            return Ok(());
        }
        // COMISS share, 0 / JC, then COMISS 0, share / JBE: a share below 0 is a fixed
        // damage, a NaN one goes on to the hit with the factor as it is
        if share < 0.0 {
            return fixed_blast(attacker, a, t, share);
        }
        if !share.is_nan() {
            f *= share;
        }
    }
    blast_hit(
        attacker,
        targets,
        i,
        (weapons, f * b.factor),
        (ledger, blast_taken),
        layer,
    )
}

/// The object a building's blast damage so far is kept on (`+0xbb8`): its main building's
/// place among `targets` for an annex, its own for another building; None for an object that
/// is not a building.
pub(super) fn blast_ledger(targets: &[&mut Victim], i: usize) -> Result<Option<usize>, GenieError> {
    let t = &**targets.checked(i)?;
    if t.kind != MasterKind::BUILDING {
        return Ok(None);
    }
    t.building.as_ref().and_then(|b| b.main).map_or_else(
        || Ok(Some(i)),
        |m| {
            targets
                .iter()
                .position(|v| v.id == m.id())
                .map(Some)
                .ok_or(NOT_HELD)
        },
    )
}

/// What the blast reads of object `i` through its `ledger`: the `area_damage` share, the blast
/// level it defends to and the blast's damage so far; an annex reads its main building's.
pub(super) fn ledger_values(
    targets: &[&mut Victim],
    i: usize,
    ledger: Option<usize>,
    a: &Attacker,
) -> Result<(f32, BlastDefense, HitPoints), GenieError> {
    let taken = |v: &Victim| {
        v.building
            .as_ref()
            .map_or(HitPoints::ZERO, |b| b.blast_taken)
    };
    let t = &**targets.checked(i)?;
    Ok(match ledger {
        Some(k) if k != i => {
            let m = &**targets.checked(k)?;
            (m.source.area_damage.get(), m.blast_defense, taken(m))
        }
        Some(_) => (a.area_damage.get(), t.blast_defense, taken(t)),
        None => (a.area_damage.get(), t.blast_defense, HitPoints::ZERO),
    })
}

/// The square distance from the blast's `center` to the object's box (the gap past each
/// radius, a NaN gap 0: `COMISS`/`JBE`); None beyond the radius `r` (a NaN square distance is
/// beyond it: `JBE`). The hit's slack (its info's `+0xc`, `+0x10`) is 0 for a melee hit.
pub(super) fn blast_distance2(t: &Victim, center: Position, r: f32) -> Option<f32> {
    let gap = |d: f32, radius: Distance| {
        if at_most_or_unordered(d, radius.get()) {
            0.0
        } else {
            d - radius.get()
        }
    };
    let dx = gap(
        (t.position.x() - center.x()).abs(),
        Distance::new(t.radius.x()),
    );
    let dy = gap(
        (t.position.y() - center.y()).abs(),
        Distance::new(t.radius.y()),
    );
    let d2 = dy * dy + dx * dx;
    if at_most_or_unordered(r * r, d2) {
        None
    } else {
        Some(d2)
    }
}

/// The blast's damage factor on `t` at square distance `d2` within radius `r`: the attacker's
/// elevation factor, times `t`'s `friendly_fire_damage` when it is of type 70 and up and its player
/// is the attacker's or allied, times the falloff `(r - d) / r` (a melee attacker's only with
/// [`BlastLevel::FALLS_OFF`] and not on `main`; a missile's everywhere, at distance 0 on a `main`
/// building).
pub(super) fn blast_falloff(
    attacker: &Victim,
    a: &Attacker,
    t: &Victim,
    (r, d2): (f32, f32),
    is_main: bool,
    damage_tuning: &DamageTuning,
) -> f32 {
    let mut f = elevation_factor(
        attacker.position.z(),
        attacker.attack_elevation,
        t,
        damage_tuning,
    );
    if MasterKind::COMBAT <= t.kind && (t.player == attacker.player || t.allied.by_target) {
        f *= t.armored.as_ref().map_or(1.0, |x| x.friendly_fire.get());
    }
    if a.missile.is_none() {
        if a.blast_level.contains(BlastLevel::FALLS_OFF) && !is_main {
            f *= (r - sqrt(d2)) / r;
        }
    } else {
        let d = if !is_main || t.kind != MasterKind::BUILDING {
            sqrt(d2)
        } else {
            0.0
        };
        f *= (r - d) / r;
    }
    f
}

/// Whether `t` lies outside the cone of a cone blast ([`BlastLevel::CONE`]): beyond half a facet of
/// the attacker's facing, or for a wide one ([`BlastLevel::WIDE_CONE`]) beyond the wide radius or
/// 1.67 facets; a NaN facet offset stays in either. A blast of no cone has none to leave.
pub(super) fn outside_cone(
    attacker: &Victim,
    a: &Attacker,
    t: &Victim,
    (d2, wide): (f32, f32),
    damage_tuning: &DamageTuning,
) -> Result<bool, GenieError> {
    if !a.blast_level.contains(BlastLevel::CONE) {
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
    if !a.blast_level.contains(BlastLevel::WIDE_CONE) {
        let off = facing_offset(attacker.position, facing, t.position);
        return Ok(damage_tuning.cone < off || off < damage_tuning.minus_cone);
    }
    if wide * wide < d2 {
        return Ok(true);
    }
    let off = facing_offset(attacker.position, facing, t.position).abs();
    Ok(damage_tuning.wide_cone < off)
}

/// A fixed blast damage (an `area_damage` `share` below 0) on `t`: it loses the share's size
/// alone (slot 0xb0); below 1 it has 0 hit points and holds nothing (`FUN_14119d320`); a combat
/// unit of a player that had 1 is counted killed (`FUN_1412005b0`).
pub(super) fn fixed_blast(
    attacker: &Victim,
    a: &Attacker,
    t: &mut Victim,
    share: f32,
) -> Result<(), GenieError> {
    let before = t.hp;
    lose(t, HitPoints::new(share.abs()));
    if t.hp < HitPoints::new(1.0) {
        t.hp = HitPoints::ZERO;
        t.amount = Amount::ZERO;
        if Amount::of_master(t.capacity) < t.amount {
            t.amount = Amount::of_master(t.capacity);
        }
        if HitPoints::new(1.0) <= before
            && t.player != PlayerId::GAIA
            && t.kind == MasterKind::COMBAT
        {
            t.killed = Some(Kill {
                attacker: attacker.id,
                attacker_player: attacker.player,
                attacker_class: attacker.class,
                to_gaia: false,
            });
            kill_bonus(a)?;
        }
    }
    Ok(())
}

/// The blast's hit on object `i` with the attacker's `weapons` and `factor` ([`take`]): the
/// object leaves the slice while its hit may reach the others (its main building); a hit marks
/// the attacker's as its charge's, and its loss goes onto the `ledger` past `blast_taken`
/// (`ADDSS`, commutative).
pub(super) fn blast_hit(
    attacker: &mut Victim,
    targets: &mut [&mut Victim],
    i: usize,
    (weapons, factor): (&[ClassAmount], f32),
    (ledger, blast_taken): (Option<usize>, HitPoints),
    layer: HitLayer<'_>,
) -> Result<(), GenieError> {
    let HitLayer { roles, .. } = layer;
    let before = targets.checked(i)?.hp;
    let mut t = replace(&mut **targets.checked_mut(i)?, Victim::placeholder());
    let hit = take(
        &mut t,
        attacker,
        targets,
        weapons,
        factor,
        blast_taken,
        layer,
    );
    if hit.is_ok() {
        if !attacker.source.charge_hit {
            attacker.source.charge_hit = charge_hit(&attacker.source, &t, roles);
        }
        let lost = before - t.hp;
        let ledger = match ledger {
            Some(k) if k == i => t.building.as_mut(),
            Some(k) => targets.checked_mut(k)?.building.as_mut(),
            None => None,
        };
        if let Some(b) = ledger {
            b.blast_taken += lost;
        }
    }
    **targets.checked_mut(i)? = t;
    hit
}
