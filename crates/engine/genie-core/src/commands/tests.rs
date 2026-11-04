//! The command enum's issuing player and the lane a failed run stops.

use crate::GenieError;
use crate::commands::Bell;
use crate::commands::Departure;
use crate::commands::GameSub;
use crate::commands::ResourceWord;
use crate::commands::Shift;
use crate::commands::UnloadPick;
use crate::commands::{
    AllBackToWorkCommand, AttackMoveCommand, AutoScoutCommand, BuildCommand, BuildWallCommand,
    BuildingQueueCommand, Command, DeleteCommand, FlareCommand, GameCommand, GateCommand,
    GatherPointCommand, GoBackToWork2Command, GoBackToWorkCommand, Order, OrderCommand,
    PatrolCommand, QueueCommand, ResearchCommand, ResignCommand, SeekShelterCommand, StopCommand,
    TownBellCommand, TradeCommand, TransformObjectCommand, TributeCommand, UnitAiStateCommand,
    UnitOrderAction, UnitOrderCommand, UnloadCommand, UnmodelledCommand, Waypoints,
};
use crate::commands::{QueueId, SpriteOverride, TradeLots};
use crate::garrison::GarrisonScope;
use crate::map::Point;
use crate::map::Tile;
use crate::objects::Frame;
use crate::orders::Interact;
use crate::orders::SavedPatrols;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::TributeFee;
use crate::players::tribute::Gift;
use crate::production::QueueButton;
use crate::production::QueuedUnits;
use crate::production::UnitIndex;
use crate::technology::TechId;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_ids;
use crate::test_ids::player_id;
use crate::test_ids::unit_list;
use crate::unit_ai::UnitStance;
use crate::world::Millis;
use crate::world::WorldTime;
use crate::world::{FrameSeed, Player, World};

/// A GAME command of `issuer`'s for player 0, its operands 0.
fn game_of(issuer: u8) -> GameCommand {
    GameCommand {
        issuer: player_id(issuer),
        sub: GameSub::Diplomacy,
        player: 0,
        a: 0,
        speed: 0.0,
        value: 0,
    }
}

/// One command of each kind, issued by player `p`.
fn one_of_each(p: u8) -> Vec<Command> {
    let ids = vec![7];
    let gift = Gift {
        attribute: ResourceWord::of_data(0),
        amount: Amount::ZERO,
        fee: TributeFee::of_data(0.0),
    };
    vec![
        Command::Build(BuildCommand {
            player: player_id(p),
            master: master_id(70),
            position: Point::new(0.0, 0.0),
            queue_id: QueueId::of_data(-1),
            sprite_override: SpriteOverride::of_data(-1),
            builders: unit_list(object_ids(ids.clone())),
            queued: false,
            commits: true,
            frame: Frame::of_data(0),
        }),
        Command::BuildWall(BuildWallCommand {
            player: player_id(p),
            master: master_id(72),
            from: Tile::new(0, 0),
            to: Tile::new(1, 1),
            queue_id: QueueId::of_data(-1),
            builders: unit_list(object_ids(ids.clone())),
            queued: false,
            commits: true,
        }),
        Command::GatherPoint(GatherPointCommand {
            player: player_id(p),
            units: unit_list(object_ids(ids.clone())),
            point: Point::new(0.0, 0.0),
            target: None,
            target_master: None,
            own: false,
        }),
        Command::Queue(QueueCommand {
            player: player_id(p),
            buildings: unit_list(object_ids(ids.clone())),
            building_master: master_id(109),
            unit: master_id(83),
            amount: QueuedUnits::ONE,
            button: QueueButton::NONE,
        }),
        Command::Research(ResearchCommand {
            player: player_id(p),
            building: Some(object_id(7)),
            count: 1,
            buildings: vec![],
            tech: TechId::of_data(22),
            queue_id: QueueId::of_data(-1),
            shift: false,
        }),
        Command::Unmodelled(UnmodelledCommand {
            player: player_id(p),
            packet: Vec::new(),
            why: GenieError::unsupported("a test's"),
        }),
        // a sub-command for no player of the world
        Command::Game(GameCommand {
            issuer: player_id(p),
            player: -1,
            ..game_of(0)
        }),
        Command::CommandQueue(BuildingQueueCommand {
            player: player_id(p),
            units: unit_list(object_ids(ids.clone())),
            target: None,
            point: Point::new(0.0, 0.0),
        }),
        Command::GoBackToWork(GoBackToWorkCommand {
            player: player_id(p),
            id: object_id(7),
        }),
        Command::GoBackToWork2(GoBackToWork2Command {
            player: player_id(p),
            units: unit_list(object_ids(ids.clone())),
        }),
        Command::AllBackToWork(AllBackToWorkCommand {
            player: player_id(p),
            id: object_id(7),
        }),
        Command::TownBell(TownBellCommand {
            player: player_id(p),
            id: object_id(7),
            bell: Bell::Ring,
        }),
        Command::Delete(DeleteCommand {
            player: player_id(p),
            id: object_id(7),
        }),
        Command::Flare(FlareCommand {
            player: player_id(p),
            target: None,
            point: Point::new(0.0, 0.0),
            players: 2,
            shown: Vec::new(),
        }),
        Command::Tribute(TributeCommand {
            player: player_id(p),
            to: player_id(1),
            gifts: [gift; 4],
        }),
        Command::Trade(TradeCommand {
            player: player_id(p),
            attribute: ResourceWord::of_data(0),
            times: TradeLots::of_data(1),
            market: None,
            buy: true,
        }),
        Command::Gate(GateCommand {
            player: player_id(p),
            id: object_id(7),
        }),
        Command::Unload(UnloadCommand {
            player: player_id(p),
            units: unit_list(object_ids(ids.clone())),
            point: Point::new(-1.0, -1.0),
            pick: UnloadPick::of_packet(0, -1),
            sound: false,
        }),
        Command::TransformObject(TransformObjectCommand {
            player: player_id(p),
            id: object_id(7),
            master: master_id(83),
        }),
        Command::Resign(ResignCommand {
            player: player_id(p),
            departure: Departure::Resigned,
        }),
        Command::AutoScout(AutoScoutCommand {
            player: player_id(p),
            units: unit_list(object_ids(ids.clone())),
        }),
        Command::Stop(StopCommand {
            player: player_id(p),
            units: unit_list(object_ids(ids.clone())),
        }),
        Command::Patrol(PatrolCommand {
            player: player_id(p),
            units: unit_list(object_ids(ids.clone())),
            waypoints: Waypoints::of(&[Point::new(1.0, 1.0)]).unwrap(),
            patrols: SavedPatrols::Kept,
        }),
        Command::AttackMove(AttackMoveCommand {
            player: player_id(p),
            units: unit_list(object_ids(ids.clone())),
            waypoints: Waypoints::of(&[Point::new(1.0, 1.0)]).unwrap(),
            patrols: SavedPatrols::Kept,
            queued: false,
            ignore_pending: false,
        }),
        Command::SeekShelter(SeekShelterCommand {
            player: player_id(p),
            units: unit_list(object_ids(ids.clone())),
        }),
        Command::UnitAiState(UnitAiStateCommand {
            player: player_id(p),
            units: unit_list(object_ids(ids.clone())),
            state: UnitStance::Defensive,
        }),
        Command::UnitOrder(UnitOrderCommand {
            player: player_id(p),
            units: unit_list(object_ids(ids.clone())),
            target: None,
            point: Point::new(-1.0, -1.0),
            queue_id: QueueId::of_data(-1),
            shift: Shift::Released,
            scope: GarrisonScope::Target,
            action: UnitOrderAction::DropOff,
            param: UnitIndex::FIRST,
            human: false,
        }),
        Command::Order(OrderCommand {
            player: player_id(p),
            units: unit_list(object_ids(ids)),
            order: Order::Retreat(Interact::default()),
        }),
    ]
}

/// Every command names its issuing player (GAME its issuer), and a failed run stops that
/// player's lane, but a GAME sub-command's, which stops its target's (or its issuer's for a
/// target outside the world).
#[test]
fn command_names_the_player_whose_lane_it_stops() {
    let commands = one_of_each(3);
    assert_eq!(commands.len(), 28);
    for command in &commands {
        assert_eq!(
            (command.player(), command.stopped_player(8)),
            (player_id(3), player_id(3)),
            "{command:?}"
        );
    }
    let game = Command::Game(GameCommand {
        player: 5,
        ..game_of(3)
    });
    assert_eq!(
        (game.player(), game.stopped_player(8)),
        (player_id(3), player_id(5))
    );
    assert_eq!(game.stopped_player(4), player_id(3));
}

/// The waypoints the handlers read: the first of their count, none without 1 to 10.
#[test]
fn patrol_reads_one_to_ten_waypoints() {
    let points = [Point::new(1.0, 2.0), Point::new(3.0, 4.0)];
    let two = Waypoints::of(&points).unwrap();
    assert_eq!(two.used(), Some(&points[..]));
    assert_eq!(Waypoints::default().used(), None);
    let eleven = Waypoints { count: 11, ..two };
    assert_eq!(eleven.used(), None);
}

/// The market trades the attribute's low byte, food, wood and stone only.
#[test]
fn trade_of_another_commodity_does_nothing() {
    let player = Player::default();
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(1),
        vec![Millis::ZERO; 2],
        vec![player; 2],
    );
    let before = world.clone();
    for attribute in [3, 0x0103] {
        TradeCommand {
            player: player_id(1),
            attribute: ResourceWord::of_data(attribute),
            times: TradeLots::of_data(1),
            market: None,
            buy: true,
        }
        .run(&mut world)
        .unwrap();
    }
    assert_eq!(world, before);
}
