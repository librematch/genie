//! A hit: where it comes from ([`Hit`]), what the swing does after it, the strike
//! (`FUN_141084230`) and the blast so far.

use crate::combat::Charge;
use crate::combat::attack::{DRAW_SCALE, QUIET_SWINGS};
use crate::combat::damage::Victim;
use crate::map::Distance;
use crate::map::Position;
use crate::math::rng::Well512;
use crate::objects::Frame;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::Relics;
use crate::objects::masters::ChargeType;
use crate::objects::masters::HitChance;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::masters::master_type::MISSILE;
use crate::world::Seconds;
use crate::{GenieError, combat::damage};

/// Where a hit comes from: the place it lands (`FUN_141084230`'s x, y, z: the attacker's for a
/// melee hit, the missile's for a missile's), the height the elevation factor compares with the
/// target's (the attacker's slot 0x1f0: a combat object's, `FUN_14120d780`, its origin record's
/// `+0x8` when the record names an owner that has its master, else its own; None for a missile
/// whose record names no owner, `FUN_14113e550`: a factor of 1), whether a missile lands it (a
/// shield of charge type 4 blocks it instead of 5) and the attacker's hit chance (`+0x83e`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Hit {
    pub at: Position,
    pub from_z: Option<f32>,
    pub missile: bool,
    pub chance: HitChance,
}

/// What the swing does after its hit, before it waits for its sprite's loop (`FUN_140a82ae0`
/// case 6, `+0x9e` clear).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AfterHit {
    /// A ranged unit without a target (and no indirect fire, `+0xa0`, which the slice's attacks
    /// never have): `set_state(2)`.
    Done,
    /// A ranged unit past its interrupt frame, else its firing frame: `set_state(6)` at once.
    Ready,
    /// The swing goes on; at or past its interrupt frame readyToAttack (`FUN_140a7f9d0` with 0)
    /// runs first.
    Continue {
        /// readyToAttack runs.
        check: bool,
    },
}

/// [`AfterHit`] of a unit with a missile or not (`ranged`, `+0x98`), its attack timer (`+0x91c`,
/// slot 0x100: `COMISS` with 0 then `SETBE`, so NaN counts as run out), a target (`+0x20`), its
/// interrupt frame (`+0x9c`, −1 for none), its firing frame (`+0x9a`) and the sprite's frame
/// (the sprite list's slot 8: the node's `+0x24`), compared as signed shorts.
pub(super) fn after_hit(
    ranged: bool,
    timer: Seconds,
    target: bool,
    interrupt: Frame,
    fire: Frame,
    frame: Frame,
) -> AfterHit {
    if ranged && crate::math::fmath::at_most_or_unordered(timer.get(), 0.0) {
        if !target {
            return AfterHit::Done;
        }
        let limit = if interrupt == Frame::NONE {
            fire
        } else {
            interrupt
        };
        if limit < frame {
            return AfterHit::Ready;
        }
    }
    AfterHit::Continue {
        check: interrupt != Frame::NONE && interrupt <= frame,
    }
}

/// What a hit did: its result (`+0x98` of a missile: 2 when a shield blocked it, else 1) and the
/// target of the executed action the attacker posts to its unit AI (notify 0x200 with 600).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Struck {
    pub result: crate::combat::missile::StrikeResult,
    pub executed: Option<ObjectId>,
}

/// `FUN_141084230`: a shield of the target (type 70, [`damage::shield`]) blocks the hit: the
/// target's unit AI gets notify 500 with the attacker, its hit points and its master's, and
/// nothing else happens (result 2). A unit of type 60 and up with an area effect blasts about the
/// hit's place with its target as the blast's main object ([`damage::blast`]) unless its charge
/// of type 3 is not its hit's, then empties that charge (slot 0x318 with 0). Else the unit tells
/// its unit AI the action executed (0x200, but for masters 73 and 559), then the 64-bit
/// Object-stream draw against the hit chance (a missile's master always hits: its missile,
/// `+0x83c`, is set); a hit is the target's slot 0xe8 ([`damage::take`]) at the elevation factor
/// (slot 0x1f0, from the origin's height) times the hit's 1. Not held: the last attack time
/// (`+0x648`), the alerts of another owner (slot 0x2e8) and the sounds.
pub(crate) fn strike(
    own: &mut Victim,
    victim: &mut Victim,
    blasted: &mut [Victim],
    hit: Hit,
    object_rng: &mut Well512,
) -> Result<Struck, GenieError> {
    if victim.kind == COMBAT && damage::shield(victim, hit.missile) {
        let master_hp = i32::from(own.master_hp.get());
        victim
            .damaged
            .push((Some(own.id), [own.id.get(), own.hp.get() as i32, master_hp]));
        return Ok(Struck {
            result: crate::combat::missile::StrikeResult::SHIELDED,
            executed: None,
        });
    }
    let a = own.source;
    let mut struck = Struck {
        result: crate::combat::missile::StrikeResult::HIT,
        executed: None,
    };
    if MISSILE <= a.kind
        && Distance::ZERO < a.area_effect
        && (a.charge_type != ChargeType::AREA || damage::charge_hit(&a, victim))
    {
        let main = victim.id;
        let mut targets: Vec<&mut Victim> = blasted.iter_mut().collect();
        targets.push(victim);
        let _ = damage::blast(own, hit.at, None, Some(main), &mut targets, 1.0)?;
        if a.charge_type == ChargeType::AREA {
            own.source.charge = Charge::ZERO;
        }
        return Ok(struck);
    }
    if !QUIET_SWINGS.contains(&own.master_id) {
        struck.executed = Some(victim.id);
    }
    let draw = object_rng.raw64();
    let r = (i64::from(draw as u32 & 0x7fff_ffff) * 100) / DRAW_SCALE;
    if i64::from(hit.chance.get()) < r && a.missile.is_none() {
        return Ok(struck);
    }
    // a building's slot 0x1f0 (FUN_1411cc890) scales the factor by its relics and a building
    // target's
    let factor = hit.from_z.map_or(1.0, |z| {
        let f = damage::elevation_factor(z, own.attack_elevation, victim);
        own.building.as_ref().map_or(f, |b| {
            let theirs = victim.building.as_ref().map_or(Relics::NONE, |t| t.relics);
            damage::building_elevation_factor(f, b.relics, victim.kind, theirs)
        })
    });
    let weapons = own.weapons.clone();
    let mut others: Vec<&mut Victim> = blasted.iter_mut().collect();
    damage::take(victim, own, &mut others, &weapons, factor, HitPoints::ZERO)?;
    Ok(struck)
}

/// The blast's damage so far that a trample's hit on `victim` leaves out (`0x141211933`): a
/// building's `+0xb90`, its main building's (`+0xbb8`, among `blasted`) for an annex; 0 for
/// another object.
pub(super) fn blast_so_far(victim: &Victim, blasted: &[Victim]) -> Result<HitPoints, GenieError> {
    // The livestock (58, `0x3a`) and farm (49, `0x31`) classes: the ally test spares an Attack on
    // one that was its unit's player's when the Attack took it ([`Attack::same_owner`]).
    // A building's master type (`'P'`), whose main building (`+0xbb8`) an area effect attacks.
    let Some(b) = victim.building.as_ref().filter(|_| victim.kind == BUILDING) else {
        return Ok(HitPoints::ZERO);
    };
    let Some(main) = b.main else {
        return Ok(b.blast_taken);
    };
    blasted
        .iter()
        .find(|v| v.id == main)
        .map(|m| {
            m.building
                .as_ref()
                .map_or(HitPoints::ZERO, |mb| mb.blast_taken)
        })
        .ok_or(crate::construction::build::NOT_HELD)
}
