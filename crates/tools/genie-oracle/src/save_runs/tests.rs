use crate::command_args::Frame;
use crate::errors::{bad_mesh_dump, bad_pool_dump, bad_tile_dump, no_such_player};
use crate::fixtures::recording;
use crate::fixtures::{combat_object, gaia, header, human};
use crate::map_and_sight::SavedStep;
use crate::save_runs::{
    compare_players, frame_commands, from_save, load_dumps, played_frames, primed, save_header,
    stock_differences, stops, world_differences,
};
use crate::setup::slice;
use crate::test_ids::object_id;
use crate::{Decoder, Dumps, TickReport, listed};
use genie_core::GenieError;
use genie_core::groups::Differentiator;
use genie_core::objects::HitPoints;
use genie_core::objects::ListIndex;
use genie_core::players::Amount;
use genie_core::players::game_status::GameStatus;
use genie_core::world::Millis;
use genie_core::world::WorldTime;
use genie_core::world::load::map::saved_sight;
use genie_core::world::{GameState, World};
use genie_core::world::{PlayState, Tick};
use genie_rec::de::Op;
use genie_rec::de::command::Move;
use genie_save::Header;

/// Gaia and a player with one unit (id 7).
fn save() -> Header {
    header(vec![gaia(Vec::new()), human(vec![combat_object(7, 83)])])
}

/// The game's save as the slice holds it: the same objects, sight and state.
fn same(save: &Header, turns: &World) -> SavedStep {
    SavedStep {
        objects: turns
            .players
            .iter()
            .map(|player| {
                player
                    .objects
                    .iter()
                    .filter(|unit| listed(unit))
                    .map(|unit| turns.object_of(unit))
                    .collect()
            })
            .collect(),
        ghosts: Vec::new(),
        seen: saved_sight(save).unwrap(),
        width: 0,
        terrain: Vec::new(),
        game: GameState::default(),
    }
}

#[test]
fn the_slice_of_the_save_compares_without_differences() {
    let save = save();
    let turns = slice(&save, None).unwrap();
    let mut report = TickReport::default();
    compare_players(&turns, &same(&save, &turns), None, &mut report).unwrap();
    assert_eq!(report, TickReport::default());
}

/// A player the game's save does not hold is an error.
#[test]
fn player_the_game_does_not_hold_is_an_error() {
    let save = save();
    let turns = slice(&save, None).unwrap();
    let mut after = same(&save, &turns);
    let _ = after.objects.pop();
    let mut report = TickReport::default();
    let result = compare_players(&turns, &after, None, &mut report);
    assert!(result.is_err_and(|error| error == no_such_player(1)));
}

/// The game state and a player's status, each where it differs; a frozen object's own
/// differences are left out, another's are not.
#[test]
fn the_state_the_status_and_the_unfrozen_objects_differ() {
    let save = header(vec![
        gaia(Vec::new()),
        human(vec![combat_object(7, 83), combat_object(8, 83)]),
    ]);
    let mut turns = slice(&save, None).unwrap();
    let mut after = same(&save, &turns);
    turns.game_state = PlayState::of_data(2);
    after.game.statuses = vec![(GameStatus::Playing, false), (GameStatus::Won, false)];
    turns.players[1].freezes.push((
        Tick::new(5),
        object_id(7),
        GenieError::unsupported("a test's freeze"),
    ));
    for object in &mut after.objects[1] {
        object.unit.life.hp += HitPoints::new(1.0);
    }
    let mut report = TickReport::default();
    compare_players(&turns, &after, Some(6), &mut report).unwrap();
    let differences: Vec<_> = report
        .differences
        .iter()
        .map(|(player, text)| (*player, text.as_str()))
        .collect();
    assert_eq!(differences[0], (0, "game state slice 2 game 0"));
    assert_eq!(
        differences[1],
        (
            1,
            "game status slice (0, false) game (1, false) (status, resigned)"
        )
    );
    assert!(
        differences[2..]
            .iter()
            .all(|(player, text)| *player == 1 && text.starts_with("8:")),
        "{differences:?}"
    );
    assert_eq!(differences.len(), 3, "{differences:?}");
    // the freeze is of update 5, not 6
    assert_eq!(report.unsupported, []);
}

#[test]
fn the_world_differs_by_its_game_state() {
    let save = save();
    let turns = slice(&save, None).unwrap();
    let mut after = same(&save, &turns);
    after.game.game_state = PlayState::of_data(3);
    let mut report = TickReport::default();
    world_differences(&turns, &after, &mut report);
    assert_eq!(
        report.differences,
        [(0, "game state slice 0 game 3".to_owned())]
    );
}

/// A stopped player's stop and the freezes of the update (every update's without one) are
/// reported; the stopped and the frozen objects are left out of the comparison.
#[test]
fn stops_and_freezes_are_reported_by_their_update() {
    let mut turns = slice(&save(), None).unwrap();
    let player = &mut turns.players[1];
    player.stopped = true;
    player.unsupported = Some((
        Tick::new(5),
        ListIndex::new(0),
        GenieError::unsupported("a test's stop"),
    ));
    player.freezes.push((
        Tick::new(6),
        object_id(40),
        GenieError::unsupported("a test's freeze"),
    ));
    player.objects[ListIndex::new(0)].stopped = true;
    let mut report = TickReport::default();
    assert_eq!(
        stops(player, 1, Some(5), &mut report),
        [object_id(7), object_id(40)]
    );
    assert_eq!(
        report
            .unsupported
            .iter()
            .map(|(player, object_id, tick, _)| (*player, *object_id, *tick))
            .collect::<Vec<_>>(),
        [(1, None, 5)]
    );
    let mut report = TickReport::default();
    let _ = stops(player, 1, None, &mut report);
    assert_eq!(report.unsupported.len(), 2);
    // update 6 reports its freeze alone: the stop is of update 5
    let mut report = TickReport::default();
    let _ = stops(player, 1, Some(6), &mut report);
    assert_eq!(
        report
            .unsupported
            .iter()
            .map(|(player, object_id, tick, _)| (*player, *object_id, *tick))
            .collect::<Vec<_>>(),
        [(1, Some(object_id(40)), 6)]
    );
    // a player that runs on reports no stop, whatever it last met
    player.stopped = false;
    player.freezes.clear();
    let mut report = TickReport::default();
    let _ = stops(player, 1, Some(5), &mut report);
    assert_eq!(report.unsupported, []);
}

/// A stopped player's resources and attributes are not compared: they stood still from its
/// stop on. A player that runs has them compared.
#[test]
fn stopped_player_stock_is_not_compared() {
    let save = save();
    let mut turns = slice(&save, None).unwrap();
    let mut after = same(&save, &turns);
    after.game.resources = vec![[0.0; 4], [9.0, 0.0, 0.0, 0.0]];
    let mut report = TickReport::default();
    compare_players(&turns, &after, None, &mut report).unwrap();
    assert!(
        report
            .differences
            .iter()
            .any(|(player, text)| *player == 1 && text.starts_with("resources ")),
        "{report:?}"
    );
    turns.players[1].stopped = true;
    let mut report = TickReport::default();
    compare_players(&turns, &after, None, &mut report).unwrap();
    assert!(
        !report
            .differences
            .iter()
            .any(|(_, text)| text.starts_with("resources ")),
        "{report:?}"
    );
}

/// The resources are one difference; each other attribute that differs is an entry of its own,
/// past the four resources.
#[test]
fn resources_and_attributes_differ_apart() {
    let mut turns = slice(&save(), None).unwrap();
    let player = &mut turns.players[1];
    player.attributes = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0].map(Amount::new).to_vec();
    let game = GameState {
        resources: vec![[0.0; 4], [1.0, 2.0, 3.0, 5.0]],
        attributes: vec![Vec::new(), vec![0.0, 0.0, 0.0, 0.0, 5.0, 7.0]],
        ..GameState::default()
    };
    let mut report = TickReport::default();
    stock_differences(player, &game, 1, &mut report);
    assert_eq!(
        report.differences,
        [(
            1,
            "resources slice [1.0, 2.0, 3.0, 4.0] game [1.0, 2.0, 3.0, 5.0]".to_owned()
        )]
    );
    assert_eq!(
        report.attributes,
        [(1, 5, 6.0_f32.to_bits(), 7.0_f32.to_bits())]
    );
}

/// Two pools, the missiles and the ghosts.
const POOLS: &str = "pool 60 2\n101 1\n102 0\n\npool 25 1\n201 0\n";

/// A mesh 2 tiles wide with one shape.
const MESH: &str = "mesh width 2 shapes 1 free 0x10\nshape 0 1 2 3 4\nowner 900 0\n";

#[test]
fn the_dumps_go_into_the_slice() {
    let mut turns = slice(&save(), None).unwrap();
    let dumps = Dumps {
        mesh: Some(MESH),
        registry: Some("registry 2 3\n"),
        tiles: Some(""),
        pools: Some(POOLS),
        inside: Some(""),
        globals: Some("group_commands 12\n"),
    };
    load_dumps(&mut turns, dumps).unwrap();
    assert!(turns.pathing.mesh().is_some());
    assert!(turns.spatial.sight.as_ref().unwrap().registry.is_some());
    assert_eq!(
        turns.recycling.missiles,
        [(object_id(102), false), (object_id(101), true)]
    );
    assert_eq!(turns.recycling.ghosts, [object_id(201)]);
    assert_eq!(turns.groups.commands, Differentiator::of_data(12));
}

/// A dump out of shape is its error.
#[test]
fn dump_out_of_shape_is_an_error() {
    let mut turns = slice(&save(), None).unwrap();
    let bad = |dumps: Dumps<'_>| load_dumps(&mut slice(&save(), None).unwrap(), dumps);
    assert!(
        bad(Dumps {
            mesh: Some("tile 0 0 1\n"),
            ..Dumps::default()
        })
        .is_err_and(|error| error == bad_mesh_dump())
    );
    assert!(
        bad(Dumps {
            tiles: Some("x\n"),
            ..Dumps::default()
        })
        .is_err_and(|error| error == bad_tile_dump())
    );
    assert!(
        bad(Dumps {
            inside: Some("x\n"),
            ..Dumps::default()
        })
        .is_err_and(|error| error == bad_tile_dump())
    );
    assert!(
        bad(Dumps {
            pools: Some("101 1\n"),
            ..Dumps::default()
        })
        .is_err_and(|error| error == bad_pool_dump())
    );
    // no dump: nothing changes
    load_dumps(&mut turns, Dumps::default()).unwrap();
    assert!(turns.pathing.mesh().is_none());
}

/// The decoder's priming sets the world's update count to the frames it read and its last
/// commanded units to the frames' last own list of each comm player.
#[test]
fn priming_sets_the_update_count_and_the_last_commanded_units() {
    let save = save();
    let mut turns = slice(&save, None).unwrap();
    let moved = Move {
        commp_id: 1,
        tid: -1,
        x: 5.0,
        y: 6.0,
        num: 1,
        field_0x12: 0,
        field_0x13: 0,
        field_0x14: 0,
        field_0x15: 1,
        gap_0x16: [0; 2],
        ids: vec![7],
    }
    .encode()
    .unwrap();
    let frames = [Frame {
        dt: Millis::new(40),
        checkpoint: None,
        commands: vec![&moved[..]],
    }];
    let _ = primed(&mut turns, &frames, 3);
    assert_eq!(turns.ticks, Tick::new(3));
    assert_eq!(turns.last_commanded, [vec![], vec![Some(object_id(7))]]);
}

/// A frame's command phase moves the world's time on; without a frame nothing happens.
#[test]
fn frames_command_phase_moves_the_time_on() {
    let save = save();
    let mut turns = slice(&save, None).unwrap();
    let mut decoder = Decoder::default();
    frame_commands(&mut turns, &mut decoder, None);
    assert_eq!((turns.ticks.get(), turns.world_time), (0, WorldTime::ZERO));
    let frame = Frame {
        dt: Millis::new(40),
        checkpoint: None,
        commands: Vec::new(),
    };
    frame_commands(&mut turns, &mut decoder, Some(&frame));
    assert_eq!(
        (turns.ticks.get(), turns.world_time),
        (1, WorldTime::from_millis(40))
    );
}

/// Bytes that are no recording are the parse's error, before any run.
#[test]
fn save_or_recording_that_does_not_parse_is_an_error() {
    assert!(save_header(&[]).is_err());
    assert!(from_save(&[], &[], 0, 1, None, Dumps::default(), false).is_err());
    let mut turns = slice(&save(), None).unwrap();
    assert!(played_frames(&mut turns, &[], 1).is_err());
}

/// Gaia and a player without objects, as a save file (one chapter, no ops).
fn quiet_save() -> Vec<u8> {
    recording(&save_of_no_objects(), &[])
}

fn save_of_no_objects() -> Header {
    header(vec![gaia(Vec::new()), human(Vec::new())])
}

/// A recording of the same save and three syncs of 50 ms.
fn three_syncs() -> Vec<u8> {
    let sync = || Op::Sync { dt: 50 };
    recording(&save_of_no_objects(), &[sync(), sync(), sync()])
}

/// A run from the save after update `start` primes the decoder with the updates before and
/// runs on; with the command phase alone it stops after the next update's commands.
#[test]
fn run_from_save_starts_after_its_update() {
    let (save, rec) = (quiet_save(), three_syncs());
    let turns = crate::run_from_save(&save, &rec, 0, 2, None, Dumps::default()).unwrap();
    assert_eq!(turns.ticks.get(), 3);
    let phase = crate::command_phase_from_save(&save, &rec, 1, None, Dumps::default()).unwrap();
    assert_eq!(
        (phase.ticks.get(), phase.world_time),
        (3, WorldTime::from_millis(50))
    );
    let groups = crate::saved_groups(&save, Some("group_commands 3\n")).unwrap();
    assert_eq!(groups.commands, Differentiator::of_data(3));
    assert!(crate::spatial_from_save(&save).is_ok());
    assert!(crate::spatial_from_save(&[]).is_err());
}
