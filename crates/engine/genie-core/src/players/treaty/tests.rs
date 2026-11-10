use super::*;
use crate::construction::BuilderCount;
use crate::construction::build::Construction;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::Snow;
use crate::map::spatial::Obstruction;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectState;
use crate::objects::layers::Building;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionKind;
use crate::players::Relation;
use crate::players::treaty::zones::zone_span;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::scene::{Scene, SceneObject};
use crate::world::Seconds;
use crate::world::TreatyLength;
use crate::world::WholeSeconds;

/// Player `index`.
const fn player(index: u8) -> PlayerId {
    PlayerId::new(index)
}

/// A building master `id` of radii and clearance 2 and 100 hit points.
fn building(id: MasterId) -> Master {
    Master {
        id,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(2.0, 2.0),
        clearance: Footprint::new(2.0, 2.0),
        hit_points: MasterHitPoints::of_data(100),
        ..Master::default()
    }
}

/// A unit master (type 70) of 25 hit points.
fn soldier() -> Master {
    Master {
        id: ids::VILLAGER_MALE,
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        hit_points: MasterHitPoints::of_data(25),
        ..Master::default()
    }
}

/// A construction of `progress` build points and a build time of `build_time` seconds.
fn construction(progress: f32, build_time: i16) -> Construction {
    Construction {
        progress: Seconds::new(progress),
        desolid: false,
        builders: BuilderCount::of_data(0),
        hit_points: MasterHitPoints::of_data(1),
        build_time: WholeSeconds::of_data(build_time),
        record: Obstruction {
            kind: ObstructionKind::NONE,
            class: ObstructionClass::NONE,
        },
        snow: Snow::None,
        sprite_override: false,
    }
}

/// Object `id` of `player` and `master` at `x`, `y`.
fn standing(id: i32, owner: PlayerId, master: Master, x: f32, y: f32) -> SceneObject {
    SceneObject::new(id, owner, master, Position::new(x, y, 0.0))
}

/// A foundation of `owner`'s house at `x`, `y`: desolid, in state 0.
fn foundation(id: i32, owner: PlayerId, x: f32, y: f32) -> SceneObject {
    let mut object = standing(id, owner, building(ids::HOUSE), x, y);
    object.state = ObjectState::Foundation;
    object.desolid = true;
    object
}

/// A world of 120 by 120 tiles and `players` players past Gaia, each every other's enemy,
/// holding `objects`; each building in state 2 has its construction built, each foundation the
/// build points `progress` gives it (none without) of 10 build seconds; the trackers file them.
fn world_of(players: u8, objects: Vec<SceneObject>, progress: &[(i32, f32)]) -> World {
    let mut world = objects
        .iter()
        .cloned()
        .fold(
            Scene::open(GridSize::new(120, 120)).players(players),
            Scene::object,
        )
        .build()
        .with_buildings(
            objects
                .iter()
                .filter(|object| {
                    object.master.kind == MasterKind::BUILDING && object.state == ObjectState::Alive
                })
                .map(|object| {
                    (
                        object.id,
                        Building {
                            construction: Some(construction(1.0, 1)),
                            ..Building::default()
                        },
                    )
                }),
        );
    for object in objects
        .iter()
        .filter(|object| object.state == ObjectState::Foundation)
    {
        let points = progress
            .iter()
            .find(|(id, _)| object_id(*id) == object.id)
            .map_or(0.0, |&(_, points)| points);
        world
            .parts
            .building_mut(object.id)
            .unwrap()
            .construction
            .clone_from(&Some(Construction {
                desolid: true,
                ..construction(points, 10)
            }));
    }
    world.load_trackers().unwrap();
    world.dt_seconds = Seconds::new(1.0);
    world
}

/// The relation of `from` toward `to`.
fn relation(world: &World, from: u8, to: u8) -> Relation {
    *world.players[usize::from(from)]
        .relations
        .checked(player(to))
        .unwrap()
}

/// The world's monitor.
fn monitor(world: &World) -> &TreatyMonitor {
    world.treaty.as_ref().unwrap()
}

/// The world's monitor to change.
fn monitor_mut(world: &mut World) -> &mut TreatyMonitor {
    world.treaty.as_mut().unwrap()
}

/// The treaty's seconds are the world's milliseconds as an `int` divided toward zero, minutes
/// too; their difference and a length's end wrap.
#[test]
fn treaty_seconds_cut_the_world_time_toward_zero_and_wrap() {
    assert_eq!(TreatySeconds::at(WorldTime::from_millis(61_999)).get(), 61);
    // a time past an int's range is negative, divided toward zero
    assert_eq!(
        TreatySeconds::at(WorldTime::from_millis(0x8000_0000)).get(),
        -2_147_483
    );
    assert_eq!(TreatySeconds(-61).minutes(), -1);
    assert_eq!(TreatySeconds(119).minutes(), 1);
    assert_eq!(
        TreatySeconds(i32::MIN).less(TreatySeconds(1)),
        TreatySeconds(i32::MAX)
    );
    assert_eq!(TreatyMinutes::of_byte(255).end(), TreatySeconds(15_300));
    assert_eq!(TreatyMinutes::UNTIMED.end(), TreatySeconds(-60));
    assert_eq!(
        TreatyMinutes(i32::MAX).end().get(),
        i32::MAX.wrapping_mul(60)
    );
    assert_eq!(TreatyMinutes::of_byte(7).get(), 7);
    assert_eq!(NomadTreaty::of_byte(0), NomadTreaty::NotForced);
    assert_eq!(NomadTreaty::of_byte(2), NomadTreaty::Forced);
}

/// `FUN_141461190`: the end in seconds, the margin 20, 6 in game mode 16 and 10 for a forced
/// nomad treaty elsewhere; a forced nomad treaty below five minutes (signed) takes five and is
/// lengthened.
#[test]
fn new_monitor_takes_its_end_margin_and_nomad_length() {
    let plain = TreatyMonitor::new(
        TreatyMinutes(10),
        NomadTreaty::NotForced,
        GameMode::RandomMap,
    );
    assert_eq!(
        (plain.end, plain.margin, plain.lengthened, plain.state),
        (TreatySeconds(600), MARGIN, false, TreatyState::New)
    );
    assert_eq!(
        (plain.borders, plain.castle_age, plain.second_zones),
        (Borders::Undrawn, false, false)
    );
    assert_eq!(plain.zones, [TreatyZone::default(); ZONES]);
    let short = TreatyMonitor::new(
        TreatyMinutes(3),
        NomadTreaty::NotForced,
        GameMode::RandomMap,
    );
    assert_eq!((short.end, short.lengthened), (TreatySeconds(180), false));
    let nomad = TreatyMonitor::new(TreatyMinutes(3), NomadTreaty::Forced, GameMode::RandomMap);
    assert_eq!(
        (nomad.end, nomad.margin, nomad.lengthened),
        (TreatySeconds(300), NOMAD_MARGIN, true)
    );
    let long = TreatyMonitor::new(TreatyMinutes(7), NomadTreaty::Forced, GameMode::RandomMap);
    assert_eq!((long.end, long.lengthened), (TreatySeconds(420), false));
    let untimed = TreatyMonitor::new(
        TreatyMinutes::UNTIMED,
        NomadTreaty::Forced,
        GameMode::OneUnit,
    );
    assert_eq!(
        (untimed.end, untimed.margin, untimed.lengthened),
        (TreatySeconds(300), ONE_UNIT_MARGIN, true)
    );
    let one_unit = TreatyMonitor::new(
        TreatyMinutes::UNTIMED,
        NomadTreaty::NotForced,
        GameMode::OneUnit,
    );
    assert_eq!(
        (one_unit.end, one_unit.margin),
        (TreatySeconds(-60), ONE_UNIT_MARGIN)
    );
}

/// A player's zones are at its number less one; Gaia and a player past the nine have none.
#[test]
fn players_past_gaia_hold_the_nine_zones() {
    let mut monitor = TreatyMonitor::new(
        TreatyMinutes(5),
        NomadTreaty::NotForced,
        GameMode::RandomMap,
    );
    monitor.zone_mut(player(1)).unwrap().set = true;
    assert!(monitor.zones[0].set);
    assert!(monitor.zone(player(9)).is_ok());
    assert!(monitor.zone(PlayerId::GAIA).is_err());
    assert!(monitor.zone(player(10)).is_err());
    assert!(monitor.zone_mut(player(10)).is_err());
}

/// `FUN_1415e8e10` and `FUN_1415c8150`: a monitor in state 1 holds and gives its seconds left;
/// none, or one in another state, holds none and gives 0.
#[test]
fn only_monitor_in_state_1_holds_and_gives_its_seconds_left() {
    let mut world = world_of(2, Vec::new(), &[]);
    assert!(!world.treaty_holds());
    assert_eq!(world.treaty_seconds_left(), TreatySeconds::ZERO);
    assert!(world.treaty_monitor().is_err());
    assert!(world.treaty_monitor_mut().is_err());
    world.treaty = Some(TreatyMonitor::new(
        TreatyMinutes(5),
        NomadTreaty::NotForced,
        GameMode::RandomMap,
    ));
    monitor_mut(&mut world).seconds_left = TreatySeconds(42);
    for (state, holds) in [
        (TreatyState::New, false),
        (TreatyState::Holds, true),
        (TreatyState::Ended, false),
        (TreatyState::Done, false),
    ] {
        monitor_mut(&mut world).state = state;
        assert_eq!(world.treaty_holds(), holds);
        let left = if holds { 42 } else { 0 };
        assert_eq!(world.treaty_seconds_left(), TreatySeconds(left));
    }
}

/// `FUN_141461620`: a new monitor turns relation 3 into 4 between the players past Gaia and has
/// no time left; any other only holds again.
#[test]
fn untimed_start_turns_enemies_into_relation_4_once() {
    let mut world = world_of(3, Vec::new(), &[]);
    world.treaty = Some(TreatyMonitor::new(
        TreatyMinutes::UNTIMED,
        NomadTreaty::NotForced,
        GameMode::OneUnit,
    ));
    world.start_treaty().unwrap();
    assert_eq!(relation(&world, 1, 2), Relation::Relation4);
    assert_eq!(relation(&world, 3, 1), Relation::Relation4);
    assert_eq!(relation(&world, 1, 1), Relation::Ally);
    // Gaia's row and column stay
    assert_eq!(relation(&world, 0, 1), Relation::Enemy);
    assert_eq!(relation(&world, 1, 0), Relation::Enemy);
    assert_eq!(
        (monitor(&world).seconds_left, monitor(&world).state),
        (TreatySeconds::UNTIMED, TreatyState::Holds)
    );
    // an ended monitor holds again without a turn
    world.end_treaty().unwrap();
    monitor_mut(&mut world).seconds_left = TreatySeconds(9);
    world.start_treaty().unwrap();
    assert_eq!(relation(&world, 1, 2), Relation::Enemy);
    assert_eq!(
        (monitor(&world).seconds_left, monitor(&world).state),
        (TreatySeconds(9), TreatyState::Holds)
    );
}

/// `FUN_141461710`: a new monitor turns 3 into 4, takes its seconds left and its next message
/// (300 before the end above 300 left, 60 before above 60, else the end); an ended one turns
/// them again; a holding or a done one only holds.
#[test]
fn timed_start_takes_its_countdown_from_the_time() {
    let mut world = world_of(2, Vec::new(), &[]);
    for (millis, left, next) in [
        (199_999, 401, 300),
        (300_000, 300, 540),
        (539_000, 61, 540),
        (540_000, 60, 600),
        (700_000, -100, 600),
    ] {
        world.treaty = Some(TreatyMonitor::new(
            TreatyMinutes(10),
            NomadTreaty::NotForced,
            GameMode::RandomMap,
        ));
        world
            .start_treaty_at(WorldTime::from_millis(millis))
            .unwrap();
        let monitor = monitor(&world);
        assert_eq!(
            (monitor.seconds_left, monitor.next_message, monitor.state),
            (TreatySeconds(left), TreatySeconds(next), TreatyState::Holds)
        );
        assert_eq!(relation(&world, 1, 2), Relation::Relation4);
        world.end_treaty().unwrap();
    }
    // ended: the relations turn again, the clock stays
    world.start_treaty_at(WorldTime::from_millis(1)).unwrap();
    assert_eq!(relation(&world, 2, 1), Relation::Relation4);
    assert_eq!(monitor(&world).seconds_left, TreatySeconds(-100));
    for state in [TreatyState::Holds, TreatyState::Done] {
        world.end_treaty().unwrap();
        monitor_mut(&mut world).state = state;
        world.start_treaty_at(WorldTime::from_millis(1)).unwrap();
        assert_eq!(relation(&world, 1, 2), Relation::Enemy);
        assert_eq!(monitor(&world).state, TreatyState::Holds);
    }
}

/// `FUN_141461310`: relation 4 turns back into 3 between the players past Gaia, the zones count
/// undrawn and the monitor has ended.
#[test]
fn end_turns_relation_4_back_and_undraws_the_zones() {
    let mut world = world_of(2, Vec::new(), &[]);
    world.treaty = Some(TreatyMonitor::new(
        TreatyMinutes(10),
        NomadTreaty::NotForced,
        GameMode::RandomMap,
    ));
    world.start_treaty().unwrap();
    world
        .set_relation(player(0), player(1), Relation::Relation4)
        .unwrap();
    monitor_mut(&mut world).borders = Borders::Drawn;
    world.end_treaty().unwrap();
    assert_eq!(relation(&world, 1, 2), Relation::Enemy);
    assert_eq!(relation(&world, 0, 1), Relation::Relation4);
    assert_eq!(
        (monitor(&world).borders, monitor(&world).state),
        (Borders::Undrawn, TreatyState::Ended)
    );
}

/// A world of two players whose treaty of 10 minutes holds, its zones drawn.
fn holding_world(game_mode: GameMode) -> World {
    let mut world = world_of(2, Vec::new(), &[]);
    world.settings.game_mode = game_mode;
    world.treaty = Some(TreatyMonitor::new(
        TreatyMinutes(10),
        NomadTreaty::NotForced,
        game_mode,
    ));
    world.start_treaty_at(WorldTime::ZERO).unwrap();
    monitor_mut(&mut world).borders = Borders::Drawn;
    world
}

/// `FUN_1414618a0` outside modes 7 and 16: the seconds left follow the time; at the end's
/// second the treaty still holds, past it it ends with none left; past the next message the
/// countdown takes the next.
#[test]
fn timed_update_counts_down_and_ends_past_its_second() {
    let mut world = holding_world(GameMode::RandomMap);
    assert_eq!(monitor(&world).next_message, TreatySeconds(300));
    world
        .update_treaty(WorldTime::from_millis(300_999))
        .unwrap();
    assert_eq!(
        (monitor(&world).seconds_left, monitor(&world).next_message),
        (TreatySeconds(300), TreatySeconds(300))
    );
    world
        .update_treaty(WorldTime::from_millis(301_000))
        .unwrap();
    assert_eq!(
        (monitor(&world).seconds_left, monitor(&world).next_message),
        (TreatySeconds(299), TreatySeconds(540))
    );
    world
        .update_treaty(WorldTime::from_millis(541_000))
        .unwrap();
    assert_eq!(monitor(&world).next_message, TreatySeconds(600));
    world
        .update_treaty(WorldTime::from_millis(600_999))
        .unwrap();
    assert_eq!(monitor(&world).state, TreatyState::Holds);
    assert_eq!(relation(&world, 1, 2), Relation::Relation4);
    world
        .update_treaty(WorldTime::from_millis(601_000))
        .unwrap();
    assert_eq!(
        (monitor(&world).seconds_left, monitor(&world).state),
        (TreatySeconds::ZERO, TreatyState::Done)
    );
    assert_eq!(relation(&world, 1, 2), Relation::Enemy);
}

/// In game modes 7 and 16 no time ends the treaty and its seconds left stay.
#[test]
fn untimed_modes_keep_their_seconds_left() {
    for game_mode in [GameMode::DefendTheWonder, GameMode::OneUnit] {
        let mut world = holding_world(game_mode);
        monitor_mut(&mut world).seconds_left = TreatySeconds(7);
        world
            .update_treaty(WorldTime::from_millis(5_000_000))
            .unwrap();
        assert_eq!(
            (monitor(&world).seconds_left, monitor(&world).state),
            (TreatySeconds(7), TreatyState::Holds)
        );
    }
}

/// Game mode 16's Castle Age ends the treaty in the next update; so do the drawn zones of a
/// lengthened nomad treaty, while undrawn zones keep it.
#[test]
fn castle_age_and_drawn_nomad_zones_end_the_treaty() {
    let mut world = holding_world(GameMode::OneUnit);
    monitor_mut(&mut world).castle_age = true;
    world.update_treaty(WorldTime::ZERO).unwrap();
    assert_eq!(monitor(&world).state, TreatyState::Done);
    assert_eq!(relation(&world, 1, 2), Relation::Enemy);
    let mut world = holding_world(GameMode::RandomMap);
    monitor_mut(&mut world).lengthened = true;
    world.update_treaty(WorldTime::ZERO).unwrap();
    assert_eq!(monitor(&world).state, TreatyState::Done);
    // undrawn: the update's walk finds no town centre, the treaty holds
    let mut world = holding_world(GameMode::RandomMap);
    let monitor_now = monitor_mut(&mut world);
    monitor_now.lengthened = true;
    monitor_now.borders = Borders::Undrawn;
    world.update_treaty(WorldTime::ZERO).unwrap();
    assert_eq!(
        (monitor(&world).state, monitor(&world).borders),
        (TreatyState::Holds, Borders::Undrawn)
    );
}

/// The world update's step: a monitor updates only with time in the update (0 and a NaN
/// skip), and one done goes.
#[test]
fn step_updates_with_time_and_lets_done_monitor_go() {
    let mut world = holding_world(GameMode::RandomMap);
    world.world_time = WorldTime::from_millis(700_000);
    for dt in [0.0, f32::NAN] {
        world.dt_seconds = Seconds::new(dt);
        world.treaty_step().unwrap();
        assert_eq!(monitor(&world).state, TreatyState::Holds);
    }
    world.dt_seconds = Seconds::new(0.5);
    world.treaty_step().unwrap();
    assert!(world.treaty.is_none());
    // without a monitor the step does nothing
    world.treaty_step().unwrap();
    assert!(world.treaty.is_none());
}

/// The load in game mode 16: a byte above 0 (signed) makes a monitor without time and starts
/// it; the options' length stays.
#[test]
fn load_in_one_unit_mode_starts_an_untimed_treaty() {
    for (byte, made) in [(0, false), (0x80, false), (3, true)] {
        let mut world = world_of(2, Vec::new(), &[]);
        world.settings.game_mode = GameMode::OneUnit;
        world.load_treaty(byte, NomadTreaty::Forced).unwrap();
        assert_eq!(world.treaty.is_some(), made);
        assert_eq!(world.settings.treaty_length, TreatyLength::NONE);
        if made {
            assert_eq!(
                (monitor(&world).end, monitor(&world).state),
                (TreatySeconds(-60), TreatyState::Holds)
            );
            assert_eq!(monitor(&world).seconds_left, TreatySeconds::UNTIMED);
            assert_eq!(relation(&world, 1, 2), Relation::Relation4);
        }
    }
}

/// The load elsewhere: the options take the byte; a treaty whose end lies past the world's
/// time makes a monitor of the byte's minutes and starts it at the time, a forced nomad one
/// counting five minutes at least; none at all for 0 or an end reached.
#[test]
fn load_makes_timed_monitor_while_its_end_lies_ahead() {
    for (byte, nomad, millis, end) in [
        (0, NomadTreaty::NotForced, 0, None),
        (0, NomadTreaty::Forced, 0, Some(300)),
        (10, NomadTreaty::NotForced, 0, Some(600)),
        (3, NomadTreaty::NotForced, 0, Some(180)),
        (3, NomadTreaty::Forced, 0, Some(300)),
        (1, NomadTreaty::NotForced, 60_000, None),
        (1, NomadTreaty::NotForced, 59_999, Some(60)),
        (3, NomadTreaty::Forced, 299_999, Some(300)),
    ] {
        let mut world = world_of(2, Vec::new(), &[]);
        world.world_time = WorldTime::from_millis(millis);
        world.load_treaty(byte, nomad).unwrap();
        assert_eq!(
            world.settings.treaty_length,
            TreatyLength::of_minutes(u32::from(byte))
        );
        assert_eq!(
            world.treaty.as_ref().map(|monitor| monitor.end),
            end.map(TreatySeconds)
        );
        if end.is_some() {
            assert_eq!(monitor(&world).state, TreatyState::Holds);
            assert_eq!(monitor(&world).borders, Borders::Undrawn);
            assert_eq!(relation(&world, 1, 2), Relation::Relation4);
        }
    }
}

/// A monitor the world holds at the load ends and goes when its minutes differ from the
/// byte's; one of the same minutes stays as it is, its zones undrawn again.
#[test]
fn load_replaces_monitor_of_other_minutes() {
    let mut world = holding_world(GameMode::RandomMap);
    monitor_mut(&mut world).seconds_left = TreatySeconds(5);
    world.load_treaty(10, NomadTreaty::NotForced).unwrap();
    assert_eq!(
        (monitor(&world).seconds_left, monitor(&world).borders),
        (TreatySeconds(5), Borders::Undrawn)
    );
    world.load_treaty(20, NomadTreaty::NotForced).unwrap();
    assert_eq!(monitor(&world).end, TreatySeconds(1200));
    assert_eq!(monitor(&world).seconds_left, TreatySeconds(1200));
    // of other minutes with an end reached: ended, relation 3 back, none made
    world.world_time = WorldTime::from_millis(2_000_000);
    world.load_treaty(30, NomadTreaty::NotForced).unwrap();
    assert!(world.treaty.is_none());
    assert_eq!(relation(&world, 1, 2), Relation::Enemy);
}

/// The no-build test: a footprint's tiles (its place less its radii to its place plus its radii
/// less 1) meet a zone of a player not allied with its owner; its own and an ally's do not
/// count, nor a zone whose low corner is not below its high one; a second zone counts once the
/// second zones are on. In game mode 7 only the owner's own zones are left out.
#[test]
fn no_build_test_reads_the_zones_of_players_not_allied() {
    let world = Scene::open(GridSize::new(8, 8))
        .players(3)
        .relation(player(3), player(1), Relation::Ally)
        .build();
    let mut monitor = TreatyMonitor::new(
        TreatyMinutes(5),
        NomadTreaty::NotForced,
        GameMode::RandomMap,
    );
    monitor.zones[1].first = TileRect::new(10, 10, 20, 20);
    monitor.zones[2].first = TileRect::new(30, 30, 40, 40);
    let radius = Footprint::new(2.0, 2.0);
    let test = |monitor: &TreatyMonitor, game_mode, x: f32, y: f32| {
        monitor.in_no_build_zone(
            &world.players,
            game_mode,
            player(1),
            Point::new(x, y),
            radius,
        )
    };
    // tiles 21..=24: the zone's x1 20 is not met; 20.5 reaches tile 20
    assert!(!test(&monitor, GameMode::RandomMap, 23.0, 15.0));
    assert!(test(&monitor, GameMode::RandomMap, 22.5, 15.0));
    // tiles 7..=10 meet x0 10
    assert!(test(&monitor, GameMode::RandomMap, 9.0, 15.0));
    assert!(!test(&monitor, GameMode::RandomMap, 8.0, 15.0));
    // player 3 counts player 1 its ally
    assert!(!test(&monitor, GameMode::RandomMap, 35.0, 35.0));
    assert!(test(&monitor, GameMode::DefendTheWonder, 35.0, 35.0));
    // its own zone, and a zone without width, count nothing
    monitor.zones[0].first = TileRect::new(50, 50, 60, 60);
    monitor.zones[1].first = TileRect::new(10, 10, 10, 20);
    assert!(!test(&monitor, GameMode::RandomMap, 55.0, 55.0));
    assert!(!test(&monitor, GameMode::DefendTheWonder, 55.0, 55.0));
    assert!(!test(&monitor, GameMode::RandomMap, 10.0, 15.0));
    // the second zone counts only once they are on, and only beside a first of width
    monitor.zones[1].first = TileRect::new(10, 10, 20, 20);
    monitor.zones[1].second = TileRect::new(70, 70, 80, 80);
    assert!(!test(&monitor, GameMode::RandomMap, 75.0, 75.0));
    monitor.second_zones = true;
    assert!(test(&monitor, GameMode::RandomMap, 75.0, 75.0));
    monitor.zones[1].first = TileRect::new(10, 10, 20, 10);
    assert!(!test(&monitor, GameMode::RandomMap, 75.0, 75.0));
}

/// A zone's span: the margin before the centre, at least 0, to the margin less 1 past it while
/// below the map's last tile, else the last tile; a NaN's place wraps as the ints do.
#[test]
fn zone_span_keeps_to_the_map() {
    let span = zone_span;
    assert_eq!(span(50, 20, 120), (30, 69));
    assert_eq!(span(10, 20, 120), (0, 29));
    assert_eq!(span(110, 20, 120), (90, 119));
    assert_eq!(span(100, 20, 120), (80, 119));
    assert_eq!(span(99, 20, 120), (79, 118));
    assert_eq!(span(i32::MIN, 20, 120), (i32::MAX - 19, i32::MIN + 19));
}

/// A world of two enemies whose town centres stand at `first` and `second` on y 30.5 with
/// `objects` beside them, its treaty of 10 minutes holding with its zones undrawn.
fn zoned_world(
    first: f32,
    second: f32,
    objects: Vec<SceneObject>,
    progress: &[(i32, f32)],
) -> World {
    let centres = vec![
        standing(1, player(1), building(ids::TOWN_CENTRE), first, 30.5),
        standing(2, player(2), building(ids::TOWN_CENTRE), second, 30.5),
    ];
    let mut world = world_of(2, centres.into_iter().chain(objects).collect(), progress);
    world.treaty = Some(TreatyMonitor::new(
        TreatyMinutes(10),
        NomadTreaty::NotForced,
        GameMode::RandomMap,
    ));
    world.start_treaty_at(WorldTime::ZERO).unwrap();
    world
}

/// `FUN_141461c00`: without the game's screen nothing is drawn; with it each player's first
/// town centre takes its zone (the margin about its place), set, and the zones stand.
#[test]
fn zones_stand_about_every_player_first_town_centre() {
    let mut world = zoned_world(30.5, 100.5, Vec::new(), &[]);
    assert_eq!(
        world.draw_treaty_zones(GameScreen::Absent).unwrap(),
        Borders::Undrawn
    );
    assert!(!monitor(&world).zones[0].set);
    assert_eq!(
        world.draw_treaty_zones(GameScreen::Shown).unwrap(),
        Borders::Drawn
    );
    let zones = &monitor(&world).zones;
    assert_eq!(
        (zones[0].first, zones[0].set),
        (TileRect::new(10, 10, 49, 49), true)
    );
    assert_eq!(zones[1].first, TileRect::new(80, 10, 119, 49));
    assert!(!monitor(&world).second_zones);
    // set zones are not drawn again
    monitor_mut(&mut world).zones[0].first = TileRect::new(1, 2, 3, 4);
    let _ = world.draw_treaty_zones(GameScreen::Shown).unwrap();
    assert_eq!(monitor(&world).zones[0].first, TileRect::new(1, 2, 3, 4));
}

/// A player without a built town centre leaves the zones undrawn: a foundation of one that
/// had work (a NaN too) draws its zone, one without none; a dying town centre draws none
/// though the zones count it.
#[test]
fn foundations_with_work_draw_and_dying_centres_do_not() {
    for (points, drawn) in [(0.0, false), (2.0, true), (f32::NAN, true), (-1.0, false)] {
        let mut centre = standing(3, player(2), building(ids::TOWN_CENTRE), 100.5, 30.5);
        centre.state = ObjectState::Foundation;
        centre.desolid = true;
        let mut world = world_of(
            2,
            vec![
                standing(1, player(1), building(ids::TOWN_CENTRE), 30.5, 30.5),
                centre,
            ],
            &[(3, points)],
        );
        world.treaty = Some(TreatyMonitor::new(
            TreatyMinutes(10),
            NomadTreaty::NotForced,
            GameMode::RandomMap,
        ));
        assert_eq!(world.town_centre_at(player(2), 0), None);
        assert_eq!(
            world.draw_treaty_zones(GameScreen::Shown).unwrap(),
            Borders::Undrawn
        );
        assert_eq!(monitor(&world).zones[1].set, drawn);
        assert!(monitor(&world).zones[0].set);
    }
    let mut world = zoned_world(30.5, 100.5, Vec::new(), &[]);
    world.players[2].objects.iter_mut().for_each(|unit| {
        unit.life.state = ObjectState::Dying;
    });
    assert_eq!(
        world.draw_treaty_zones(GameScreen::Shown).unwrap(),
        Borders::Drawn
    );
    assert!(!monitor(&world).zones[1].set);
}

/// A player with a second town centre turns the second zones on; its second centre's tiles
/// become its second zone, and every later player's walk takes its second one too.
#[test]
fn second_town_centre_turns_the_second_zones_on() {
    let mut world = zoned_world(
        30.5,
        100.5,
        vec![standing(
            4,
            player(1),
            building(ids::TOWN_CENTRE_444),
            30.5,
            90.5,
        )],
        &[],
    );
    assert_eq!(world.town_centre_entries(player(1)), 2);
    assert_eq!(world.town_centre_entries(player(2)), 1);
    let _ = world.draw_treaty_zones(GameScreen::Shown).unwrap();
    let monitor = monitor(&world);
    assert!(monitor.second_zones);
    assert_eq!(monitor.zones[0].second, TileRect::new(10, 70, 49, 109));
    // player 2 has no second town centre: its second zone stays 0
    assert_eq!(monitor.zones[1].second, TileRect::default());
}

/// `FUN_1411c7ae0`: a town centre whose footprint lies in an enemy's zone removes the
/// foundations without work of the other players on the tiles of its own zone and five past
/// it (its high sides left out); its own player's, those with work (a NaN's too), the units
/// and the foundations off those tiles stay.
#[test]
fn town_centre_in_an_enemy_zone_sweeps_the_foundations_about_it() {
    let objects = vec![
        foundation(10, player(1), 40.5, 40.5),
        foundation(11, player(1), 41.5, 40.5),
        foundation(12, player(1), 42.5, 40.5),
        foundation(13, player(2), 43.5, 40.5),
        standing(14, player(1), soldier(), 44.5, 40.5),
        foundation(15, PlayerId::GAIA, 38.5, 40.5),
        foundation(16, player(1), 100.5, 100.5),
        foundation(17, player(1), 69.5, 40.5),
        foundation(18, player(1), 68.5, 40.5),
    ];
    let mut world = zoned_world(30.5, 45.5, objects, &[(11, 2.0), (12, f32::NAN)]);
    assert_eq!(
        world.draw_treaty_zones(GameScreen::Shown).unwrap(),
        Borders::Drawn
    );
    let removed: Vec<i32> = (10..=18)
        .filter(|&id| {
            world
                .object(object_id(id))
                .is_none_or(|(_, unit)| unit.life.hp.get() <= 0.0)
        })
        .collect();
    assert_eq!(removed, [10, 15, 18]);
}

/// Player 1's own sweep finds player 2's zone unset and removes nothing; the zones far apart
/// sweep nothing either.
#[test]
fn town_centres_outside_enemy_zones_sweep_nothing() {
    let objects = vec![
        foundation(10, player(2), 32.5, 40.5),
        foundation(11, player(1), 100.5, 40.5),
    ];
    let mut world = zoned_world(30.5, 100.5, objects, &[]);
    let _ = world.draw_treaty_zones(GameScreen::Shown).unwrap();
    for id in [10, 11] {
        let (_, unit) = world.object(object_id(id)).unwrap();
        assert!(0.0 < unit.life.hp.get());
    }
}

/// The update draws undrawn zones of a treaty that holds and keeps their answer.
#[test]
fn update_draws_the_zones_of_holding_treaty() {
    let mut world = zoned_world(30.5, 100.5, Vec::new(), &[]);
    world.update_treaty(WorldTime::ZERO).unwrap();
    assert_eq!(monitor(&world).borders, Borders::Drawn);
    assert!(monitor(&world).zones[1].set);
    // a lengthened nomad treaty ends once they stand
    let mut world = zoned_world(30.5, 100.5, Vec::new(), &[]);
    monitor_mut(&mut world).lengthened = true;
    world.update_treaty(WorldTime::ZERO).unwrap();
    assert_eq!(monitor(&world).state, TreatyState::Done);
}
