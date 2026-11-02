//! The forced replay (FD-008): its steps, its continuous run, the old pather's answers and the
//! game's state in a save.

use crate::masters::MasterTable;
use crate::{
    Dumps, Error, SavedStep, command_phase_from_save, compare_players, ghost_pool_from_dump,
    groups, loaded_slice, object, played_frames, primed, recycle_pools_from_dump, run,
    run_from_save, save_header, tech_state, tile_lists_from_dump, tile_order_differences,
    turns_from_save,
};
use genie_core::GenieError;
use genie_core::groups::GroupId;
use genie_core::map::Distance;
use genie_core::map::Point;
use genie_core::objects::ObjectId;
use genie_core::pathing::ShortFound;
use genie_core::pathing::hull_path::HullPather;
use genie_core::players::Amount;
use genie_core::players::CivId;
use genie_core::players::game_status::GameStatus;
use genie_core::vision::ghost::Ghost;
use genie_core::world::FrameSeed;
use genie_core::world::GameState;
use genie_core::world::PlayState;
use genie_core::world::World;
use genie_core::world::index::ObjectsRef;
use genie_save::Header;
use std::collections::BTreeMap;
use std::mem::take as mem_take;
use std::slice::from_ref;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// The old short pather's result and path for one call ([`old_pather_paths`]).
pub type PatherAnswer = (ShortFound, Vec<Point>);

/// One call of the old short pather ([`old_pather_paths`]): the unit, its place then, the target
/// and the range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatherCall {
    /// The unit that asks for the path.
    pub unit: ObjectId,
    /// Where it stands for the call.
    pub from: Point,
    /// The object the path leads to.
    pub target: ObjectId,
    /// How near the target the path ends.
    pub range: f32,
}

/// The old short pather's answers (slot 0x3e8, [`genie_core::pathing::hull_path::HullPather::find`]) in
/// the world of the slice started from the tick oracle's save at tick `start_tick` with `dumps`,
/// for each call (the unit, its place then, the target, the range): the pather's result and path.
/// Each unit stands at its place for its call. Dev only: a check of the pather against the game's
/// own calls (`load_oracle/probes/_obspath.py`), whose world is the save's but for the units the
/// update moved before the call.
pub fn old_pather_paths(
    save: &[u8],
    recording: &[u8],
    start_tick: usize,
    masters: Option<&MasterTable>,
    dumps: Dumps<'_>,
    calls: &[PatherCall],
) -> Result<Vec<Result<PatherAnswer, GenieError>>, Error> {
    let mut turns = turns_from_save(save, recording, start_tick, masters, dumps)?;
    let mut out = Vec::new();
    for &PatherCall {
        unit: id,
        from,
        target,
        range,
    } in calls
    {
        let Some(unit) = turns
            .players
            .iter()
            .flat_map(|p| &p.objects)
            .find(|u| u.id == id)
        else {
            out.push(Err(GenieError::unsupported(
                "a caller the save does not hold",
            )));
            continue;
        };
        let Some(mover) = turns.parts.mover_of(unit.id) else {
            out.push(Err(GenieError::unsupported("a caller that does not move")));
            continue;
        };
        let (mask, radius) = (mover.stats.restriction_mask, mover.stats.radius.x());
        let z = turns.spatial.get(id).map_or(0.0, |p| p.position.z());
        if let Err(e) = turns
            .spatial
            .set_position(id, from.with_height(z), &turns.model.roles)
        {
            out.push(Err(e));
            continue;
        }
        let Some(to) = turns.spatial.get(target).map(|p| p.position.ground()) else {
            out.push(Err(GenieError::unsupported(
                "a target the save does not hold",
            )));
            continue;
        };
        let mut pather = HullPather::new(
            (
                &mut turns.spatial,
                ObjectsRef::new(
                    &turns.players,
                    &turns.index,
                    &turns.model.masters,
                    &turns.parts,
                    &turns.model.roles,
                ),
            ),
            id,
            Some(target),
            mask,
            Distance::new(radius),
        );
        out.push(
            pather
                .find(from, to, Some(target), Distance::new(range))
                .map(|(r, p)| (r, p.points)),
        );
    }
    Ok(out)
}

/// What one forced replay step found ([`forced_ticks`]): the slice started from the game's save
/// after `start`, run `ticks` updates, against the game's save after `start + ticks`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TickReport {
    /// The save the step starts from.
    pub start: usize,
    /// How many updates it ran.
    pub ticks: usize,
    /// What differs, by player: the objects that ran modelled (an object that froze, or every
    /// object of a player that stopped, is left out: its reason is in `unsupported`) and the
    /// player's sight.
    pub differences: Vec<(u32, String)>,
    /// What the slice could not model: the player, the object (None for the player's stop), the
    /// update and why.
    pub unsupported: Vec<(u32, Option<ObjectId>, u32, GenieError)>,
    /// The players' attributes past the resources that differ, apart from the differences (the
    /// slice does not keep every statistic yet): the player, the attribute, the slice's value
    /// and the game's (their f32 bits).
    pub attributes: Vec<(u32, usize, u32, u32)>,
}

/// One step of the forced replay (FD-008, CONTINUE.md): genie-core from the game's own save after
/// update `start` (`save`, with that save's mesh and registry dumps), `ticks` updates, compared
/// with the game's save after `start + ticks` (`after`) for every player. Every step starts from
/// the game's state, so the frame seed and every object, a frozen one too, are the game's again:
/// nothing the slice gets wrong carries into the next step. Dev only: the saves and dumps come
/// from the tick oracle, which genie-core never reads.
pub fn forced_ticks(
    save: &[u8],
    after: &[u8],
    recording: &[u8],
    start: usize,
    ticks: usize,
    masters: Option<&MasterTable>,
    dumps: Dumps<'_>,
) -> Result<TickReport, Error> {
    let turns = run_from_save(save, recording, start, ticks, masters, dumps)?;
    let mut report = TickReport {
        start,
        ticks,
        ..TickReport::default()
    };
    let mut saved = SavedStep::of(after, masters)?;
    compare_players(&turns, &saved, None, &mut report)?;
    report
        .differences
        .extend(owned_group_differences(&turns, &mut saved.game, None));
    Ok(report)
}

/// The command phase of the update after save `start` against the game's state at its end
/// (`after`, the tick oracle's `snapshot_at` at `0x141552990`, with the process globals dumped
/// there when given): every player's objects and the world's unit groups, as [`forced_ticks`]
/// reports them. Dev only.
///
/// # Errors
///
/// When a save or the recording does not parse.
pub fn command_phase(
    save: &[u8],
    (after, after_globals): (&[u8], Option<&str>),
    recording: &[u8],
    start: usize,
    masters: Option<&MasterTable>,
    dumps: Dumps<'_>,
) -> Result<TickReport, Error> {
    let turns = command_phase_from_save(save, recording, start, masters, dumps)?;
    let mut report = TickReport {
        start,
        ticks: 1,
        ..TickReport::default()
    };
    let mut saved = SavedStep::of(after, masters)?;
    compare_players(&turns, &saved, None, &mut report)?;
    report.differences.extend(owned_group_differences(
        &turns,
        &mut saved.game,
        after_globals,
    ));
    Ok(report)
}

/// The forced replay's continuous mode (FD-008, CONTINUE.md): genie-core from the game's save
/// after `start` (with its dumps), then `ticks` updates, each compared with the game's save after
/// it (`save_after(update)`, which may end the run early with None) and then forced to the game's
/// state where the slice did not model it or got it wrong ([`genie_core::world::World::force`]: the
/// frame seed, the frozen and the differing objects, a stopped player, the sight, and what the
/// game's dumps after the update hold, `dump_after(update, extension)`: the tiles' object lists
/// (`tiles`) and the recycled missiles (`pools`), whose differences are Gaia's). The objects the
/// slice gets right keep their own state, the saved and the unsaved, from tick to tick. One report
/// per update, each with that update's own differences, and where the run's time went. The saves
/// are parsed on rayon's pool while the run goes on, at most `window` ahead of it. Dev only.
#[allow(clippy::too_many_arguments)]
pub fn forced_run(
    start_save: &[u8],
    recording: &[u8],
    start: usize,
    ticks: usize,
    masters: Option<&MasterTable>,
    dumps: Dumps<'_>,
    save_after: &(dyn Fn(usize) -> Option<Vec<u8>> + Sync),
    dump_after: &dyn Fn(usize, &str) -> Option<String>,
    window: usize,
) -> Result<(Vec<TickReport>, Times), Error> {
    let header = save_header(start_save)?;
    let mut turns = loaded_slice(&header, masters, dumps)?;
    let updates = start + 1;
    let all = played_frames(&mut turns, recording, updates + ticks)?;
    let mut decoder = primed(&mut turns, &all, updates);
    let mut reports = Vec::new();
    // the game's saves after each update, parsed on rayon's pool while the run goes on, at most
    // `window` ahead of it; the pool is shared by every run going on at once
    let window = window.max(1);
    let ahead = Ahead::new();
    let parse = |i: usize| save_after(start + i + 1).map(|save| SavedStep::of(&save, masters));
    let mut times = Times::default();
    rayon::in_place_scope(|tasks| {
        let (ahead, parse) = (&ahead, &parse);
        let ask = |i: usize| {
            if i < ticks {
                tasks.spawn(move |_| ahead.put(i, parse(i)));
            }
        };
        (0..window).for_each(ask);
        let mut run_all = || -> Result<(), Error> {
            for k in 0..ticks {
                let Some(frame) = all.get(updates + k) else {
                    break;
                };
                let clock = Instant::now();
                let taken = ahead.take(k);
                ask(k + window);
                let Some(after) = taken else {
                    break;
                };
                let after = after?;
                times.parse += clock.elapsed();
                let clock = Instant::now();
                run(&mut turns, &mut decoder, from_ref(frame));
                times.run += clock.elapsed();
                let clock = Instant::now();
                let mut report = TickReport {
                    start: start + k,
                    ticks: 1,
                    ..TickReport::default()
                };
                let tick = turns.ticks.get();
                compare_players(&turns, &after, Some(tick), &mut report)?;
                let tiles = dump_after(start + k + 1, "tiles")
                    .map(|text| tile_lists_from_dump(&text))
                    .transpose()?;
                if let Some(t) = tiles.as_ref() {
                    for d in tile_order_differences(&turns.spatial, t) {
                        report.differences.push((0, d));
                    }
                }
                report
                    .differences
                    .extend(ghost_differences(&turns, &after.ghosts));
                let pools_text = dump_after(start + k + 1, "pools");
                let pool = pools_text
                    .as_deref()
                    .map(recycle_pools_from_dump)
                    .transpose()?;
                let ghost_pool = pools_text
                    .as_deref()
                    .map(ghost_pool_from_dump)
                    .transpose()?;
                if let Some(game_pool) = pool.as_ref()
                    && *game_pool != turns.recycling.missiles
                {
                    report.differences.push((
                        0,
                        format!(
                            "recycled missiles slice {:?}, the game {game_pool:?}",
                            turns.recycling.missiles
                        ),
                    ));
                }
                if let Some(game_pool) = ghost_pool.as_ref()
                    && *game_pool != turns.recycling.ghosts
                {
                    report.differences.push((
                        0,
                        format!(
                            "recycled ghosts slice {:?}, the game {game_pool:?}",
                            turns.recycling.ghosts
                        ),
                    ));
                }
                // the world's next reusable id, which ghosts and missiles take
                if turns.recycling.next_id != after.game.next_reusable_id {
                    report.differences.push((
                        0,
                        format!(
                            "next reusable id slice {} game {}",
                            turns.recycling.next_id, after.game.next_reusable_id
                        ),
                    ));
                }
                // the world's unit groups, by their owners; the process globals the tick
                // oracle dumped beside the save are the game's
                let mut after = after;
                let globals = dump_after(start + k + 1, "globals");
                let group_differences =
                    owned_group_differences(&turns, &mut after.game, globals.as_deref());
                let groups_differ = !group_differences.is_empty();
                report.differences.extend(group_differences);
                // the next frame seed: a free run draws the game's
                if turns.frame_seed != after.game.frame_seed {
                    report.differences.push((
                        0,
                        format!(
                            "frame seed slice {} game {}",
                            turns.frame_seed.get(),
                            after.game.frame_seed.get()
                        ),
                    ));
                }
                times.compare += clock.elapsed();
                let clock = Instant::now();
                let SavedStep {
                    objects,
                    mut seen,
                    terrain,
                    mut game,
                    ..
                } = after;
                game.tile_lists = tiles;
                game.missile_pool = pool;
                game.ghost_pool = ghost_pool;
                // the groups take the game's state only where they differ
                if !groups_differ {
                    game.groups = None;
                }
                // what the slice got wrong takes the game's state too: each update's differences are
                // its own, not an earlier update's carried on
                game.resync = report
                    .differences
                    .iter()
                    .filter_map(|(_, d)| {
                        d.split(':')
                            .next()
                            .and_then(|o| o.parse::<i32>().ok().and_then(ObjectId::of_field))
                    })
                    .collect();
                if report
                    .differences
                    .iter()
                    .any(|(_, d)| d.starts_with("player "))
                {
                    game.sight = Some((mem_take(&mut seen.maps), mem_take(&mut seen.unified)));
                }
                for (p, id, why) in turns.force(&game) {
                    report.unsupported.push((p.index() as u32, id, tick, why));
                }
                // a parsed save's state is dropped on the pool: its drop costs the run as much as
                // its forcing
                let gone = (game, objects, seen, terrain);
                tasks.spawn(move |_| drop(gone));
                times.force += clock.elapsed();
                reports.push(report);
            }
            Ok(())
        };
        run_all()
    })?;
    Ok((reports, times))
}

/// Each player's ghosts against the game's (`game`), in the list's order, by what the save holds
/// of them: one difference per player whose list differs.
pub(super) fn ghost_differences(turns: &World, game: &[Vec<Ghost>]) -> Vec<(u32, String)> {
    let key = |g: &Ghost| {
        // the report's line in the game's terms: the ids as the save holds them
        (
            g.id.get(),
            ObjectId::field(g.original_id),
            g.state,
            g.cant_see,
            g.position.get().map(f32::to_bits),
        )
    };
    let only = |x: &Vec<_>, y: &Vec<_>| -> Vec<_> {
        x.iter().filter(|e| !y.contains(*e)).copied().collect()
    };
    let mut out = Vec::new();
    for (p, game) in game.iter().enumerate() {
        let slice = turns.players.get(p).map_or(&[][..], |q| &q.ghosts[..]);
        let (a, b): (Vec<_>, Vec<_>) = (
            slice.iter().map(key).collect(),
            game.iter().map(key).collect(),
        );
        if a != b {
            let (sl, gm) = (only(&a, &b), only(&b, &a));
            let what = if sl.is_empty() && gm.is_empty() {
                format!("ghosts in another order: slice {a:?}, the game {b:?}")
            } else {
                format!("ghosts the slice's alone {sl:?}, the game's alone {gm:?}")
            };
            out.push((p as u32, what));
        }
    }
    out
}

/// The world's unit groups against the game's (`game`), each difference with its owner
/// ([`group_owner`]). The process globals dumped beside the game's save (`globals`) go into the
/// game's groups first, and the comparison then holds the globals too.
pub(super) fn owned_group_differences(
    turns: &World,
    game: &mut GameState,
    globals: Option<&str>,
) -> Vec<(u32, String)> {
    if let (Some(text), Some(g)) = (globals, game.groups.as_mut()) {
        groups::globals_from_dump(text, g);
    }
    let Some(g) = game.groups.as_ref() else {
        return Vec::new();
    };
    groups::group_differences(&turns.groups, g, globals.is_some())
        .into_iter()
        .map(|d| (group_owner(turns, game, &d), d))
        .collect()
}

/// The player a group difference (`group ID: …`) goes with: the group's owner in the game or the
/// slice, Gaia for the world's list and counter.
pub(super) fn group_owner(turns: &World, game: &GameState, difference: &str) -> u32 {
    let id = difference
        .strip_prefix("group ")
        .and_then(|d| d.split(':').next())
        .and_then(|d| d.parse::<i32>().ok())
        .and_then(GroupId::of_field);
    let owner = id.and_then(|id| {
        game.groups
            .as_ref()
            .and_then(|g| g.get(id))
            .or_else(|| turns.groups.get(id))
            .and_then(|g| g.owner)
    });
    owner
        .filter(|o| o.index() < turns.players.len())
        .map_or(0, |o| u32::from(o.get()))
}

/// The game's saves parsed ahead of a forced run ([`forced_run`]): the pool's tasks put them by
/// index, the run takes them in order.
pub(super) struct Ahead<T> {
    pub(super) parsed: Mutex<BTreeMap<usize, T>>,
    pub(super) ready: Condvar,
}

impl<T> Ahead<T> {
    pub(super) const fn new() -> Self {
        Self {
            parsed: Mutex::new(BTreeMap::new()),
            ready: Condvar::new(),
        }
    }

    /// The parsed ones. A lock a panic poisoned still holds the parses that were put: the map is
    /// only ever inserted into and taken from.
    fn locked(&self) -> MutexGuard<'_, BTreeMap<usize, T>> {
        self.parsed.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A task's parse of index `k`.
    pub(super) fn put(&self, k: usize, v: T) {
        let _ = self.locked().insert(k, v);
        self.ready.notify_all();
    }

    /// The run takes index `k`, waiting until it is parsed.
    pub(super) fn take(&self, k: usize) -> T {
        let mut parsed = self.locked();
        loop {
            if let Some(v) = parsed.remove(&k) {
                return v;
            }
            parsed = self
                .ready
                .wait(parsed)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// Where a forced run's time went ([`forced_run`]): waiting for the game's saves to be parsed
/// (the parse of a batch runs on every core while the run waits), the slice's updates, the
/// comparisons (with the dumps' reading) and the forcing.
#[derive(Clone, Copy, Debug, Default)]
pub struct Times {
    /// Waiting for the parsed saves.
    pub parse: Duration,
    /// The slice's updates.
    pub run: Duration,
    /// The comparisons with the game's saves and dumps.
    pub compare: Duration,
    /// The forcing.
    pub force: Duration,
}

/// The game's state in a save as the slice's units ([`genie_core::world::GameState`]).
pub(super) fn game_state(
    header: &Header,
    masters: Option<&MasterTable>,
) -> Result<GameState, GenieError> {
    let world = &header.world.base_world;
    let mut game = GameState {
        frame_seed: FrameSeed::of_data(world.current_frame_random_seed),
        next_reusable_id: ObjectId::try_from(world.next_reusable_object_id)?,
        game_state: PlayState::of_data(u32::from(world.temp_game_state)),
        groups: groups::unit_groups(world).ok(),
        ..Default::default()
    };
    for p in header.players() {
        let civ = CivId::new(p.world_player().base.civ_id);
        game.players.push(
            p.live_objects()
                .map(|o| {
                    object(
                        o,
                        p.objects(),
                        &world.path,
                        civ,
                        masters,
                        header.players().len(),
                    )
                })
                .collect(),
        );
        let attributes = &p.world_player().base.attributes;
        let r = |i: usize| attributes.get(i).copied().unwrap_or_default();
        game.resources.push([r(0), r(1), r(2), r(3)]);
        game.attributes.push(attributes.clone());
        game.pending_debit
            .push(p.world_player().base.pending_debit.map(Amount::new));
        let base = &p.world_player().base;
        game.statuses.push((
            GameStatus::of_data(base.game_status),
            base.resigned & 2 == 0 && base.resigned != 0,
        ));
        let techs = &p.world_player().tech_tree.tech_player_tree;
        game.techs
            .push(techs.iter().map(tech_state).collect::<Result<_, _>>()?);
    }
    Ok(game)
}

#[cfg(test)]
mod tests;
