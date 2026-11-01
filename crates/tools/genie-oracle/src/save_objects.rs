//! A save's live objects and the tick oracle's dumps beside it; the slice started from them
//! ([`objects_from_save`]).

use crate::masters::MasterTable;
use crate::{Error, listed, run_from_save};
use genie_core::GenieError;
use genie_core::map::Distance;
use genie_core::map::Tile;
use genie_core::objects::ListIndex;
use genie_core::objects::ObjectId;
use genie_core::objects::unit::Unit;
use genie_core::vision::VisionBits;
use genie_core::vision::visibility::Visibility;
use genie_core::world::World;
use genie_save::generated::world::RgeStaticObject;

/// A slice run from a save ([`objects_from_save`]): one player's objects after it, and the first
/// update the slice could not model, if any.
#[derive(Clone, Debug)]
pub struct SliceRun {
    /// Every listed object the player holds after the run.
    pub objects: Vec<Unit>,
    /// The first update the slice could not model: the tick, the object's index and why.
    pub unsupported: Option<(u32, ListIndex, GenieError)>,
    /// The id of the object whose update was not modelled.
    pub unsupported_id: Option<ObjectId>,
    /// Every player's first unsupported update, by player.
    pub stopped: Vec<Option<(u32, ListIndex, GenieError)>>,
    /// What the players see after the run.
    pub visibility: Option<Visibility>,
    /// The map's terrain after the run.
    pub terrain: Option<genie_core::map::terrain::Terrain>,
    /// The objects the slice stopped updating, of every player.
    pub frozen: Vec<ObjectId>,
    /// Every object's freeze, by player: the tick, the object and why.
    pub freezes: Vec<Vec<(u32, ObjectId, GenieError)>>,
}

/// An object's unit group link as saved (`mGroupID`, `mGroupFlag`).
pub(super) const fn group_link(s: &RgeStaticObject) -> genie_core::groups::GroupLink {
    genie_core::groups::GroupLink {
        id: genie_core::groups::GroupId::of_field(s.group_id),
        flag: s.group_flag,
    }
}

/// What the tick oracle dumps of the game at a save's tick that the save does not hold.
#[derive(Clone, Copy, Debug, Default)]
pub struct Dumps<'a> {
    /// The shape mesh with each object's shape (`load_oracle/probes/mesh.py`); without it a path
    /// request stops the slice where it needs the short pather.
    pub mesh: Option<&'a str>,
    /// The objects-in-sight registry and the objects' masks (`load_oracle/probes/registry.py`);
    /// without it the slice keeps no registry, and a unit AI's search stops it.
    pub registry: Option<&'a str>,
    /// The map tiles' object lists in the game's order ([`tile_lists_from_dump`]); without it
    /// the slice keeps the order of its load.
    pub tiles: Option<&'a str>,
    /// The world's pools of recycled objects ([`recycle_pools_from_dump`]); without it the slice
    /// starts them empty, as a game loaded from the save does.
    pub pools: Option<&'a str>,
    /// The objects' inside lists in the game's order ([`inside_lists_from_dump`]); without it
    /// the slice keeps the order of the objects, as a game loaded from the save does.
    pub inside: Option<&'a str>,
    /// The process globals the game reads and no save holds ([`groups::globals_from_dump`]);
    /// without it the slice takes what a load gives (the group command counter past the saved
    /// groups', the scratch array zero).
    ///
    /// [`groups::globals_from_dump`]: crate::groups::globals_from_dump
    pub globals: Option<&'a str>,
}

/// The objects' inside lists as the tick oracle dumps them (`state_dumps.inside_lists_text`):
/// `container id...` per object with objects inside, in the list's order.
pub fn inside_lists_from_dump(text: &str) -> Result<Vec<(ObjectId, Vec<ObjectId>)>, Error> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let words = l
                .split_whitespace()
                .map(|w| {
                    w.parse::<i32>()
                        .ok()
                        .and_then(ObjectId::of_field)
                        .ok_or(Error::BadTileDump)
                })
                .collect::<Result<Vec<_>, _>>()?;
            match words[..] {
                [c, ..] => Ok((c, words[1..].to_vec())),
                [] => Err(Error::BadTileDump),
            }
        })
        .collect()
}

/// The pool of recycled missiles (type 60, `world+0x1d8`) as the tick oracle dumps it
/// (`state_dumps.recycle_pools_text`: per type a `pool TYPE COUNT` line, then `ID FLYING` per
/// object from the list's first, the next to come back), in the slice's order
/// ([`genie_core::world::Recycling::missiles`]: the next to come back last). The slice recycles no
/// object of another type; their pools are not read.
pub fn recycle_pools_from_dump(text: &str) -> Result<Vec<(ObjectId, bool)>, Error> {
    let mut missiles = Vec::new();
    let mut kind = None;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words[..] {
            ["pool", k, _] => kind = Some(k.parse::<i32>().map_err(|_| Error::BadPoolDump)?),
            [id, flying] if kind == Some(60) => missiles.push((
                pooled_id(id)?,
                flying.parse::<u8>().map_err(|_| Error::BadPoolDump)? != 0,
            )),
            [_, _] if kind.is_some() => {}
            _ => return Err(Error::BadPoolDump),
        }
    }
    missiles.reverse();
    Ok(missiles)
}

/// The id a pool dump line names.
fn pooled_id(text: &str) -> Result<ObjectId, Error> {
    let value = text.parse::<i32>().map_err(|_| Error::BadPoolDump)?;
    ObjectId::try_from(value).map_err(|_| Error::BadPoolDump)
}

/// The world's pool of recycled ghosts (type 25) as the tick oracle dumps it
/// (`state_dumps.recycle_pools_text`): their ids, the last to come back first in the slice's
/// order ([`genie_core::world::Recycling::ghosts`]).
pub fn ghost_pool_from_dump(text: &str) -> Result<Vec<ObjectId>, Error> {
    let mut ghosts = Vec::new();
    let mut kind = None;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words[..] {
            ["pool", k, _] => kind = Some(k.parse::<i32>().map_err(|_| Error::BadPoolDump)?),
            [id, _]
                if kind
                    == Some(i32::from(
                        genie_core::objects::masters::master_type::DOPPELGANGER.get(),
                    )) =>
            {
                ghosts.push(pooled_id(id)?);
            }
            [_, _] if kind.is_some() => {}
            _ => return Err(Error::BadPoolDump),
        }
    }
    ghosts.reverse();
    Ok(ghosts)
}

/// The map tiles' object lists as the tick oracle dumps them (`state_dumps.tile_lists_text`):
/// `x y id...` per tile of two objects or more, in the tile's order (an id signed, or its 32
/// bits unsigned as the first dumps wrote a missile's).
pub fn tile_lists_from_dump(text: &str) -> Result<Vec<(Tile, Vec<ObjectId>)>, Error> {
    let word = |v: &str| {
        v.parse::<i64>()
            .ok()
            .filter(|&n| i64::from(i32::MIN) <= n && n <= i64::from(u32::MAX))
            .map(|n| n as u32 as i32)
            .ok_or(Error::BadTileDump)
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let words = l
                .split_whitespace()
                .map(word)
                .collect::<Result<Vec<_>, _>>()?;
            match words[..] {
                [x, y, ..] => Ok((Tile::new(x, y), ids(&words[2..])?)),
                _ => Err(Error::BadTileDump),
            }
        })
        .collect()
}

/// A tile list dump's ids, which name no none (−1).
fn ids(words: &[i32]) -> Result<Vec<ObjectId>, Error> {
    words
        .iter()
        .map(|&w| ObjectId::of_field(w).ok_or(Error::BadTileDump))
        .collect()
}

/// The tiles whose objects the slice holds as the game does (the dump's `game`) but in another
/// order, as differences: `tile (x, y) order slice [...], the game [...]`. Tiles whose objects
/// differ are left out: their objects' places differ.
pub(super) fn tile_order_differences(
    spatial: &genie_core::map::spatial::Spatial,
    game: &[(Tile, Vec<ObjectId>)],
) -> Vec<String> {
    game.iter()
        .filter_map(|(tile, ids)| {
            let (x, y) = (tile.x(), tile.y());
            let mine = spatial.tiles().list(*tile);
            let (mut a, mut b) = (mine.to_vec(), ids.clone());
            a.sort_unstable();
            b.sort_unstable();
            (a == b && mine != &ids[..])
                .then(|| format!("tile ({x}, {y}) order slice {mine:?}, the game {ids:?}"))
        })
        .collect()
}

/// The objects-in-sight registry as the tick oracle dumps it (`load_oracle/probes/registry.py`):
/// a `registry PLAYERS CLASSES` line, a `list PLAYER CLASS COUNT CAPACITY` line before each list's
/// `rec ID DIST OWNER X Y RX RY` lines (the floats as hex bits), then `mask ID HEX SLOTS...` per
/// combat object (its record's index in each player's list of its class, -1 for none).
pub fn registry_from_dump(text: &str) -> Result<genie_core::vision::registry::Registry, Error> {
    use genie_core::vision::registry::{Entry, Record, Registry};
    let bad = || Error::BadRegistryDump;
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().ok_or_else(bad)?.split_whitespace().collect();
    let [_, players, classes] = head[..] else {
        return Err(bad());
    };
    let number = |w: &str| w.parse::<i64>().map_err(|_| bad());
    let object = |w: &str| {
        w.parse::<i32>()
            .ok()
            .and_then(ObjectId::of_field)
            .ok_or_else(bad)
    };
    let bits = |w: &str| {
        u32::from_str_radix(w, 16)
            .map(f32::from_bits)
            .map_err(|_| bad())
    };
    let mut out = Registry::new(number(players)? as usize, number(classes)? as usize);
    let mut list: Option<(usize, usize)> = None;
    for line in lines {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.first().copied() {
            Some("list") if w.len() == 5 => {
                let (p, c) = (number(w[1])? as usize, number(w[2])? as usize);
                if out.players <= p || out.classes <= c {
                    return Err(bad());
                }
                list = Some((p, c));
            }
            Some("rec") if w.len() == 8 => {
                let (p, c) = list.ok_or_else(bad)?;
                out.lists[p][c].push(Record {
                    id: object(w[1])?,
                    distance: Distance::new(bits(w[2])?),
                    owner: genie_core::players::PlayerId::new(number(w[3])? as u8),
                    x: bits(w[4])?,
                    y: bits(w[5])?,
                    rx: bits(w[6])?,
                    ry: bits(w[7])?,
                });
            }
            Some("mask") if w.len() == 3 + genie_core::vision::registry::SLOTS => {
                let mut entry = Entry {
                    mask: VisionBits::from_bits_retain(
                        u64::from_str_radix(w[2], 16).map_err(|_| bad())?,
                    ),
                    ..Entry::default()
                };
                for (k, slot) in entry.slots.iter_mut().enumerate() {
                    let v = number(w[3 + k])?;
                    *slot = usize::try_from(v).ok();
                }
                let _ = out.entries.insert(object(w[1])?, entry);
            }
            _ => return Err(bad()),
        }
    }
    Ok(out)
}

/// The slice started from the tick oracle's save at tick `start_tick` (`--snapshot`), run through
/// the `recording`'s next `ticks` ticks: player `id`'s objects after it. The save gives the state,
/// the recording the ticks' lengths and commands, `dumps` what the save does not hold. The tick
/// oracle counts ticks from 0, as its trace does: its tick N is the game's update N + 1, and the
/// save at tick N holds the state after it.
pub fn objects_from_save(
    save: &[u8],
    recording: &[u8],
    start_tick: usize,
    ticks: usize,
    id: u32,
    masters: Option<&MasterTable>,
    dumps: Dumps<'_>,
) -> Result<SliceRun, Error> {
    let mut turns = run_from_save(save, recording, start_tick, ticks, masters, dumps)?;
    if turns.players.len() <= id as usize {
        return Err(Error::NoSuchPlayer(id));
    }
    let player = &turns.players[id as usize];
    Ok(SliceRun {
        objects: player
            .objects
            .iter()
            .filter(|u| listed(u))
            .cloned()
            .collect(),
        unsupported: player.unsupported.clone(),
        unsupported_id: player
            .unsupported
            .as_ref()
            .and_then(|&(_, i, _)| player.objects.get(i).map(|u| u.id)),
        stopped: turns
            .players
            .iter()
            .map(|p| p.unsupported.clone())
            .collect(),
        visibility: turns.spatial.sight.take(),
        terrain: turns.spatial.terrain.take(),
        frozen: turns
            .players
            .iter()
            .flat_map(|p| p.objects.iter().filter(|u| p.stopped || u.stopped))
            .map(|u| u.id)
            .collect(),
        freezes: turns.players.iter().map(|p| p.freezes.clone()).collect(),
    })
}

/// The slice as it starts from the tick oracle's save at tick `start_tick` with `dumps`, before
/// any update, its world told what it reads of the units (as an update starts). Dev only.
pub fn turns_from_save(
    save: &[u8],
    recording: &[u8],
    start_tick: usize,
    masters: Option<&MasterTable>,
    dumps: Dumps<'_>,
) -> Result<World, Error> {
    let mut turns = run_from_save(save, recording, start_tick, 0, masters, dumps)?;
    turns.sync_world();
    Ok(turns)
}

#[cfg(test)]
mod tests;
