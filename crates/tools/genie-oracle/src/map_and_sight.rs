//! The map and the players' sight against the game's: the terrain's differences, the forced
//! replay's saved step, the visible maps and resources.

use crate::masters::MasterTable;
use crate::{game_state, header_objects, saved_ghosts};
use genie_core::GenieResult;
use genie_core::map::terrain::Terrain;
use genie_core::map::terrain::TileTerrain;
use genie_core::players::PlayerId;
use genie_core::vision::VisionBits;
use genie_core::vision::ghost::Ghost;
use genie_core::vision::visibility::{Visibility, VisibleResource};
use genie_core::world::GameState;
use genie_core::world::load::map::saved_sight;
use genie_core::world::parts::Object;
use genie_rec::de::chapters;
use genie_save::Header;

/// The differences between the slice's map terrain and a save's, tile by tile.
pub fn terrain_differences(slice: &Terrain, save: &[u8]) -> GenieResult<Vec<String>> {
    let chapter = chapters(save)?[0];
    Ok(header_terrain_differences(slice, &chapter.header()?))
}

/// [`terrain_differences`] of a read header.
pub(super) fn header_terrain_differences(slice: &Terrain, header: &Header) -> Vec<String> {
    let (width, tiles) = saved_terrain(header);
    tile_differences(slice, width, &tiles)
}

/// A save's terrain per tile, row by row: the terrain, the terrain to restore, the layer; and the
/// map's width.
pub(super) fn saved_terrain(header: &Header) -> (usize, Vec<(u8, u8, i16)>) {
    let map = &header.world.base_world.map;
    let tiles = map
        .rows
        .iter()
        .flat_map(|r| {
            r.tiles
                .iter()
                .map(|t| (t.terrain_kind, t.restore_terrain_kind, t.layer))
        })
        .collect();
    (map.map_width as usize, tiles)
}

/// The tiles whose terrain differs between the slice and the game's.
pub(super) fn tile_differences(
    slice: &Terrain,
    width: usize,
    tiles: &[(u8, u8, i16)],
) -> Vec<String> {
    let mut out = Vec::new();
    for (i, &theirs) in tiles.iter().enumerate() {
        let ours = TileTerrain {
            kind: slice.kinds[i],
            restore: slice.restore[i],
            layer: slice.layers[i],
        };
        let theirs = TileTerrain::of_save(theirs.0, theirs.1, theirs.2);
        if ours != theirs {
            let (x, y) = (i % width, i / width);
            out.push(format!(
                "terrain tile ({x}, {y}): slice {ours:?} game {theirs:?}"
            ));
        }
    }
    out
}

/// What the forced replay reads of the game's save after an update: every player's object rows,
/// the sight, the map's terrain and the state the forcing takes. Parsing a save takes most of
/// the replay's time, so [`forced_run`] builds these on worker threads ahead of the replay.
///
/// [`forced_run`]: crate::forced_replay::forced_run
pub(super) struct SavedStep {
    pub(super) objects: Vec<Vec<Object>>,
    /// Each player's ghosts (type 25) in its list's order.
    pub(super) ghosts: Vec<Vec<Ghost>>,
    pub(super) seen: Visibility,
    pub(super) width: usize,
    pub(super) terrain: Vec<(u8, u8, i16)>,
    pub(super) game: GameState,
}

impl SavedStep {
    pub(super) fn of(save: &[u8], masters: Option<&MasterTable>) -> GenieResult<Self> {
        let chapter = chapters(save)?[0];
        let header = chapter.header()?;
        let objects = (0..header.players().len() as u32)
            .map(|id| header_objects(&header, id, masters))
            .collect::<Result<_, _>>()?;
        let (width, terrain) = saved_terrain(&header);
        Ok(Self {
            objects,
            ghosts: header
                .players()
                .iter()
                .map(saved_ghosts)
                .collect::<Result<_, _>>()?,
            seen: saved_sight(&header)?,
            width,
            terrain,
            game: game_state(&header, masters)?,
        })
    }
}

/// The players' visible maps and the unified map of a save, as [`Visibility`] holds them.
pub fn saved_visibility(save: &[u8]) -> GenieResult<Visibility> {
    let chapter = chapters(save)?[0];
    let header = chapter.header()?;
    saved_sight(&header)
}

/// Where player `player`'s sight in `slice` differs from `game`'s: its explored count, each tile's
/// count and its two bits of the unified map.
#[must_use]
pub fn visibility_differences(slice: &Visibility, game: &Visibility, player: usize) -> Vec<String> {
    let mut out = Vec::new();
    let (Some(a), Some(b)) = (slice.maps.get(player), game.maps.get(player)) else {
        return vec![format!("player {player} has no visible map")];
    };
    if a.explored != b.explored {
        out.push(format!(
            "player {player} explored {} tiles, the game {}",
            a.explored.get(),
            b.explored.get()
        ));
    }
    out.extend(resource_differences(slice, game, player));
    let w = slice.size.width().max(1) as usize;
    let bits = PlayerId::try_from(player).map_or(VisionBits::NONE, |p| {
        VisionBits::visible(p) | VisionBits::explored(p)
    });
    let unified_same = || {
        slice
            .unified
            .iter()
            .zip(&game.unified)
            .all(|(&u, &v)| u & bits == v & bits)
    };
    // the usual case, every tile the same, without a look at each tile's place
    if a.counts == b.counts && unified_same() {
        return out;
    }
    for (i, (&c, &g)) in a.counts.iter().zip(&b.counts).enumerate() {
        let (u, v) = (slice.unified[i] & bits, game.unified[i] & bits);
        if c == g && u == v {
            continue;
        }
        let (x, y) = (i % w, i / w);
        if c != g {
            out.push(format!(
                "player {player} tile ({x}, {y}) count {c}, the game {g}"
            ));
        }
        if u != v {
            out.push(format!(
                "player {player} tile ({x}, {y}) unified bits {u:#x}, the game {v:#x}"
            ));
        }
    }
    out
}

/// Where player `player`'s visible-resource lists in `slice` differ from `game`'s: their count,
/// each list's capacity and its first record that differs (every field, the floats by their
/// bits), or its length.
pub(super) fn resource_differences(
    slice: &Visibility,
    game: &Visibility,
    player: usize,
) -> Vec<String> {
    let (Some(a), Some(b)) = (slice.resources.get(player), game.resources.get(player)) else {
        return vec![format!("player {player} has no visible-resource lists")];
    };
    let mut out = Vec::new();
    if a.lists.len() != b.lists.len() {
        out.push(format!(
            "player {player} holds {} visible-resource lists, the game {}",
            a.lists.len(),
            b.lists.len()
        ));
    }
    if a.capacities != b.capacities {
        out.push(format!(
            "player {player} visible-resource capacities {:?}, the game {:?}",
            a.capacities, b.capacities
        ));
    }
    let bits = |r: &VisibleResource| {
        (
            r.id,
            r.distance.get().to_bits(),
            r.zone,
            r.at.x().to_bits(),
            r.at.y().to_bits(),
        )
    };
    for (g, (x, y)) in a.lists.iter().zip(&b.lists).enumerate() {
        if let Some(k) = x.iter().zip(y).position(|(r, s)| bits(r) != bits(s)) {
            out.push(format!(
                "player {player} visible resources {g}: record {k} {:?}, the game {:?}",
                x[k], y[k]
            ));
        } else if x.len() != y.len() {
            out.push(format!(
                "player {player} visible resources {g}: {} records, the game {}",
                x.len(),
                y.len()
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests;
