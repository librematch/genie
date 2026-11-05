//! The movement damage tasks (action type 152): their timer and reach, the bystanders they
//! strike and the hit.

use crate::GenieResult;
use crate::combat::abilities::{ALWAYS_REACHED, ONE, Own};
use crate::combat::damage::Victim;
use crate::combat::damage::lose;
use crate::combat::victims::victim_of;
use crate::construction::build::NOT_HELD;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Tile;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered, length};
use crate::movement::footprint_gap_squared;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::BlastDefense;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Task;
use crate::players::AllyTest;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::task_finder::allied;
use crate::world::Seconds;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::slots::PlayerSlotsExt;

/// A movement damage task's timer (`+0x9a4`, `0x141207758`): above 0 (not a NaN) it runs down by
/// the world's dt, never below 0. The task hits once it is at most 0 (`COMISS 0, seconds` / `JC`: a
/// NaN never hits).
pub fn movement_damage_due(timer: &mut Seconds, dt: Seconds) -> bool {
    let mut seconds = *timer;
    if Seconds::ZERO < seconds {
        seconds -= dt;
        *timer = seconds;
        if seconds < Seconds::ZERO {
            *timer = Seconds::ZERO;
            seconds = Seconds::ZERO;
        }
    }
    !below_or_unordered(0.0, seconds.get())
}

/// The tiles a movement damage task reaches about the object at `position` (`0x1412077e1`): the
/// truncated range (`CVTTSS2SI`) each way from its truncated place, from 0 (a negative result
/// is 0, `CMOVS`) to the map's width and height (`CMOVL`; both included). Returns the low and
/// the high corner.
#[must_use]
pub fn reach(position: Position, range: Distance, map: GridSize) -> (Tile, Tile) {
    let range_tiles = cvttss2si(range.get());
    let low = |value: f32| {
        let tile = cvttss2si(value).wrapping_sub(range_tiles);
        if tile < 0 { 0 } else { tile }
    };
    let high = |value: f32, size: i32| {
        let tile = cvttss2si(value).wrapping_add(range_tiles);
        if size < tile { size } else { tile }
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
    pub(crate) id: ObjectId,
    /// Its player (`+0x138`'s `+0x718`).
    pub(crate) player: PlayerId,
    /// The hitter's player counts it allied (`FUN_141663d90` with the ally flag).
    pub(crate) allied: bool,
    /// Its type (`+0xf`).
    pub(crate) kind: MasterKind,
    /// Its master's class (`+0x410`).
    pub(crate) class: ClassId,
    /// Its state (`+0x640`).
    pub(crate) state: ObjectState,
    /// Its master's `area_effect_object_level` (`+0x62b`).
    pub(crate) blast_defense: BlastDefense,
    /// Its place (`+0x3c8`, `+0x4c8`).
    pub(crate) position: Position,
    /// Its master's radii (`+0x5d8`, `+0x5dc`).
    pub(crate) radius: Footprint,
}

/// What a movement damage reads of its hitter: its id, its master's `area_effect_level`
/// (`+0x830`), its place and its master's radii.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hitter {
    /// The object.
    pub(crate) id: ObjectId,
    /// Its master's `area_effect_level` (`+0x830`).
    pub(crate) blast_level: BlastLevel,
    /// Its place.
    pub(crate) position: Position,
    /// Its master's radii.
    pub(crate) radius: Footprint,
}

/// Whether a movement damage of `range` hits `bystander` (`0x141207a29`..`0x141207c0d`): not the
/// hitter itself, not allied nor Gaia's, a building only of class 51 or 54, in state 2 at most,
/// reached by the hitter's level (its own level 2 or more, or the hitter's at most it), and its
/// footprint gap squared (`FUN_14118f8c0`) not past the range squared (`COMISS`/`JA`: a NaN is
/// within).
#[must_use]
pub fn struck(hitter: &Hitter, range: Distance, bystander: &Bystander, roles: &Roles) -> bool {
    if bystander.id == hitter.id || bystander.allied || bystander.player == PlayerId::GAIA {
        return false;
    }
    if bystander.kind == MasterKind::BUILDING
        && !roles
            .of_class(bystander.class)
            .contains(ClassRoles::STRUCK_BUILDING)
    {
        return false;
    }
    if ObjectState::Alive < bystander.state {
        return false;
    }
    if !(ALWAYS_REACHED <= bystander.blast_defense
        || hitter.blast_level.reaches(bystander.blast_defense))
    {
        return false;
    }
    let d2 = footprint_gap_squared(
        hitter.position,
        hitter.radius,
        bystander.position,
        bystander.radius,
    );
    at_most_or_unordered(d2.get(), range.squared().get())
}

/// The hit's end on an object left below 1 hit point (`COMISS 1.0, hp` / `JBE`: a NaN is not):
/// it has 0, and holds 0 (`FUN_14119d320` with 0, 0 and 1: at most its master's capacity, a
/// capacity below 0 taken as it is): [`HitEnd::Killed`], else [`HitEnd::Survived`].
pub fn finish(victim: &mut Victim) -> HitEnd {
    if victim.hp < HitPoints::new(ONE) {
        victim.hp = HitPoints::ZERO;
        victim.amount = Amount::ZERO;
        let capacity = Amount::of_master(victim.capacity);
        if capacity < victim.amount {
            victim.amount = capacity;
        }
        return HitEnd::Killed;
    }
    HitEnd::Survived
}

/// Whether a hit left its object below 1 hit point ([`finish`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum HitEnd {
    /// It was: it has 0 hit points and holds 0.
    Killed,
    /// It has 1 or more.
    Survived,
}

impl World {
    /// A movement damage task of the object at `(player_id, list_index)` as `owner`'s
    /// ([`movement_damage_due`]): once due, a combat unit (its master of type 70) that moves (slot
    /// 0x5c8, `FUN_1411583d0`: its velocity's length above 0) hits each object on the tiles of
    /// [`reach`], row by row, each tile's list in order, that it strikes ([`struck`]): the object
    /// loses the task's `work_val1` (slot 0xb0, [`crate::combat::damage::lose`]), the timer takes
    /// the task's `work_val2`, and one left below 1 hit point ends the hit ([`finish`]): a combat
    /// unit (type 70) counts killed by the owner (`FUN_1412005b0`, [`World::count_kill`]). A ghost
    /// on a tile is struck as an object of type 25 of its viewer, of the ghost master, in its
    /// state; the hit points it loses are not held. Not held: the alerts the hit's player hears
    /// (its slot 0x2e8 with 0x201, `FUN_14162dab0`: the human player's alerts and a computer
    /// player's AI), as for a hit ([`crate::combat::damage`]). A reach past the map's last row or
    /// column reads a tile the map does not have (`FUN_140f5ec30`'s null tile, whose count the
    /// routine reads: the game faults); the scan stops there.
    ///
    /// # Errors
    ///
    /// An object on a tile the slice does not hold, and what [`World::count_kill`] does not
    /// model.
    pub(super) fn movement_damage(
        &mut self,
        (player_id, list_index): (PlayerId, ListIndex),
        owner: PlayerId,
        master: Own,
        task: &Task,
    ) -> GenieResult<()> {
        let dt = self.dt_seconds;
        let unit = self.players.live_mut(player_id, list_index)?;
        if !movement_damage_due(
            &mut self
                .parts
                .combat_mut(unit.id)?
                .abilities
                .movement_damage_timer,
            dt,
        ) {
            return Ok(());
        }
        let moving = self
            .parts
            .mover_of(unit.id)
            .and_then(|mover| mover.velocity.as_ref())
            .is_some_and(|velocity| 0.0 < length(velocity.velocity));
        if master.kind != MasterKind::COMBAT || !moving {
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
                    let Some(bystander) = self.bystander(id, owner)? else {
                        continue;
                    };
                    if !struck(&hitter, range, &bystander, &self.model.roles) {
                        continue;
                    }
                    self.parts
                        .combat_mut(self.players.live(player_id, list_index)?.id)?
                        .abilities
                        .movement_damage_timer = Seconds::new(task.work_val2.get());
                    self.movement_hit(bystander, owner, HitPoints::new(task.work_val1.get()))?;
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
    ) -> GenieResult<Option<Bystander>> {
        let relations = &self.players.player(owner)?.relations;
        let allied =
            |player_id: PlayerId| allied(relations, owner, player_id, AllyTest::WithRelation4);
        if let Some((player_id, unit)) = self.object(id) {
            let master = unit.master(self.master_tables(), player_id)?;
            return Ok(Some(Bystander {
                id,
                player: player_id,
                allied: allied(player_id),
                kind: master.kind,
                class: master.class,
                state: unit.life.state,
                blast_defense: master.blast_defense,
                position: unit.position(),
                radius: master.radius,
            }));
        }
        for (player_id, player) in PlayerId::each(self.players.len()).zip(&self.players) {
            if let Some(ghost) = player.ghosts.iter().find(|ghost| ghost.id == id) {
                let master = self.master(player_id, self.model.slots.master.doppelganger)?;
                return Ok(Some(Bystander {
                    id,
                    player: player_id,
                    allied: allied(player_id),
                    kind: MasterKind::DOPPELGANGER,
                    class: master.class,
                    state: ghost.state,
                    blast_defense: master.blast_defense,
                    position: ghost.position,
                    radius: master.radius,
                }));
            }
        }
        Err(NOT_HELD)
    }

    /// A movement damage's hit of `bystander` by player `owner`'s hitter: `damage` hit points off
    /// (slot 0xb0, with its stingers' kill credits and its kill flag `+0x954`), then [`finish`]
    /// and a combat unit's kill count (`FUN_1412005b0` alone, whatever `+0x954` holds).
    pub(super) fn movement_hit(
        &mut self,
        bystander: Bystander,
        owner: PlayerId,
        damage: HitPoints,
    ) -> GenieResult<()> {
        if bystander.kind == MasterKind::DOPPELGANGER {
            return Ok(());
        }
        let Some((player_id, unit)) = self.object(bystander.id) else {
            return Err(NOT_HELD);
        };
        let master = unit.master(self.master_tables(), player_id)?;
        let mut victim = victim_of(
            (
                unit,
                self.parts.unit_ai.contains(unit.id),
                self.parts.building_of(unit.id),
                self.parts.combat_of(unit.id),
                self.parts.actions.of(unit.id),
                self.parts.unread.get(unit.id),
            ),
            master,
            &self.players.player(player_id)?.attributes,
            player_id,
            self.spatial.terrain.as_ref(),
            self.mode_680,
            &self.model.roles,
        );
        lose(&mut victim, damage);
        let killed = finish(&mut victim) == HitEnd::Killed;
        let (_, unit) = self.object_mut(bystander.id).ok_or(NOT_HELD)?;
        unit.life.hp = victim.hp;
        unit.life.amount = victim.amount;
        if let Some(armored) = victim.armored.as_ref() {
            let combat = self.parts.combat_mut(bystander.id)?;
            combat.hp_powerup = armored.hp_powerup;
            combat.kill_state = victim.kill_state;
        }
        // the loss's own counts first: its stingers' credits (slot 22)
        if !victim.kill_counts.is_empty() {
            let (player_id, unit) = self.object(bystander.id).ok_or(NOT_HELD)?;
            let master = unit.master(self.master_tables(), player_id)?.clone();
            self.count_kills(player_id, &master, &victim.kill_counts)?;
        }
        if killed && bystander.kind == MasterKind::COMBAT {
            self.count_kill(bystander.id, owner)?;
        }
        Ok(())
    }
}
