use crate::combat::Firepower;
use crate::objects::masters::FlyMode;
use crate::objects::masters::MasterId;
use crate::players::CivId;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::history::HistoryUpdates;
use crate::technology::ResearchState;
use crate::test_ids::action_kind;
use crate::test_ids::object_id;
use crate::testing::layer;
use crate::world::Departures;
use crate::world::WorldTime;
use crate::world::load::fixtures::static_mut;
use crate::world::load::fixtures::{
    action_list, building_mut, building_object, combat_object, gaia, header, human, human_mut,
};
use crate::world::load::world::{saved_world, wonder_time};
use genie_import::dat::MasterTable;
use genie_save::generated::world::{Action, Object, RgeAction, TechPlayerTree, TribeActionWonder};

/// A wonder whose action is in state `state` with the timer `timer`.
fn wonder(id: i32, state: u8, timer: f32) -> Object {
    let mut wonder = building_object(id, 276);
    let action = Action {
        tribe_action_wonder: Some(Box::new(TribeActionWonder {
            rge_action: RgeAction {
                state,
                timer,
                ..RgeAction::default()
            },
            ..TribeActionWonder::default()
        })),
        ..Action::default()
    };
    building_mut(&mut wonder)
        .tribe_combat_object
        .rge_combat_object
        .rge_action_object
        .actions = action_list(vec![(action_kind(120), action)]);
    wonder
}

#[test]
fn player_without_standing_wonder_has_no_countdown() {
    let save = header(vec![human(vec![combat_object(1, 83), wonder(2, 1, 50.0)])]);
    assert_eq!(
        wonder_time(&save.players()[0]).get().to_bits(),
        (-1.0_f32).to_bits()
    );
}

/// The least timer of the standing wonders counts, the sleeping ones' too.
#[test]
fn the_countdown_is_the_least_standing_wonders_timer() {
    let mut player = human(vec![wonder(1, 7, 300.0), wonder(2, 1, 10.0)]);
    let objects = &mut player
        .human_or_coop_player
        .as_deref_mut()
        .unwrap()
        .player_objects;
    objects.sleeping_objects.object_kind_list = human(vec![wonder(3, 7, 200.0)])
        .human_or_coop_player
        .unwrap()
        .player_objects
        .objects
        .object_kind_list;
    let save = header(vec![player]);
    assert_eq!(
        wonder_time(&save.players()[0]).get().to_bits(),
        200.0_f32.to_bits()
    );
}

/// A save's world and players as the load sets them up, without the master table.
#[test]
fn the_load_takes_the_worlds_and_the_players_fields() {
    let mut player = human(vec![combat_object(7, 83)]);
    let wp = human_mut(&mut player);
    wp.base.civ_id = 3;
    wp.base.attributes = vec![200.0, 150.0, 100.0, 50.0];
    wp.base.resigned = 2;
    wp.ruin_held_time = 12.5;
    let mut save = header(vec![gaia(vec![combat_object(1, 594)]), player]);
    let world = &mut save.world.base_world;
    world.world_time = 1500;
    world.current_frame_random_seed = 77;
    world.player_turn = -1;
    world.next_object_id = 300;
    world.next_reusable_object_id = -5;
    world.local_player_index = 1;
    save.world.player_teams = vec![0, 2];
    let turns = saved_world(&save, None, layer()).unwrap();
    assert_eq!(
        (
            turns.world_time,
            turns.next_object_id.get(),
            turns.local_player
        ),
        (WorldTime::from_millis(1500), 300, PlayerId::new(1))
    );
    assert_eq!(turns.recycling.next_id, object_id(-5));
    assert_eq!(
        turns.teams[..2].iter().map(|t| t.get()).collect::<Vec<_>>(),
        [0, 2]
    );
    let p = &turns.players[1];
    assert_eq!((p.civ, p.kind), (CivId::new(3), PlayerKind::Human));
    assert_eq!(
        p.resources().map(|r| r.get().to_bits()),
        [200.0_f32, 150.0, 100.0, 50.0].map(f32::to_bits)
    );
    assert!(
        p.departures.contains(Departures::DROPPED) && !p.departures.contains(Departures::RESIGNED)
    );
    assert_eq!(p.victory_times.ruins.get().to_bits(), 12.5_f32.to_bits());
    assert_eq!(p.victory_times.wonder.get().to_bits(), (-1.0_f32).to_bits());
    assert_eq!(
        p.objects.iter().map(|u| u.id.get()).collect::<Vec<_>>(),
        [7]
    );
    assert_eq!(
        turns.players[0]
            .objects
            .iter()
            .map(|u| u.id.get())
            .collect::<Vec<_>>(),
        [1]
    );
    // no table: no masters, so no unit lines and no planners
    assert_eq!(turns.model.masters.len(), 0);
}

/// A resign is the flag without the drop's bit.
#[test]
fn resign_is_not_drop() {
    let mut player = human(Vec::new());
    human_mut(&mut player).base.resigned = 1;
    let turns = saved_world(&header(vec![gaia(Vec::new()), player]), None, layer()).unwrap();
    assert!(
        turns.players[1].departures.contains(Departures::RESIGNED)
            && !turns.players[1].departures.contains(Departures::DROPPED)
    );
}

/// A saved technology state reads as its state (4, queued); one that names none (5) stops
/// every player.
#[test]
fn technology_state_of_none_stops_every_player() {
    let with = |state: i16| {
        let mut player = human(Vec::new());
        human_mut(&mut player).tech_tree.tech_player_tree = vec![TechPlayerTree {
            state,
            ..TechPlayerTree::default()
        }];
        saved_world(&header(vec![gaia(Vec::new()), player]), None, layer()).unwrap()
    };
    let turns = with(4);
    assert_eq!(turns.players[1].techs[0].state, ResearchState::Queued);
    assert!(!turns.players[1].stopped);
    let turns = with(5);
    assert!(
        turns
            .players
            .iter()
            .all(|p| p.stopped && p.unsupported.is_some())
    );
}

/// With the master table, the load takes its tables and builds every player's masters.
#[test]
fn the_load_takes_the_master_tables() {
    let mut table = MasterTable::of_master(
        CivId::GAIA,
        MasterId::of_data(83).unwrap(),
        None,
        FlyMode::of_data(0),
    );
    table.time_slice = HistoryUpdates::of_data(9);
    let save = header(vec![gaia(Vec::new()), human(Vec::new())]);
    let turns = saved_world(&save, Some(&table), layer()).unwrap();
    assert_eq!(turns.history_period, HistoryUpdates::of_data(9));
    assert_eq!(turns.model.masters.len(), 2);
    assert_eq!(turns.spatial.fastest.to_bits(), 0.0_f32.to_bits());
}

/// With the master table, a unit inside a building of its player is in the building's garrison
/// list; a building whose master the player lacks takes no share, so its volleys stay.
#[test]
fn unit_inside_building_is_in_its_garrison() {
    let table = MasterTable::of_master(
        CivId::GAIA,
        MasterId::of_data(83).unwrap(),
        None,
        FlyMode::of_data(0),
    );
    let mut inside = combat_object(11, 83);
    static_mut(&mut inside).inside_obj = 10;
    let save = header(vec![
        gaia(Vec::new()),
        human(vec![building_object(10, 12), inside]),
    ]);
    let turns = saved_world(&save, Some(&table), layer()).unwrap();
    let building = turns.players[1]
        .objects
        .iter()
        .find(|u| u.id == object_id(10))
        .unwrap();
    assert_eq!(building.garrison, [object_id(11)]);
    assert_eq!(
        turns.parts.combat_of(object_id(10)).firepower,
        Firepower::ZERO
    );
}
