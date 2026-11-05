//! The fog's ghosts (`RGE_Doppleganger_Object`, type 25, each player's master 243 "DOPL"): what a
//! player keeps seeing of an object it lost sight of. An object whose master leaves ghosts
//! (`create_doppelganger_on_death` 2, a mine or a tree say) leaves one at its death for every
//! player who explored its tile but does not see it (`FUN_14118e990`), and its owner's watched
//! list (`player+0x78`) then follows the tile: a player who loses sight of it gets one too
//! (`FUN_1410905e0`, after the player loop). A building in its owner's list is followed by its own
//! visibility mask (`+0x8f8`). A ghost goes once its viewer sees its tile again (`FUN_141090000`,
//! the destroy slot 0xd0), two updates later it leaves for the world's pool of type 25, and the
//! next ghost takes it back; a ghost takes the world's next reusable id otherwise, which the
//! recycled missiles share.
//!
//! 158041: the list's add `FUN_141090430` and removal `FUN_141091310`; a ghost's making
//! `FUN_1410cfe80` (constructor `FUN_14108dae0`, the pool's `FUN_1410909f0`), its link to the
//! original `FUN_14108f9b0`, the twin check `FUN_1410928a0`, its update `FUN_141090000`, its
//! destroy `FUN_14118f1e0` and its leaving `FUN_141090ad0`.

use crate::map::Position;
use crate::map::Tile;
use crate::objects::ListFate;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::GhostMode;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::players::PlayerId;
use crate::players::PlayerSet;
use crate::vision::VisionBits;
use crate::world::World;
use crate::world::layer::MasterRoles;
use crate::world::slots::{CheckedIndexExt, PlayerSlotsExt};
use crate::{GenieError, GenieResult};

/// The type from which an object's entry and its ghost follow its footprint's UMV entry
/// (`FUN_141090430`, `FUN_14108f9b0`: master type above 0x31).
pub(super) const MASKED_KIND: MasterKind = MasterKind::RGE_COMBAT;

/// What an entry of a watched list follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cell {
    /// A tile of the unified map (its index).
    Tile(usize),
    /// A building's own visibility mask (`+0x8f8`, the registry's), for one of type 50 and up
    /// with its footprint's UMV entry (`+0x904`).
    Mask(ObjectId),
}

/// An entry of a player's watched list (32 bytes): the object, what it follows and the visible
/// and explored bits last seen there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Watched {
    /// The object (`+0x18`, and its pointer `+0`).
    pub(crate) id: ObjectId,
    /// What it follows (`+8`).
    pub(crate) cell: Cell,
    /// The bits last seen (`+0x10`).
    pub(crate) bits: VisionBits,
}

/// Whether a player's watched list took an object ([`World::watch`], `FUN_141090430`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub(crate) enum GhostWatch {
    /// The list holds it now.
    Watched,
    /// An object of another owner, or one the list holds already.
    Refused,
}

/// What a ghost's link (`FUN_14108f9b0`) reads of its original.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Original {
    /// Its id (`+8`).
    pub id: ObjectId,
    /// Its owner (`+0x138`'s `+0x718`).
    pub owner: PlayerId,
    /// Its state (`+0x640`).
    pub state: ObjectState,
    /// Its master (`+0xa0`).
    pub master: MasterId,
}

/// A ghost.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ghost {
    /// Its id (a reusable one, negative).
    pub id: ObjectId,
    /// The object it shows while that one has not left (`+0x7c0`).
    pub original: Option<ObjectId>,
    /// The original's id (`+0x7f8`, the save's `long_val`), −1 once it is gone.
    pub original_id: Option<ObjectId>,
    /// The original's master (`+0x7d0`), while it is known.
    pub original_master: Option<MasterId>,
    /// The original's owner (`+0x7d8`, the save's `doppled_player`).
    pub original_owner: PlayerId,
    /// Its place.
    pub position: Position,
    /// Its tile (`+0x7dc`, `+0x7e0`).
    pub tile: Tile,
    /// The original's UMV entry (`+0x7f6`, the save's `Multi_Unified_Points`): the tiles of a
    /// building's footprint it follows besides its own; 0 for none.
    pub footprint: u8,
    /// The bits last seen (`+0x7e8`, not saved).
    pub bits: VisionBits,
    /// The players who cannot see it (`+0x7f0`, the save's `CantSeeBits`).
    pub cant_see: PlayerSet,
    /// `+0x7f7`: its viewer saw its tile while the original's mask had not.
    pub seen: bool,
    /// Its state: alive, then removed once destroyed, gone, and it leaves.
    pub state: ObjectState,
}

/// Whether a building's annex link is made, or not yet (during a load, before the rehook).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum AnnexLink {
    /// It is made.
    Made,
    /// Not yet.
    Pending,
}

impl World {
    /// The vision mask of player `player_id` (`player+0x2a8`, [`crate::vision`]).
    fn vision_mask(&self, player_id: PlayerId) -> VisionBits {
        self.spatial.vision(player_id).visible
    }

    /// The unified map's visible and explored bits at `tile` (`map+0x5e30`, masked); 0 off the
    /// map or without the players' sight.
    pub(super) fn tile_bits(&self, tile: Tile) -> GenieResult<VisionBits> {
        let Some(visibility) = self.spatial.sight.as_ref() else {
            return Ok(VisionBits::NONE);
        };
        let Some(index) = visibility.size.index(tile) else {
            return Ok(VisionBits::NONE);
        };
        Ok(*visibility.unified.checked(index)? & VisionBits::OBJECT_MASK)
    }

    /// `tile`'s index in the unified map.
    pub(super) fn tile_index(&self, tile: Tile) -> Option<usize> {
        self.spatial.sight.as_ref()?.size.index(tile)
    }

    /// What a cell shows now.
    pub(super) fn cell_bits(&self, cell: Cell) -> GenieResult<VisionBits> {
        match cell {
            Cell::Tile(index) => Ok(self
                .spatial
                .sight
                .as_ref()
                .and_then(|visibility| visibility.unified.get(index))
                .map_or(VisionBits::NONE, |&vision_bits| {
                    vision_bits & VisionBits::OBJECT_MASK
                })),
            Cell::Mask(id) => {
                let registry = self
                    .spatial
                    .sight
                    .as_ref()
                    .and_then(|visibility| visibility.registry.as_ref())
                    .ok_or(GenieError::unsupported(
                        "a watched building without the objects-in-sight registry",
                    ))?;
                Ok(registry
                    .entries
                    .get(id)
                    .map_or(VisionBits::NONE, |entry| entry.mask)
                    & VisionBits::OBJECT_MASK)
            }
        }
    }

    /// `FUN_14118e990`, at the death of object `id` (the static `set_state` 3, while the game
    /// state is not 2): for an id of 0 and up whose master leaves ghosts (`+0x682` 2), every
    /// player who explored its tile but does not see it, Gaia aside, gets a ghost of it
    /// ([`World::make_ghost`]), in the players' order; the first bit past the players ends it.
    /// Then its owner's list watches it at its tile ([`World::watch`]).
    pub(crate) fn ghosts_at_death(&mut self, _owner: usize, id: ObjectId) -> GenieResult<()> {
        let Some((owner, unit)) = self.object(id) else {
            return Ok(());
        };
        let original = Original {
            id,
            owner,
            state: unit.life.state,
            master: unit.life.master_id,
        };
        let position = unit.position();
        if id.is_recyclable()
            || unit.master(self.master_tables(), owner)?.ghost_on_death != GhostMode::LEAVES
        {
            return Ok(());
        }
        let tile = Self::tile_at(position);
        let bits = self.tile_bits(tile)?;
        // each player past Gaia that explored the tile and does not see it, in order
        for player_id in PlayerId::each(self.players.len()).skip(1) {
            if bits.intersects(VisionBits::explored(player_id))
                && !bits.intersects(VisionBits::visible(player_id))
            {
                self.make_ghost(player_id, position, original)?;
            }
        }
        let _ = self.watch(owner, id, tile)?;
        Ok(())
    }

    /// The viewer `viewer`'s ghost of `original` at `at` (its DOPL master's slot 0xf8,
    /// `FUN_1410cfe80`). A recyclable master's pool of type 25 gives one back (`FUN_141546870`,
    /// the list's head; its id kept, `FUN_1410909f0`); else a new one (`FUN_14108dae0`) takes an
    /// id by its master (`FUN_1411a45a0`): the world's next reusable one (`FUN_1415301b0`) for a
    /// recyclable master, else the next object id (`FUN_14152fd70`). It links its original
    /// (`FUN_14108f9b0`: its id, owner, master and, for a master of type 50 and up, its UMV
    /// entry; an original past state 6 is forgotten, its master kept), joins its tile's list and
    /// its viewer's ghosts after the twin check ([`World::check_twins`]). Both callers pass an
    /// object they hold, so the link's branch for no original (`param_2` 0) is not reached.
    pub(crate) fn make_ghost(
        &mut self,
        viewer: PlayerId,
        at: Position,
        original: Original,
    ) -> GenieResult<()> {
        let recyclable = self
            .master(viewer, self.model.slots.master.doppelganger)?
            .recyclable;
        let kind = self.master(original.owner, original.master)?.kind;
        let umv = self
            .spatial
            .sight
            .as_ref()
            .and_then(|visibility| visibility.seer(original.id))
            .map_or(0, |seer| seer.umv);
        let pooled = if recyclable.recycled() {
            self.recycling.ghosts.pop()
        } else {
            None
        };
        let id = match pooled {
            Some(id) => id,
            None if recyclable.recycled() => {
                let id = self.recycling.next_id;
                self.recycling.next_id = id.predecessor();
                id
            }
            None => self.take_object_id(viewer, self.model.slots.master.doppelganger),
        };
        let gone = ObjectState::Undead < original.state;
        let mut ghost = Ghost {
            id,
            original: (!gone).then_some(original.id),
            original_id: if gone { None } else { Some(original.id) },
            original_master: Some(original.master),
            original_owner: original.owner,
            position: at,
            tile: Self::tile_at(at),
            footprint: if MASKED_KIND <= kind { umv } else { 0 },
            bits: VisionBits::NONE,
            cant_see: PlayerSet::NONE,
            seen: false,
            state: ObjectState::Alive,
        };
        self.spatial.insert_ghost(id, at);
        self.check_twins(viewer, &mut ghost)?;
        self.players.player_mut(viewer)?.ghosts.push(ghost);
        Ok(())
    }

    /// The twin check (`FUN_1410928a0`, inlined in `FUN_14108dae0`) of the viewer `viewer`'s
    /// new `ghost`: each other ghost on the tile of its place (`(int)x`, `(int)y`), in state 2,
    /// of the same original id: the viewer's own is destroyed (slot 0xd0), another viewer's and
    /// the new ghost note each other's player as one who cannot see it (`+0x7f0`). The game walks
    /// the tile's list; the effects are ORs and destroys that change no other ghost's test, so the
    /// players' lists in their order give the same.
    fn check_twins(&mut self, viewer: PlayerId, ghost: &mut Ghost) -> GenieResult<()> {
        let tile = Self::tile_at(ghost.position);
        for player_id in PlayerId::each(self.players.len()) {
            for index in 0..self.players.player(player_id)?.ghosts.len() {
                let player = *self.players.player(player_id)?.ghosts.checked(index)?;
                if Self::tile_at(player.position) != tile
                    || player.id == ghost.id
                    || player.original_id != ghost.original_id
                    || player.state != ObjectState::Alive
                {
                    continue;
                }
                if player_id == viewer {
                    self.destroy_ghost(player_id, index)?;
                } else {
                    self.players
                        .player_mut(player_id)?
                        .ghosts
                        .checked_mut(index)?
                        .cant_see |= PlayerSet::of(viewer);
                    ghost.cant_see |= PlayerSet::of(player_id);
                }
            }
        }
        Ok(())
    }

    /// The destroy (slot 0xd0, `FUN_14118f1e0`) of ghost `index` of player `player_id`: below state
    /// 7, its hit points go and it goes to state 7.
    fn destroy_ghost(&mut self, player_id: PlayerId, index: usize) -> GenieResult<()> {
        let ghost = self
            .players
            .player_mut(player_id)?
            .ghosts
            .checked_mut(index)?;
        if ghost.state < ObjectState::Removed {
            ghost.state = ObjectState::Removed;
        }
        Ok(())
    }

    /// The ghosts of player `player_id` in their list's order (`FUN_141166550` on `+0x40`), each
    /// one's update (`FUN_141090000`): its static step (state 7 goes to 8, a ghost in state 8
    /// leaves); one its viewer saw while the original's mask had not is destroyed; its original
    /// past state 6 is forgotten; then, its tile's bits (and its footprint's) changed, every player
    /// sharing the original owner's sight notes itself in the ghost's `cant_see` (`+0x7f0`), and a
    /// ghost its viewer sees in state 2, unless one of the players sharing the viewer's sight does
    /// not, is destroyed (or marked seen, for a building's ghost whose original the viewer's mask
    /// misses and whose tile it does not see). A ghost that leaves (`FUN_141090ad0`) goes off its
    /// tile, the last ghost taking its place in the list, and to the pool of type 25
    /// (`RGE_Static_Object::recycle_out_of_game`), or for a master that is not recyclable it is
    /// deleted. The viewer's `+0x2b0` table is not held (empty).
    pub(crate) fn update_ghosts(&mut self, player_id: PlayerId) -> GenieResult<()> {
        let mut index = 0;
        while index < self.players.player(player_id)?.ghosts.len() {
            if self.update_ghost(player_id, index)? == ListFate::Leaves {
                let ghost = self
                    .players
                    .player_mut(player_id)?
                    .ghosts
                    .swap_remove(index);
                if ghost.original_master.is_some_and(|master_id| {
                    self.model
                        .roles
                        .of_master(master_id)
                        .contains(MasterRoles::TERRAIN_ORIGINAL)
                }) {
                    return Err(GenieError::unsupported(
                        "a farm's ghost leaving (the local player's terrain, FUN_141092190)",
                    ));
                }
                self.spatial.remove_ghost(ghost.id, ghost.position);
                if (self
                    .master(player_id, self.model.slots.master.doppelganger)?
                    .recyclable)
                    .recycled()
                {
                    self.recycling.ghosts.push(ghost.id);
                }
                continue;
            }
            index += 1;
        }
        Ok(())
    }

    /// An original past state 6 is forgotten with its master; without one only the id goes.
    fn forget_original(
        &mut self,
        player_id: PlayerId,
        index: usize,
        original: Option<ObjectId>,
    ) -> GenieResult<()> {
        let live = original.is_some_and(|object_id| {
            self.object(object_id)
                .is_some_and(|(_, unit)| unit.life.state < ObjectState::Removed)
        });
        let ghost = self
            .players
            .player_mut(player_id)?
            .ghosts
            .checked_mut(index)?;
        if original.is_none() {
            ghost.original_id = None;
        } else if !live {
            ghost.original = None;
            ghost.original_master = None;
            ghost.original_id = None;
        }
        Ok(())
    }

    /// The bits of the ghost's tile and, for a building's ghost, of its footprint's tiles.
    fn ghost_bits(&self, ghost: &Ghost) -> GenieResult<VisionBits> {
        let (px, py) = (ghost.position.x(), ghost.position.y());
        let mut bits = self.tile_bits(ghost.tile)?;
        if ghost.footprint != 0 {
            let offsets = self
                .spatial
                .sight
                .as_ref()
                .and_then(|visibility| visibility.extents.get(usize::from(ghost.footprint) - 1))
                .map(|extent| extent.offsets.clone())
                .ok_or(GenieError::unsupported(
                    "a building's ghost without its UMV entry",
                ))?;
            for pair in offsets {
                let (ox, oy) = (pair.dx(), pair.dy());
                bits |= self.tile_bits(Tile::new(px as i32 + ox, py as i32 + oy))?;
            }
        }
        Ok(bits)
    }

    /// Whether the ghost is unseen by a player that sees its original's tile no more, and the
    /// players who cannot see it: those sharing the original owner's sight note themselves.
    fn ghost_viewers(
        &self,
        player: PlayerId,
        ghost: &Ghost,
        bits: VisionBits,
    ) -> (bool, PlayerSet) {
        let mut unseen = false;
        let mut cant_see = ghost.cant_see;
        for player_id in PlayerId::each(self.players.len()).skip(1) {
            let mask = self.vision_mask(player_id);
            if mask.intersects(VisionBits::visible(ghost.original_owner)) {
                cant_see |= PlayerSet::of(player_id);
            } else if mask.intersects(VisionBits::visible(player)) && !bits.intersects(mask) {
                unseen = true;
            }
        }
        (unseen, cant_see)
    }

    /// Whether the viewer `player_id` has now seen the ghost's tile while its original's mask had
    /// not, and whether the ghost is destroyed for it.
    fn ghost_seen(
        &self,
        player_id: PlayerId,
        ghost: &Ghost,
        bits: VisionBits,
        unseen: bool,
    ) -> GenieResult<(bool, bool)> {
        let viewer = self.vision_mask(player_id);
        let mut seen = ghost.seen;
        let mut destroy = false;
        if bits.intersects(viewer) && !unseen && ghost.state < ObjectState::Dying {
            if ghost.footprint != 0
                && let Some(object_id) = ghost.original
            {
                let original_mask = self.cell_bits(Cell::Mask(object_id))?;
                if !original_mask.intersects(viewer)
                    && !self.tile_bits(ghost.tile)?.intersects(viewer)
                {
                    seen = true;
                }
            }
            destroy = !seen;
        }
        Ok((seen, destroy))
    }

    fn update_ghost(&mut self, player_id: PlayerId, index: usize) -> GenieResult<ListFate> {
        let player_ghost = *self.players.player(player_id)?.ghosts.checked(index)?;
        let leaves = match player_ghost.state {
            ObjectState::Removed => {
                self.players
                    .player_mut(player_id)?
                    .ghosts
                    .checked_mut(index)?
                    .state = ObjectState::Gone;
                ListFate::Stays
            }
            ObjectState::Gone => ListFate::Leaves,
            _ => ListFate::Stays,
        };
        if player_ghost.seen && player_ghost.state < ObjectState::Dying {
            self.destroy_ghost(player_id, index)?;
            return Ok(leaves);
        }
        self.forget_original(player_id, index, player_ghost.original)?;
        let player = *self.players.player(player_id)?.ghosts.checked(index)?;
        let bits = self.ghost_bits(&player)?;
        if bits == player.bits {
            return Ok(leaves);
        }
        let (unseen, cant_see) = self.ghost_viewers(player_id, &player, bits);
        let (seen, destroy) = self.ghost_seen(player_id, &player, bits, unseen)?;
        let ghost = self
            .players
            .player_mut(player_id)?
            .ghosts
            .checked_mut(index)?;
        ghost.seen = seen;
        ghost.cant_see = cant_see;
        ghost.bits = bits;
        if destroy {
            self.destroy_ghost(player_id, index)?;
        }
        Ok(leaves)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use crate::construction::BuilderCount;
    use crate::construction::MainBuildingId;
    #[cfg(test)]
    use crate::construction::build::Construction;
    use crate::map::Distance;
    use crate::map::GridSize;
    #[cfg(test)]
    use crate::map::Snow;
    use crate::map::TileOffset;
    #[cfg(test)]
    use crate::map::spatial::Obstruction;
    use crate::objects::ListIndex;
    use crate::objects::MasterHitPoints;
    #[cfg(test)]
    use crate::objects::layers::Static;
    use crate::objects::masters::MasterKind;
    use crate::objects::masters::ObstructionClass;
    use crate::objects::masters::ObstructionKind;
    use crate::objects::masters::RecycleMode;
    use crate::objects::masters::{Master, Masters};
    use crate::objects::unit::Unit;
    use crate::ownership::CaptureKind;
    use crate::players::game_status::GameStatus;
    #[cfg(test)]
    use crate::players::{ByPlayer, PlayerKind};
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::testing::class;
    use crate::testing::ids;
    use crate::testing::ids::HOUSE;
    use crate::testing::ids::MASTER_1808;
    #[cfg(test)]
    use crate::vision::own_masks;
    #[cfg(test)]
    use crate::vision::registry::{Entry, Registry};
    #[cfg(test)]
    use crate::vision::visibility::Extent;
    use crate::vision::visibility::{Seer, Visibility};
    #[cfg(test)]
    use crate::world::FrameSeed;
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::Seconds;
    use crate::world::WholeSeconds;
    use crate::world::WorldTime;
    use std::sync::Arc;

    /// A gold mine's master (it leaves ghosts), a house's and the ghosts' own.
    const MINE: MasterId = MasterId::named(66);
    const TREE: MasterId = MasterId::named(411);

    fn masters(recyclable: RecycleMode) -> Masters {
        Masters::new([
            Master {
                id: ids::DOPPELGANGER,
                // The type of a ghost (`+0xf`).
                kind: MasterKind::DOPPELGANGER,
                recyclable,
                ..Master::default()
            },
            Master {
                id: MINE,
                kind: MasterKind::MOVING,
                ghost_on_death: GhostMode::LEAVES,
                ..Master::default()
            },
            Master {
                id: HOUSE,
                kind: MasterKind::BUILDING,
                class: class::BUILDING,
                ghost_on_death: GhostMode::LEAVES,
                ..Master::default()
            },
            Master {
                id: TREE,
                kind: MasterKind::MOVING,
                ..Master::default()
            },
        ])
    }

    fn object(id: i32, master: MasterId, state: ObjectState, position: Position) -> Unit {
        let mut unit = Unit {
            id: object_id(id),
            place: Static::at(position),
            ..Unit::default()
        };
        unit.life.master_id = master;
        unit.life.state = state;
        unit
    }

    /// Gaia and two players on a 4×4 map, each with its masters, Gaia's mine 5 at (1.5, 2.5)
    /// and player 1's house 9 at (2.5, 1.5) (its footprint's UMV entry 1).
    fn world(recyclable: RecycleMode) -> World {
        let players = (0..3).map(|_| Player::default()).collect();
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(0),
            PlayerId::new(0),
            vec![Millis::ZERO; 3],
            players,
        );
        world.model.masters = (0..3).map(|_| masters(recyclable)).map(Arc::new).collect();
        let _ = world
            .push_live(
                PlayerId::new(0),
                object(5, MINE, ObjectState::Alive, Position::new(1.5, 2.5, 0.0)),
            )
            .unwrap();
        let _ = world
            .push_live(
                PlayerId::new(1),
                object(9, HOUSE, ObjectState::Alive, Position::new(2.5, 1.5, 0.0)),
            )
            .unwrap();
        let mut sight = Visibility::new(
            GridSize::new(4, 4),
            Vec::new(),
            vec![VisionBits::NONE; 16],
            Vec::new(),
            0,
            Vec::new(),
        );
        sight.add_seer(
            object_id(9),
            Seer {
                player: PlayerId::new(1),
                master: HOUSE,
                los: Distance::new(2.0),
                los_powerup: Distance::ZERO,
                state: ObjectState::Alive,
                explored: true,
                building: None,
                umv: 1,
                capture: CaptureKind::None,
                kind: MasterKind::BUILDING,
            },
        );
        world.spatial.sight = Some(sight);
        world.recycling.next_id = object_id(-10);
        world.next_object_id = object_id(100);
        world
    }

    fn set_tile(world: &mut World, x: usize, y: usize, bits: u64) {
        world.spatial.sight.as_mut().unwrap().unified[y * 4 + x] =
            VisionBits::from_bits_retain(bits);
    }

    #[test]
    fn watched_entry_keeps_the_tiles_bits_and_building_with_umv_entry_follows_its_mask() {
        // FUN_141090430: the tile's word masked, then the cell switched to the mask for an
        // object of type 50 and up with +0x904
        let mut world = world(RecycleMode::RECYCLED);
        set_tile(&mut world, 2, 1, 0x0000_0002_0000_0002 | (1 << 30));
        assert_eq!(
            world.watch(PlayerId::new(1), object_id(9), Tile::new(2, 1)),
            Ok(GhostWatch::Watched)
        );
        assert_eq!(
            world.players[1].watched,
            [Watched {
                id: object_id(9),
                cell: Cell::Mask(object_id(9)),
                bits: VisionBits::from_bits_retain(0x0000_0002_0000_0002),
            }]
        );
        // a second watch and another owner's object are refused
        assert_eq!(
            world.watch(PlayerId::new(1), object_id(9), Tile::new(2, 1)),
            Ok(GhostWatch::Refused)
        );
        assert_eq!(
            world.watch(PlayerId::new(1), object_id(5), Tile::new(1, 2)),
            Ok(GhostWatch::Refused)
        );
        // the mine (type 30) follows its tile
        set_tile(&mut world, 1, 2, 0x0000_0001_0000_0000);
        assert_eq!(
            world.watch(PlayerId::new(0), object_id(5), Tile::new(1, 2)),
            Ok(GhostWatch::Watched)
        );
        assert_eq!(
            world.players[0].watched,
            [Watched {
                id: object_id(5),
                cell: Cell::Tile(9),
                bits: VisionBits::from_bits_retain(0x0000_0001_0000_0000),
            }]
        );
    }

    #[test]
    fn death_leaves_ghost_for_each_player_who_explored_its_tile_and_does_not_see_it() {
        // FUN_14118e990: player 1 explored and sees the mine's tile, player 2 explored it only;
        // Gaia gets none; the bit of a fourth player ends the walk
        let mut game = world(RecycleMode::RECYCLED);
        game.players[2].kind = PlayerKind::Computer;
        set_tile(&mut game, 1, 2, 0x0000_000f_0000_0002);
        game.recycling.ghosts = vec![object_id(-3)];
        game.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Dying;
        assert_eq!(game.ghosts_at_death(0, object_id(5)), Ok(()));
        assert!(game.players[0].ghosts.is_empty() && game.players[1].ghosts.is_empty());
        // a computer viewer gets its ghost as any other (the notify is the watched list's)
        let ghost = game.players[2].ghosts[0];
        assert_eq!(
            (ghost.id, ghost.original, ghost.original_id),
            (object_id(-3), Some(object_id(5)), Some(object_id(5)))
        );
        assert_eq!(
            (ghost.original_owner, ghost.original_master),
            (PlayerId::GAIA, Some(MINE))
        );
        assert_eq!(
            (ghost.tile, ghost.state, ghost.cant_see, ghost.footprint),
            (Tile::new(1, 2), ObjectState::Alive, PlayerSet::NONE, 0)
        );
        // then the owner watches its tile at the tile's bits
        assert_eq!(
            game.players[0].watched,
            [Watched {
                id: object_id(5),
                cell: Cell::Tile(9),
                bits: VisionBits::from_bits_retain(0x0000_000f_0000_0002),
            }]
        );
        // a master that leaves no ghosts, or an id below 0, gives none and no watch
        let mut game = world(RecycleMode::RECYCLED);
        set_tile(&mut game, 1, 2, 0x0000_0004_0000_0000);
        game.players[0].objects[ListIndex::new(0)].life.master_id = TREE;
        assert_eq!(game.ghosts_at_death(0, object_id(5)), Ok(()));
        assert!(game.players[2].ghosts.is_empty() && game.players[0].watched.is_empty());
        let mut world = world(RecycleMode::RECYCLED);
        set_tile(&mut world, 1, 2, 0x0000_0004_0000_0000);
        world.players[0].objects[ListIndex::new(0)].id = object_id(-5);
        world.reindex();
        assert_eq!(world.ghosts_at_death(0, object_id(-5)), Ok(()));
        assert!(world.players[2].ghosts.is_empty() && world.players[0].watched.is_empty());
    }

    #[test]
    fn ghost_takes_pooled_id_reusable_one_or_for_master_not_recyclable_new_one() {
        // FUN_1410cfe80: the pool's head, else FUN_1411a45a0's id by the master's +0x680
        let original = Original {
            id: object_id(9),
            owner: PlayerId::new(1),
            state: ObjectState::Alive,
            master: HOUSE,
        };
        let mut game = world(RecycleMode::RECYCLED);
        game.recycling.ghosts = vec![object_id(-4), object_id(-6)];
        game.make_ghost(PlayerId::new(2), Position::new(2.5, 1.5, 0.0), original)
            .unwrap();
        game.make_ghost(PlayerId::new(2), Position::new(2.5, 1.5, 0.0), original)
            .unwrap();
        game.make_ghost(PlayerId::new(2), Position::new(2.5, 1.5, 0.0), original)
            .unwrap();
        let ids: Vec<ObjectId> = game.players[2]
            .ghosts
            .iter()
            .map(|ghost| ghost.id)
            .collect();
        assert_eq!(ids, [object_id(-6), object_id(-4), object_id(-10)]);
        assert_eq!(game.recycling.next_id, object_id(-11));
        // each new one destroyed the viewer's twin before it
        let states: Vec<ObjectState> = game.players[2]
            .ghosts
            .iter()
            .map(|ghost| ghost.state)
            .collect();
        assert_eq!(
            states,
            [
                ObjectState::Removed,
                ObjectState::Removed,
                ObjectState::Alive
            ]
        );
        // a building's ghost follows its original's UMV entry
        assert_eq!(game.players[2].ghosts[2].footprint, 1);
        let mut game = world(RecycleMode::KEPT);
        game.recycling.ghosts = vec![object_id(-4)];
        game.make_ghost(PlayerId::new(2), Position::new(2.5, 1.5, 0.0), original)
            .unwrap();
        assert_eq!(game.players[2].ghosts[0].id, object_id(100));
        assert_eq!(
            (game.next_object_id.get(), game.recycling.ghosts.len()),
            (101, 1)
        );
        // an original past state 6 is forgotten, its master kept
        let mut world = world(RecycleMode::RECYCLED);
        let gone = Original {
            state: ObjectState::Removed,
            ..original
        };
        world
            .make_ghost(PlayerId::new(2), Position::new(2.5, 1.5, 0.0), gone)
            .unwrap();
        let ghost = world.players[2].ghosts[0];
        assert_eq!(
            (ghost.original, ghost.original_id, ghost.original_master),
            (None, None, Some(HOUSE))
        );
    }

    #[test]
    fn the_twin_check_destroys_the_viewers_own_twin_and_marks_another_viewers() {
        // FUN_1410928a0: ghosts on the tile, in state 2, of the same original id
        let mut world = world(RecycleMode::RECYCLED);
        let twin = |id, original_id, position: Position, state| Ghost {
            id,
            original: None,
            original_id,
            original_master: Some(HOUSE),
            original_owner: PlayerId::new(1),
            position,
            tile: Tile::new(position.x() as i32, position.y() as i32),
            footprint: 0,
            bits: VisionBits::NONE,
            cant_see: PlayerSet::NONE,
            seen: false,
            state,
        };
        world.players[1].ghosts = vec![twin(
            object_id(-20),
            Some(object_id(9)),
            Position::new(2.1, 1.9, 0.0),
            ObjectState::Alive,
        )];
        world.players[2].ghosts = vec![
            twin(
                object_id(-21),
                Some(object_id(9)),
                Position::new(2.9, 1.1, 0.0),
                ObjectState::Alive,
            ),
            twin(
                object_id(-22),
                Some(object_id(9)),
                Position::new(3.0, 1.5, 0.0),
                ObjectState::Alive,
            ),
            twin(
                object_id(-23),
                Some(object_id(8)),
                Position::new(2.5, 1.5, 0.0),
                ObjectState::Alive,
            ),
            twin(
                object_id(-24),
                Some(object_id(9)),
                Position::new(2.5, 1.5, 0.0),
                ObjectState::Removed,
            ),
            // the tile list holds a ghost by its place, whatever its +0x7dc/+0x7e0 say
            Ghost {
                tile: Tile::new(0, 3),
                ..twin(
                    object_id(-25),
                    Some(object_id(9)),
                    Position::new(2.2, 1.2, 0.0),
                    ObjectState::Alive,
                )
            },
        ];
        let mut ghost = twin(
            object_id(-30),
            Some(object_id(9)),
            Position::new(2.5, 1.5, 0.0),
            ObjectState::Alive,
        );
        world.check_twins(PlayerId::new(2), &mut ghost).unwrap();
        // the viewer's own twins on the tile are destroyed
        let states: Vec<ObjectState> = world.players[2]
            .ghosts
            .iter()
            .map(|ghost| ghost.state)
            .collect();
        assert_eq!(
            states,
            [
                ObjectState::Removed,
                ObjectState::Alive,
                ObjectState::Alive,
                ObjectState::Removed,
                ObjectState::Removed
            ]
        );
        // player 1's twin and the new ghost note each other
        assert_eq!(
            world.players[1].ghosts[0].cant_see,
            PlayerSet::of(PlayerId::new(2))
        );
        assert_eq!(ghost.cant_see, PlayerSet::of(PlayerId::new(1)));
        assert!(
            world.players[2]
                .ghosts
                .iter()
                .all(|ghost| ghost.cant_see == PlayerSet::NONE)
        );
    }

    #[test]
    fn ghost_without_an_original_keeps_its_master_and_one_not_recyclable_is_deleted() {
        // FUN_141090000: a null +0x7c0 sets only +0x7f8 = -1, +0x7d0 stays
        let mut game = world(RecycleMode::KEPT);
        let ghost = Ghost {
            id: object_id(100),
            original: None,
            original_id: Some(object_id(9)),
            original_master: Some(HOUSE),
            original_owner: PlayerId::new(1),
            position: Position::new(2.5, 1.5, 0.0),
            tile: Tile::new(2, 1),
            footprint: 0,
            bits: VisionBits::NONE,
            cant_see: PlayerSet::NONE,
            seen: false,
            state: ObjectState::Alive,
        };
        game.players[2].ghosts = vec![ghost];
        game.update_ghosts(PlayerId::new(2)).unwrap();
        let players = game.players[2].ghosts[0];
        assert_eq!(
            (players.original_id, players.original_master),
            (None, Some(HOUSE))
        );
        // an original past state 6 takes its master with it
        game.players[2].ghosts[0].original = Some(object_id(9));
        game.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Removed;
        game.update_ghosts(PlayerId::new(2)).unwrap();
        assert_eq!(game.players[2].ghosts[0].original_master, None);
        // states 7, then 8, then it leaves; a master not recyclable gives the pool nothing
        game.players[2].ghosts[0].state = ObjectState::Removed;
        game.update_ghosts(PlayerId::new(2)).unwrap();
        assert_eq!(game.players[2].ghosts[0].state, ObjectState::Gone);
        game.update_ghosts(PlayerId::new(2)).unwrap();
        assert!(game.players[2].ghosts.is_empty() && game.recycling.ghosts.is_empty());
        let mut world = world(RecycleMode::RECYCLED);
        world.players[2].ghosts = vec![Ghost {
            state: ObjectState::Gone,
            ..ghost
        }];
        world.update_ghosts(PlayerId::new(2)).unwrap();
        assert_eq!(world.recycling.ghosts, [object_id(100)]);
    }

    /// Player 2's ghost of player 1's house 9 on tile (2, 1), in state 2, of footprint entry
    /// `footprint` (0 for none), its bits 0.
    fn house_ghost(footprint: u8) -> Ghost {
        Ghost {
            id: object_id(100),
            original: Some(object_id(9)),
            original_id: Some(object_id(9)),
            original_master: Some(HOUSE),
            original_owner: PlayerId::new(1),
            position: Position::new(2.5, 1.5, 0.0),
            tile: Tile::new(2, 1),
            footprint,
            bits: VisionBits::NONE,
            cant_see: PlayerSet::NONE,
            seen: false,
            state: ObjectState::Alive,
        }
    }

    /// `FUN_141090000` once its tile's bits change: the players sharing the original owner's
    /// sight note themselves in `cant_see` and the ghost takes the bits; the same bits again
    /// change nothing; its viewer seeing them destroys it. A ghost marked seen below state 3 is
    /// destroyed before any of that.
    #[test]
    fn ghost_its_viewer_sees_again_is_destroyed_and_the_owner_notes_itself() {
        let mut game = world(RecycleMode::RECYCLED);
        game.players[2].ghosts = vec![house_ghost(0)];
        set_tile(&mut game, 2, 1, 1 << 1);
        game.update_ghosts(PlayerId::new(2)).unwrap();
        let players = game.players[2].ghosts[0];
        assert_eq!(
            (players.state, players.bits, players.cant_see),
            (
                ObjectState::Alive,
                VisionBits::from_bits_retain(1 << 1),
                PlayerSet::of(PlayerId::new(1))
            )
        );
        game.players[2].ghosts[0].cant_see = PlayerSet::NONE;
        game.update_ghosts(PlayerId::new(2)).unwrap();
        assert_eq!(game.players[2].ghosts[0].cant_see, PlayerSet::NONE);
        set_tile(&mut game, 2, 1, (1 << 1) | (1 << 2));
        game.update_ghosts(PlayerId::new(2)).unwrap();
        assert_eq!(game.players[2].ghosts[0].state, ObjectState::Removed);
        let mut world = world(RecycleMode::RECYCLED);
        world.players[2].ghosts = vec![Ghost {
            seen: true,
            ..house_ghost(0)
        }];
        set_tile(&mut world, 2, 1, 1 << 1);
        world.update_ghosts(PlayerId::new(2)).unwrap();
        let ghost = world.players[2].ghosts[0];
        assert_eq!(
            (ghost.state, ghost.bits),
            (ObjectState::Removed, VisionBits::NONE)
        );
    }

    /// A ghost its viewer sees stays while a player sharing the viewer's sight (but not the
    /// owner's) does not see it: viewer 2 shares the sight of players 3 and 4, player 3 only the
    /// viewer's; the tile seen by player 4 alone keeps the ghost, by player 3 too destroys it.
    #[test]
    fn ghost_stays_while_player_sharing_the_viewer_sight_does_not_see_it() {
        let mut world = world(RecycleMode::RECYCLED);
        for _ in 0..2 {
            world.players.push(Player::default());
            world.reindex();
            world
                .model
                .masters
                .push(Arc::new(masters(RecycleMode::RECYCLED)));
        }
        let own = |player_id| own_masks(player_id, GameStatus::Playing);
        let with = |player_id, bits: u64| {
            let mut masks = own(player_id);
            masks.visible |= VisionBits::from_bits_retain(bits);
            masks
        };
        world.spatial.vision = ByPlayer::new(vec![
            own(PlayerId::new(0)),
            own(PlayerId::new(1)),
            with(PlayerId::new(2), (1 << 3) | (1 << 4)),
            with(PlayerId::new(3), 1 << 2),
            own(PlayerId::new(4)),
        ]);
        world.players[2].ghosts = vec![house_ghost(0)];
        set_tile(&mut world, 2, 1, 1 << 4);
        world.update_ghosts(PlayerId::new(2)).unwrap();
        let ghost = world.players[2].ghosts[0];
        assert_eq!(
            (ghost.state, ghost.bits, ghost.cant_see),
            (
                ObjectState::Alive,
                VisionBits::from_bits_retain(1 << 4),
                PlayerSet::of(PlayerId::new(1))
            )
        );
        set_tile(&mut world, 2, 1, (1 << 3) | (1 << 4));
        world.update_ghosts(PlayerId::new(2)).unwrap();
        assert_eq!(world.players[2].ghosts[0].state, ObjectState::Removed);
    }

    /// A building's ghost reads its footprint's tiles too (its UMV entry's offsets): seen there
    /// by its viewer while the original's mask and the ghost's own tile miss the viewer, it is
    /// marked seen, not destroyed, and the next update destroys it; without its UMV entry it is
    /// not modelled.
    #[test]
    fn building_ghost_seen_on_its_footprint_alone_is_marked_seen() {
        let mut game = world(RecycleMode::RECYCLED);
        let sight = game.spatial.sight.as_mut().unwrap();
        sight.extents = vec![Extent {
            guid: 1,
            offsets: vec![TileOffset::new(0, 0), TileOffset::new(1, 0)],
            capacity: 2,
        }];
        let mut registry = Registry::default();
        let _ = registry.entries.insert(
            object_id(9),
            Entry {
                mask: VisionBits::from_bits_retain(1 << 1),
                ..Entry::default()
            },
        );
        sight.registry = Some(registry);
        game.players[2].ghosts = vec![house_ghost(1)];
        set_tile(&mut game, 2, 1, 1 << 1);
        set_tile(&mut game, 3, 1, 1 << 2);
        game.update_ghosts(PlayerId::new(2)).unwrap();
        let ghost = game.players[2].ghosts[0];
        assert_eq!(
            (ghost.state, ghost.seen, ghost.bits),
            (
                ObjectState::Alive,
                true,
                VisionBits::from_bits_retain((1 << 1) | (1 << 2))
            )
        );
        game.update_ghosts(PlayerId::new(2)).unwrap();
        assert_eq!(game.players[2].ghosts[0].state, ObjectState::Removed);
        let mut world = world(RecycleMode::RECYCLED);
        world.players[2].ghosts = vec![house_ghost(1)];
        assert_eq!(
            world.update_ghosts(PlayerId::new(2)).unwrap_err().reason(),
            "a building's ghost without its UMV entry"
        );
    }

    #[test]
    fn watched_list_gives_ghosts_to_the_players_who_lost_sight_and_tells_computer() {
        // FUN_1410905e0: lost = ((old ^ new) & old) >> 1, from player 1, not the list's player
        let mut game = world(RecycleMode::RECYCLED);
        set_tile(&mut game, 1, 2, 0x0000_0007_0000_0007);
        assert_eq!(
            game.watch(PlayerId::new(0), object_id(5), Tile::new(1, 2)),
            Ok(GhostWatch::Watched)
        );
        set_tile(&mut game, 1, 2, 0x0000_0007_0000_0001);
        game.watch_pass(PlayerId::new(0)).unwrap();
        assert_eq!(game.players[1].ghosts.len(), 1);
        assert_eq!(game.players[2].ghosts.len(), 1);
        assert_eq!(
            game.players[0].watched[0].bits,
            VisionBits::from_bits_retain(0x0000_0007_0000_0001)
        );
        // a computer viewer gets its ghost, then its AI's notify is not modelled
        let mut game = world(RecycleMode::RECYCLED);
        game.players[2].kind = PlayerKind::Computer;
        set_tile(&mut game, 1, 2, 0x0000_0007_0000_0005);
        assert_eq!(
            game.watch(PlayerId::new(0), object_id(5), Tile::new(1, 2)),
            Ok(GhostWatch::Watched)
        );
        set_tile(&mut game, 1, 2, 0x0000_0007_0000_0001);
        assert!(game.watch_pass(PlayerId::new(0)).is_err());
        assert_eq!(game.players[2].ghosts.len(), 1);
        // an entry whose object is gone leaves the list
        let mut world = world(RecycleMode::RECYCLED);
        set_tile(&mut world, 1, 2, 0x0000_0007_0000_0007);
        assert_eq!(
            world.watch(PlayerId::new(0), object_id(5), Tile::new(1, 2)),
            Ok(GhostWatch::Watched)
        );
        world.players[0].objects.clear();
        world.reindex();
        set_tile(&mut world, 1, 2, 0x0000_0007_0000_0001);
        world.watch_pass(PlayerId::new(0)).unwrap();
        assert_eq!(world.players[0].watched.len(), 0);
        assert_eq!(world.players[1].ghosts.len(), 0);
    }

    #[test]
    fn load_watches_dead_objects_that_leave_ghosts_and_buildings_but_their_annexes() {
        // FUN_14119df60's dead objects, FUN_1411ac170's FUN_1411cc980 with no annex link yet,
        // then the rehook drops every annex but a gate's
        let mut world = world(RecycleMode::RECYCLED);
        let gate = Master {
            id: master_id(88),
            kind: MasterKind::BUILDING,
            class: class::GATE,
            ..Master::default()
        };
        let annex_master = Master {
            id: master_id(89),
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            ..Master::default()
        };
        let wonder = Master {
            id: ids::WONDER,
            kind: MasterKind::BUILDING,
            class: class::CLASS_51,
            ..Master::default()
        };
        let skipped = Master {
            id: MASTER_1808,
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            ..Master::default()
        };
        world.model.masters = (0..3)
            .map(|_| {
                Masters::new(masters(RecycleMode::RECYCLED).iter().cloned().chain([
                    gate.clone(),
                    annex_master.clone(),
                    wonder.clone(),
                    skipped.clone(),
                ]))
            })
            .map(Arc::new)
            .collect();
        // Gaia's mine 5 dead (state 3), house 9's annexes 10 (a gate) and 11, a class-51
        // building 12, master 1808's 13, a desolid foundation 14 in the sleeping list
        world.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Dying;
        let gate = object(
            10,
            master_id(88),
            ObjectState::Alive,
            Position::new(2.5, 2.5, 0.0),
        );
        let annex = object(
            11,
            master_id(89),
            ObjectState::Alive,
            Position::new(3.5, 2.5, 0.0),
        );
        world
            .extend_live(
                PlayerId::new(1),
                [
                    gate,
                    annex,
                    object(
                        12,
                        ids::WONDER,
                        ObjectState::Alive,
                        Position::new(0.5, 0.5, 0.0),
                    ),
                    object(
                        13,
                        MASTER_1808,
                        ObjectState::Alive,
                        Position::new(0.5, 1.5, 0.0),
                    ),
                ],
            )
            .unwrap();
        for annex in [10, 11] {
            world.parts.building_mut(object_id(annex)).unwrap().annex_of =
                Some(MainBuildingId::new(object_id(9)));
        }
        let foundation = object(14, HOUSE, ObjectState::Alive, Position::new(0.5, 3.5, 0.0));
        let _ = world.push_asleep(PlayerId::new(1), foundation).unwrap();
        world
            .parts
            .building_mut(object_id(14))
            .unwrap()
            .construction = Some(Construction {
            progress: Seconds::ZERO,
            desolid: true,
            builders: BuilderCount::NONE,
            hit_points: MasterHitPoints::of_data(0),
            build_time: WholeSeconds::ZERO,
            record: Obstruction {
                kind: ObstructionKind::KIND_2,
                class: ObstructionClass::UNIT,
            },
            snow: Snow::None,
            sprite_override: false,
        });
        world.build_watched().unwrap();
        let ids = |world: &World, player: usize| {
            world.players[player]
                .watched
                .iter()
                .map(|watched| watched.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&world, 0), [object_id(5)]);
        assert_eq!(ids(&world, 1), [object_id(9), object_id(10)]);
        // the house follows its mask (its UMV entry), the gate its tile
        assert_eq!(world.players[1].watched[0].cell, Cell::Mask(object_id(9)));
        assert_eq!(world.players[1].watched[1].cell, Cell::Tile(2 * 4 + 2));
        // a building in the game, its annex linked, is not watched as an annex
        assert!(
            !world
                .watches_building(PlayerId::new(1), object_id(11))
                .unwrap()
        );
        assert!(
            world
                .watches_building(PlayerId::new(1), object_id(10))
                .unwrap()
        );
    }
}
