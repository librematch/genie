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

use crate::GenieError;
use crate::map::Position;
use crate::map::Tile;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::objects::masters::ids::MASTER_1808;
use crate::objects::masters::master_type;
use crate::objects::masters::master_type::BUILDING;
use crate::players::PlayerId;
use crate::players::PlayerSet;
use crate::vision::VisionBits;
use crate::world::World;
use crate::world::slots::{CheckedIndex, PlayerSlots};

/// The ghosts' master in every player's list (`player+0x28`'s entry 243, "DOPL").
pub const GHOST_MASTER: MasterId = MasterId::named(243);
/// A master's `create_doppelganger_on_death` that leaves ghosts.
pub const LEAVES_GHOSTS: u8 = 2;
/// The classes a building's watch leaves out (`FUN_1411cc980`) and the gates' class, whose annex
/// stays watched.
const UNWATCHED_CLASSES: [ClassId; 2] = [class::CLASS_51, class::CLASS_54];
/// The originals whose ghost's leaving changes the local player's terrain (`FUN_141090ad0`).
const TERRAIN_ORIGINALS: [MasterId; 2] = [MasterId::named(357), MasterId::named(1188)];
/// The type from which an object's entry and its ghost follow its footprint's UMV entry
/// (`FUN_141090430`, `FUN_14108f9b0`: master type above 0x31).
const MASKED_KIND: MasterKind = master_type::RGE_COMBAT;

/// What an entry of a watched list follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    pub id: ObjectId,
    /// What it follows (`+8`).
    pub cell: Cell,
    /// The bits last seen (`+0x10`).
    pub bits: VisionBits,
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

impl World {
    /// The vision mask of player `p` (`player+0x2a8`, [`crate::vision`]).
    fn vision_mask(&self, p: PlayerId) -> VisionBits {
        self.spatial.vision(p).visible
    }

    /// The unified map's visible and explored bits at `tile` (`map+0x5e30`, masked); 0 off the
    /// map or without the players' sight.
    fn tile_bits(&self, tile: Tile) -> Result<VisionBits, GenieError> {
        let Some(v) = self.spatial.sight.as_ref() else {
            return Ok(VisionBits::NONE);
        };
        let Some(i) = v.size.index(tile) else {
            return Ok(VisionBits::NONE);
        };
        Ok(*v.unified.checked(i)? & VisionBits::OBJECT_MASK)
    }

    /// `tile`'s index in the unified map.
    fn tile_index(&self, tile: Tile) -> Option<usize> {
        self.spatial.sight.as_ref()?.size.index(tile)
    }

    /// What a cell shows now.
    fn cell_bits(&self, cell: Cell) -> Result<VisionBits, GenieError> {
        match cell {
            Cell::Tile(i) => Ok(self
                .spatial
                .sight
                .as_ref()
                .and_then(|v| v.unified.get(i))
                .map_or(VisionBits::NONE, |&b| b & VisionBits::OBJECT_MASK)),
            Cell::Mask(id) => {
                let registry = self
                    .spatial
                    .sight
                    .as_ref()
                    .and_then(|v| v.registry.as_ref())
                    .ok_or(GenieError::unsupported(
                        "a watched building without the objects-in-sight registry",
                    ))?;
                Ok(registry
                    .entries
                    .get(&id)
                    .map_or(VisionBits::NONE, |e| e.mask)
                    & VisionBits::OBJECT_MASK)
            }
        }
    }

    /// `FUN_141090430`: player `owner`'s watched list takes object `id` at tile (`x`, `y`) (an
    /// object of another owner, or one it holds already, is refused). The entry keeps the tile's
    /// bits; it follows the tile, or for an object of type 50 and up with its footprint's UMV
    /// entry (`+0x904`) its own visibility mask (`+0x8f8`).
    pub(crate) fn watch(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
        at: Tile,
    ) -> Result<bool, GenieError> {
        let Some((p, u)) = self.object(id) else {
            return Ok(false);
        };
        if p != owner
            || self
                .players
                .player(owner)?
                .watched
                .iter()
                .any(|w| w.id == id)
        {
            return Ok(false);
        }
        // the game takes the tile's word without a bounds test: every caller passes the
        // object's own tile, which is on the map
        let Some(tile) = self.tile_index(at) else {
            return Ok(false);
        };
        let bits = self.tile_bits(at)?;
        let kind = u.master(&self.masters, p)?.kind;
        let umv = self
            .spatial
            .sight
            .as_ref()
            .and_then(|v| v.seer(id))
            .map_or(0, |s| s.umv);
        let cell = if MASKED_KIND <= kind && umv != 0 {
            Cell::Mask(id)
        } else {
            Cell::Tile(tile)
        };
        self.players
            .player_mut(owner)?
            .watched
            .push(Watched { id, cell, bits });
        Ok(true)
    }

    /// The tile (`(int)x`, `(int)y`) of a place, as the game's callers truncate it.
    fn tile_at(position: Position) -> Tile {
        position.tile()
    }

    /// `FUN_141091310`: the watched list of player `owner` lets object `id` go (the last entry
    /// takes its place).
    pub(crate) fn unwatch(&mut self, owner: PlayerId, id: ObjectId) -> Result<(), GenieError> {
        let list = &mut self.players.player_mut(owner)?.watched;
        if let Some(k) = list.iter().position(|w| w.id == id) {
            let _ = list.swap_remove(k);
        }
        Ok(())
    }

    /// `FUN_1411cc980`: a building its owner watches: not of class 51 or 54, not desolid
    /// (`+0xc60`), not an annex (`+0xbb8`, but a gate's), not master 1808.
    fn watches_building(&self, p: PlayerId, id: ObjectId) -> Result<bool, GenieError> {
        self.watches_building_at(p, id, true)
    }

    /// [`World::watches_building`], with the annex link made (`linked`) or, during a load before
    /// the rehook, not yet: then an annex passes too.
    fn watches_building_at(
        &self,
        p: PlayerId,
        id: ObjectId,
        linked: bool,
    ) -> Result<bool, GenieError> {
        let Some((_, u)) = self.object(id) else {
            return Ok(false);
        };
        let m = u.master(&self.masters, p)?;
        let annex = linked && u.annex_of.is_some();
        Ok(!UNWATCHED_CLASSES.contains(&m.class)
            && !u.construction.is_some_and(|c| c.desolid)
            // The master a building's watch leaves out (`FUN_1411cc980`).
            && (!annex || m.class == class::GATE)
            && m.id != MASTER_1808)
    }

    /// A building whose construction started (`FUN_1411e5670`): its owner watches it
    /// ([`World::watches_building`]).
    pub(crate) fn watch_started(&mut self, p: PlayerId, id: ObjectId) -> Result<(), GenieError> {
        if !self.watches_building(p, id)? {
            return Ok(());
        }
        let Some((_, u)) = self.object(id) else {
            return Ok(());
        };
        let _ = self.watch(p, id, Self::tile_at(u.position()))?;
        Ok(())
    }

    /// The watched lists the game's load builds, which no save holds: each player's objects in
    /// the load's order (its live list, then its sleeping list) join their owner's list, an
    /// object whose master leaves ghosts once it is dead (`FUN_14119df60`, at its tile) and a
    /// building its owner watches ([`World::watches_building`]; the load constructor
    /// `FUN_1411ac170`, at what it follows, an annex's link not known yet); then the rehook lets
    /// every annex but a gate's go (`TRIBE_Building_Object::rehook`). A building's mask is empty
    /// at a load.
    pub fn build_watched(&mut self) -> Result<(), GenieError> {
        // each id's object as [`World::object`] finds it (the first player in order, its live list
        // before its sleeping list), looked up once: the loop changes only the watched lists
        let mut first: std::collections::HashMap<ObjectId, (PlayerId, bool, ListIndex)> =
            std::collections::HashMap::new();
        for (q, player) in PlayerId::each(self.players.len()).zip(&self.players) {
            for (sleeping, list) in [(false, &player.objects), (true, &player.sleeping)] {
                for (k, u) in list.indexed() {
                    let _ = first.entry(u.id).or_insert((q, sleeping, k));
                }
            }
        }
        for p in PlayerId::each(self.players.len()) {
            self.players.player_mut(p)?.watched.clear();
            let ids: Vec<ObjectId> = self
                .players
                .player(p)?
                .objects
                .iter()
                .chain(&self.players.player(p)?.sleeping)
                .map(|u| u.id)
                .collect();
            for id in ids {
                let Some(&(q, sleeping, k)) = first.get(&id) else {
                    continue;
                };
                let u = if sleeping {
                    &self.players.asleep(q, k)?
                } else {
                    &self.players.live(q, k)?
                };
                let m = u.master(&self.masters, p)?;
                let (state, kind, ghosts, position) =
                    (u.life.state, m.kind, m.ghost_on_death, u.position());
                let tile = Self::tile_at(position);
                // the static load's watch of a dead object (FUN_14119df60)
                if ghosts == LEAVES_GHOSTS && ObjectState::Alive < state {
                    let _ = self.watch(p, id, tile)?;
                }
                // a building's load constructor (FUN_1411ac170, type 80 only): FUN_1411cc980
                // before the rehook links its annexes
                if kind == BUILDING && self.watches_building_at(p, id, false)? {
                    let _ = self.watch(p, id, tile)?;
                }
            }
            for list in [false, true] {
                let held = self.players.player(p)?;
                let objects = if list { &held.sleeping } else { &held.objects };
                let mut annexes: Vec<ObjectId> = Vec::new();
                for u in objects.iter().filter(|u| u.annex_of.is_some()) {
                    if u.master(&self.masters, p)?.class != class::GATE {
                        annexes.push(u.id);
                    }
                }
                for id in annexes {
                    self.unwatch(p, id)?;
                }
            }
        }
        Ok(())
    }

    /// `FUN_14118e990`, at the death of object `id` (the static `set_state` 3, while the game
    /// state is not 2): for an id of 0 and up whose master leaves ghosts (`+0x682` 2), every
    /// player who explored its tile but does not see it, Gaia aside, gets a ghost of it
    /// ([`World::make_ghost`]), in the players' order; the first bit past the players ends it.
    /// Then its owner's list watches it at its tile ([`World::watch`]).
    pub(crate) fn ghosts_at_death(
        &mut self,
        _owner: usize,
        id: ObjectId,
    ) -> Result<(), GenieError> {
        let Some((p, u)) = self.object(id) else {
            return Ok(());
        };
        let original = Original {
            id,
            owner: p,
            state: u.life.state,
            master: u.life.master_id,
        };
        let position = u.position();
        if id.is_recyclable() || u.master(&self.masters, p)?.ghost_on_death != LEAVES_GHOSTS {
            return Ok(());
        }
        let tile = Self::tile_at(position);
        let bits = self.tile_bits(tile)?;
        // each player past Gaia that explored the tile and does not see it, in order
        for i in PlayerId::each(self.players.len()).skip(1) {
            if bits.intersects(VisionBits::explored(i)) && !bits.intersects(VisionBits::visible(i))
            {
                self.make_ghost(i, position, original)?;
            }
        }
        let _ = self.watch(p, id, tile)?;
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
    ) -> Result<(), GenieError> {
        let recyclable = self.master(viewer, GHOST_MASTER)?.recyclable;
        let kind = self.master(original.owner, original.master)?.kind;
        let umv = self
            .spatial
            .sight
            .as_ref()
            .and_then(|v| v.seer(original.id))
            .map_or(0, |s| s.umv);
        let pooled = if recyclable {
            self.recycling.ghosts.pop()
        } else {
            None
        };
        let id = match pooled {
            Some(id) => id,
            None if recyclable => {
                let id = self.recycling.next_id;
                self.recycling.next_id = id.predecessor();
                id
            }
            None => self.take_object_id(viewer, GHOST_MASTER),
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
    fn check_twins(&mut self, viewer: PlayerId, ghost: &mut Ghost) -> Result<(), GenieError> {
        let tile = Self::tile_at(ghost.position);
        for q in PlayerId::each(self.players.len()) {
            for k in 0..self.players.player(q)?.ghosts.len() {
                let g = *self.players.player(q)?.ghosts.checked(k)?;
                if Self::tile_at(g.position) != tile
                    || g.id == ghost.id
                    || g.original_id != ghost.original_id
                    || g.state != ObjectState::Alive
                {
                    continue;
                }
                if q == viewer {
                    self.destroy_ghost(q, k)?;
                } else {
                    self.players.player_mut(q)?.ghosts.checked_mut(k)?.cant_see |=
                        PlayerSet::of(viewer);
                    ghost.cant_see |= PlayerSet::of(q);
                }
            }
        }
        Ok(())
    }

    /// The destroy (slot 0xd0, `FUN_14118f1e0`) of ghost `k` of player `p`: below state 7, its
    /// hit points go and it goes to state 7.
    fn destroy_ghost(&mut self, p: PlayerId, k: usize) -> Result<(), GenieError> {
        let g = self.players.player_mut(p)?.ghosts.checked_mut(k)?;
        if g.state < ObjectState::Removed {
            g.state = ObjectState::Removed;
        }
        Ok(())
    }

    /// The ghosts of player `p` in their list's order (`FUN_141166550` on `+0x40`), each one's
    /// update (`FUN_141090000`): its static step (state 7 goes to 8, a ghost in state 8 leaves);
    /// one its viewer saw while the original's mask had not is destroyed; its original past state
    /// 6 is forgotten; then, its tile's bits (and its footprint's) changed, every player sharing
    /// the original owner's sight notes itself in the ghost's `cant_see` (`+0x7f0`), and a ghost
    /// its viewer sees in state 2, unless one of the players sharing the viewer's sight does not, is
    /// destroyed (or marked seen, for a building's ghost whose original the viewer's mask misses
    /// and whose tile it does not see). A ghost that leaves (`FUN_141090ad0`) goes off its tile,
    /// the last ghost taking its place in the list, and to the pool of type 25
    /// (`RGE_Static_Object::recycle_out_of_game`), or for a master that is not recyclable it is
    /// deleted. The viewer's `+0x2b0` table is not held (empty).
    pub(crate) fn update_ghosts(&mut self, p: PlayerId) -> Result<(), GenieError> {
        let mut k = 0;
        while k < self.players.player(p)?.ghosts.len() {
            let leaves = self.update_ghost(p, k)?;
            if leaves {
                let g = self.players.player_mut(p)?.ghosts.swap_remove(k);
                if g.original_master
                    .is_some_and(|m| TERRAIN_ORIGINALS.contains(&m))
                {
                    return Err(GenieError::unsupported(
                        "a farm's ghost leaving (the local player's terrain, FUN_141092190)",
                    ));
                }
                self.spatial.remove_ghost(g.id, g.position);
                if self.master(p, GHOST_MASTER)?.recyclable {
                    self.recycling.ghosts.push(g.id);
                }
                continue;
            }
            k += 1;
        }
        Ok(())
    }

    /// An original past state 6 is forgotten with its master; without one only the id goes.
    fn forget_original(
        &mut self,
        p: PlayerId,
        k: usize,
        original: Option<ObjectId>,
    ) -> Result<(), GenieError> {
        let live = original.is_some_and(|o| {
            self.object(o)
                .is_some_and(|(_, u)| u.life.state < ObjectState::Removed)
        });
        let g = self.players.player_mut(p)?.ghosts.checked_mut(k)?;
        if original.is_none() {
            g.original_id = None;
        } else if !live {
            g.original = None;
            g.original_master = None;
            g.original_id = None;
        }
        Ok(())
    }

    /// The bits of the ghost's tile and, for a building's ghost, of its footprint's tiles.
    fn ghost_bits(&self, g: &Ghost) -> Result<VisionBits, GenieError> {
        let (px, py) = (g.position.x(), g.position.y());
        let mut bits = self.tile_bits(g.tile)?;
        if g.footprint != 0 {
            let offsets = self
                .spatial
                .sight
                .as_ref()
                .and_then(|v| v.extents.get(usize::from(g.footprint) - 1))
                .map(|e| e.offsets.clone())
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
    fn ghost_viewers(&self, p: PlayerId, g: &Ghost, bits: VisionBits) -> (bool, PlayerSet) {
        let mut unseen = false;
        let mut cant_see = g.cant_see;
        for i in PlayerId::each(self.players.len()).skip(1) {
            let mask = self.vision_mask(i);
            if mask.intersects(VisionBits::visible(g.original_owner)) {
                cant_see |= PlayerSet::of(i);
            } else if mask.intersects(VisionBits::visible(p)) && !bits.intersects(mask) {
                unseen = true;
            }
        }
        (unseen, cant_see)
    }

    /// Whether the viewer `p` has now seen the ghost's tile while its original's mask had not,
    /// and whether the ghost is destroyed for it.
    fn ghost_seen(
        &self,
        p: PlayerId,
        g: &Ghost,
        bits: VisionBits,
        unseen: bool,
    ) -> Result<(bool, bool), GenieError> {
        let viewer = self.vision_mask(p);
        let mut seen = g.seen;
        let mut destroy = false;
        if bits.intersects(viewer) && !unseen && g.state < ObjectState::Dying {
            if g.footprint != 0
                && let Some(o) = g.original
            {
                let original_mask = self.cell_bits(Cell::Mask(o))?;
                if !original_mask.intersects(viewer) && !self.tile_bits(g.tile)?.intersects(viewer)
                {
                    seen = true;
                }
            }
            destroy = !seen;
        }
        Ok((seen, destroy))
    }

    fn update_ghost(&mut self, p: PlayerId, k: usize) -> Result<bool, GenieError> {
        let g = *self.players.player(p)?.ghosts.checked(k)?;
        let leaves = match g.state {
            ObjectState::Removed => {
                self.players.player_mut(p)?.ghosts.checked_mut(k)?.state = ObjectState::Gone;
                false
            }
            ObjectState::Gone => true,
            _ => false,
        };
        if g.seen && g.state < ObjectState::Dying {
            self.destroy_ghost(p, k)?;
            return Ok(leaves);
        }
        self.forget_original(p, k, g.original)?;
        let g = *self.players.player(p)?.ghosts.checked(k)?;
        let bits = self.ghost_bits(&g)?;
        if bits == g.bits {
            return Ok(leaves);
        }
        let (unseen, cant_see) = self.ghost_viewers(p, &g, bits);
        let (seen, destroy) = self.ghost_seen(p, &g, bits, unseen)?;
        let g = self.players.player_mut(p)?.ghosts.checked_mut(k)?;
        g.seen = seen;
        g.cant_see = cant_see;
        g.bits = bits;
        if destroy {
            self.destroy_ghost(p, k)?;
        }
        Ok(leaves)
    }

    /// The watched list of player `p` after the player loop (`FUN_1410905e0`): an entry whose
    /// bits changed, of an object the world still holds, gives a ghost to every player, but its
    /// owner, who saw the cell and no longer does ([`World::make_ghost`], at the object's place),
    /// in the players' order, and takes the new bits; the last entry whose object is gone leaves
    /// the list (the last entry taking its place). A computer player's AI then hears of its ghost
    /// (its slot 0x2e8 with notify 0x20d), which is not modelled.
    pub(crate) fn watch_pass(&mut self, p: PlayerId) -> Result<(), GenieError> {
        let mut gone = None;
        for k in 0..self.players.player(p)?.watched.len() {
            let w = *self.players.player(p)?.watched.checked(k)?;
            let bits = self.cell_bits(w.cell)?;
            if bits == w.bits {
                continue;
            }
            match self.object(w.id) {
                None => gone = Some(w.id),
                Some((owner, u)) => {
                    let at = u.position();
                    let original = Original {
                        id: w.id,
                        owner,
                        state: u.life.state,
                        master: u.life.master_id,
                    };
                    // the visible bits the entry lost, of players 1 to 31 other than its own
                    let lost = w.bits & !bits;
                    for i in PlayerId::each(32).skip(1) {
                        if lost.intersects(VisionBits::visible(i)) && i != p {
                            self.make_ghost(i, at, original)?;
                            if self.players.player(i).is_ok_and(|q| q.computer) {
                                return Err(GenieError::unsupported(
                                    "a computer player's notify of its ghost (0x20d)",
                                ));
                            }
                        }
                    }
                }
            }
            self.players.player_mut(p)?.watched.checked_mut(k)?.bits = bits;
        }
        if let Some(id) = gone {
            self.unwatch(p, id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Distance;
    use crate::map::GridSize;
    use crate::map::TileOffset;
    use crate::objects::masters::ObstructionClass;
    use crate::objects::masters::ObstructionType;
    use crate::objects::masters::ids;
    use crate::objects::masters::ids::HOUSE;
    use crate::objects::masters::master_type::DOPPELGANGER;
    use crate::objects::masters::{Master, Masters};
    use crate::objects::unit::Unit;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::vision::visibility::{Seer, Visibility};
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::Seconds;
    use crate::world::WorldTime;

    /// A gold mine's master (it leaves ghosts), a house's and the ghosts' own.
    const MINE: MasterId = MasterId::named(66);
    const TREE: MasterId = MasterId::named(411);

    fn masters(recyclable: bool) -> Masters {
        Masters::new([
            Master {
                id: GHOST_MASTER,
                // The type of a ghost (`+0xf`).
                kind: DOPPELGANGER,
                recyclable,
                ..Master::default()
            },
            Master {
                id: MINE,
                kind: master_type::MOVING,
                ghost_on_death: LEAVES_GHOSTS,
                ..Master::default()
            },
            Master {
                id: HOUSE,
                kind: BUILDING,
                class: class::BUILDING,
                ghost_on_death: LEAVES_GHOSTS,
                ..Master::default()
            },
            Master {
                id: TREE,
                kind: master_type::MOVING,
                ..Master::default()
            },
        ])
    }

    fn object(id: i32, master: MasterId, state: ObjectState, position: Position) -> Unit {
        let mut u = Unit {
            id: object_id(id),
            body: crate::objects::layers::Body::at(position),
            ..Unit::default()
        };
        u.life.master_id = master;
        u.life.state = state;
        u
    }

    /// Gaia and two players on a 4×4 map, each with its masters, Gaia's mine 5 at (1.5, 2.5)
    /// and player 1's house 9 at (2.5, 1.5) (its footprint's UMV entry 1).
    fn world(recyclable: bool) -> World {
        let players = (0..3).map(|_| Player::default()).collect();
        let mut t = World::new(
            WorldTime::ZERO,
            0,
            PlayerId::new(0),
            vec![Millis::ZERO; 3],
            players,
        );
        t.masters = (0..3).map(|_| masters(recyclable)).collect();
        t.players[0].objects.push(object(
            5,
            MINE,
            ObjectState::Alive,
            Position::new(1.5, 2.5, 0.0),
        ));
        t.players[1].objects.push(object(
            9,
            HOUSE,
            ObjectState::Alive,
            Position::new(2.5, 1.5, 0.0),
        ));
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
                state: ObjectState::Alive,
                explored: true,
                building: None,
                umv: 1,
                capture: 0,
                kind: BUILDING,
            },
        );
        t.spatial.sight = Some(sight);
        t.recycling.next_id = object_id(-10);
        t.next_object_id = object_id(100);
        t
    }

    fn set_tile(t: &mut World, x: usize, y: usize, bits: u64) {
        t.spatial.sight.as_mut().unwrap().unified[y * 4 + x] = VisionBits::from_bits_retain(bits);
    }

    #[test]
    fn a_watched_entry_keeps_the_tiles_bits_and_a_building_with_a_umv_entry_follows_its_mask() {
        // FUN_141090430: the tile's word masked, then the cell switched to the mask for an
        // object of type 50 and up with +0x904
        let mut t = world(true);
        set_tile(&mut t, 2, 1, 0x0000_0002_0000_0002 | (1 << 30));
        assert_eq!(
            t.watch(PlayerId::new(1), object_id(9), Tile::new(2, 1)),
            Ok(true)
        );
        assert_eq!(
            t.players[1].watched,
            [Watched {
                id: object_id(9),
                cell: Cell::Mask(object_id(9)),
                bits: VisionBits::from_bits_retain(0x0000_0002_0000_0002),
            }]
        );
        // a second watch and another owner's object are refused
        assert_eq!(
            t.watch(PlayerId::new(1), object_id(9), Tile::new(2, 1)),
            Ok(false)
        );
        assert_eq!(
            t.watch(PlayerId::new(1), object_id(5), Tile::new(1, 2)),
            Ok(false)
        );
        // the mine (type 30) follows its tile
        set_tile(&mut t, 1, 2, 0x0000_0001_0000_0000);
        assert_eq!(
            t.watch(PlayerId::new(0), object_id(5), Tile::new(1, 2)),
            Ok(true)
        );
        assert_eq!(
            t.players[0].watched,
            [Watched {
                id: object_id(5),
                cell: Cell::Tile(9),
                bits: VisionBits::from_bits_retain(0x0000_0001_0000_0000),
            }]
        );
    }

    #[test]
    fn a_death_leaves_a_ghost_for_each_player_who_explored_its_tile_and_does_not_see_it() {
        // FUN_14118e990: player 1 explored and sees the mine's tile, player 2 explored it only;
        // Gaia gets none; the bit of a fourth player ends the walk
        let mut t = world(true);
        t.players[2].computer = true;
        set_tile(&mut t, 1, 2, 0x0000_000f_0000_0002);
        t.recycling.ghosts = vec![object_id(-3)];
        t.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Dying;
        assert_eq!(t.ghosts_at_death(0, object_id(5)), Ok(()));
        assert!(t.players[0].ghosts.is_empty() && t.players[1].ghosts.is_empty());
        // a computer viewer gets its ghost as any other (the notify is the watched list's)
        let g = t.players[2].ghosts[0];
        assert_eq!(
            (g.id, g.original, g.original_id),
            (object_id(-3), Some(object_id(5)), Some(object_id(5)))
        );
        assert_eq!(
            (g.original_owner, g.original_master),
            (PlayerId::GAIA, Some(MINE))
        );
        assert_eq!(
            (g.tile, g.state, g.cant_see, g.footprint),
            (Tile::new(1, 2), ObjectState::Alive, PlayerSet::NONE, 0)
        );
        // then the owner watches its tile at the tile's bits
        assert_eq!(
            t.players[0].watched,
            [Watched {
                id: object_id(5),
                cell: Cell::Tile(9),
                bits: VisionBits::from_bits_retain(0x0000_000f_0000_0002),
            }]
        );
        // a master that leaves no ghosts, or an id below 0, gives none and no watch
        let mut t = world(true);
        set_tile(&mut t, 1, 2, 0x0000_0004_0000_0000);
        t.players[0].objects[ListIndex::new(0)].life.master_id = TREE;
        assert_eq!(t.ghosts_at_death(0, object_id(5)), Ok(()));
        assert!(t.players[2].ghosts.is_empty() && t.players[0].watched.is_empty());
        let mut t = world(true);
        set_tile(&mut t, 1, 2, 0x0000_0004_0000_0000);
        t.players[0].objects[ListIndex::new(0)].id = object_id(-5);
        assert_eq!(t.ghosts_at_death(0, object_id(-5)), Ok(()));
        assert!(t.players[2].ghosts.is_empty() && t.players[0].watched.is_empty());
    }

    #[test]
    fn a_ghost_takes_a_pooled_id_a_reusable_one_or_for_a_master_not_recyclable_a_new_one() {
        // FUN_1410cfe80: the pool's head, else FUN_1411a45a0's id by the master's +0x680
        let original = Original {
            id: object_id(9),
            owner: PlayerId::new(1),
            state: ObjectState::Alive,
            master: HOUSE,
        };
        let mut t = world(true);
        t.recycling.ghosts = vec![object_id(-4), object_id(-6)];
        t.make_ghost(PlayerId::new(2), Position::new(2.5, 1.5, 0.0), original)
            .unwrap();
        t.make_ghost(PlayerId::new(2), Position::new(2.5, 1.5, 0.0), original)
            .unwrap();
        t.make_ghost(PlayerId::new(2), Position::new(2.5, 1.5, 0.0), original)
            .unwrap();
        let ids: Vec<ObjectId> = t.players[2].ghosts.iter().map(|g| g.id).collect();
        assert_eq!(ids, [object_id(-6), object_id(-4), object_id(-10)]);
        assert_eq!(t.recycling.next_id, object_id(-11));
        // each new one destroyed the viewer's twin before it
        let states: Vec<ObjectState> = t.players[2].ghosts.iter().map(|g| g.state).collect();
        assert_eq!(
            states,
            [
                ObjectState::Removed,
                ObjectState::Removed,
                ObjectState::Alive
            ]
        );
        // a building's ghost follows its original's UMV entry
        assert_eq!(t.players[2].ghosts[2].footprint, 1);
        let mut t = world(false);
        t.recycling.ghosts = vec![object_id(-4)];
        t.make_ghost(PlayerId::new(2), Position::new(2.5, 1.5, 0.0), original)
            .unwrap();
        assert_eq!(t.players[2].ghosts[0].id, object_id(100));
        assert_eq!((t.next_object_id.get(), t.recycling.ghosts.len()), (101, 1));
        // an original past state 6 is forgotten, its master kept
        let mut t = world(true);
        let gone = Original {
            state: ObjectState::Removed,
            ..original
        };
        t.make_ghost(PlayerId::new(2), Position::new(2.5, 1.5, 0.0), gone)
            .unwrap();
        let g = t.players[2].ghosts[0];
        assert_eq!(
            (g.original, g.original_id, g.original_master),
            (None, None, Some(HOUSE))
        );
    }

    #[test]
    fn the_twin_check_destroys_the_viewers_own_twin_and_marks_another_viewers() {
        // FUN_1410928a0: ghosts on the tile, in state 2, of the same original id
        let mut t = world(true);
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
        t.players[1].ghosts = vec![twin(
            object_id(-20),
            Some(object_id(9)),
            Position::new(2.1, 1.9, 0.0),
            ObjectState::Alive,
        )];
        t.players[2].ghosts = vec![
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
        t.check_twins(PlayerId::new(2), &mut ghost).unwrap();
        // the viewer's own twins on the tile are destroyed
        let states: Vec<ObjectState> = t.players[2].ghosts.iter().map(|g| g.state).collect();
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
            t.players[1].ghosts[0].cant_see,
            PlayerSet::of(PlayerId::new(2))
        );
        assert_eq!(ghost.cant_see, PlayerSet::of(PlayerId::new(1)));
        assert!(
            t.players[2]
                .ghosts
                .iter()
                .all(|g| g.cant_see == PlayerSet::NONE)
        );
    }

    #[test]
    fn a_ghost_without_an_original_keeps_its_master_and_one_not_recyclable_is_deleted() {
        // FUN_141090000: a null +0x7c0 sets only +0x7f8 = -1, +0x7d0 stays
        let mut t = world(false);
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
        t.players[2].ghosts = vec![ghost];
        t.update_ghosts(PlayerId::new(2)).unwrap();
        let g = t.players[2].ghosts[0];
        assert_eq!((g.original_id, g.original_master), (None, Some(HOUSE)));
        // an original past state 6 takes its master with it
        t.players[2].ghosts[0].original = Some(object_id(9));
        t.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Removed;
        t.update_ghosts(PlayerId::new(2)).unwrap();
        assert_eq!(t.players[2].ghosts[0].original_master, None);
        // states 7, then 8, then it leaves; a master not recyclable gives the pool nothing
        t.players[2].ghosts[0].state = ObjectState::Removed;
        t.update_ghosts(PlayerId::new(2)).unwrap();
        assert_eq!(t.players[2].ghosts[0].state, ObjectState::Gone);
        t.update_ghosts(PlayerId::new(2)).unwrap();
        assert!(t.players[2].ghosts.is_empty() && t.recycling.ghosts.is_empty());
        let mut t = world(true);
        t.players[2].ghosts = vec![Ghost {
            state: ObjectState::Gone,
            ..ghost
        }];
        t.update_ghosts(PlayerId::new(2)).unwrap();
        assert_eq!(t.recycling.ghosts, [object_id(100)]);
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
    fn a_ghost_its_viewer_sees_again_is_destroyed_and_the_owner_notes_itself() {
        let mut t = world(true);
        t.players[2].ghosts = vec![house_ghost(0)];
        set_tile(&mut t, 2, 1, 1 << 1);
        t.update_ghosts(PlayerId::new(2)).unwrap();
        let g = t.players[2].ghosts[0];
        assert_eq!(
            (g.state, g.bits, g.cant_see),
            (
                ObjectState::Alive,
                VisionBits::from_bits_retain(1 << 1),
                PlayerSet::of(PlayerId::new(1))
            )
        );
        t.players[2].ghosts[0].cant_see = PlayerSet::NONE;
        t.update_ghosts(PlayerId::new(2)).unwrap();
        assert_eq!(t.players[2].ghosts[0].cant_see, PlayerSet::NONE);
        set_tile(&mut t, 2, 1, (1 << 1) | (1 << 2));
        t.update_ghosts(PlayerId::new(2)).unwrap();
        assert_eq!(t.players[2].ghosts[0].state, ObjectState::Removed);
        let mut t = world(true);
        t.players[2].ghosts = vec![Ghost {
            seen: true,
            ..house_ghost(0)
        }];
        set_tile(&mut t, 2, 1, 1 << 1);
        t.update_ghosts(PlayerId::new(2)).unwrap();
        let g = t.players[2].ghosts[0];
        assert_eq!((g.state, g.bits), (ObjectState::Removed, VisionBits::NONE));
    }

    /// A ghost its viewer sees stays while a player sharing the viewer's sight (but not the
    /// owner's) does not see it: viewer 2 shares the sight of players 3 and 4, player 3 only the
    /// viewer's; the tile seen by player 4 alone keeps the ghost, by player 3 too destroys it.
    #[test]
    fn a_ghost_stays_while_a_player_sharing_the_viewer_s_sight_does_not_see_it() {
        let mut t = world(true);
        for _ in 0..2 {
            t.players.push(Player::default());
            t.masters.push(masters(true));
        }
        let own = |p| crate::vision::own_masks(p, false);
        let with = |p, bits: u64| {
            let mut masks = own(p);
            masks.visible |= VisionBits::from_bits_retain(bits);
            masks
        };
        t.spatial.vision = crate::players::ByPlayer::new(vec![
            own(PlayerId::new(0)),
            own(PlayerId::new(1)),
            with(PlayerId::new(2), (1 << 3) | (1 << 4)),
            with(PlayerId::new(3), 1 << 2),
            own(PlayerId::new(4)),
        ]);
        t.players[2].ghosts = vec![house_ghost(0)];
        set_tile(&mut t, 2, 1, 1 << 4);
        t.update_ghosts(PlayerId::new(2)).unwrap();
        let g = t.players[2].ghosts[0];
        assert_eq!(
            (g.state, g.bits, g.cant_see),
            (
                ObjectState::Alive,
                VisionBits::from_bits_retain(1 << 4),
                PlayerSet::of(PlayerId::new(1))
            )
        );
        set_tile(&mut t, 2, 1, (1 << 3) | (1 << 4));
        t.update_ghosts(PlayerId::new(2)).unwrap();
        assert_eq!(t.players[2].ghosts[0].state, ObjectState::Removed);
    }

    /// A building's ghost reads its footprint's tiles too (its UMV entry's offsets): seen there
    /// by its viewer while the original's mask and the ghost's own tile miss the viewer, it is
    /// marked seen, not destroyed, and the next update destroys it; without its UMV entry it is
    /// not modelled.
    #[test]
    fn a_building_s_ghost_seen_on_its_footprint_alone_is_marked_seen() {
        let mut t = world(true);
        let sight = t.spatial.sight.as_mut().unwrap();
        sight.extents = vec![crate::vision::visibility::Extent {
            guid: 1,
            offsets: vec![TileOffset::new(0, 0), TileOffset::new(1, 0)],
            capacity: 2,
        }];
        let mut registry = crate::vision::registry::Registry::default();
        let _ = registry.entries.insert(
            object_id(9),
            crate::vision::registry::Entry {
                mask: VisionBits::from_bits_retain(1 << 1),
                ..crate::vision::registry::Entry::default()
            },
        );
        sight.registry = Some(registry);
        t.players[2].ghosts = vec![house_ghost(1)];
        set_tile(&mut t, 2, 1, 1 << 1);
        set_tile(&mut t, 3, 1, 1 << 2);
        t.update_ghosts(PlayerId::new(2)).unwrap();
        let g = t.players[2].ghosts[0];
        assert_eq!(
            (g.state, g.seen, g.bits),
            (
                ObjectState::Alive,
                true,
                VisionBits::from_bits_retain((1 << 1) | (1 << 2))
            )
        );
        t.update_ghosts(PlayerId::new(2)).unwrap();
        assert_eq!(t.players[2].ghosts[0].state, ObjectState::Removed);
        let mut t = world(true);
        t.players[2].ghosts = vec![house_ghost(1)];
        assert_eq!(
            t.update_ghosts(PlayerId::new(2)).unwrap_err().reason(),
            "a building's ghost without its UMV entry"
        );
    }

    #[test]
    fn a_watched_list_gives_ghosts_to_the_players_who_lost_sight_and_tells_a_computer() {
        // FUN_1410905e0: lost = ((old ^ new) & old) >> 1, from player 1, not the list's player
        let mut t = world(true);
        set_tile(&mut t, 1, 2, 0x0000_0007_0000_0007);
        assert_eq!(
            t.watch(PlayerId::new(0), object_id(5), Tile::new(1, 2)),
            Ok(true)
        );
        set_tile(&mut t, 1, 2, 0x0000_0007_0000_0001);
        t.watch_pass(PlayerId::new(0)).unwrap();
        assert_eq!(t.players[1].ghosts.len(), 1);
        assert_eq!(t.players[2].ghosts.len(), 1);
        assert_eq!(
            t.players[0].watched[0].bits,
            VisionBits::from_bits_retain(0x0000_0007_0000_0001)
        );
        // a computer viewer gets its ghost, then its AI's notify is not modelled
        let mut t = world(true);
        t.players[2].computer = true;
        set_tile(&mut t, 1, 2, 0x0000_0007_0000_0005);
        assert_eq!(
            t.watch(PlayerId::new(0), object_id(5), Tile::new(1, 2)),
            Ok(true)
        );
        set_tile(&mut t, 1, 2, 0x0000_0007_0000_0001);
        assert!(t.watch_pass(PlayerId::new(0)).is_err());
        assert_eq!(t.players[2].ghosts.len(), 1);
        // an entry whose object is gone leaves the list
        let mut t = world(true);
        set_tile(&mut t, 1, 2, 0x0000_0007_0000_0007);
        assert_eq!(
            t.watch(PlayerId::new(0), object_id(5), Tile::new(1, 2)),
            Ok(true)
        );
        t.players[0].objects.clear();
        set_tile(&mut t, 1, 2, 0x0000_0007_0000_0001);
        t.watch_pass(PlayerId::new(0)).unwrap();
        assert_eq!(t.players[0].watched.len(), 0);
        assert_eq!(t.players[1].ghosts.len(), 0);
    }

    #[test]
    fn a_load_watches_dead_objects_that_leave_ghosts_and_buildings_but_their_annexes() {
        // FUN_14119df60's dead objects, FUN_1411ac170's FUN_1411cc980 with no annex link yet,
        // then the rehook drops every annex but a gate's
        let mut t = world(true);
        let gate = Master {
            id: master_id(88),
            kind: BUILDING,
            class: class::GATE,
            ..Master::default()
        };
        let annex_master = Master {
            id: master_id(89),
            kind: BUILDING,
            class: class::BUILDING,
            ..Master::default()
        };
        let wonder = Master {
            id: ids::WONDER,
            kind: BUILDING,
            class: class::CLASS_51,
            ..Master::default()
        };
        let skipped = Master {
            id: MASTER_1808,
            kind: BUILDING,
            class: class::BUILDING,
            ..Master::default()
        };
        t.masters = (0..3)
            .map(|_| {
                Masters::new(masters(true).iter().cloned().chain([
                    gate.clone(),
                    annex_master.clone(),
                    wonder.clone(),
                    skipped.clone(),
                ]))
            })
            .collect();
        // Gaia's mine 5 dead (state 3), house 9's annexes 10 (a gate) and 11, a class-51
        // building 12, master 1808's 13, a desolid foundation 14 in the sleeping list
        t.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Dying;
        let mut g = object(
            10,
            master_id(88),
            ObjectState::Alive,
            Position::new(2.5, 2.5, 0.0),
        );
        g.annex_of = Some(object_id(9));
        let mut a = object(
            11,
            master_id(89),
            ObjectState::Alive,
            Position::new(3.5, 2.5, 0.0),
        );
        a.annex_of = Some(object_id(9));
        t.players[1].objects.extend([
            g,
            a,
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
        ]);
        let mut foundation = object(14, HOUSE, ObjectState::Alive, Position::new(0.5, 3.5, 0.0));
        foundation.construction = Some(crate::construction::build::Construction {
            progress: Seconds::ZERO,
            desolid: true,
            builders: 0,
            hit_points: 0,
            build_time: 0,
            record: crate::map::spatial::Obstruction {
                kind: ObstructionType::TYPE_2,
                class: ObstructionClass::UNIT,
            },
            snow: false,
            sprite_override: false,
        });
        t.players[1].sleeping.push(foundation);
        t.build_watched().unwrap();
        let ids = |t: &World, p: usize| {
            t.players[p]
                .watched
                .iter()
                .map(|w| w.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&t, 0), [object_id(5)]);
        assert_eq!(ids(&t, 1), [object_id(9), object_id(10)]);
        // the house follows its mask (its UMV entry), the gate its tile
        assert_eq!(t.players[1].watched[0].cell, Cell::Mask(object_id(9)));
        assert_eq!(t.players[1].watched[1].cell, Cell::Tile(2 * 4 + 2));
        // a building in the game, its annex linked, is not watched as an annex
        assert!(!t.watches_building(PlayerId::new(1), object_id(11)).unwrap());
        assert!(t.watches_building(PlayerId::new(1), object_id(10)).unwrap());
    }
}
