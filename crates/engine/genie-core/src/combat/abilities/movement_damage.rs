//! The movement damage tasks (action type 152): their timer and reach, the bystanders they
//! strike and the hit.

use crate::combat::abilities::{ALWAYS_REACHED, ONE, Own, STRUCK_BUILDING_CLASSES};
use crate::combat::damage::Victim;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Tile;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::BlastDefense;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Task;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::masters::master_type::DOPPELGANGER;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::PlayerSlots;
use crate::{GenieError, math::fmath};

/// A movement damage task's timer (`+0x9a4`, `0x141207758`): above 0 (not a NaN) it runs down by
/// the world's dt, never below 0. The task hits once it is at most 0 (`COMISS 0, t` / `JC`: a
/// NaN never hits).
pub fn movement_damage_due(timer: &mut Seconds, dt: Seconds) -> bool {
    let mut t = *timer;
    if Seconds::ZERO < t {
        t -= dt;
        *timer = t;
        if t < Seconds::ZERO {
            *timer = Seconds::ZERO;
            t = Seconds::ZERO;
        }
    }
    !fmath::below_or_unordered(0.0, t.get())
}

/// The tiles a movement damage task reaches about the object at `position` (`0x1412077e1`): the
/// truncated range (`CVTTSS2SI`) each way from its truncated place, from 0 (a negative result
/// is 0, `CMOVS`) to the map's width and height (`CMOVL`; both included). Returns the low and
/// the high corner.
#[must_use]
pub fn reach(position: Position, range: Distance, map: GridSize) -> (Tile, Tile) {
    let r = crate::math::conversions::cvttss2si(range.get());
    let low = |v: f32| {
        let d = crate::math::conversions::cvttss2si(v).wrapping_sub(r);
        if d < 0 { 0 } else { d }
    };
    let high = |v: f32, size: i32| {
        let d = crate::math::conversions::cvttss2si(v).wrapping_add(r);
        if size < d { size } else { d }
    };
    (
        Tile::new(low(position.x()), low(position.y())),
        Tile::new(
            high(position.x(), map.width()),
            high(position.y(), map.height()),
        ),
    )
}

/// What a movement damage reads of an object on a tile it reaches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bystander {
    /// The object.
    pub id: ObjectId,
    /// Its player (`+0x138`'s `+0x718`).
    pub player: PlayerId,
    /// The hitter's player counts it allied (`FUN_141663d90` with the ally flag).
    pub allied: bool,
    /// Its type (`+0xf`).
    pub kind: MasterKind,
    /// Its master's class (`+0x410`).
    pub class: ClassId,
    /// Its state (`+0x640`).
    pub state: ObjectState,
    /// Its master's `area_effect_object_level` (`+0x62b`).
    pub blast_defense: BlastDefense,
    /// Its place (`+0x3c8`, `+0x4c8`).
    pub position: Position,
    /// Its master's radii (`+0x5d8`, `+0x5dc`).
    pub radius: Footprint,
}

/// What a movement damage reads of its hitter: its id, its master's `area_effect_level`
/// (`+0x830`), its place and its master's radii.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hitter {
    /// The object.
    pub id: ObjectId,
    /// Its master's `area_effect_level` (`+0x830`).
    pub blast_level: BlastLevel,
    /// Its place.
    pub position: Position,
    /// Its master's radii.
    pub radius: Footprint,
}

/// Whether a movement damage of `range` hits `b` (`0x141207a29`..`0x141207c0d`): not the hitter
/// itself, not allied nor Gaia's, a building only of class 51 or 54, in state 2 at most, reached
/// by the hitter's level (its own level 2 or more, or the hitter's at most it), and its
/// footprint gap squared (`FUN_14118f8c0`) not past the range squared (`COMISS`/`JA`: a NaN is
/// within).
#[must_use]
pub fn struck(hitter: &Hitter, range: Distance, b: &Bystander) -> bool {
    if b.id == hitter.id || b.allied || b.player == PlayerId::GAIA {
        return false;
    }
    if b.kind == BUILDING && !STRUCK_BUILDING_CLASSES.contains(&b.class) {
        return false;
    }
    if ObjectState::Alive < b.state {
        return false;
    }
    if !(ALWAYS_REACHED <= b.blast_defense || hitter.blast_level.reaches(b.blast_defense)) {
        return false;
    }
    let d2 = crate::movement::footprint_gap_squared(
        hitter.position,
        hitter.radius,
        b.position,
        b.radius,
    );
    fmath::at_most_or_unordered(d2.get(), range.squared().get())
}

/// The hit's end on an object left below 1 hit point (`COMISS 1.0, hp` / `JBE`: a NaN is not):
/// it has 0, and holds 0 (`FUN_14119d320` with 0, 0 and 1: at most its master's capacity, a
/// capacity below 0 taken as it is). Returns whether it was.
pub fn finish(v: &mut Victim) -> bool {
    if v.hp < HitPoints::new(ONE) {
        v.hp = HitPoints::ZERO;
        v.amount = Amount::ZERO;
        let capacity = Amount::of_master(v.capacity);
        if capacity < v.amount {
            v.amount = capacity;
        }
        return true;
    }
    false
}

impl World {
    /// A movement damage task of the object at `(p, i)` as `owner`'s ([`movement_damage_due`]):
    /// once due, a combat unit (its master of type 70) that moves (slot 0x5c8, `FUN_1411583d0`:
    /// its velocity's length above 0) hits each object on the tiles of [`reach`], row by row,
    /// each tile's list in order, that it strikes ([`struck`]): the object loses the task's
    /// `work_val1` (slot 0xb0, [`crate::combat::damage::lose`]), the timer takes the task's `work_val2`,
    /// and one left below 1 hit point ends the hit ([`finish`]): a combat unit (type 70) counts
    /// killed by the owner (`FUN_1412005b0`, [`World::count_kill`]). A ghost on a tile is struck
    /// as an object of type 25 of its viewer, of the ghost master, in its state; the hit points
    /// it loses are not held. Not held: the alerts the hit's player hears (its slot 0x2e8 with
    /// 0x201, `FUN_14162dab0`: the human player's alerts and a computer player's AI), as for a
    /// hit ([`crate::combat::damage`]). A reach past the map's last row or column reads a tile the map
    /// does not have (`FUN_140f5ec30`'s null tile, whose count the routine reads: the game
    /// faults); the scan stops there.
    ///
    /// # Errors
    ///
    /// An object on a tile the slice does not hold, and what [`World::count_kill`] does not
    /// model.
    pub(super) fn movement_damage(
        &mut self,
        (p, i): (PlayerId, ListIndex),
        owner: PlayerId,
        master: Own,
        task: &Task,
    ) -> Result<(), GenieError> {
        let dt = self.dt_seconds;
        let unit = self.players.live_mut(p, i)?;
        if !movement_damage_due(&mut unit.combat.abilities.movement_damage_timer, dt) {
            return Ok(());
        }
        let moving = unit
            .body
            .mover()
            .and_then(|m| m.velocity.as_ref())
            .is_some_and(|v| 0.0 < fmath::length(v.velocity));
        if master.kind != COMBAT || !moving {
            return Ok(());
        }
        let hitter = Hitter {
            id: unit.id,
            blast_level: master.blast_level,
            position: unit.position(),
            radius: master.radius,
        };
        let range = task.work_range;
        let (low, high) = reach(unit.position(), range, self.spatial.map);
        for y in low.y()..=high.y() {
            for x in low.x()..=high.x() {
                if x < 0 || y < 0 || self.spatial.map.width() <= x || self.spatial.map.height() <= y
                {
                    return Ok(());
                }
                for id in self.spatial.tile_list(Tile::new(x, y)).to_vec() {
                    let Some(b) = self.bystander(id, owner)? else {
                        continue;
                    };
                    if !struck(&hitter, range, &b) {
                        continue;
                    }
                    self.players
                        .live_mut(p, i)?
                        .combat
                        .abilities
                        .movement_damage_timer = Seconds::new(task.work_val2.get());
                    self.movement_hit(b, owner, HitPoints::new(task.work_val1.get()))?;
                }
            }
        }
        Ok(())
    }

    /// What a movement damage reads of the object `id` on a tile, as player `owner`'s hitter
    /// sees it: a ghost as its viewer's object of the ghost master, any other as the world holds
    /// it (its hitter's update wrote its hits back where they ran).
    ///
    /// # Errors
    ///
    /// An object the slice does not hold ([`crate::construction::build::NOT_HELD`]).
    pub(super) fn bystander(
        &self,
        id: ObjectId,
        owner: PlayerId,
    ) -> Result<Option<Bystander>, GenieError> {
        let relations = &self.players.player(owner)?.relations;
        let allied = |q: PlayerId| crate::unit_ai::task_finder::allied(relations, owner, q, true);
        if let Some((q, u)) = self.object(id) {
            let m = u.master(self.master_tables(), q)?;
            return Ok(Some(Bystander {
                id,
                player: q,
                allied: allied(q),
                kind: m.kind,
                class: m.class,
                state: u.life.state,
                blast_defense: m.blast_defense,
                position: u.position(),
                radius: m.radius,
            }));
        }
        for (q, player) in PlayerId::each(self.players.len()).zip(&self.players) {
            if let Some(g) = player.ghosts.iter().find(|g| g.id == id) {
                let m = self.master(q, crate::vision::ghost::GHOST_MASTER)?;
                return Ok(Some(Bystander {
                    id,
                    player: q,
                    allied: allied(q),
                    kind: DOPPELGANGER,
                    class: m.class,
                    state: g.state,
                    blast_defense: m.blast_defense,
                    position: g.position,
                    radius: m.radius,
                }));
            }
        }
        Err(crate::construction::build::NOT_HELD)
    }

    /// A movement damage's hit of `b` by player `owner`'s hitter: `damage` hit points off (slot
    /// 0xb0), then [`finish`] and a combat unit's kill count.
    pub(super) fn movement_hit(
        &mut self,
        b: Bystander,
        owner: PlayerId,
        damage: HitPoints,
    ) -> Result<(), GenieError> {
        if b.kind == DOPPELGANGER {
            return Ok(());
        }
        let Some((q, u)) = self.object(b.id) else {
            return Err(crate::construction::build::NOT_HELD);
        };
        let m = u.master(self.master_tables(), q)?;
        let mut v = crate::combat::victims::victim_of(
            u,
            m,
            &self.players.player(q)?.attributes,
            q,
            self.spatial.terrain.as_ref(),
            self.mode_680,
        );
        crate::combat::damage::lose(&mut v, damage);
        let killed = finish(&mut v);
        let (_, u) = self
            .object_mut(b.id)
            .ok_or(crate::construction::build::NOT_HELD)?;
        u.life.hp = v.hp;
        u.life.amount = v.amount;
        if let Some(a) = v.armored.as_ref() {
            u.combat.hp_powerup = a.hp_powerup;
        }
        if killed && b.kind == COMBAT {
            self.count_kill(b.id, owner)?;
        }
        Ok(())
    }
}
