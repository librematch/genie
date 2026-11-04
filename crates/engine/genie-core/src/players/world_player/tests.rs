use super::*;
use crate::map::GridSize;
use crate::map::Tile;
use crate::math::fmath::log;
use crate::movement::explore::InfluenceMap;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::Availability;
use crate::objects::masters::BuiltMode;
use crate::objects::masters::ClassId;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::DisableMode;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::{BuildingMaster, Master, Masters};
use crate::objects::unit::Unit;
use crate::players::CivId;
use crate::players::EscrowShare;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::ResourceStat;
use crate::players::game_status::GameStatus;
use crate::players::history::HistoryUpdates;
use crate::players::relation_row;
use crate::players::tracker::Key;
use crate::players::world_player::ResourceShare;
use crate::technology::ResearchState;
use crate::technology::TechState;
use crate::test_ids::amounts;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::testing::attribute;
use crate::testing::attribute::POPULATION;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::ids::BUILDER;
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::vision::visibility::{Visibility, VisibleMap};
use crate::world::AllySharing;
use crate::world::Millis;
use crate::world::Player;
use crate::world::Seconds;
use crate::world::settings::OptionFlags;
use crate::world::settings::Settings;
use crate::world::{Age, FrameSeed, GameMode, SubModeFlags, Visibility as WorldVisibility};
use std::sync::Arc;

/// Attributes 0 to 299, all 0.
fn attributes() -> Vec<Amount> {
    vec![Amount::ZERO; 300]
}

/// A human player (type 1) still playing, with 300 attributes, whose ruins timer keeps it in
/// the game (the defeat check passes it by).
fn human() -> Player {
    Player {
        kind: PlayerKind::Human,
        attributes: attributes(),
        relations: relation_row([1, 0, 3, 3]),
        victory_times: VictoryTimes {
            ruins: Seconds::ZERO,
            ..VictoryTimes::default()
        },
        ..Player::default()
    }
}

/// Gaia and three humans, at world time 10 s, one second's update ahead.
fn world() -> World {
    let gaia = Player {
        kind: PlayerKind::Gaia,
        attributes: attributes(),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::from_millis(10_000),
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO; 4],
        vec![gaia, human(), human(), human()],
    );
    t.dt_seconds = Seconds::new(1.0);
    t
}

/// A master of `kind` and `class`, `id`.
fn master(id: MasterId, kind: MasterKind, class: ClassId) -> Master {
    Master {
        id,
        kind,
        class,
        available: Availability::AVAILABLE,
        ..Master::default()
    }
}

/// An object `id` of master `master_id` in `state`.
fn object(id: i32, master_id: MasterId, state: ObjectState) -> Unit {
    let mut unit = Unit {
        id: object_id(id),
        ..Unit::default()
    };
    unit.life.master_id = master_id;
    unit.life.state = state;
    unit
}

/// The update time grows by the update's seconds; each whole second runs the loop once (the
/// history's update counts it) with the time before its step; Gaia's own time grows alone.
#[test]
fn each_whole_second_of_update_time_runs_the_loop_once() {
    let mut world = world();
    world.dt_seconds = Seconds::new(0.25);
    world.players[1].update_time = Seconds::new(0.9);
    world.update_world_player(PlayerId::new(1)).unwrap();
    assert_eq!(
        world.players[1].update_time,
        Seconds::new((0.25f32 + 0.9) - 1.0)
    );
    assert_eq!(
        world.players[1].history.update_count,
        HistoryUpdates::of_data(1)
    );
    world.dt_seconds = Seconds::new(2.5);
    world.players[2].update_time = Seconds::ZERO;
    world.update_world_player(PlayerId::new(2)).unwrap();
    assert_eq!(world.players[2].update_time, Seconds::new(0.5));
    assert_eq!(
        world.players[2].history.update_count,
        HistoryUpdates::of_data(2)
    );
    // below a second, or NaN, no loop
    world.dt_seconds = Seconds::new(0.5);
    world.players[3].update_time = Seconds::new(f32::NAN);
    world.update_world_player(PlayerId::new(3)).unwrap();
    assert_eq!(
        world.players[3].history.update_count,
        HistoryUpdates::of_data(0)
    );
    // Gaia: its own update time, no loop
    world.update_world_player(PlayerId::new(0)).unwrap();
    assert_eq!(
        (
            world.players[0].gaia_update_time,
            world.players[0].update_time
        ),
        (Seconds::new(0.5), Seconds::ZERO)
    );
    assert_eq!(
        world.players[0].history.update_count,
        HistoryUpdates::of_data(0)
    );
}

/// Attribute 22: the explored tiles over the map's, in percent; 100 under the explored and
/// all-visible types; left in a slice without the players' sight.
#[test]
fn attribute_22_is_the_explored_share_in_percent() {
    let mut world = world();
    world.players[1].attributes[22] = Amount::new(7.0);
    world.update_world_player(PlayerId::new(1)).unwrap();
    assert_eq!(world.players[1].attributes[22], Amount::new(7.0));
    let map = |explored| VisibleMap {
        counts: vec![-1; 100],
        explored,
    };
    world.spatial.sight = Some(Visibility::new(
        GridSize::new(10, 10),
        vec![
            map(TileCount::ZERO),
            map(TileCount::new(37)),
            map(TileCount::ZERO),
            map(TileCount::ZERO),
        ],
        vec![VisionBits::NONE; 100],
        Vec::new(),
        0,
        vec![PlayerKind::Human; 4],
    ));
    world.update_world_player(PlayerId::new(1)).unwrap();
    assert_eq!(
        world.players[1].attributes[22],
        Amount::new(37.0 / 100.0 * 100.0)
    );
    world.settings.visibility_kind = WorldVisibility::of_data(2);
    world.update_world_player(PlayerId::new(1)).unwrap();
    assert_eq!(world.players[1].attributes[22], Amount::new(100.0));
}

/// Attribute 44: kills (20) less the units owned (19) less the population (11); below 0 it
/// is 0, a NaN stays.
#[test]
fn attribute_44_is_the_kills_less_the_losses() {
    let mut world = world();
    let amounts = &mut world.players[1].attributes;
    (amounts[20], amounts[19], amounts[11]) =
        (Amount::new(5.0), Amount::new(12.0), Amount::new(10.0));
    world.update_world_player(PlayerId::new(1)).unwrap();
    assert_eq!(world.players[1].attributes[44], Amount::new(3.0));
    world.players[1].attributes[19] = Amount::new(20.0);
    world.update_world_player(PlayerId::new(1)).unwrap();
    assert_eq!(world.players[1].attributes[44], Amount::ZERO);
    world.players[1].attributes[20] = Amount::new(f32::NAN);
    world.update_world_player(PlayerId::new(1)).unwrap();
    assert!(world.players[1].attributes[44].get().is_nan());
}

/// The trickles: each of attributes 230 to 233 above 0 gives its sixtieth of the resource
/// and of its gathered total (gold's also of attribute 49); 0, below 0 and NaN give nothing.
/// A lost player's trickles stop.
#[test]
fn the_trickles_give_sixtieth_of_their_rate_each_second() {
    let mut world = world();
    let players = &mut world.players[1].attributes;
    (players[230], players[231], players[232], players[233]) = (
        Amount::new(6.0),
        Amount::new(-1.0),
        Amount::new(f32::NAN),
        Amount::new(60.0),
    );
    world.update_world_player(PlayerId::new(1)).unwrap();
    let player = &world.players[1];
    assert_eq!(player.resources(), amounts([6.0 / 60.0, 0.0, 0.0, 1.0]));
    assert_eq!(
        [166, 167, 168, 169, 49].map(|k| player.attributes[k]),
        amounts([6.0 / 60.0, 0.0, 0.0, 1.0, 1.0])
    );
    world.players[1].game_status = GameStatus::Lost;
    world.update_world_player(PlayerId::new(1)).unwrap();
    assert_eq!(world.players[1].attributes[3], Amount::new(1.0));
}

/// The mills' food: the livestock of the objects filed under the mill (68) that are
/// buildings sums (another type's is passed by); food grows by attribute 254 over 60 times
/// (the update time before the step times logf(sum / 200 + 1)), its gathered total too.
#[test]
fn mill_livestock_makes_food() {
    let mut world = world();
    let masters = Masters::new([
        master(master_id(68), MasterKind::BUILDING, class::BUILDING),
        master(master_id(5), MasterKind::COMBAT, class::LIVESTOCK),
    ]);
    world.model.masters = vec![
        Arc::new(masters.clone()),
        Arc::new(masters.clone()),
        Arc::new(masters.clone()),
        Arc::new(masters),
    ];
    let mill = object(10, master_id(68), ObjectState::Alive);
    let other = object(11, master_id(68), ObjectState::Alive);
    let sheep = object(12, master_id(5), ObjectState::Alive);
    let key = Key {
        master: Some(ids::MILL),
        class: Some(class::BUILDING),
        pending: false,
        military: false,
    };
    let player = &mut world.players[1];
    player.objects = vec![mill, other, sheep].into();
    for id in [10, 11, 12] {
        player.tracker.add(object_id(id), key);
    }
    player.attributes[254] = Amount::new(15.0);
    player.update_time = Seconds::new(0.5);
    world.reindex();
    for (id, sum) in [(10, 100.0), (11, 300.0), (12, 1000.0)] {
        world
            .parts
            .building_mut(object_id(id))
            .unwrap()
            .livestock_summation = Amount::new(sum);
    }
    world.update_world_player(PlayerId::new(1)).unwrap();
    let grows = log(400.0 / 200.0 + 1.0);
    let food = 15.0 / 60.0 * (1.5 * grows);
    assert_eq!(world.players[1].attributes[0], Amount::new(food));
    assert_eq!(world.players[1].attributes[166], Amount::new(food));
}

/// A computer player's income fills its escrow by the share, held to the resource; with
/// sharing on, the allies that count it as theirs get the resource and its gathered total,
/// an enemy, Gaia and a one-sided ally nothing.
#[test]
fn the_income_fills_computer_escrow_and_goes_to_mutual_allies() {
    let mut t = world();
    t.players[1].kind = PlayerKind::Computer;
    t.players[1].escrow_percentage = [0.5, 0.0, 0.0, 0.0].map(EscrowShare::of_data);
    t.add_resource(
        PlayerId::new(1),
        attribute::FOOD,
        Amount::new(10.0),
        ResourceStat::Gathered,
        ResourceShare::Shared,
    )
    .unwrap();
    assert_eq!(
        (t.players[1].attributes[0], t.players[1].escrow[0]),
        (Amount::new(10.0), Amount::new(5.0))
    );
    t.add_resource(
        PlayerId::new(1),
        attribute::FOOD,
        Amount::new(-20.0),
        ResourceStat::Gathered,
        ResourceShare::Shared,
    )
    .unwrap();
    assert_eq!(t.players[1].escrow[0], Amount::ZERO);
    // sharing: 1 and 2 count each other, 3 counts 1 but not the other way round
    let mut t = world();
    t.players[1].ally_sharing.set(AllySharing::RESOURCES, true);
    t.players[1].relations = relation_row([1, 0, 0, 3]);
    t.players[2].relations = relation_row([1, 0, 0, 3]);
    t.players[3].relations = relation_row([1, 0, 3, 0]);
    t.add_resource(
        PlayerId::new(1),
        attribute::WOOD,
        Amount::new(4.0),
        ResourceStat::Gathered,
        ResourceShare::Shared,
    )
    .unwrap();
    assert_eq!(
        [1, 2, 3].map(|p| (
            t.players[p].attributes[1].get(),
            t.players[p].attributes[167].get()
        )),
        [(4.0, 4.0), (4.0, 4.0), (0.0, 0.0)]
    );
    assert_eq!(t.players[0].attributes[1], Amount::ZERO);
    assert!(
        t.shares_with(PlayerId::new(1), PlayerId::new(2))
            && !t.shares_with(PlayerId::new(1), PlayerId::new(3))
            && !t.shares_with(PlayerId::new(1), PlayerId::new(0))
    );
}

/// The defeat check: a unit in state 2 keeps its player; a wall or a farm does not; a
/// building keeps it while something trains there; a packed trebuchet in state 0; a wonder in
/// a scenario mode; a storage of attribute 11 in store mode 0x20. A player kept by nothing
/// loses: the status setter takes 2 and attribute 45 is cleared.
#[test]
fn player_without_an_object_that_keeps_it_loses() {
    let masters = Masters::new([
        master(ids::VILLAGER_MALE, MasterKind::COMBAT, class::VILLAGER),
        master(master_id(117), MasterKind::BUILDING, class::WALL),
        master(ids::FARM, MasterKind::BUILDING, class::FARM),
        master(ids::TOWN_CENTRE, MasterKind::BUILDING, class::BUILDING),
        master(master_id(12), MasterKind::BUILDING, class::BUILDING),
        master(ids::ID_331, MasterKind::COMBAT, class::CLASS_51),
        master(ids::WONDER, MasterKind::BUILDING, class::BUILDING),
        Master {
            storage: vec![
                (Some(POPULATION), Amount::new(1.0), 0x20),
                (None, Amount::ZERO, 0),
                (None, Amount::ZERO, 0),
            ],
            ..master(master_id(900), MasterKind::COMBAT, class::CLASS_11)
        },
        Master {
            creatable: Some(CreatablePart {
                trained_at: vec![ids::TOWN_CENTRE],
                ..master(ids::MARKET, MasterKind::COMBAT, class::VILLAGER)
                    .creatable
                    .unwrap_or_default()
            }),
            ..master(ids::MARKET, MasterKind::COMBAT, class::VILLAGER)
        },
    ]);
    let keeps = |objects: Vec<Unit>, game_mode: i32| {
        let mut t = world();
        t.model.masters = vec![masters.clone(); 4].into_iter().map(Arc::new).collect();
        t.settings.game_mode = GameMode::of_data(game_mode);
        t.players[1].objects = objects.into();
        t.reindex();
        let player = &t.players[1];
        let kept: Vec<bool> = player
            .objects
            .iter()
            .map(|unit| t.keeps_alive(PlayerId::new(1), unit).unwrap())
            .collect();
        kept
    };
    assert_eq!(
        keeps(
            vec![
                object(1, ids::VILLAGER_MALE, ObjectState::Alive),
                object(2, ids::VILLAGER_MALE, ObjectState::Dying),
                object(3, master_id(117), ObjectState::Alive),
                object(4, ids::FARM, ObjectState::Alive),
                object(5, ids::TOWN_CENTRE, ObjectState::Alive),
                object(6, master_id(12), ObjectState::Alive),
                object(7, ids::ID_331, ObjectState::Foundation),
                object(8, ids::ID_331, ObjectState::Unfinished),
                object(9, ids::WONDER, ObjectState::Alive),
                object(10, master_id(900), ObjectState::Dying),
                object(11, master_id(900), ObjectState::Dead),
            ],
            0
        ),
        [
            true, false, false, false, true, false, true, false, false, true, false
        ]
    );
    assert_eq!(
        keeps(vec![object(9, ids::WONDER, ObjectState::Alive)], 3),
        [true]
    );
    assert_eq!(
        keeps(vec![object(9, ids::WONDER, ObjectState::Foundation)], 3),
        [false]
    );
    // a player with only a wall loses, attribute 45 cleared
    let mut t = world();
    t.model.masters = vec![masters.clone(); 4].into_iter().map(Arc::new).collect();
    t.players[1].victory_times = VictoryTimes::default();
    t.players[1].objects = vec![object(3, master_id(117), ObjectState::Alive)].into();
    t.reindex();
    t.players[1].attributes[45] = Amount::new(1.0);
    t.update_world_player(PlayerId::new(1)).unwrap();
    assert_eq!(t.players[1].game_status, GameStatus::Lost);
    assert_eq!(t.players[1].attributes[45], Amount::ZERO);
    // one that a villager keeps plays on
    let mut t = world();
    t.model.masters = vec![masters; 4].into_iter().map(Arc::new).collect();
    t.players[1].victory_times = VictoryTimes::default();
    t.players[1].objects = vec![
        object(3, master_id(117), ObjectState::Alive),
        object(1, ids::VILLAGER_MALE, ObjectState::Alive),
    ]
    .into();
    t.reindex();
    t.players[1].attributes[45] = Amount::new(1.0);
    t.update_world_player(PlayerId::new(1)).unwrap();
    assert_eq!(t.players[1].game_status, GameStatus::Playing);
    assert_eq!(t.players[1].attributes[45], Amount::new(1.0));
}

/// Each victory timer at 0 or more keeps the player; −1 and NaN do not.
#[test]
fn running_victory_timer_keeps_the_player() {
    assert!(!VictoryTimes::default().running());
    for victory_times in [
        VictoryTimes {
            wonder: Seconds::new(0.0),
            ..VictoryTimes::default()
        },
        VictoryTimes {
            ruins: Seconds::new(12.5),
            ..VictoryTimes::default()
        },
        VictoryTimes {
            artifacts: Seconds::new(0.0),
            ..VictoryTimes::default()
        },
    ] {
        assert!(victory_times.running(), "{victory_times:?}");
    }
    let nan = VictoryTimes {
        wonder: Seconds::new(f32::NAN),
        ..VictoryTimes::default()
    };
    assert!(!nan.running());
}

/// `FUN_1416597e0`: the tracker's town centres in its order, 109's list then 444's, those the
/// world holds in any state; the `n`-th of them.
#[test]
fn the_nth_town_centre_follows_the_tracker_and_skips_what_the_world_lacks() {
    let key = |master| Key {
        master: Some(master),
        class: Some(class::BUILDING),
        pending: false,
        military: false,
    };
    let mut world = world();
    world.players[1].objects = vec![
        object(7, master_id(444), ObjectState::Alive),
        object(5, ids::TOWN_CENTRE, ObjectState::Dying),
        object(6, ids::TOWN_CENTRE, ObjectState::Alive),
    ]
    .into();
    world.reindex();
    // 9 is filed but gone from the world
    for (id, master) in [(5, 109), (9, 109), (6, 109), (7, 444)] {
        world.players[1]
            .tracker
            .add(object_id(id), key(master_id(master)));
    }
    let all: Vec<Option<ObjectId>> = (0..4)
        .map(|n| world.town_centre_at(PlayerId::new(1), n))
        .collect();
    assert_eq!(
        all,
        [
            Some(object_id(5)),
            Some(object_id(6)),
            Some(object_id(7)),
            None
        ]
    );
    assert_eq!(world.town_centre_at(PlayerId::new(2), 0), None);
}

/// Sudden death: the first built town centre found sets the latch; once set, a player
/// without one loses. Regicide: a player without a king loses though its units keep it.
#[test]
fn sudden_death_and_regicide_end_player_their_ways() {
    let masters = Masters::new([
        master(ids::VILLAGER_MALE, MasterKind::COMBAT, class::VILLAGER),
        master(ids::TOWN_CENTRE, MasterKind::BUILDING, class::BUILDING),
    ]);
    let tc_key = Key {
        master: Some(ids::TOWN_CENTRE),
        class: Some(class::BUILDING),
        pending: false,
        military: false,
    };
    let mut t = world();
    t.model.masters = vec![masters.clone(); 4].into_iter().map(Arc::new).collect();
    t.settings.game_mode = GameMode::of_data(11);
    t.players[1].victory_times = VictoryTimes::default();
    t.players[1].objects = vec![
        object(1, ids::VILLAGER_MALE, ObjectState::Alive),
        object(5, ids::TOWN_CENTRE, ObjectState::Alive),
    ]
    .into();
    t.reindex();
    t.players[1].tracker.add(object_id(5), tc_key);
    t.update_world_player(PlayerId::new(1)).unwrap();
    assert!(t.players[1].sudden_death_eliminable);
    assert_eq!(t.players[1].game_status, GameStatus::Playing);
    assert_eq!(t.first_town_centre(PlayerId::new(1)), Some(object_id(5)));
    // the town centre falls (state 3): the villager no longer keeps the player
    t.players[1].objects[ListIndex::new(1)].life.state = ObjectState::Dying;
    assert_eq!(t.first_town_centre(PlayerId::new(1)), None);
    t.update_world_player(PlayerId::new(1)).unwrap();
    assert_eq!(t.players[1].game_status, GameStatus::Lost);
    // regicide (sub-mode bit 2): no king, a villager notwithstanding
    let mut t = world();
    t.model.masters = vec![masters; 4].into_iter().map(Arc::new).collect();
    t.settings.sub_game_mode_flag = SubModeFlags::from_bits_retain(4);
    t.players[1].objects = vec![object(1, ids::VILLAGER_MALE, ObjectState::Alive)].into();
    t.reindex();
    t.update_world_player(PlayerId::new(1)).unwrap();
    assert_eq!(t.players[1].game_status, GameStatus::Lost);
    let mut t2 = world();
    t2.settings.game_mode = GameMode::of_data(1);
    t2.players[1].tracker.add(
        object_id(7),
        Key {
            master: Some(ids::KING),
            class: Some(class::KING),
            pending: false,
            military: true,
        },
    );
    t2.update_world_player(PlayerId::new(1)).unwrap();
    assert_eq!(t2.players[1].game_status, GameStatus::Playing);
}

/// `FUN_141625a20`'s count: masters of type 70 and up, had, not disabled, trained at the
/// building or at the stack unit of the local player's building of class 3 that disappears
/// when built.
#[test]
fn the_trainable_count_takes_the_had_masters_trained_at_the_building() {
    let mut world = world();
    let site = BuildingMaster {
        disappears_when_built: BuiltMode::DISAPPEARS,
        stack_unit: Some(ids::TOWN_CENTRE),
        ..BuildingMaster::default()
    };
    let masters = Masters::new([
        Master {
            creatable: Some(CreatablePart {
                trained_at: vec![ids::TOWN_CENTRE],
                ..master(ids::VILLAGER_MALE, MasterKind::COMBAT, class::VILLAGER)
                    .creatable
                    .unwrap_or_default()
            }),
            ..master(ids::VILLAGER_MALE, MasterKind::COMBAT, class::VILLAGER)
        },
        Master {
            available: Availability::UNAVAILABLE,
            creatable: Some(CreatablePart {
                trained_at: vec![ids::TOWN_CENTRE],
                ..master(ids::MARKET, MasterKind::COMBAT, class::VILLAGER)
                    .creatable
                    .unwrap_or_default()
            }),
            ..master(ids::MARKET, MasterKind::COMBAT, class::VILLAGER)
        },
        Master {
            disabled: DisableMode::DISABLED,
            creatable: Some(CreatablePart {
                trained_at: vec![ids::TOWN_CENTRE],
                ..master(master_id(85), MasterKind::COMBAT, class::VILLAGER)
                    .creatable
                    .unwrap_or_default()
            }),
            ..master(master_id(85), MasterKind::COMBAT, class::VILLAGER)
        },
        Master {
            creatable: Some(CreatablePart {
                trained_at: vec![ids::TOWN_CENTRE],
                ..master(master_id(86), MasterKind::MISSILE, class::VILLAGER)
                    .creatable
                    .unwrap_or_default()
            }),
            ..master(master_id(86), MasterKind::MISSILE, class::VILLAGER)
        },
        Master {
            creatable: Some(CreatablePart {
                trained_at: vec![master_id(12)],
                ..master(master_id(74), MasterKind::COMBAT, class::CLASS_6)
                    .creatable
                    .unwrap_or_default()
            }),
            ..master(master_id(74), MasterKind::COMBAT, class::CLASS_6)
        },
        master(ids::TOWN_CENTRE, MasterKind::BUILDING, class::BUILDING),
        Master {
            building: Some(site),
            ..master(ids::TOWN_CENTRE_621, MasterKind::BUILDING, class::BUILDING)
        },
    ]);
    world.model.masters = vec![masters.clone(); 4].into_iter().map(Arc::new).collect();
    assert_eq!(world.trainable_count(PlayerId::new(1), ids::TOWN_CENTRE), 1);
    assert_eq!(world.trainable_count(PlayerId::new(1), master_id(12)), 1);
    assert_eq!(world.trainable_count(PlayerId::new(1), master_id(101)), 0);
    // the local player's 621 stands for 109
    assert_eq!(
        world.trainable_count(PlayerId::new(1), ids::TOWN_CENTRE_621),
        1
    );
    // a builder with attribute 48 at 0 and the town centre not had: one more below the
    // town centre limit (218)
    let mut builder = masters;
    builder.get_mut(ids::TOWN_CENTRE_621).unwrap().available = Availability::UNAVAILABLE;
    world.model.masters[1] = Arc::new(builder);
    world.players[1].attributes[218] = Amount::new(1.0);
    assert_eq!(world.trainable_count(PlayerId::new(1), BUILDER), 1);
    world.players[1].attributes[48] = Amount::new(1.0);
    assert_eq!(world.trainable_count(PlayerId::new(1), BUILDER), 0);
}

/// The tech tree's lookups: a unit at its building, its link and technology; a later age
/// than the game's last less 1 is out, and so are ages the player cannot reach; a building
/// the civilisation lacks under the full tech tree is the fallback's.
#[test]
fn the_tech_tree_answers_by_civilisation_building_and_unit() {
    let unit = |id, link, age, tech| TreeUnit {
        id: TreeMaster::of_data(id),
        link: TreeMaster::of_data(link),
        age,
        tech: TreeTech::of_data(tech),
    };
    let tree = TechTree {
        civs: vec![
            vec![TreeBuilding {
                id: TreeMaster::of_data(12),
                units: vec![
                    unit(74, 75, Age::of_data(1), 87),
                    unit(93, -1, Age::of_data(3), 197),
                ],
            }],
            vec![TreeBuilding {
                id: TreeMaster::of_data(87),
                units: vec![unit(4, 24, Age::of_data(2), -1)],
            }],
        ],
        unique_units: Vec::new(),
    };
    let settings = Settings::default();
    let q = |civ, building, unit, ages_out| TreeQuery {
        civ,
        building: TreeMaster::of_data(building),
        unit: TreeMaster::of_data(unit),
        ages_out,
        settings: &settings,
        fallback: CivId::new(0),
    };
    let none = [false; 3];
    assert!(tree.has(&q(CivId::new(0), 12, 74, none)));
    assert_eq!(
        (
            tree.link(&q(CivId::new(0), 12, 74, none)),
            tree.tech(&q(CivId::new(0), 12, 74, none))
        ),
        (TreeMaster::of_data(75), TreeTech::of_data(87))
    );
    assert!(!tree.has(&q(CivId::new(0), 12, 4, none)));
    assert_eq!(tree.link(&q(CivId::new(0), 12, 4, none)), TreeMaster::NONE);
    assert!(!tree.has(&q(CivId::new(0), 87, 4, none)));
    assert_eq!(
        tree.link(&q(CivId::new(5), 12, 74, none)),
        TreeMaster::of_data(0)
    );
    // the Castle Age out of reach
    assert!(!tree.has(&q(CivId::new(0), 12, 93, [false, true, false])));
    assert_eq!(
        tree.tech(&q(CivId::new(0), 12, 93, [false, true, false])),
        TreeTech::of_data(0)
    );
    // the last age 3: age 3 is out
    let early = Settings {
        ending_age: Age::of_data(3),
        ..Settings::default()
    };
    let late = TreeQuery {
        settings: &early,
        ..q(CivId::new(0), 12, 93, none)
    };
    assert!(!tree.has(&late));
    // the full tech tree: civilisation 1 lacks the barracks, the fallback 0 has it
    let full = Settings {
        flags: OptionFlags::FULL_TECH_TREE,
        ..Settings::default()
    };
    let fell = TreeQuery {
        settings: &full,
        ..q(CivId::new(1), 12, 74, none)
    };
    assert!(tree.has(&fell));
    assert_eq!(tree.link(&fell), TreeMaster::of_data(75));
    assert!(!tree.has(&q(CivId::new(1), 12, 74, none)));
}

/// `FUN_141610470`'s count: a master trained at the building counts when the tree has it,
/// its link is had (or none) and its technology researched (or none).
#[test]
fn the_upgradable_count_follows_the_tech_tree() {
    let mut world = world();
    let masters = Masters::new([
        Master {
            available: Availability::UNAVAILABLE,
            creatable: Some(CreatablePart {
                trained_at: vec![master_id(12)],
                ..master(master_id(74), MasterKind::COMBAT, class::CLASS_6)
                    .creatable
                    .unwrap_or_default()
            }),
            ..master(master_id(74), MasterKind::COMBAT, class::CLASS_6)
        },
        Master {
            available: Availability::UNAVAILABLE,
            creatable: Some(CreatablePart {
                trained_at: vec![master_id(12)],
                ..master(master_id(75), MasterKind::COMBAT, class::CLASS_6)
                    .creatable
                    .unwrap_or_default()
            }),
            ..master(master_id(75), MasterKind::COMBAT, class::CLASS_6)
        },
        master(master_id(12), MasterKind::BUILDING, class::BUILDING),
    ]);
    world.model.masters = vec![masters; 4].into_iter().map(Arc::new).collect();
    world.players[1].civ = CivId::new(0);
    world.players[1].techs = vec![TechState::default(); 200];
    world.model.tech_tree = Arc::new(TechTree {
        civs: vec![vec![TreeBuilding {
            id: TreeMaster::of_data(12),
            units: vec![
                TreeUnit {
                    id: TreeMaster::of_data(74),
                    link: TreeMaster::NONE,
                    age: Age::of_data(1),
                    tech: TreeTech::of_data(-1),
                },
                TreeUnit {
                    id: TreeMaster::of_data(75),
                    link: TreeMaster::of_data(74),
                    age: Age::of_data(2),
                    tech: TreeTech::of_data(150),
                },
            ],
        }]],
        unique_units: Vec::new(),
    });
    // 74 counts (no link, no technology); 75's link 74 is not had
    assert_eq!(world.upgradable_count(PlayerId::new(1), master_id(12)), 1);
    Arc::make_mut(&mut world.model.masters[1])
        .get_mut(master_id(74))
        .unwrap()
        .available = Availability::AVAILABLE;
    // 75: its link had, its technology not researched
    assert_eq!(world.upgradable_count(PlayerId::new(1), master_id(12)), 1);
    world.players[1].techs[150].state = ResearchState::Researched;
    assert_eq!(world.upgradable_count(PlayerId::new(1), master_id(12)), 2);
    // the Feudal Age out of reach puts 75 out
    world.players[1].techs[101].state = ResearchState::Disabled;
    assert_eq!(world.upgradable_count(PlayerId::new(1), master_id(12)), 1);
}

/// The scout map takes the tiles of the player's tile list: before 5000 ms only those it
/// sees, after it all; a player without a scout map, and Gaia, take nothing.
#[test]
fn the_scout_map_takes_the_new_tiles() {
    let mut world = world();
    let mut unified = vec![VisionBits::NONE; 100];
    unified[0] = VisionBits::from_bits_retain(1 << 1);
    let map = VisibleMap {
        counts: vec![-1; 100],
        explored: TileCount::ZERO,
    };
    let mut sight = Visibility::new(
        GridSize::new(10, 10),
        vec![map.clone(), map.clone(), map.clone(), map],
        unified,
        Vec::new(),
        0,
        vec![PlayerKind::Human; 4],
    );
    sight.new_tiles[1] = vec![Tile::new(0, 0), Tile::new(1, 0)];
    world.spatial.sight = Some(sight);
    world.world_player_tail(PlayerId::new(1)).unwrap();
    world.spatial.set_scout_map(
        PlayerId::new(1),
        InfluenceMap::new(GridSize::new(10, 10), 0),
    );
    world.world_time = WorldTime::from_millis(4999);
    world.world_player_tail(PlayerId::new(1)).unwrap();
    let influence_map = world.spatial.scout_map(PlayerId::new(1)).unwrap();
    assert_eq!(
        (
            influence_map.get(Tile::new(0, 0)),
            influence_map.get(Tile::new(1, 0))
        ),
        (Some(1), Some(0))
    );
    world.world_time = WorldTime::from_millis(5000);
    world.world_player_tail(PlayerId::new(1)).unwrap();
    assert_eq!(
        world
            .spatial
            .scout_map(PlayerId::new(1))
            .unwrap()
            .get(Tile::new(1, 0)),
        Some(1)
    );
}
