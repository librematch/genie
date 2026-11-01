//! The slice's runs from the recording's header: the commands' trace, word 9's simulation, the
//! blocks after each tick and the header's slice.

use crate::masters::MasterTable;
use crate::{Decoder, Error, Frame, frames, hunted_origins, listed, player_totals, slice};
use genie_core::GenieError;
use genie_core::commands::Command;
use genie_core::objects::ListIndex;
use genie_core::objects::unit::Unit;
use genie_core::ownership::artifact::ARTIFACT_TYPE;
use genie_core::world::Millis;
use genie_core::world::Tick;
use genie_core::world::checkpoint::{ActionTerm, CheckpointObject, TopAction, player_block};
use genie_core::world::{Player, World};
use genie_rec::de;
use genie_save::{Header, PlayerSection};

/// Commands whose handlers can create a unit group (158041 `FUN_14152efc0`): INTERACT, WORK,
/// MOVE, `FORM_FORMATION`, `GROUP_MULTI_WAYPOINTS`, BUILD, REPAIR. More player-side callers exist and
/// are not mapped yet. A group draws from the World stream every tick while active and every 3 s
/// while idle (`FUN_14103d910` → `FUN_14102d370`), which the slice does not model. Whether a command
/// creates one depends on its units: the splitter (`FUN_140fd76d0`) groups only clusters of two or
/// more formation-capable units, and villagers never join a formation (as openage's
/// `doc/reverse_engineering/game_mechanics/formations.md` describes for `AoC`).
pub const GROUP_COMMANDS: [u8; 7] = [0x00, 0x02, 0x03, 0x17, 0x1f, 0x66, 0x6e];

/// A command that may create a unit group and so take the frame-seed chain out of the slice's model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupCommand {
    /// The tick (1-based) the command executes in: the frame of the sync before it.
    pub tick: u32,
    /// The commanding player.
    pub player: u8,
    /// The command's byte.
    pub command: u8,
}

/// Word 9 of one player's first checkpoint, from the unit-AI slice and as recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word9 {
    /// The player's id.
    pub id: u32,
    /// Word 9 from the slice.
    pub simulated: u32,
    /// Word 9 as the checkpoint stores it.
    pub checkpoint: u32,
    /// The first command before the checkpoint, by any player, that may create a unit group.
    pub group_command: Option<GroupCommand>,
    /// The first unit update of this player the slice could not model.
    pub unsupported: Option<(u32, ListIndex, GenieError)>,
}

/// Whether the runs list each decoded command with the update it runs in ([`trace_commands`]).
pub(super) static TRACE_COMMANDS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Every run from now on lists each decoded command with the update it runs in, on standard
/// error (`on`), or stops listing them.
pub fn trace_commands(on: bool) {
    TRACE_COMMANDS.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Run the slice over `frames`, each tick with its commands.
pub(super) fn run(turns: &mut World, decoder: &mut Decoder, frames: &[Frame<'_>]) {
    let trace = TRACE_COMMANDS.load(std::sync::atomic::Ordering::Relaxed);
    for frame in frames {
        let commands: Vec<Command> = frame
            .commands
            .iter()
            .filter_map(|p| decoder.command(p))
            .collect();
        if trace {
            for c in &commands {
                eprintln!("update {}: {c:?}", turns.ticks.get() + 1);
            }
        }
        turns.tick_with(Millis::new(frame.dt), &commands);
    }
}

/// Run the unit-AI slice from the header save up to the first checkpoint and compare word 9.
/// `masters` gives the dead-object values the save lacks; without it, an object that dies stops
/// its player's slice.
///
/// The checkpoint block is built before the player loop of the tick that records it, so it shows
/// the state after one tick fewer than the syncs before it.
///
/// # Panics
///
/// Never: the first checkpoint it reads is the one it found.
pub fn simulate_word9(
    recording: &[u8],
    masters: Option<&MasterTable>,
) -> Result<Vec<Word9>, Error> {
    let chapter = de::chapters(recording)?[0];
    let header = chapter.header()?;
    let all = frames(&chapter, usize::MAX)?;
    let (first, checkpoint) = all
        .iter()
        .enumerate()
        .find_map(|(i, f)| f.checkpoint.as_ref().map(|c| (i, c)))
        .ok_or(Error::NoCheckpoint)?;
    let group_command = all[..first].iter().enumerate().find_map(|(i, f)| {
        let packet = f.commands.iter().find(|p| GROUP_COMMANDS.contains(&p[0]))?;
        Some(GroupCommand {
            tick: i as u32 + 1,
            player: packet.get(1).copied().unwrap_or_default(),
            command: packet[0],
        })
    });
    let sections = header.players();
    // from the recording's header: the load's run-time state too, not the bare `slice`
    let mut turns = header_slice(&header, masters);
    run(&mut turns, &mut Decoder::default(), &all[..first]);
    checkpoint
        .lanes
        .iter()
        .filter(|lane| lane.iter().any(|&w| w != 0))
        .map(|lane| {
            let id = lane[8];
            let section = sections.get(id as usize).ok_or(Error::NoSuchPlayer(id))?;
            let simulated = &turns.players[id as usize];
            Ok(Word9 {
                id,
                simulated: lane_block(section, id, simulated)[9],
                checkpoint: lane[9],
                group_command,
                unsupported: simulated
                    .unsupported
                    .clone()
                    .map(|(t, i, e)| (t.get(), i, e)),
            })
        })
        .collect()
}

/// A player's block with the slice's objects in place of the save's: the save gives what the slice
/// leaves alone (the sleeping objects), the slice its resources, its live list (the objects it
/// created included) and each object's master, position, state, held amount, top action and
/// retarget timer.
pub(super) fn lane_block(section: &PlayerSection<'_>, id: u32, simulated: &Player) -> [u32; 11] {
    let mut state = player_totals(section, id);
    state.resources = simulated.resources();
    state.sleeping_count = simulated.sleeping.len() as u32;
    state.live = simulated
        .objects
        .iter()
        .filter(|unit| listed(unit))
        .map(|unit| CheckpointObject {
            id: unit.id,
            main_id: unit.life.master_id,
            state: unit.life.state,
            amount_held: unit.life.amount,
            position: unit.position(),
            retarget_timer: unit.ai.as_ref().map(|ai| ai.retarget_timer),
            top_action: checkpoint_action(unit),
        })
        .collect();
    player_block(&state)
}

/// What word 9 reads of a unit's actions: its top action's type (slot 166), and its top
/// action's state (slot 86), but a building's (`TRIBE_Building_Object::actionState`,
/// `0x1411cb7e0`) prefers the state of its production list's action (`+0xbf8`): a town centre
/// that attacks while it trains counts its Attack's type with its make action's state.
pub(super) fn checkpoint_action(unit: &Unit) -> Option<ActionTerm> {
    let making = unit
        .production
        .as_ref()
        .and_then(|p| p.action)
        .map(|a| a.state());
    match (top_action(unit), making) {
        (Some(top), Some(state)) => Some(ActionTerm {
            kind: Some(top.kind),
            state,
        }),
        (top, None) => top.map(ActionTerm::from),
        (None, Some(state)) => Some(ActionTerm { kind: None, state }),
    }
}

/// A unit's top action as the slice models it, type and state.
pub(super) fn top_action(unit: &Unit) -> Option<TopAction> {
    unit.top_action()
}

/// One live object's word-9 terms in the slice after a number of ticks: its id, the top action's
/// type (or -1) and state, and the unit-AI retarget timer (0 without a unit AI).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Word9Terms {
    /// The object's id.
    pub id: u32,
    /// Its top action's type, −1 for none.
    pub action: i32,
    /// Its top action's state.
    pub state: u8,
    /// Its unit AI's retarget timer, 0 without a unit AI.
    pub retarget: i32,
}

/// The slice's [`Word9Terms`] of player `id`'s live objects after `ticks` ticks.
pub fn word9_terms(
    recording: &[u8],
    id: u32,
    ticks: usize,
    masters: Option<&MasterTable>,
) -> Result<Vec<Word9Terms>, Error> {
    let chapter = de::chapters(recording)?[0];
    let header = chapter.header()?;
    let sections = header.players();
    if sections.len() <= id as usize {
        return Err(Error::NoSuchPlayer(id));
    }
    // from the recording's header: the load's run-time state too, not the bare `slice`
    let mut turns = header_slice(&header, masters);
    run(
        &mut turns,
        &mut Decoder::default(),
        &frames(&chapter, ticks)?,
    );
    Ok(turns.players[id as usize]
        .objects
        .iter()
        .map(|unit| Word9Terms {
            id: unit.id.get().cast_unsigned(),
            action: unit
                .actions
                .artifact()
                .map_or(-1, |_| i32::from(ARTIFACT_TYPE.get())),
            state: unit.actions.artifact().map_or(0, |a| a.state.get()),
            retarget: unit
                .ai
                .as_ref()
                .map_or(0, |ai| ai.retarget_timer.get() as i32),
        })
        .collect())
}

/// The slice's blocks of the players `ids` after each of the first `ticks` ticks, from the header
/// save on: the per-tick counterpart of [`simulate_word9`], for the tick oracle's trace
/// ([`tick_trace::TickTrace`]) to name the first tick and word where the slice and the game part.
///
/// [`tick_trace::TickTrace`]: crate::tick_trace::TickTrace
pub fn blocks_per_tick(
    recording: &[u8],
    ids: &[u32],
    ticks: usize,
    masters: Option<&MasterTable>,
) -> Result<Vec<Vec<[u32; 11]>>, Error> {
    let chapter = de::chapters(recording)?[0];
    let header = chapter.header()?;
    let sections = header.players();
    if let Some(&id) = ids.iter().find(|&&id| sections.len() <= id as usize) {
        return Err(Error::NoSuchPlayer(id));
    }
    let mut turns = header_slice(&header, masters);
    let mut out = Vec::with_capacity(ticks);
    let mut decoder = Decoder::default();
    for frame in frames(&chapter, ticks)? {
        run(&mut turns, &mut decoder, std::slice::from_ref(&frame));
        out.push(
            ids.iter()
                .map(|&id| lane_block(&sections[id as usize], id, &turns.players[id as usize]))
                .collect(),
        );
    }
    Ok(out)
}

/// The slice set up from a save's header ([`slice`]): every player's live objects, attributes,
/// update times and history, the masters and the sight, without the run-time state a per-tick
/// save's dumps add. For a check of a player-level update against the tick oracle's next save
/// (the world player's, [`genie_core::players::world_player`]). Dev only.
#[must_use]
pub fn slice_from_header(header: &Header, masters: Option<&MasterTable>) -> World {
    slice(header, masters)
}

/// The slice as the recording's header load leaves it: [`slice`] with the objects-in-sight
/// registry empty, every object's mask 0, as the game's load makes it, and the hunted animals'
/// unit AI fields their load made.
pub(super) fn header_slice(header: &Header, masters: Option<&MasterTable>) -> World {
    let mut turns = slice(header, masters);
    if let Some(sight) = turns.spatial.sight.as_mut() {
        sight.registry = Some(genie_core::vision::registry::Registry::new(
            header.players().len(),
            genie_core::vision::registry::CLASSES,
        ));
    }
    // the hunted animals' origins are their places in this very save
    hunted_origins(&mut turns, header);
    watch_at_load(&mut turns);
    turns
}

/// The ghosts' watched lists as the game's load builds them ([`World::build_watched`]), once the
/// slice holds its registry; what it cannot build stops every player.
pub(super) fn watch_at_load(turns: &mut World) {
    if let Err(e) = turns.build_watched() {
        for p in &mut turns.players {
            let _ = p
                .unsupported
                .get_or_insert_with(|| (Tick::ZERO, ListIndex::new(0), e.clone()));
            p.stopped = true;
        }
    }
}

/// The slice from the recording's header ([`header_slice`]) after its first `ticks` updates,
/// nothing forced: for comparing a run from the start with the tick oracle's saves. Dev only.
pub fn header_run(
    recording: &[u8],
    ticks: usize,
    masters: Option<&MasterTable>,
) -> Result<World, Error> {
    let chapter = de::chapters(recording)?[0];
    let header = chapter.header()?;
    let mut turns = header_slice(&header, masters);
    let mut decoder = Decoder::default();
    run(&mut turns, &mut decoder, &frames(&chapter, ticks)?);
    Ok(turns)
}

#[cfg(test)]
mod tests;
