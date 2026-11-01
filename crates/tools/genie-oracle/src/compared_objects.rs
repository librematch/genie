//! The objects the comparison with the game's saves reads: a player's listed objects in genie's
//! world ([`object_rows`]) and in the game's save, imported as the slice imports a save
//! ([`saved_objects`]), so that both are genie's objects and compare field by field.

use genie_core::objects::unit::Unit;
use genie_core::players::CivId;
use genie_core::world::World;
use genie_rec::de;
use genie_save::Header;

use crate::masters::MasterTable;
use crate::saved_unit::unit;
use crate::{Error, listed};

/// A player's listed objects in genie's world, in its list's order; none for a player it lacks.
#[must_use]
pub fn object_rows(world: &World, player: usize) -> Vec<&Unit> {
    world.players.get(player).map_or_else(Vec::new, |p| {
        p.objects.iter().filter(|u| listed(u)).collect()
    })
}

/// The listed live objects of player `id` in a save (a recording's header, or the tick oracle's
/// `--snapshot`), imported as genie's objects with the `.dat`'s `masters`.
pub fn saved_objects(
    save: &[u8],
    id: u32,
    masters: Option<&MasterTable>,
) -> Result<Vec<Unit>, Error> {
    let chapter = de::chapters(save)?[0];
    let header = chapter.header()?;
    header_objects(&header, id, masters)
}

/// [`saved_objects`] of a read header.
pub(crate) fn header_objects(
    header: &Header,
    id: u32,
    masters: Option<&MasterTable>,
) -> Result<Vec<Unit>, Error> {
    let sections = header.players();
    let section = sections.get(id as usize).ok_or(Error::NoSuchPlayer(id))?;
    let civ = CivId::new(section.world_player().base.civ_id);
    let paths = &header.world.base_world.path;
    Ok(section
        .live_objects()
        .map(|o| unit(o, section.objects(), paths, civ, masters, sections.len()))
        .filter(listed)
        .collect())
}
