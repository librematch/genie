//! Runs from a tick oracle's save and their comparison with the game's: the players' objects,
//! sight and technologies, the unit groups, the hunted animals' origins.

use crate::command_args::Frame;
use crate::masters::{MasterTable, TerrainTable};
use crate::{
    Decoder, Dumps, Error, SavedStep, TickReport, frames, ghost_pool_from_dump, groups,
    inside_lists_from_dump, listed, mesh_from_dump, mesh_owners_from_dump, object_differences,
    recycle_pools_from_dump, registry_from_dump, run, slice, spatial, tile_differences,
    tile_lists_from_dump, visibility_differences, watch_at_load,
};
use genie_core::commands::Command;
use genie_core::map::Distance;
use genie_core::objects::ObjectId;
use genie_core::objects::masters::action_type;
use genie_core::players::Amount;
use genie_core::world::Millis;
use genie_core::world::Tick;
use genie_core::world::{Player, World};
use genie_rec::de;
use genie_save::Header;

/// Every player's objects and sight after a run against the game's save `after`; with `tick`,
/// only the freezes of that update count, else all. A frozen object's differences are left out.
pub(super) fn compare_players(
    turns: &World,
    after: &SavedStep,
    tick: Option<u32>,
    report: &mut TickReport,
) -> Result<(), Error> {
    world_differences(turns, after, report);
    for p in 0..turns.players.len() {
        player_differences(turns, after, tick, p, report)?;
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
    if let Some(t) = turns.spatial.terrain.as_ref() {
        for d in tile_differences(t, after.width, &after.terrain) {
            report.differences.push((0, d));
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
) -> Result<(), Error> {
    let player = &turns.players[p];
    let id = p as u32;
    let frozen = stops(player, id, tick, report);
    let game = after.objects.get(p).ok_or(Error::NoSuchPlayer(id))?;
    if !player.stopped {
        stock_differences(player, &after.game, p, report);
    }
    if let Some(&(status, resigned)) = after.game.statuses.get(p)
        && (status, resigned) != (player.game_status, player.resigned)
    {
        let d = format!(
            "game status slice {:?} game {:?} (status, resigned)",
            (player.game_status.get(), player.resigned),
            (status.get(), resigned)
        );
        report.differences.push((id, d));
    }
    if !player.stopped {
        for d in tech_differences(player, &after.game, p) {
            report.differences.push((id, d));
        }
        for d in object_differences(player.objects.iter().filter(|u| listed(u)), game) {
            let object = d
                .split(':')
                .next()
                .and_then(|o| o.parse::<i32>().ok().and_then(ObjectId::of_field));
            if !object.is_some_and(|o| frozen.contains(&o)) {
                report.differences.push((id, d));
            }
        }
    }
    if let Some(sight) = turns.spatial.sight.as_ref() {
        for d in visibility_differences(sight, &after.seen, p) {
            report.differences.push((id, d));
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
        .map(|u| u.id)
        .chain(player.freezes.iter().map(|&(_, o, _)| o))
        .collect()
}

/// A player's resources against the game's (one difference), then each other attribute the
/// slice holds that differs (reported apart).
fn stock_differences(
    player: &Player,
    game: &genie_core::world::GameState,
    p: usize,
    report: &mut TickReport,
) {
    let id = p as u32;
    let resources = player.resources().map(Amount::get);
    if let Some(r) = game.resources.get(p)
        && *r != resources
    {
        let d = format!("resources slice {resources:?} game {r:?}");
        report.differences.push((id, d));
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
                .map(|(i, (a, b))| (id, i, a.get().to_bits(), b.to_bits())),
        );
    }
}

/// A player's pending debit and technology states against the game's: per technology its state,
/// researches queued and done, work done and building.
pub(super) fn tech_differences(
    player: &Player,
    game: &genie_core::world::GameState,
    p: usize,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(d) = game
        .pending_debit
        .get(p)
        .filter(|&&d| d != player.pending_debit)
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
    let key = |t: &genie_core::technology::TechState| {
        (
            t.state,
            t.pending,
            t.times_researched,
            t.research_done,
            t.building_master,
        )
    };
    for (i, (a, b)) in player.techs.iter().zip(theirs).enumerate() {
        if key(a) != key(b) {
            out.push(format!(
                "tech {i}: slice {:?} game {:?} (state, queued, done, work, building)",
                key(a),
                key(b)
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
) -> Result<World, Error> {
    from_save(save, recording, start_tick, ticks, masters, dumps, false)
}

/// The world's unit groups of a save, with the process globals dumped beside it when given.
///
/// # Errors
///
/// When the save does not parse or its groups do not import.
pub fn saved_groups(
    save: &[u8],
    globals: Option<&str>,
) -> Result<genie_core::groups::UnitGroups, Error> {
    let chapter = de::chapters(save)?[0];
    let header = chapter.header()?;
    let mut g = groups::unit_groups(&header.world.base_world).map_err(Error::Unmodelled)?;
    if let Some(text) = globals {
        groups::globals_from_dump(text, &mut g);
    }
    Ok(g)
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
) -> Result<World, Error> {
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
) -> Result<World, Error> {
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
pub(super) fn save_header(save: &[u8]) -> Result<Header, Error> {
    Ok(de::chapters(save)?[0].header()?)
}

/// The slice from a save as the game's load leaves it: the tick oracle's dumps beside the save
/// put in ([`load_dumps`]), the watched lists built.
pub(super) fn loaded_slice(
    header: &Header,
    masters: Option<&MasterTable>,
    dumps: Dumps<'_>,
) -> Result<World, Error> {
    let mut turns = slice(header, masters);
    load_dumps(&mut turns, dumps)?;
    watch_at_load(&mut turns);
    Ok(turns)
}

/// The tick oracle's dumps beside a save, put into its slice: the shape mesh and its owners, the
/// objects-in-sight registry, the tiles' object lists, the recycled missiles and ghosts, the
/// garrisons and the process globals.
pub(super) fn load_dumps(turns: &mut World, dumps: Dumps<'_>) -> Result<(), Error> {
    if let Some(text) = dumps.mesh {
        let m = mesh_from_dump(text).ok_or(Error::BadMeshDump)?;
        turns.pathing.set_mesh(m, mesh_owners_from_dump(text));
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
        groups::globals_from_dump(text, &mut turns.groups);
    }
    Ok(())
}

/// The recording's first `limit` frames, the hunted animals' origins taken from its start
/// ([`hunted_origins`]).
pub(super) fn played_frames<'a>(
    turns: &mut World,
    recording: &'a [u8],
    limit: usize,
) -> Result<Vec<Frame<'a>>, Error> {
    let played = de::chapters(recording)?[0];
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
        let most = genie_core::players::PlayerId::try_from(p).map_or(0, |q| turns.most_units(q));
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
        turns.command_phase(Millis::new(frame.dt), &commands);
    }
}

/// The hunted animals' unit AI fields the save does not hold (`TribeHuntedAnimalUnitAIModule`,
/// made with the unit: `FUN_140c11920`): the origin is the animal's place at the recording's
/// start (`start`, the recording's header save, whose load made the AI), the leash its master's
/// last task of action type 6's work range (the unit's `+0xb74`, `FUN_141206d50`).
pub(super) fn hunted_origins(turns: &mut World, start: &Header) {
    let places: std::collections::BTreeMap<i32, genie_core::map::Position> = start
        .players()
        .iter()
        .flat_map(|p| {
            p.live_objects()
                .chain(p.sleeping_objects())
                .collect::<Vec<_>>()
        })
        .filter_map(|o| {
            o.static_part()
                .map(|s| (s.id, genie_core::map::Position::new(s.t_x, s.t_y, s.t_z)))
        })
        .collect();
    for (p, player) in turns.players.iter_mut().enumerate() {
        let masters = turns.masters.get(p);
        for unit in player.objects.iter_mut().chain(player.sleeping.iter_mut()) {
            let Some(ai) = unit
                .ai
                .as_mut()
                .filter(|ai| ai.ai_type == genie_core::unit_ai::UnitAiType::HUNTED)
            else {
                continue;
            };
            let Some(&origin) = places.get(&unit.id.get()) else {
                continue;
            };
            let leash = masters
                .and_then(|m| m.get(unit.life.master_id))
                .and_then(|m| {
                    m.tasks
                        .iter()
                        .rev()
                        .find(|t| t.action_type == action_type::HUNT_RANGE)
                })
                .map_or(Distance::ZERO, |t| t.work_range);
            ai.hunted = Some(genie_core::unit_ai::Hunted { origin, leash });
        }
    }
}

/// The world's objects as movement sees them, from a save ([`spatial`]), on a map without terrain.
pub fn spatial_from_save(save: &[u8]) -> Result<genie_core::map::spatial::Spatial, Error> {
    spatial_with_tiles(save, &TerrainTable::default())
}

/// The world's objects as movement sees them, from a save, with each tile's terrain class.
pub fn spatial_with_tiles(
    save: &[u8],
    terrain: &TerrainTable,
) -> Result<genie_core::map::spatial::Spatial, Error> {
    let chapter = de::chapters(save)?[0];
    let header = chapter.header()?;
    spatial(&header, terrain).map_err(Error::Unmodelled)
}

#[cfg(test)]
mod tests;
