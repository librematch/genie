//! The watched lists (`RGE_Player::watched`): the objects of other players each player watches,
//! so that one it loses sight of leaves a ghost ([`Ghost`](crate::vision::ghost::Ghost)).

use crate::map::Position;
use crate::map::Tile;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::GhostMode;
use crate::objects::masters::MasterKind;
use crate::players::PlayerId;
use crate::vision::VisionBits;
use crate::vision::ghost::AnnexLink;
use crate::vision::ghost::{Cell, GhostWatch, MASKED_KIND, Original, Watched};
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::slots::{CheckedIndexExt, PlayerSlotsExt};
use crate::{GenieError, GenieResult};
use std::collections::HashMap;

impl World {
    /// `FUN_141090430`: player `owner`'s watched list takes object `id` at tile (`x`, `y`) (an
    /// object of another owner, or one it holds already, is refused: [`GhostWatch::Refused`]). The
    /// entry keeps the tile's bits; it follows the tile, or for an object of type 50 and up with
    /// its footprint's UMV entry (`+0x904`) its own visibility mask (`+0x8f8`).
    pub(crate) fn watch(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
        at: Tile,
    ) -> GenieResult<GhostWatch> {
        let Some((player_id, unit)) = self.object(id) else {
            return Ok(GhostWatch::Refused);
        };
        if player_id != owner
            || self
                .players
                .player(owner)?
                .watched
                .iter()
                .any(|watched| watched.id == id)
        {
            return Ok(GhostWatch::Refused);
        }
        // the game takes the tile's word without a bounds test: every caller passes the
        // object's own tile, which is on the map
        let Some(tile) = self.tile_index(at) else {
            return Ok(GhostWatch::Refused);
        };
        let bits = self.tile_bits(at)?;
        let kind = unit.master(self.master_tables(), player_id)?.kind;
        let umv = self
            .spatial
            .sight
            .as_ref()
            .and_then(|visibility| visibility.seer(id))
            .map_or(0, |seer| seer.umv);
        let cell = if MASKED_KIND <= kind && umv != 0 {
            Cell::Mask(id)
        } else {
            Cell::Tile(tile)
        };
        self.players
            .player_mut(owner)?
            .watched
            .push(Watched { id, cell, bits });
        Ok(GhostWatch::Watched)
    }

    /// The tile (`(int)x`, `(int)y`) of a place, as the game's callers truncate it.
    pub(super) fn tile_at(position: Position) -> Tile {
        position.tile()
    }

    /// `FUN_141091310`: the watched list of player `owner` lets object `id` go (the last entry
    /// takes its place).
    pub(crate) fn unwatch(&mut self, owner: PlayerId, id: ObjectId) -> GenieResult<()> {
        let list = &mut self.players.player_mut(owner)?.watched;
        if let Some(index) = list.iter().position(|watched| watched.id == id) {
            let _ = list.swap_remove(index);
        }
        Ok(())
    }

    /// `FUN_1411cc980`: a building its owner watches: not of class 51 or 54, not desolid
    /// (`+0xc60`), not an annex (`+0xbb8`, but a gate's), not master 1808.
    pub(super) fn watches_building(&self, player_id: PlayerId, id: ObjectId) -> GenieResult<bool> {
        self.watches_building_at(player_id, id, AnnexLink::Made)
    }

    /// [`World::watches_building`], with the annex link made or, during a load before the rehook,
    /// not yet ([`AnnexLink`]): then an annex passes too.
    fn watches_building_at(
        &self,
        player_id: PlayerId,
        id: ObjectId,
        link: AnnexLink,
    ) -> GenieResult<bool> {
        let Some((_, unit)) = self.object(id) else {
            return Ok(false);
        };
        let master = unit.master(self.master_tables(), player_id)?;
        let annex = link == AnnexLink::Made && self.parts.building_of(unit.id).annex_of.is_some();
        Ok(
            !self.model.roles.of_class(master.class).contains(ClassRoles::UNWATCHED)
            && !self.parts.building_of(unit.id).construction.is_some_and(|construction| construction.desolid)
            // The master a building's watch leaves out (`FUN_1411cc980`).
            && (!annex || self.model.roles.of_class(master.class).contains(ClassRoles::GATE))
            && !self.model.roles.of_master(master.id).contains(MasterRoles::MULE_CART),
        )
    }

    /// A building whose construction started (`FUN_1411e5670`): its owner watches it
    /// ([`World::watches_building`]).
    pub(crate) fn watch_started(&mut self, player_id: PlayerId, id: ObjectId) -> GenieResult<()> {
        if !self.watches_building(player_id, id)? {
            return Ok(());
        }
        let Some((_, unit)) = self.object(id) else {
            return Ok(());
        };
        let _ = self.watch(player_id, id, Self::tile_at(unit.position()))?;
        Ok(())
    }

    /// The watched lists the game's load builds, which no save holds: each player's objects in
    /// the load's order (its live list, then its sleeping list) join their owner's list, an
    /// object whose master leaves ghosts once it is dead (`FUN_14119df60`, at its tile) and a
    /// building its owner watches ([`World::watches_building`]; the load constructor
    /// `FUN_1411ac170`, at what it follows, an annex's link not known yet); then the rehook lets
    /// every annex but a gate's go (`TRIBE_Building_Object::rehook`). A building's mask is empty
    /// at a load.
    pub fn build_watched(&mut self) -> GenieResult<()> {
        // each id's object as [`World::object`] finds it (the first player in order, its live list
        // before its sleeping list), looked up once: the loop changes only the watched lists
        let mut first: HashMap<ObjectId, (PlayerId, bool, ListIndex)> = HashMap::new();
        for (player_id, player) in PlayerId::each(self.players.len()).zip(&self.players) {
            for (sleeping, list) in [(false, &player.objects), (true, &player.sleeping)] {
                for (list_index, unit) in list.indexed() {
                    let _ = first
                        .entry(unit.id)
                        .or_insert((player_id, sleeping, list_index));
                }
            }
        }
        for watcher in PlayerId::each(self.players.len()) {
            self.players.player_mut(watcher)?.watched.clear();
            let ids: Vec<ObjectId> = self
                .players
                .player(watcher)?
                .objects
                .iter()
                .chain(&self.players.player(watcher)?.sleeping)
                .map(|unit| unit.id)
                .collect();
            for id in ids {
                let Some(&(player_id, sleeping, list_index)) = first.get(&id) else {
                    continue;
                };
                let unit = if sleeping {
                    &self.players.asleep(player_id, list_index)?
                } else {
                    &self.players.live(player_id, list_index)?
                };
                let master = unit.master(self.master_tables(), watcher)?;
                let (state, kind, ghosts, position) = (
                    unit.life.state,
                    master.kind,
                    master.ghost_on_death,
                    unit.position(),
                );
                let tile = Self::tile_at(position);
                // the static load's watch of a dead object (FUN_14119df60)
                if ghosts == GhostMode::LEAVES && ObjectState::Alive < state {
                    let _ = self.watch(watcher, id, tile)?;
                }
                // a building's load constructor (FUN_1411ac170, type 80 only): FUN_1411cc980
                // before the rehook links its annexes
                if kind == MasterKind::BUILDING
                    && self.watches_building_at(watcher, id, AnnexLink::Pending)?
                {
                    let _ = self.watch(watcher, id, tile)?;
                }
            }
            for list in [false, true] {
                let held = self.players.player(watcher)?;
                let objects = if list { &held.sleeping } else { &held.objects };
                let mut annexes: Vec<ObjectId> = Vec::new();
                for unit in objects
                    .iter()
                    .filter(|unit| self.parts.building_of(unit.id).annex_of.is_some())
                {
                    if !self
                        .model
                        .roles
                        .of_class(unit.master(self.master_tables(), watcher)?.class)
                        .contains(ClassRoles::GATE)
                    {
                        annexes.push(unit.id);
                    }
                }
                for id in annexes {
                    self.unwatch(watcher, id)?;
                }
            }
        }
        Ok(())
    }

    /// The watched list of player `watcher` after the player loop (`FUN_1410905e0`): an entry whose
    /// bits changed, of an object the world still holds, gives a ghost to every player, but its
    /// owner, who saw the cell and no longer does ([`World::make_ghost`], at the object's place),
    /// in the players' order, and takes the new bits; the last entry whose object is gone leaves
    /// the list (the last entry taking its place). A computer player's AI then hears of its ghost
    /// (its slot 0x2e8 with notify 0x20d), which is not modelled.
    pub(crate) fn watch_pass(&mut self, watcher: PlayerId) -> GenieResult<()> {
        let mut gone = None;
        for index in 0..self.players.player(watcher)?.watched.len() {
            let watched = *self.players.player(watcher)?.watched.checked(index)?;
            let bits = self.cell_bits(watched.cell)?;
            if bits == watched.bits {
                continue;
            }
            match self.object(watched.id) {
                None => gone = Some(watched.id),
                Some((owner, unit)) => {
                    let at = unit.position();
                    let original = Original {
                        id: watched.id,
                        owner,
                        state: unit.life.state,
                        master: unit.life.master_id,
                    };
                    // the visible bits the entry lost, of players 1 to 31 other than its own
                    let lost = watched.bits & !bits;
                    for player_id in PlayerId::each(32).skip(1) {
                        if lost.intersects(VisionBits::visible(player_id)) && player_id != watcher {
                            self.make_ghost(player_id, at, original)?;
                            if self
                                .players
                                .player(player_id)
                                .is_ok_and(|player| player.kind.is_computer())
                            {
                                return Err(GenieError::unsupported(
                                    "a computer player's notify of its ghost (0x20d)",
                                ));
                            }
                        }
                    }
                }
            }
            self.players
                .player_mut(watcher)?
                .watched
                .checked_mut(index)?
                .bits = bits;
        }
        if let Some(id) = gone {
            self.unwatch(watcher, id)?;
        }
        Ok(())
    }
}
