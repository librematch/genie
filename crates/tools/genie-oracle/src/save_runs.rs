//! Runs from a tick oracle's save and their comparison with the game's: the players' objects,
//! sight and technologies, the unit groups, the hunted animals' origins.

use crate::command_args::Frame;
use crate::errors::{bad_mesh_dump, no_such_player};
use crate::groups::globals_from_dump;
use crate::masters::{MasterTable, TerrainTable};
use crate::setup::layer;
use crate::{
    Decoder, Dumps, SavedStep, TickReport, frames, ghost_pool_from_dump, inside_lists_from_dump,
    mesh_from_dump, mesh_owners_from_dump, object_differences, recycle_pools_from_dump,
    registry_from_dump, run, slice, tile_differences, tile_lists_from_dump, visibility_differences,
};
use genie_core::GenieResult;
use genie_core::commands::Command;
use genie_core::groups::UnitGroups;
use genie_core::map::spatial::Spatial;
use genie_core::objects::ObjectId;
use genie_core::players::Amount;
use genie_core::players::PlayerId;
use genie_core::technology::TechState;
use genie_core::world::Departures;
use genie_core::world::GameState;
use genie_core::world::Tick;
use genie_core::world::load::groups::unit_groups;
use genie_core::world::load::map::spatial;
use genie_core::world::load::world::loaded::{hunted_origins, watch_at_load};
use genie_core::world::{Player, World};
use genie_rec::de::chapters;
use genie_save::Header;

/// Every player's objects and sight after a run against the game's save `after`; with `tick`,
/// only the freezes of that update count, else all. A frozen object's differences are left out.
pub(super) fn compare_players(
    turns: &World,
    after: &SavedStep,
    tick: Option<u32>,
    report: &mut TickReport,
) -> GenieResult<()> {
    world_differences(turns, after, report);
    for index in 0..turns.players.len() {
        player_differences(turns, after, tick, index, report)?;
    }
    Ok(())
}

/// The world's game state and the map's terrain against the game's: Gaia's differences.
pub(super) fn world_differences(turns: &World, after: &SavedStep, report: &mut TickReport) {
    if turns.game_state != after.game.game_state {
        let d = format!(
            "game state slice {} game {}",
            turns.game_state.get(),
            after.game.game_state.get()
        );
        report.differences.push((0, d));
    }
    if let Some(terrain) = turns.spatial.terrain.as_ref() {
        for text in tile_differences(terrain, after.width, &after.terrain) {
            report.differences.push((0, text));
        }
    }
}

/// Player `p`'s stops, state, objects and sight against the game's save `after`.
fn player_differences(
    turns: &World,
    after: &SavedStep,
    tick: Option<u32>,
    p: usize,
    report: &mut TickReport,
) -> GenieResult<()> {
    let player = &turns.players[p];
    let id = p as u32;
    let frozen = stops(player, id, tick, report);
    let game = after.objects.get(p).ok_or_else(|| no_such_player(id))?;
    if !player.stopped {
        stock_differences(player, &after.game, p, report);
    }
    if let Some(&(status, resigned)) = after.game.statuses.get(p)
        && (status, resigned)
            != (
                player.game_status,
                player.departures.contains(Departures::RESIGNED),
            )
    {
        let d = format!(
            "game status slice {:?} game {:?} (status, resigned)",
            (
                player.game_status.get(),
                player.departures.contains(Departures::RESIGNED)
            ),
            (status.get(), resigned)
        );
        report.differences.push((id, d));
    }
    if !player.stopped {
        for d in tech_differences(player, &after.game, p) {
            report.differences.push((id, d));
        }
        for d in object_differences(crate::object_rows(turns, p), game) {
            let object = d
                .split(':')
                .next()
                .and_then(|o| o.parse::<i32>().ok().and_then(ObjectId::of_field));
            if !object.is_some_and(|object_id| frozen.contains(&object_id)) {
                report.differences.push((id, d));
            }
        }
    }
    if let Some(sight) = turns.spatial.sight.as_ref() {
        for text in visibility_differences(sight, &after.seen, p) {
            report.differences.push((id, text));
        }
    }
    Ok(())
}

/// A player's stop and its objects' freezes as the report's unsupported entries (with `tick`,
/// only that update's); returns the objects whose differences are left out: the stopped and the
/// frozen ones, of any update.
fn stops(player: &Player, id: u32, tick: Option<u32>, report: &mut TickReport) -> Vec<ObjectId> {
    let now = |t: u32| tick.is_none_or(|n| n == t);
    if let Some((t, _, why)) = player
        .unsupported
        .clone()
        .filter(|u| player.stopped && now(u.0.get()))
    {
        report.unsupported.push((id, None, t.get(), why));
    }
    for (t, o, why) in player.freezes.iter().filter(|f| now(f.0.get())) {
        report
            .unsupported
            .push((id, Some(*o), t.get(), why.clone()));
    }
    player
        .objects
        .iter()
        .filter(|u| u.stopped)
        .map(|unit| unit.id)
        .chain(player.freezes.iter().map(|&(_, object_id, _)| object_id))
        .collect()
}

/// A player's resources against the game's (one difference), then each other attribute the
/// slice holds that differs (reported apart).
fn stock_differences(player: &Player, game: &GameState, p: usize, report: &mut TickReport) {
    let id = p as u32;
    let resources = player.resources().map(Amount::get);
    if let Some(r) = game.resources.get(p)
        && *r != resources
    {
        let text = format!("resources slice {resources:?} game {r:?}");
        report.differences.push((id, text));
    }
    if let Some(saved) = game.attributes.get(p) {
        report.attributes.extend(
            player
                .attributes
                .iter()
                .zip(saved)
                .enumerate()
                .skip(4)
                .filter(|(_, (a, b))| a.get().to_bits() != b.to_bits())
                .map(|(i, (amount, b))| (id, i, amount.get().to_bits(), b.to_bits())),
        );
    }
}

/// A player's pending debit and technology states against the game's: per technology its state,
/// researches queued and done, work done and building.
pub(super) fn tech_differences(player: &Player, game: &GameState, p: usize) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(d) = game
        .pending_debit
        .get(p)
        .filter(|&&amounts| amounts != player.pending_debit)
    {
        out.push(format!(
            "pending debit slice {:?} game {:?}",
            player.pending_debit.map(Amount::get),
            d.map(Amount::get)
        ));
    }
    let Some(theirs) = game.techs.get(p) else {
        return out;
    };
    let key = |t: &TechState| {
        (
            t.state,
            t.pending,
            t.times_researched,
            t.research_done,
            t.building_master,
        )
    };
    for (i, (index, tech_state)) in player.techs.iter().zip(theirs).enumerate() {
        if key(index) != key(tech_state) {
            out.push(format!(
                "tech {i}: slice {:?} game {:?} (state, queued, done, work, building)",
                key(index),
                key(tech_state)
            ));
        }
    }
    out
}

/// The slice from a save: [`objects_from_save`]'s run, every player's state after it. Dev only.
///
/// # Errors
///
/// When a save or the recording does not parse.
///
/// [`objects_from_save`]: crate::save_objects::objects_from_save
pub fn run_from_save(
    save: &[u8],
    recording: &[u8],
    start_tick: usize,
    ticks: usize,
    masters: Option<&MasterTable>,
    dumps: Dumps<'_>,
) -> GenieResult<World> {
    from_save(save, recording, start_tick, ticks, masters, dumps, false)
}

/// The world's unit groups of a save, with the process globals dumped beside it when given.
///
/// # Errors
///
/// When the save does not parse or its groups do not import.
pub fn saved_groups(save: &[u8], globals: Option<&str>) -> GenieResult<UnitGroups> {
    let chapter = chapters(save)?[0];
    let header = chapter.header()?;
    let mut groups = unit_groups(&header.world.base_world)?;
    if let Some(text) = globals {
        globals_from_dump(text, &mut groups);
    }
    Ok(groups)
}

/// The slice from a save after the command phase of the next update alone
/// ([`genie_core::world::World::command_phase`]), for a check against the game's save taken
/// there (the tick oracle's `snapshot_at` at `0x141552990`). Dev only.
///
/// # Errors
///
/// When a save or the recording does not parse.
pub fn command_phase_from_save(
    save: &[u8],
    recording: &[u8],
    start_tick: usize,
    masters: Option<&MasterTable>,
    dumps: Dumps<'_>,
) -> GenieResult<World> {
    from_save(save, recording, start_tick, 1, masters, dumps, true)
}

/// [`run_from_save`]'s run; with `commands_only` the first update stops after its commands.
pub(super) fn from_save(
    save: &[u8],
    recording: &[u8],
    start_tick: usize,
    ticks: usize,
    masters: Option<&MasterTable>,
    dumps: Dumps<'_>,
    commands_only: bool,
) -> GenieResult<World> {
    let header = save_header(save)?;
    let mut turns = loaded_slice(&header, masters, dumps)?;
    let updates = start_tick + 1;
    let all = played_frames(&mut turns, recording, updates + ticks)?;
    let mut decoder = primed(&mut turns, &all, updates);
    let next = all.get(updates..).unwrap_or_default();
    if commands_only {
        frame_commands(&mut turns, &mut decoder, next.first());
    } else {
        run(&mut turns, &mut decoder, next);
    }
    Ok(turns)
}

/// A save's world: its first chapter's header.
pub(super) fn save_header(save: &[u8]) -> GenieResult<Header> {
    chapters(save)?[0].header()
}

/// The slice from a save as the game's load leaves it: the tick oracle's dumps beside the save
/// put in ([`load_dumps`]), the watched lists built.
pub(super) fn loaded_slice(
    header: &Header,
    masters: Option<&MasterTable>,
    dumps: Dumps<'_>,
) -> GenieResult<World> {
    let mut turns = slice(header, masters)?;
    load_dumps(&mut turns, dumps)?;
    watch_at_load(&mut turns);
    Ok(turns)
}

/// The tick oracle's dumps beside a save, put into its slice: the shape mesh and its owners, the
/// objects-in-sight registry, the tiles' object lists, the recycled missiles and ghosts, the
/// garrisons and the process globals.
pub(super) fn load_dumps(turns: &mut World, dumps: Dumps<'_>) -> GenieResult<()> {
    if let Some(text) = dumps.mesh {
        let mesh = mesh_from_dump(text).ok_or(bad_mesh_dump())?;
        turns.pathing.set_mesh(mesh, mesh_owners_from_dump(text));
    }
    if let (Some(text), Some(sight)) = (dumps.registry, turns.spatial.sight.as_mut()) {
        sight.registry = Some(registry_from_dump(text)?);
    }
    if let Some(text) = dumps.tiles {
        let _ = turns.spatial.force_tile_lists(&tile_lists_from_dump(text)?);
    }
    if let Some(text) = dumps.pools {
        turns.recycling.missiles = recycle_pools_from_dump(text)?;
        turns.recycling.ghosts = ghost_pool_from_dump(text)?;
    }
    if let Some(text) = dumps.inside {
        let _ = turns.force_garrisons(&inside_lists_from_dump(text)?);
    }
    if let Some(text) = dumps.globals {
        globals_from_dump(text, &mut turns.groups);
    }
    Ok(())
}

/// The recording's first `limit` frames, the hunted animals' origins taken from its start
/// ([`hunted_origins`]).
pub(super) fn played_frames<'a>(
    turns: &mut World,
    recording: &'a [u8],
    limit: usize,
) -> GenieResult<Vec<Frame<'a>>> {
    let played = chapters(recording)?[0];
    hunted_origins(turns, &played.header()?);
    frames(&played, limit)
}

/// A decoder primed with the commands of the recording's first `updates` frames, the world's
/// update count set to them and its last commanded units the frames' ([`Decoder::prime`]).
pub(super) fn primed(turns: &mut World, all: &[Frame<'_>], updates: usize) -> Decoder {
    turns.ticks = Tick::new(updates as u32);
    let mut decoder = Decoder::default();
    decoder.prime(&all[..updates.min(all.len())]);
    for (&p, units) in &decoder.last_commanded {
        if turns.last_commanded.len() <= p {
            turns.last_commanded.resize(p + 1, Vec::new());
        }
        let most = PlayerId::try_from(p).map_or(0, |player_id| turns.most_units(player_id));
        turns.last_commanded[p] = units[..units.len().min(most)].to_vec();
    }
    decoder
}

/// The command phase of `frame`'s update alone ([`World::command_phase`]); none without a frame.
pub(super) fn frame_commands(turns: &mut World, decoder: &mut Decoder, frame: Option<&Frame<'_>>) {
    if let Some(frame) = frame {
        let commands: Vec<Command> = frame
            .commands
            .iter()
            .filter_map(|p| decoder.command(p))
            .collect();
        turns.command_phase(frame.dt, &commands);
    }
}

/// The world's objects as movement sees them, from a save ([`spatial`]), on a map without terrain.
pub fn spatial_from_save(save: &[u8]) -> GenieResult<Spatial> {
    spatial_with_tiles(save, &TerrainTable::default())
}

/// The world's objects as movement sees them, from a save, with each tile's terrain class.
pub fn spatial_with_tiles(save: &[u8], terrain: &TerrainTable) -> GenieResult<Spatial> {
    let chapter = chapters(save)?[0];
    let header = chapter.header()?;
    let layer = layer()?;
    spatial(&header, terrain, layer)
}

#[cfg(test)]
mod tests;
