//! What the pathing reads of a save and its dumps: the shape mesh and its owners, a unit's
//! terrain and the tile pathfinder's planner.

use genie_core::map::GridSize;
use genie_core::map::Tile;
use genie_core::objects::ObjectId;
use genie_core::pathing::{FixedDistance, FixedPoint, FixedRect};
use genie_core::pathing::{ShapeId, ShapeSide};

use crate::masters::TerrainTable;
use crate::setup::layer;
use crate::{Error, live, spatial};
use genie_core::map::{Restriction, RestrictionId, TerrainId};
use genie_core::pathing::mesh::{Contact, Mesh};
use genie_core::pathing::tile_path::Planner;
use genie_rec::de;

/// The obstruction manager's shape mesh as the tick oracle dumps it (`load_oracle/probes/mesh.py`):
/// a header line `mesh width W shapes N free F`, a `shape ID X0 Y0 X1 Y1` line per slot with its
/// sides' contacts (`  side S` and seven hex words), then `tile X Y IDS...` for every tile that
/// holds shapes. The save does not hold the mesh: its ids and orders follow the game's history.
///
/// # Panics
///
/// When a contact line of the dump does not hold numbers (the dump's own format).
#[must_use]
pub fn mesh_from_dump(text: &str) -> Option<Mesh> {
    use genie_core::pathing::mesh::Mesh;
    let mut mesh: Option<Mesh> = None;
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.first().copied() {
            Some("mesh") => {
                let width = words.get(2)?.parse().ok()?;
                let free = u32::from_str_radix(words.get(6)?.trim_start_matches("0x"), 16).ok()?;
                let mut m = Mesh::new(width);
                m.free = ShapeId::of_link(free as i32);
                mesh = Some(m);
            }
            Some("shape") => {
                let m = mesh.as_mut()?;
                let v: Vec<i32> = words
                    .get(2..6)?
                    .iter()
                    .map(|w| w.parse().ok())
                    .collect::<Option<_>>()?;
                m.shapes.push(FixedRect::new(v[0], v[1], v[2], v[3]));
                m.sides.push(Default::default());
            }
            Some("side") => {
                let m = mesh.as_mut()?;
                let side: usize = words.get(1)?.parse().ok()?;
                let w: Vec<u32> = words
                    .get(2..9)?
                    .iter()
                    .map(|w| u32::from_str_radix(w, 16).ok())
                    .collect::<Option<_>>()?;
                m.sides.last_mut()?[side].push(contact_from_words(&w)?);
            }
            Some("tile") => {
                let m = mesh.as_mut()?;
                let x: u32 = words.get(1)?.parse().ok()?;
                let y: u32 = words.get(2)?.parse().ok()?;
                *m.tiles.get_mut((y * m.width + x) as usize)? = words
                    .get(3..)?
                    .iter()
                    .map(|w| w.parse().ok().map(ShapeId::new))
                    .collect::<Option<_>>()?;
            }
            _ => {}
        }
    }
    mesh
}

/// A contact line's seven words: the other shape, its side, the junction, the junction along
/// the side's axis, the start flag (its byte, 0 or 1: the game writes the byte and leaves the
/// word's upper bytes as the stack held them) and the half overlap; None for a side past 3 or
/// another flag.
fn contact_from_words(w: &[u32]) -> Option<Contact> {
    let at_side_start = match w.get(5)? & 0xff {
        0 => false,
        1 => true,
        _ => return None,
    };
    Some(Contact {
        other: ShapeId::new(*w.first()?),
        side: ShapeSide::of_word(*w.get(1)?).ok()?,
        at: FixedPoint::new(*w.get(2)? as i32, *w.get(3)? as i32),
        along: *w.get(4)? as i32,
        at_side_start,
        half: FixedDistance::new(*w.get(6)? as i32),
    })
}

/// The owner of each shape in a mesh dump (its `owner ID SHAPE` lines): the objects' `+0x64c`.
#[must_use]
pub fn mesh_owners_from_dump(text: &str) -> Vec<(ObjectId, ShapeId)> {
    text.lines()
        .filter_map(|line| {
            let mut w = line.split_whitespace();
            (w.next()? == "owner").then_some(())?;
            Some((
                ObjectId::of_field(w.next()?.parse().ok()?)?,
                ShapeId::new(w.next()?.parse().ok()?),
            ))
        })
        .collect()
}

/// What the tile pathfinder reads of object `id` in a save: its master's x radius and terrain
/// restriction (master `+0x5d8`, `+0x606`). None for an object the save does not hold.
pub fn unit_terrain(save: &[u8], id: i32) -> Result<Option<(f32, RestrictionId)>, Error> {
    let chapter = de::chapters(save)?[0];
    let header = chapter.header()?;
    for section in header.players() {
        for (_, s) in live(&section) {
            if s.id != id {
                continue;
            }
            let Some(m) = section
                .objects()
                .master(s.master_obj)
                .and_then(|m| m.static_part())
            else {
                return Ok(None);
            };
            return Ok(
                RestrictionId::of_data(m.terrain_restriction_mask_id).map(|r| (m.radius_x, r))
            );
        }
    }
    Ok(None)
}

/// The tile pathfinder's planner for one terrain restriction (`passable`, its value per terrain),
/// built from a save's map and the tiles its static obstructions block, as the game builds it at
/// load (`FUN_140f7d310`). Checked against the game's grid, dumped by the tick oracle.
pub fn planner_from_save(
    save: &[u8],
    terrain: &TerrainTable,
    passable: Restriction,
) -> Result<Planner, Error> {
    use genie_core::map::spatial::BLOCKED;
    let chapter = de::chapters(save)?[0];
    let header = chapter.header()?;
    let map = &header.world.base_world.map;
    let terrain_ids: Vec<TerrainId> = map
        .rows
        .iter()
        .flat_map(|r| r.tiles.iter().map(|t| TerrainId::named(t.terrain_type)))
        .collect();
    let layer = layer().map_err(Error::Unmodelled)?;
    let spatial = spatial(&header, terrain, layer).map_err(Error::Unmodelled)?;
    let tiles = spatial.tiles();
    let blocked = |tile: Tile| tiles.word(tile).is_some_and(|w| w & BLOCKED != 0);
    Ok(Planner::new(
        GridSize::new(map.map_width, map.map_height),
        terrain_ids,
        passable,
        &blocked,
    ))
}

#[cfg(test)]
mod tests;
