use crate::census::{
    CensusEnd, Followed, census_run, header_divergence, header_divergences, next_id_divergence,
};
use crate::errors::no_such_player;
use crate::fixtures::{combat_object, gaia, header, human, recording};
use crate::{header_slice, lane_block};
use genie_rec::de::{Checkpoint, Op};
use genie_save::Header;

/// Gaia and a player, without objects: nothing either cannot model.
fn quiet() -> Header {
    header(vec![gaia(Vec::new()), human(Vec::new())])
}

/// `count` syncs.
fn syncs(count: usize) -> Vec<Op<'static>> {
    (0..count).map(|_| Op::Sync { dt: 50 }).collect()
}

/// A checkpoint of `lanes`.
fn checkpoint(lanes: &[[u32; 11]; 8]) -> Op<'static> {
    Op::Checkpoint {
        dt: 50,
        checkpoint: Box::new(Checkpoint {
            lanes: *lanes,
            world_time: 50,
        }),
    }
}

/// Player `player`'s lane, every word but its player wrong.
fn wrong(player: u32) -> [u32; 11] {
    let mut lane = [7; 11];
    lane[8] = player;
    lane
}

#[test]
fn recording_without_checkpoints_runs_to_its_end_or_its_limit() {
    let rec = recording(&quiet(), &[Op::Sync { dt: 50 }, Op::Sync { dt: 50 }]);
    let run = census_run(&rec, None, None).unwrap();
    assert_eq!((run.updates, run.syncs, run.checkpoints), (2, 2, 0));
    assert_eq!(run.end, CensusEnd::Recording);
    assert_eq!(run.players.len(), 2);
    let short = census_run(&rec, None, Some(1)).unwrap();
    assert_eq!((short.updates, short.end), (1, CensusEnd::Limit));
}

/// A checkpoint after N syncs holds the state after N updates: a player's lane that equals its
/// block holds, one that differs is its mismatch, and its comparisons end there.
#[test]
fn checkpoint_holds_or_is_the_players_mismatch() {
    let save = quiet();
    let start = header_slice(&save, None).unwrap();
    let mut lanes = [[0; 11]; 8];
    lanes[0] = lane_block(&save.players()[1], 1, &start.players[1], &start.parts);
    let mut later = [[0; 11]; 8];
    later[0] = wrong(1);
    let rec = recording(
        &save,
        &[
            Op::Sync { dt: 50 },
            checkpoint(&lanes),
            checkpoint(&later),
            checkpoint(&later),
        ],
    );
    let run = census_run(&rec, None, None).unwrap();
    assert_eq!((run.syncs, run.checkpoints), (4, 3));
    let player = &run.players[1];
    assert_eq!(player.checkpoints_held, 1);
    let mismatch = player.mismatch.as_ref().unwrap();
    assert_eq!(mismatch.syncs, 2);
    assert_eq!(mismatch.words.len(), 10);
    // Gaia has no lane: it runs on
    assert!(!run.players[0].done());
    assert_eq!(run.end, CensusEnd::Recording);
}

/// Once every player stopped or left its checkpoints the run ends; a lane of a player the save
/// does not hold is skipped.
#[test]
fn the_run_ends_once_every_player_is_done() {
    let mut lanes = [[0; 11]; 8];
    lanes[0] = wrong(0);
    lanes[1] = wrong(1);
    lanes[2] = wrong(5);
    let rec = recording(
        &quiet(),
        &[Op::Sync { dt: 50 }, checkpoint(&lanes), Op::Sync { dt: 50 }],
    );
    let run = census_run(&rec, None, None).unwrap();
    assert_eq!((run.updates, run.end), (1, CensusEnd::AllDone));
}

/// A player whose unit the slice cannot model stops before its first update.
#[test]
fn stop_is_the_players_first_unmodelled_update() {
    let save = header(vec![gaia(Vec::new()), human(vec![combat_object(7, 83)])]);
    let rec = recording(&save, &[Op::Sync { dt: 50 }, Op::Sync { dt: 50 }]);
    let run = census_run(&rec, None, None).unwrap();
    let stop = run.players[1].stop.as_ref().unwrap();
    // the world step before its first update finds the object without a master
    assert_eq!(stop.update, 0);
    assert_ne!(stop.reason, "");
}

/// The game's saves after each update (a chapter, as the tick oracle writes them), the same as
/// the start's; none after update 2.
fn same(save: &Header) -> impl FnMut(usize) -> Option<Vec<u8>> + '_ {
    move |update| (update != 2).then(|| recording(save, &[]))
}

#[test]
fn player_follows_while_its_objects_match_the_games() {
    let save = quiet();
    let rec = recording(&save, &syncs(3));
    let followed = header_divergences(&rec, None, &[0, 1], (2, 3), &mut same(&save)).unwrap();
    assert_eq!(followed, [Followed::Follows, Followed::Follows]);
}

/// The first update whose objects differ from the game's save; a player not in the save is an
/// error.
#[test]
fn player_leaves_where_its_objects_first_differ() {
    let save = quiet();
    let rec = recording(&save, &syncs(3));
    let game = header(vec![gaia(Vec::new()), human(vec![combat_object(7, 83)])]);
    let mut games = |update: usize| (update == 3).then(|| recording(&game, &[]));
    let Followed::Leaves(divergence) =
        header_divergence(&rec, None, 1, (1, 3), &mut games).unwrap()
    else {
        panic!("the player's objects differ at update 3");
    };
    assert_eq!(divergence.update, 3);
    assert_ne!(divergence.differences.len(), 0);
    let missing = header_divergences(&rec, None, &[2], (1, 3), &mut same(&save));
    assert!(missing.is_err_and(|error| error == no_such_player(2)));
}

/// A player that stops says nothing more: the run ends once every player has.
#[test]
fn stopped_player_says_nothing_more() {
    let save = header(vec![gaia(Vec::new()), human(vec![combat_object(7, 83)])]);
    let rec = recording(&save, &syncs(3));
    let mut never = |_: usize| -> Option<Vec<u8>> { panic!("no save is read after the stop") };
    let followed = header_divergence(&rec, None, 1, (1, 3), &mut never).unwrap();
    assert!(matches!(followed, Followed::Stops(census_stop) if census_stop.update == 0));
}

/// The first update after which the world's next object id differs from the game's save, with
/// both ids; none while they agree. A missing save is skipped, as is an update before `first`.
#[test]
fn the_next_object_id_leaves_the_game_at_its_first_differing_save() {
    let save = quiet();
    let rec = recording(&save, &syncs(3));
    assert_eq!(
        next_id_divergence(&rec, None, (1, 3), &mut same(&save)).unwrap(),
        None
    );
    let mut ahead = quiet();
    ahead.world.base_world.next_object_id = save.world.base_world.next_object_id + 1;
    let start = save.world.base_world.next_object_id;
    let mut games = |update: usize| Some(recording(if update < 3 { &save } else { &ahead }, &[]));
    assert_eq!(
        next_id_divergence(&rec, None, (1, 3), &mut games).unwrap(),
        Some((3, start, start + 1))
    );
    let mut from_two =
        |update: usize| Some(recording(if update < 2 { &ahead } else { &save }, &[]));
    assert_eq!(
        next_id_divergence(&rec, None, (2, 3), &mut from_two).unwrap(),
        None
    );
}
