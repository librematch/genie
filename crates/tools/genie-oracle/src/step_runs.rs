//! One update from a save step by step (FD-045-D13): genie-core's world after each step of its
//! update schedule against the game's save at the end of the same step (the tick oracle's probe
//! `step_saves`), for the first step where they part. Dev only.

use genie_core::world::World;
use genie_core::world::schedule::UpdateStep;

use crate::forced_replay::owned_group_differences;
use crate::{
    Dumps, Error, MasterTable, SavedStep, TickReport, compare_players, loaded_slice, played_frames,
    primed, save_header,
};

/// The game's save at the end of a step, with the process globals dumped there; none where the
/// tick oracle has none.
pub type StepSave = Option<(Vec<u8>, Option<String>)>;

/// The update after save `start` (`save`, with its dumps), run step by step: after each step of
/// the schedule, the step and genie's differences from the game's save at its end (`game_at`),
/// every player's objects and the world's unit groups as [`crate::command_phase`] reports
/// them; none for a step without the game's save. No steps where the recording ends before the
/// update.
///
/// # Errors
///
/// When a save or the recording does not parse.
pub fn steps_from_save(
    save: &[u8],
    recording: &[u8],
    start: usize,
    masters: Option<&MasterTable>,
    dumps: Dumps<'_>,
    game_at: &dyn Fn(UpdateStep) -> StepSave,
) -> Result<Vec<(UpdateStep, Option<TickReport>)>, Error> {
    let header = save_header(save)?;
    let mut turns = loaded_slice(&header, masters, dumps)?;
    let updates = start + 1;
    let all = played_frames(&mut turns, recording, updates + 1)?;
    let mut decoder = primed(&mut turns, &all, updates);
    let Some(frame) = all.get(updates) else {
        return Ok(Vec::new());
    };
    let commands: Vec<_> = frame
        .commands
        .iter()
        .filter_map(|p| decoder.command(p))
        .collect();
    let mut steps = Vec::new();
    let mut failed = None;
    turns.tick_observed(frame.dt, &commands, &mut |w, step| {
        let compared = game_at(step)
            .map(|(save, globals)| step_report(w, start, (&save, globals.as_deref()), masters))
            .transpose();
        match compared {
            Ok(report) => steps.push((step, report)),
            Err(e) => {
                let _ = failed.get_or_insert(e);
            }
        }
    });
    failed.map_or(Ok(steps), Err)
}

/// Genie's world `w` in the update after save `start` against the game's save at the same step
/// (`after`, with its globals).
fn step_report(
    w: &World,
    start: usize,
    (after, globals): (&[u8], Option<&str>),
    masters: Option<&MasterTable>,
) -> Result<TickReport, Error> {
    let mut saved = SavedStep::of(after, masters)?;
    let mut report = TickReport {
        start,
        ticks: 1,
        ..TickReport::default()
    };
    compare_players(w, &saved, None, &mut report)?;
    report
        .differences
        .extend(owned_group_differences(w, &mut saved.game, globals));
    Ok(report)
}

#[cfg(test)]
mod tests;
