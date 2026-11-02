//! One update step by step against the game's saves at the end of its steps.

use genie_core::world::schedule::{
    AFTER_PLAYERS, BEFORE_PLAYERS, EACH_PLAYER, PlayerStep, UpdateStep, WorldStep,
};

use crate::Dumps;
use crate::fixtures::recording;
use crate::fixtures::{gaia, header, human};
use crate::step_runs::steps_from_save;
use genie_rec::de::Op;

/// Gaia and a player without objects, as a save file (one chapter, no ops).
fn quiet() -> Vec<u8> {
    recording(&header(vec![gaia(Vec::new()), human(Vec::new())]), &[])
}

/// A recording of the same save and `n` syncs of 50 ms.
fn syncs(n: usize) -> Vec<u8> {
    let ops: Vec<_> = (0..n).map(|_| Op::Sync { dt: 50 }).collect();
    recording(&header(vec![gaia(Vec::new()), human(Vec::new())]), &ops)
}

/// Every step of the update is reported in the schedule's order, and a quiet world against the
/// same quiet save parts at none of them.
#[test]
fn quiet_update_parts_at_no_step() {
    let (save, rec) = (quiet(), syncs(3));
    let game_at = |_| Some((quiet(), None));
    let steps = steps_from_save(&save, &rec, 0, None, Dumps::default(), &game_at).unwrap();
    let names: Vec<UpdateStep> = steps.iter().map(|(s, _)| *s).collect();
    let players = 2 * EACH_PLAYER.len();
    assert_eq!(
        names.len(),
        BEFORE_PLAYERS.len() + players + AFTER_PLAYERS.len()
    );
    assert_eq!(names[0], UpdateStep::World(WorldStep::Commands));
    assert_eq!(names.last(), Some(&UpdateStep::World(WorldStep::Vision)));
    for (step, report) in &steps {
        let report = report.as_ref().unwrap();
        assert_eq!(report.differences, [], "{step}");
        assert_eq!((report.start, report.ticks), (0, 1));
    }
}

/// A step without the game's save is not compared; the others are.
#[test]
fn step_without_the_game_save_is_not_compared() {
    let (save, rec) = (quiet(), syncs(3));
    let game_at =
        |s| matches!(s, UpdateStep::Player(PlayerStep::Objects, _)).then(|| (quiet(), None));
    let steps = steps_from_save(&save, &rec, 1, None, Dumps::default(), &game_at).unwrap();
    let compared: Vec<String> = steps
        .iter()
        .filter(|(_, r)| r.is_some())
        .map(|(s, _)| s.to_string())
        .collect();
    assert_eq!(compared, ["Objects of player 0", "Objects of player 1"]);
}

/// A recording that ends before the update has no steps; a save that does not parse is an
/// error, and so is a game's save that does not.
#[test]
fn an_update_past_the_recording_has_no_steps() {
    let (save, rec) = (quiet(), syncs(1));
    let none = |_| None;
    let steps = steps_from_save(&save, &rec, 3, None, Dumps::default(), &none).unwrap();
    assert_eq!(steps, []);
    assert!(steps_from_save(&[], &rec, 0, None, Dumps::default(), &none).is_err());
    let broken = |_| Some((vec![1, 2, 3], None));
    let rec = syncs(3);
    assert!(steps_from_save(&save, &rec, 0, None, Dumps::default(), &broken).is_err());
}
