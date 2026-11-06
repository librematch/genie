//! What the pathing reads of a save and its dumps: the shape mesh and its owners, a unit's
//! terrain and the tile pathfinder's planner.

use genie_core::GenieResult;
use genie_core::map::GridSize;
use genie_core::objects::ObjectId;
use genie_core::pathing::{FixedDistance, FixedPoint, FixedRect};
use genie_core::pathing::{ShapeId, ShapeSide};

use crate::live;
use crate::masters::TerrainTable;
use crate::setup::layer;
use genie_core::map::{Restriction, RestrictionId, TerrainId};
use genie_core::pathing::mesh::{Contact, Mesh};
use genie_core::pathing::tile_path::Planner;
use genie_core::world::load::map::spatial;
use genie_rec::de::chapters;

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
                let mut width_mesh = Mesh::new(width);
                width_mesh.free = ShapeId::of_link(free as i32);
                mesh = Some(width_mesh);
            }
            Some("shape") => {
                let mesh = mesh.as_mut()?;
                let words: Vec<i32> = words
                    .get(2..6)?
                    .iter()
                    .map(|text| text.parse().ok())
                    .collect::<Option<_>>()?;
                mesh.shapes
                    .push(FixedRect::new(words[0], words[1], words[2], words[3]));
                mesh.sides.push(Default::default());
            }
            Some("side") => {
                let mesh = mesh.as_mut()?;
                let side: usize = words.get(1)?.parse().ok()?;
                let words: Vec<u32> = words
                    .get(2..9)?
                    .iter()
                    .map(|text| u32::from_str_radix(text, 16).ok())
                    .collect::<Option<_>>()?;
                mesh.sides.last_mut()?[side].push(contact_from_words(&words)?);
            }
            Some("tile") => {
                let mesh = mesh.as_mut()?;
                let x: u32 = words.get(1)?.parse().ok()?;
                let y: u32 = words.get(2)?.parse().ok()?;
                *mesh.tiles.get_mut((y * mesh.width + x) as usize)? = words
                    .get(3..)?
                    .iter()
                    .map(|text| text.parse().ok().map(ShapeId::new))
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
fn contact_from_words(words: &[u32]) -> Option<Contact> {
    let at_side_start = match words.get(5)? & 0xff {
        0 => false,
        1 => true,
        _ => return None,
    };
    Some(Contact {
        other: ShapeId::new(*words.first()?),
        side: ShapeSide::of_word(*words.get(1)?).ok()?,
        at: FixedPoint::new(*words.get(2)? as i32, *words.get(3)? as i32),
        along: *words.get(4)? as i32,
        at_side_start,
        half: FixedDistance::new(*words.get(6)? as i32),
    })
}

/// The owner of each shape in a mesh dump (its `owner ID SHAPE` lines): the objects' `+0x64c`.
#[must_use]
pub fn mesh_owners_from_dump(text: &str) -> Vec<(ObjectId, ShapeId)> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            (fields.next()? == "owner").then_some(())?;
            Some((
                ObjectId::of_field(fields.next()?.parse().ok()?)?,
                ShapeId::new(fields.next()?.parse().ok()?),
            ))
        })
        .collect()
}

/// What the tile pathfinder reads of object `id` in a save: its master's x radius and terrain
/// restriction (master `+0x5d8`, `+0x606`). None for an object the save does not hold.
pub fn unit_terrain(save: &[u8], id: i32) -> GenieResult<Option<(f32, RestrictionId)>> {
    let chapter = chapters(save)?[0];
    let header = chapter.header()?;
    for section in header.players() {
        for (_, rge_static_object) in live(&section) {
            if rge_static_object.id != id {
                continue;
            }
            let Some(rge_master_static_object) = section
                .objects()
                .master(rge_static_object.master_obj)
                .and_then(|master| master.static_part())
            else {
                return Ok(None);
            };
            return Ok(RestrictionId::of_data(
                rge_master_static_object.terrain_restriction_mask_id,
            )
            .map(|restriction_id| (rge_master_static_object.radius_x, restriction_id)));
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
) -> GenieResult<Planner> {
    let chapter = chapters(save)?[0];
    let header = chapter.header()?;
    let map = &header.world.base_world.map;
    let terrain_ids: Vec<TerrainId> = map
        .rows
        .iter()
        .flat_map(|rge_map_row| {
            rge_map_row
                .tiles
                .iter()
                .map(|rge_tile| TerrainId::named(rge_tile.terrain_kind))
        })
        .collect();
    let layer = layer()?;
    let spatial = spatial(&header, terrain, layer)?;
    Planner::new(
        GridSize::new(map.map_width, map.map_height),
        terrain_ids,
        passable,
        spatial.tiles(),
    )
}

#[cfg(test)]
mod tests;
