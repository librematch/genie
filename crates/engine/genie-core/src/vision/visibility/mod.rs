//! What each player sees: the players' `RGE_Visible_Map`s and the map's `RGE_Unified_Visible_Map`
//! (both saved), which the objects' lines of sight keep.
//!
//! A player's map counts, per tile, the player's objects whose line of sight covers the tile: -1
//! for a tile the player never saw, 0 for one it saw before, more for one it sees now. The unified
//! map holds, per tile, bit `p` while player `p` sees the tile and bit `32 + p` once it saw it. An
//! object's sight comes and goes with its tile (`RGE_Static_Object::new_position`, `0x1411a58b0`),
//! its master (`FUN_1411a6a70`), its death (`FUN_14119d710`) and a building's construction
//! (`FUN_1411e5670`, `FUN_1411dbb70`).
//!
//! The map's tiles also link a building's tiles to its anchor tile (the map tile's `+0x19` and
//! `+0x1a`, `FUN_140f73900`), and the map keeps a list of the buildings' extents (the saved UMV
//! data list, map `+0x5e00`): a player whose sight first reaches a tile of a building whose anchor
//! it never saw sees the building's tiles as explored (`FUN_141285da0`).
//!
//! Each player's tile list (player `+0x180`) holds the tiles its map explored since its update
//! last ended, whose objects that gather by themselves join its visible-resource lists
//! ([`VisibleResources`], player `+0x2f0`).
//!
//! Not held: the tile's fog byte (`+0x17`, for the local player's view), the minimap's and the
//! renderer's dirty tiles (`FUN_141285c70`'s `+0x1f0`, `FUN_141778750`), the
//! cheat that reveals the map (`DAT_143f68a30`, probed 0), shared sight (`+0x798`, `+0x79c`, 0 in
//! every object probed) and the line-of-sight powerup (`+0xad4`, unsaved, 0 after a load).

use crate::GenieError;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::map::TileOffset;
use crate::objects::masters::ClassId;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::{ObjectId, ObjectKey};
use crate::ownership::CaptureKind;
use crate::players::PlayerId;
use crate::players::PlayerType;
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::vision::registry::Registry;
use crate::world::slots::CheckedIndex;

mod building_links;
mod explore;
mod object_events;
mod sight;

pub use self::object_events::sight_taker;
pub use self::sight::LosSight;
pub use self::sight::MasterChange;
use crate::math::fmath::{at_most_or_unordered, sqrt};
use crate::math::image::visibility::EDGE as VISIBILITY_EDGE;
use crate::objects::{IdTable, ObjectState};
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use std::mem::take;

/// The map tile's `+0x19` bit of a building's tile that is not its anchor (`FUN_140f73900`).
const LINKED: u8 = 0x20;
/// The map tile's `+0x19` bit of a building's anchor tile, whose `+0x1a` is the building's entry
/// in the UMV list, from 1.
const ANCHOR: u8 = 0x40;

/// `DAT_1439e6800`: a direction nibble's step back to the anchor.
const STEP: [i32; 16] = [0, 1, 2, 3, 4, 5, 6, 7, -8, -7, -6, -5, -4, -3, -2, -1];

/// The cap of a tile's count (`explore_terrain`).
const MOST: i16 = 0x7ffe;

/// `DAT_142eb99cc`: a footprint's high edge, less this, is its last tile.
const EDGE: f32 = VISIBILITY_EDGE;

/// One player's `RGE_Visible_Map` (player `+0x70`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibleMap {
    /// Per tile, row by row (`+0x18`).
    pub counts: Vec<i16>,
    /// `numberTilesExploredValue` (`+0x28`).
    pub explored: TileCount,
}

/// A building's sight state (`TRIBE_Building_Object`: its slots 0x1d0 `FUN_1411d0190` and 0x1d8
/// `FUN_1411e6f60`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BuildingSight {
    /// `built` (`+0xca1`): the construction completed, and the sight is the line of sight.
    pub built: bool,
    /// `build_pts` (`+0xc50`) is not 0: a foundation's construction started, and its sight is its
    /// footprint.
    pub started: bool,
}

/// What an object's sight reads of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seer {
    /// The object's player.
    pub player: PlayerId,
    /// The master's id (`+0x58`, decoded).
    pub master: MasterId,
    /// The master's `line_of_sight` (`+0x5d0`); slot 0x210 adds the object's powerup (`+0xad4`).
    pub los: Distance,
    /// `object_state` (`+0x640`): only state 2 sees from a new tile.
    pub state: ObjectState,
    /// `+0x684`: the object explored since it was made (unsaved; a load sets it, `FUN_14116a500`,
    /// and a foundation made in the game has it clear until its construction starts). Only a
    /// building's slot 0x1d8 reads it.
    pub explored: bool,
    /// A building's sight, which its construction changes.
    pub building: Option<BuildingSight>,
    /// A building's UMV entry (`+0x904`, from 1; 0 for none): the extra tiles its mask reads.
    pub umv: u8,
    /// `+0x924` (`FUN_14121df60`): 1 for an object another player takes over by seeing it, 3
    /// for livestock.
    pub capture: CaptureKind,
    /// The object's type (`+0xf`): 70 a combat unit, 80 a building, below 70 anything else.
    pub kind: MasterKind,
}

/// An entry of the map's UMV data list (map `+0x5e08`, `+0x5e10`): a building's extent around
/// its anchor, packed as bytes (anchor less low x, anchor less low y, high x less anchor, high y
/// less anchor), and the offsets of the tiles the visible-object updator checks for it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extent {
    /// The guid of its entry.
    pub guid: u32,
    /// The offsets of the tiles the visible-object updater checks for it.
    pub offsets: Vec<TileOffset>,
    /// The offsets' capacity (the entry's second word).
    pub capacity: i32,
}

/// A record of a player's visible-resource lists (`VISIBLE_RESOURCE_REC`, 32 bytes, saved).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VisibleResource {
    /// The object (`+0xc`).
    pub id: ObjectId,
    /// Its distance from the point of the last search that chose it (`+0x10`,
    /// [`VisibleResources::closest`]); 0 when added.
    pub distance: Distance,
    /// `+0x14`, HD's `zone`: 0 when added.
    pub zone: u8,
    /// The object's place when added (`+0x18`, `+0x1c`).
    pub at: Point,
}

/// A player's `Visible_Resource_Manager` (player `+0x2f0`, saved): per resource group (a master's
/// `resource_gather_group`, `+0x683`) the objects of masters that gather by themselves
/// (`enable_auto_gather`, `+0x681`) the player found on the tiles it explored.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VisibleResources {
    /// `VR_ListSize`: each list's capacity, which a full list's next record doubles.
    pub capacities: Vec<i32>,
    /// The lists (`num_visible_resource_lists` of them), `VR_ListUsed` records each.
    pub lists: Vec<Vec<VisibleResource>>,
}

impl VisibleResources {
    /// `AddResource` (`FUN_141288830`) of the object `id` at (`x`, `y`): a manager with lists
    /// puts its record last in list `group`, doubling a full list's capacity. The masters' groups
    /// are 0 to 4 and every manager holds 5 lists; the game writes past its lists for a group out
    /// of range.
    pub fn add(&mut self, id: ObjectId, group: GatherGroup, at: Point) {
        let g = group.index();
        let (Some(list), Some(capacity)) = (self.lists.get_mut(g), self.capacities.get_mut(g))
        else {
            return;
        };
        if usize::try_from(*capacity).is_ok_and(|c| c == list.len()) {
            *capacity *= 2;
        }
        list.push(VisibleResource {
            id,
            distance: Distance::ZERO,
            zone: 0,
            at,
        });
    }

    /// `Remove_Resource` (`FUN_141289b80`): the first record of the object `id` in list `group`
    /// takes the last record and the list shortens. Its index; 0 for an empty list, -1 when the
    /// object is not in it or the manager has no lists.
    pub fn remove(&mut self, id: ObjectId, group: GatherGroup) -> i32 {
        let Some(list) = self.lists.get_mut(group.index()) else {
            return -1;
        };
        if list.is_empty() {
            return 0;
        }
        let Some(i) = list.iter().position(|r| r.id == id) else {
            return -1;
        };
        let _ = list.swap_remove(i);
        i as i32
    }

    /// `Get_Closest_Resource` (`FUN_141289000`): the record of list `group` nearest (`x`, `y`)
    /// (squared, the first of equals) whose object is not in `skip`; its distance becomes the
    /// root. None for a group out of range or an empty list.
    pub fn closest(&mut self, at: Point, group: GatherGroup, skip: &[ObjectId]) -> Option<usize> {
        let list = self.lists.get_mut(group.index())?;
        let (x, y) = (at.x(), at.y());
        // COMISS r, p; JBE: a record at or below the point (or NaN) measures p - r
        let gap = |r: f32, p: f32| {
            if at_most_or_unordered(r, p) {
                p - r
            } else {
                r - p
            }
        };
        let mut best = None;
        let mut nearest = f32::MAX;
        for (i, r) in list.iter().enumerate() {
            let (dx, dy) = (gap(r.at.x(), x), gap(r.at.y(), y));
            let d = dy * dy + dx * dx;
            if d < nearest && !skip.contains(&r.id) {
                best = Some(i);
                nearest = d;
            }
        }
        if let Some(i) = best {
            list.get_mut(i)?.distance = Distance::new(sqrt(nearest));
        }
        best
    }
}

/// The players' visible maps, the unified map, the map tiles' building links and the objects'
/// sight.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Visibility {
    /// The map's size in tiles.
    pub size: GridSize,
    /// By player index (Gaia 0).
    pub maps: Vec<VisibleMap>,
    /// Map `+0x5e30`, row by row.
    pub unified: Vec<VisionBits>,
    /// Per tile, the `+0x19` link bits and the `+0x1a` byte (a direction to the anchor, or the
    /// anchor's UMV entry).
    links: Vec<(u8, u8)>,
    /// The UMV list: the entries of the buildings' extents.
    pub extents: Vec<Extent>,
    /// The UMV list's capacity (`+0x5e04`): it grows by 8.
    pub extents_capacity: i32,
    /// Each player's type: a computer player's (type 3) map tells its AI of a newly explored
    /// tile (its slot 0x88, [`crate::players::ai_module`]).
    pub player_types: Vec<PlayerType>,
    /// The tiles a computer player's map explored that its AI module has not heard of yet, in
    /// order ([`Visibility::take_ai_tiles`]).
    ai_tiles: Vec<(PlayerId, Tile)>,
    seers: IdTable<Seer>,
    /// The objects-in-sight registry, when the slice holds it (the save does not).
    pub registry: Option<Registry>,
    /// Each player's tile list (`RGE_Tile_List`, player `+0x180`, not saved): the tiles its map
    /// explored (`FUN_14165d500`) since its last update ended, in order, which the update's
    /// `Process_New_Tiles` reads; its count of new tiles (`+8`) only a computer player's AI
    /// reads.
    pub new_tiles: Vec<Vec<Tile>>,
    /// Each player's visible-resource lists.
    pub resources: Vec<VisibleResources>,
    /// The first part of the game the slice met and does not model, for its caller.
    unsupported: Option<GenieError>,
}

/// `+0x924` as a combat object's setup sets it (`FUN_14121df60`): 1 for Gaia's objects other
/// than its animals, buildings and a few classes (those of 40, 42, 58, 14, 11, 1, 60, 38, 41, 9
/// and 10, and master 826), and for masters 854 and Gaia's 814, which another player takes over
/// by seeing them; 3 for classes 58 and 1 (livestock).
#[must_use]
pub fn capture_kind(
    player: PlayerId,
    class: ClassId,
    master: MasterId,
    roles: &Roles,
) -> CaptureKind {
    let mut kind = CaptureKind::NONE;
    if (player == PlayerId::GAIA
        && !roles.of_class(class).contains(ClassRoles::GAIA_KEPT)
        && !roles.of_master(master).contains(MasterRoles::MONUMENT))
        || roles
            .of_master(master)
            .contains(MasterRoles::TORCH_CONVERTING)
    {
        kind = CaptureKind::BY_SIGHT;
    }
    if player == PlayerId::GAIA && roles.of_master(master).contains(MasterRoles::HORSE) {
        kind = CaptureKind::BY_SIGHT;
    }
    if roles.of_class(class).contains(ClassRoles::LIVESTOCK)
        || roles.of_class(class).contains(ClassRoles::ARTIFACT)
    {
        kind = CaptureKind::LIVESTOCK;
    }
    kind
}

/// The tile of a position as `new_position` files the object (`0x1411a58b0`): the position held
/// on the map (`DAT_142eb1e9c` below its size), the tile held on the map.
fn tile_of(v: f32, size: i32) -> i32 {
    let v = if v < 0.0 { 0.0 } else { v };
    let v = if size as f32 <= v {
        size as f32 - 0.001
    } else {
        v
    };
    (v as i32).clamp(0, size - 1)
}

impl Visibility {
    /// The saved maps of a map of `size`, with the saved UMV list.
    #[must_use]
    pub fn new(
        size: GridSize,
        maps: Vec<VisibleMap>,
        unified: Vec<VisionBits>,
        extents: Vec<Extent>,
        extents_capacity: i32,
        player_types: Vec<PlayerType>,
    ) -> Self {
        let n = (size.width().max(0) * size.height().max(0)) as usize;
        let players = maps.len();
        Self {
            size,
            maps,
            unified,
            links: vec![(0, 0); n],
            extents,
            extents_capacity,
            player_types,
            ai_tiles: Vec::new(),
            seers: IdTable::new(),
            registry: None,
            new_tiles: vec![Vec::new(); players],
            resources: vec![VisibleResources::default(); players],
            unsupported: None,
        }
    }

    /// `tile`'s place in the maps, row by row; unchecked, as the game reads them.
    const fn index(&self, tile: Tile) -> usize {
        (tile.y() * self.size.width() + tile.x()) as usize
    }

    /// The first unmodelled case met since the last call.
    pub const fn take_unsupported(&mut self) -> Option<GenieError> {
        self.unsupported.take()
    }

    /// The tiles a computer player's map explored since the last call, as (player, tile).
    pub fn take_ai_tiles(&mut self) -> Vec<(PlayerId, Tile)> {
        take(&mut self.ai_tiles)
    }

    fn unsupported(&mut self, what: &'static str) {
        let _ = self
            .unsupported
            .get_or_insert_with(|| GenieError::unsupported(what));
    }

    /// `RGE_Tile_List::add_node` (`FUN_14165d500`) on player `player`'s tile list.
    fn new_tile(&mut self, player: PlayerId, tile: Tile) {
        if let Some(list) = self.new_tiles.get_mut(player.index()) {
            list.push(tile);
        }
    }

    /// `RGE_Tile_List::del_list` (`FUN_141661030`) on player `player`'s tile list, after the
    /// player's update (`BaseWorld::update`); the list is never frozen in a game (`+0x10`).
    pub fn clear_new_tiles(&mut self, player: PlayerId) {
        if let Some(list) = self.new_tiles.get_mut(player.index()) {
            list.clear();
        }
    }

    /// `FUN_1411980b0`'s visible resources, for an object of a master that gathers by itself
    /// (`+0x681`) of group `group` at `position`: from player 0 up while players remain to be
    /// found in the explored bits of the unified map at its tile (bits 32 to 56), each player
    /// there (`FUN_14152b700`: one the world has) loses the object from its list
    /// ([`VisibleResources::remove`]).
    pub fn forget_resource(
        &mut self,
        id: ObjectId,
        position: Position,
        group: GatherGroup,
    ) -> Result<(), GenieError> {
        let tile = Tile::new(position.x() as i32, position.y() as i32);
        if !self.size.holds(tile) {
            return Ok(());
        }
        let word = *self.unified.checked(self.index(tile))? & VisionBits::OBJECT_MASK;
        for (p, r) in PlayerId::each(self.resources.len()).zip(&mut self.resources) {
            if word.intersects(VisionBits::explored(p)) {
                let _ = r.remove(id, group);
            }
        }
        Ok(())
    }

    /// The object `id` comes into the players' sight with `seer`.
    pub fn add_seer(&mut self, id: ObjectId, seer: Seer) {
        let _ = self.seers.insert(id, seer);
    }

    /// The object `id` leaves the sight.
    pub fn remove_seer(&mut self, id: ObjectId) {
        let _ = self.seers.remove(id);
    }

    /// What the object `id`'s sight reads of it.
    #[must_use]
    pub fn seer(&self, id: impl ObjectKey) -> Option<&Seer> {
        self.seers.get(id.key()?)
    }

    /// The same, to change.
    pub fn seer_mut(&mut self, id: ObjectId) -> Option<&mut Seer> {
        self.seers.get_mut(id)
    }

    /// The link bits and byte of `tile`.
    pub fn link(&self, tile: Tile) -> Result<(u8, u8), GenieError> {
        Ok(*self.links.checked(self.index(tile))?)
    }
}

#[cfg(test)]
mod tests;
