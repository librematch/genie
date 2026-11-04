//! The objects the comparison with the game's saves reads, each whole (its row and its parts): a
//! player's listed objects in genie's world ([`object_rows`]) and in the game's save, imported
//! as the slice imports a save ([`saved_objects`]), so that both are genie's objects and compare
//! field by field.

use crate::errors::no_such_player;
use genie_core::GenieResult;
use genie_core::players::CivId;
use genie_core::world::World;
use genie_core::world::parts::{Object, ObjectView};
use genie_rec::de::chapters;
use genie_save::Header;

use crate::masters::MasterTable;
use crate::{layer, listed};
use genie_core::world::load::object::object;

/// A player's listed objects in genie's world, in its list's order, each with its parts from the
/// world's tables ([`World::object_view`]); none for a player it lacks.
#[must_use]
pub fn object_rows(world: &World, player: usize) -> Vec<ObjectView<'_>> {
    world.players.get(player).map_or_else(Vec::new, |player| {
        player
            .objects
            .iter()
            .filter(|unit| listed(unit))
            .map(|unit| world.object_view(unit))
            .collect()
    })
}

/// The listed live objects of player `id` in a save (a recording's header, or the tick oracle's
/// `--snapshot`), imported as genie's objects with the `.dat`'s `masters`.
pub fn saved_objects(
    save: &[u8],
    id: u32,
    masters: Option<&MasterTable>,
) -> GenieResult<Vec<Object>> {
    let chapter = chapters(save)?[0];
    let header = chapter.header()?;
    header_objects(&header, id, masters)
}

/// [`saved_objects`] of a read header, each object with the parts the import reads (the dead
/// master is its gap, MISSING.md).
pub(crate) fn header_objects(
    header: &Header,
    id: u32,
    masters: Option<&MasterTable>,
) -> GenieResult<Vec<Object>> {
    let sections = header.players();
    let section = sections
        .get(id as usize)
        .ok_or_else(|| no_such_player(id))?;
    let civ = CivId::new(section.world_player().base.civ_id);
    let paths = &header.world.base_world.path;
    let missile_tuning = &layer()?.tuning.missile;
    Ok(section
        .live_objects()
        .map(|saved| {
            let players = sections.len();
            object(
                saved,
                section.objects(),
                paths,
                civ,
                masters,
                players,
                missile_tuning,
            )
        })
        .filter(|object| listed(&object.unit))
        .collect())
}
