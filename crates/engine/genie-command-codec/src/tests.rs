//! The codec's round trips: every packet decodes into its command and that command writes back
//! the packet's bytes; a packet whose command would not stays raw.

use crate::{decode, encode};
use genie_core::commands::{
    AllBackToWorkCommand, AttackMoveCommand, AutoScoutCommand, BuildCommand, BuildWallCommand,
    BuildingQueueCommand, Command, DeleteCommand, FlareCommand, FormFormationCommand, GameCommand,
    GateCommand, GatherPointCommand, GoBackToWork2Command, GoBackToWorkCommand, Order,
    OrderCommand, PatrolCommand, QueueCommand, ResearchCommand, ResignCommand, SeekShelterCommand,
    SetGatherStateCommand, StopCommand, TownBellCommand, TradeCommand, TransformObjectCommand,
    TributeCommand, UnitAiStateCommand, UnitList, UnitOrderAction, UnitOrderCommand, UnloadCommand,
    UnloadPick, UnmodelledCommand, Waypoints,
};
use genie_core::groups::FormationKind;
use genie_core::map::Point;
use genie_core::objects::ObjectId;
use genie_core::objects::masters::MasterId;
use genie_core::orders::{Interact, MoveOrder};
use genie_core::players::Amount;
use genie_core::players::PlayerId;
use genie_core::players::tribute::Gift;
use genie_core::production::QueueButton;
use genie_core::unit_ai::UnitStance;
use genie_core::{ErrorKind, GenieError};
use genie_rec::de::command as packets;

/// The object id `value` (not −1).
fn object_id(value: i32) -> ObjectId {
    ObjectId::try_from(value).expect("a test's object id is not −1")
}

/// The entries of a list of ids of `values`, −1 for none.
fn object_ids(values: impl IntoIterator<Item = i32>) -> Vec<Option<ObjectId>> {
    values.into_iter().map(ObjectId::of_field).collect()
}

/// The unit list of `ids`, its count theirs (fewer than `0xffff`).
fn unit_list(ids: Vec<Option<ObjectId>>) -> UnitList {
    UnitList::try_from(ids).expect("a test's unit list is shorter than 0xffff ids")
}

/// The player `number`.
fn player_id(number: u8) -> PlayerId {
    PlayerId::from(number)
}

/// The master `value` (0 or more).
fn master_id(value: i16) -> MasterId {
    MasterId::try_from(value).expect("a test's master is 0 or more")
}

/// The command of a packet, which writes the packet back.
fn round_trip(packet: &[u8]) -> Command {
    let command = decode(packet).expect("a packet with its player byte");
    assert!(
        !matches!(command, Command::Unmodelled(_)),
        "{command:?} stays raw"
    );
    assert_eq!(encode(&command).expect("a modelled command"), packet);
    command
}

/// Why a packet stays raw, which it writes back as it is.
fn raw(packet: &[u8]) -> GenieError {
    let command = decode(packet).expect("a packet with its player byte");
    assert_eq!(encode(&command).expect("its own bytes"), packet);
    match command {
        Command::Unmodelled(UnmodelledCommand { player, why, .. }) => {
            assert_eq!(player, PlayerId::from(packet[1]));
            why
        }
        other => panic!("{other:?} is modelled"),
    }
}

/// INTERACT's packet of player 2 with `num` units listed and the human byte `human`.
fn interact(num: u16, human: u8, ids: Vec<i32>) -> Vec<u8> {
    packets::Interact {
        commp_id: 2,
        tid: 500,
        x: 1.0,
        y: 2.0,
        num,
        field_0x12: 1,
        field_0x13: 0,
        field_0x14: 1,
        field_0x15: human,
        field_0x16: 3,
        ids,
    }
    .encode()
    .unwrap()
}

/// INTERACT keeps its target, point, flags and filter, and its count and every id the packet
/// holds, the last commanded units' `0xffff` too; WORK and RETREAT share its layout.
#[test]
fn an_interact_keeps_its_order_and_units() {
    let order = Interact {
        target: Some(object_id(500)),
        point: Point::new(1.0, 2.0),
        carry_test: true,
        queued: false,
        direct: true,
        human: true,
        filter: Some(genie_core::garrison::ENTER),
    };
    assert_eq!(
        round_trip(&interact(2, 1, vec![10, 11, 12])),
        Command::Order(OrderCommand {
            player: player_id(2),
            units: UnitList {
                count: 2,
                ids: object_ids([10, 11, 12]),
            },
            order: Order::Interact(order),
        })
    );
    assert_eq!(
        round_trip(&interact(UnitList::LAST_COMMANDED, 1, vec![])),
        Command::Order(OrderCommand {
            player: player_id(2),
            units: UnitList::last_commanded(),
            order: Order::Interact(order),
        })
    );
    let work = packets::Work {
        commp_id: 4,
        tid: 600,
        x: 0.0,
        y: 0.0,
        num: 1,
        field_0x12: 0,
        field_0x13: 1,
        field_0x14: 0,
        field_0x15: 0,
        field_0x16: u16::MAX,
        ids: vec![20],
    }
    .encode()
    .unwrap();
    let worked = Interact {
        target: Some(object_id(600)),
        queued: true,
        ..Interact::default()
    };
    assert_eq!(
        round_trip(&work),
        Command::Order(OrderCommand {
            player: player_id(4),
            units: unit_list(object_ids([20])),
            order: Order::Work(worked),
        })
    );
    let retreat = packets::Retreat {
        commp_id: 1,
        tid: 0,
        x: 0.0,
        y: 0.0,
        num: 1,
        field_0x12: 0,
        field_0x13: 0,
        field_0x14: 0,
        field_0x15: 0,
        field_0x16: 0xffff,
        ids: vec![7],
    }
    .encode()
    .unwrap();
    assert_eq!(
        round_trip(&retreat),
        Command::Order(OrderCommand {
            player: player_id(1),
            units: unit_list(object_ids([7])),
            order: Order::Retreat(Interact {
                target: Some(object_id(0)),
                filter: None,
                ..Interact::default()
            }),
        })
    );
}

/// MOVE keeps its point and flags; FOLLOW and GUARD their target at `+8`.
#[test]
fn move_follow_and_guard_keep_their_orders() {
    let many: Vec<i32> = (0..70).collect();
    let moved = packets::Move {
        commp_id: 1,
        tid: -1,
        x: 5.0,
        y: 6.0,
        num: 70,
        field_0x12: 0,
        field_0x13: 1,
        field_0x14: 0,
        field_0x15: 1,
        gap_0x16: [0; 2],
        ids: many.clone(),
    }
    .encode()
    .unwrap();
    assert_eq!(
        round_trip(&moved),
        Command::Order(OrderCommand {
            player: player_id(1),
            units: unit_list(object_ids(many)),
            order: Order::Move(MoveOrder {
                target: None,
                point: Point::new(5.0, 6.0),
                queued: true,
                direct: false,
                human: true,
                control: false,
            }),
        })
    );
    let follow = packets::Follow {
        commp_id: 1,
        num: 1,
        gap_0x06: [0; 2],
        target: 900,
        ids: vec![8],
    }
    .encode()
    .unwrap();
    let guard = packets::Guard {
        commp_id: 1,
        num: 1,
        gap_0x06: [0; 2],
        target: 901,
        ids: vec![8],
    }
    .encode()
    .unwrap();
    let ordered = |order| {
        Command::Order(OrderCommand {
            player: player_id(1),
            units: unit_list(object_ids([8])),
            order,
        })
    };
    assert_eq!(round_trip(&follow), ordered(Order::Follow(object_id(900))));
    assert_eq!(round_trip(&guard), ordered(Order::Guard(object_id(901))));
}

/// SET GATHER POINT and `BUILDING_COMMAND_QUEUE` keep their count and every id the packet
/// holds (the runs read them).
#[test]
fn the_buildings_lists_keep_their_counted_ids() {
    let gather = packets::SetGatherPoint {
        commp_id: 3,
        num: 1,
        gap_0x06: [0; 2],
        x: 1.0,
        y: 2.0,
        tid: 40,
        tm_id: 83,
        field_0x18: 1,
        ids: vec![30, 31],
    }
    .encode()
    .unwrap();
    assert_eq!(
        round_trip(&gather),
        Command::GatherPoint(GatherPointCommand {
            player: player_id(3),
            units: UnitList {
                count: 1,
                ids: object_ids([30, 31]),
            },
            point: Point::new(1.0, 2.0),
            target: Some(object_id(40)),
            target_master: Some(master_id(83)),
            own: true,
        })
    );
    let queue = packets::BuildingCommandQueue {
        commp_id: 3,
        tid: 41,
        x: 3.0,
        y: 4.0,
        num: 2,
        gap_0x12: [0, 1, 1, 0, 0, 0],
        ids: vec![32, 33],
    }
    .encode()
    .unwrap();
    assert_eq!(
        round_trip(&queue),
        Command::CommandQueue(BuildingQueueCommand {
            player: player_id(3),
            units: unit_list(object_ids([32, 33])),
            target: Some(object_id(41)),
            point: Point::new(3.0, 4.0),
        })
    );
}

/// The units' listed commands keep their count and every id the packet holds, a count past
/// the ids too.
#[test]
fn the_unit_lists_keep_their_counted_ids() {
    let state = packets::UnitAiState {
        commp_id: 3,
        num: 1,
        gap_0x06: [0; 2],
        state: 2,
        gap_0x09: [0; 3],
        ids: vec![34],
    }
    .encode()
    .unwrap();
    assert_eq!(
        round_trip(&state),
        Command::UnitAiState(UnitAiStateCommand {
            player: player_id(3),
            units: unit_list(object_ids([34])),
            state: UnitStance::STAND_GROUND,
        })
    );
    let scout = packets::AutoScout {
        commp_id: 3,
        field_0x04: 1,
        gap_0x06: [0; 2],
        ids: vec![35],
    }
    .encode()
    .unwrap();
    assert_eq!(
        round_trip(&scout),
        Command::AutoScout(AutoScoutCommand {
            player: player_id(3),
            units: unit_list(object_ids([35])),
        })
    );
    let shelter = packets::SeekShelter {
        commp_id: 3,
        num: 1,
        gap_0x06: [0; 2],
        ids: vec![36],
    }
    .encode()
    .unwrap();
    assert_eq!(
        round_trip(&shelter),
        Command::SeekShelter(SeekShelterCommand {
            player: player_id(3),
            units: unit_list(object_ids([36])),
        })
    );
    let form = packets::FormFormation {
        commp_id: 3,
        num: 2,
        gap_0x06: [0; 2],
        formation: 7,
        ids: vec![37, 38],
    }
    .encode()
    .unwrap();
    assert_eq!(
        round_trip(&form),
        Command::FormFormation(FormFormationCommand {
            player: player_id(3),
            units: unit_list(object_ids([37, 38])),
            formation: FormationKind::STAGGERED,
        })
    );
    let gather = packets::SetGatherState {
        commp_id: 4,
        num: 2,
        gap_0x06: [0; 2],
        field_0x08: 0xff,
        ids: vec![39, 40],
    }
    .encode()
    .unwrap();
    assert_eq!(
        round_trip(&gather),
        Command::SetGatherState(SetGatherStateCommand {
            player: player_id(4),
            units: unit_list(object_ids([39, 40])),
            state: Some(genie_core::commands::GatherPointState::KEPT),
        })
    );
    let stop = packets::Stop {
        commp_id: 2,
        num: 2,
        gap_0x06: [0; 2],
        ids: vec![50],
    }
    .encode()
    .unwrap();
    // a count past the ids: STOP's check refuses it in the run
    assert_eq!(
        round_trip(&stop),
        Command::Stop(StopCommand {
            player: player_id(2),
            units: UnitList {
                count: 2,
                ids: object_ids([50]),
            },
        })
    );
}

/// PATROL keeps all ten waypoint slots, the unused ones too; `ATTACK_MOVE` its first slot from
/// its fields and the other nine from its gaps.
#[test]
fn patrol_and_attack_move_keep_every_waypoint_slot() {
    let patrol = packets::Patrol {
        commp_id: 1,
        num: 1,
        gap_0x06: [0; 2],
        field_0x08: 1,
        gap_0x09: [0; 2],
        waypoints_count: 2,
        field_0x0c: 1.5_f32.to_bits(),
        field_0x10: 3.5_f32.to_bits(),
        field_0x14: 9.0_f32.to_bits(),
        field_0x18: 0,
        field_0x1c: 0,
        field_0x20: 0,
        field_0x24: 0,
        field_0x28: 0,
        field_0x2c: 0,
        field_0x30: 0,
        field_0x34: 2.5,
        field_0x38: 4.5,
        field_0x3c: 0.0,
        field_0x40: 0.0,
        field_0x44: 0.0,
        field_0x48: 0.0,
        field_0x4c: 0.0,
        field_0x50: 0.0,
        field_0x54: 0.0,
        field_0x58: -0.0,
        ids: vec![5],
    }
    .encode()
    .unwrap();
    let Command::Patrol(PatrolCommand {
        waypoints, reset, ..
    }) = round_trip(&patrol)
    else {
        panic!("a patrol")
    };
    assert!(reset);
    assert_eq!(
        waypoints.used(),
        Some(&[Point::new(1.5, 2.5), Point::new(3.5, 4.5)][..])
    );
    assert_eq!(waypoints.points[2], Point::new(9.0, 0.0));

    let mut gap_0x10 = [0; 36];
    gap_0x10[..4].copy_from_slice(&7.0_f32.to_le_bytes());
    let mut gap_0x38 = [0; 36];
    gap_0x38[32..].copy_from_slice(&8.0_f32.to_le_bytes());
    let attack = packets::AttackMove {
        commp_id: 1,
        num: 1,
        gap_0x06: [0; 2],
        field_0x08: 0,
        field_0x09: 1,
        field_0x0a: 1,
        waypoints_count: 2,
        field_0x0c: 1.0_f32.to_bits(),
        gap_0x10,
        field_0x34: 2.0,
        gap_0x38,
        ids: vec![5],
    }
    .encode()
    .unwrap();
    assert_eq!(
        round_trip(&attack),
        Command::AttackMove(AttackMoveCommand {
            player: player_id(1),
            units: unit_list(object_ids([5])),
            waypoints: Waypoints {
                count: 2,
                points: std::array::from_fn(|i| match i {
                    0 => Point::new(1.0, 2.0),
                    1 => Point::new(7.0, 0.0),
                    9 => Point::new(0.0, 8.0),
                    _ => Point::new(0.0, 0.0),
                }),
            },
            reset: false,
            queued: true,
            ignore_pending: true,
        })
    );
}

/// UNLOAD and `UNIT_ORDER` keep their count, `0xffff` too (their checks refuse it in the run),
/// and `UNIT_ORDER` its point and `+0x14`.
#[test]
fn unload_and_unit_order_keep_their_lists_and_counts() {
    let unload = |num| {
        packets::Unload {
            commp_id: 2,
            num,
            gap_0x06: [0; 2],
            x: 1.0,
            y: 1.0,
            unit_type: -1,
            flag: 1,
            gap_0x15: [0; 3],
            field_0x18: 1,
            ids: vec![60],
        }
        .encode()
        .unwrap()
    };
    assert_eq!(
        round_trip(&unload(1)),
        Command::Unload(UnloadCommand {
            player: player_id(2),
            units: unit_list(object_ids([60])),
            point: Point::new(1.0, 1.0),
            pick: UnloadPick::Player(-1),
            sound: true,
        })
    );
    let _ = round_trip(&unload(UnitList::LAST_COMMANDED));
    let order = |num| {
        packets::UnitOrder {
            commp_id: 2,
            num,
            gap_0x06: [0; 2],
            tid: 70,
            x: 3.0,
            y: 4.0,
            qid: 71,
            param: -1,
            action: 5,
            field_0x1d: 1,
            field_0x1e: 0,
            gap_0x1f: [1],
            field_0x20: 1,
            ids: vec![61],
        }
        .encode()
        .unwrap()
    };
    assert_eq!(
        round_trip(&order(1)),
        Command::UnitOrder(UnitOrderCommand {
            player: player_id(2),
            units: unit_list(object_ids([61])),
            target: Some(object_id(70)),
            point: Point::new(3.0, 4.0),
            queue_id: genie_core::commands::QueueId::of_data(71),
            through_ai: true,
            nearby: false,
            action: UnitOrderAction::Garrison,
            param: genie_core::production::UnitIndex::of_data(u32::MAX),
            human: true,
        })
    );
    let _ = round_trip(&order(UnitList::LAST_COMMANDED));
}

/// `GO_BACK_TO_WORK2`: the count at `+4` and the ids from `+8`; a tail not of whole ids, or too
/// short for the count, stays raw.
#[test]
fn go_back_to_work_2_keeps_its_counted_ids() {
    let mut rest = vec![2, 0, 0, 0];
    for id in [80_i32, 81, 82] {
        rest.extend(id.to_le_bytes());
    }
    let packet = packets::GoBackToWork2 { commp_id: 1, rest }
        .encode()
        .unwrap();
    assert_eq!(
        round_trip(&packet),
        Command::GoBackToWork2(GoBackToWork2Command {
            player: player_id(1),
            units: UnitList {
                count: 2,
                ids: object_ids([80, 81, 82]),
            },
        })
    );
    let ragged = packets::GoBackToWork2 {
        commp_id: 1,
        rest: vec![1, 0, 0, 0, 9],
    }
    .encode()
    .unwrap();
    assert_eq!(raw(&ragged).kind(), ErrorKind::InvalidInput);
    let short = packets::GoBackToWork2 {
        commp_id: 1,
        rest: vec![1, 0, 0],
    }
    .encode()
    .unwrap();
    assert_eq!(raw(&short).kind(), ErrorKind::Unsupported);
}

/// BUILD and `BUILD_WALL` keep every field their packet holds.
#[test]
fn build_and_build_wall_keep_their_fields() {
    let build = packets::Build {
        commp_id: 1,
        num: 1,
        gap_0x06: [0; 2],
        x: 10.5,
        y: 11.5,
        oid: 70,
        qid: -1,
        field_0x18: 0x2c9,
        frame: 3,
        gap_0x1d: [0],
        field_0x1e: 1,
        field_0x1f: 1,
        ids: vec![9],
    }
    .encode()
    .unwrap();
    assert_eq!(
        round_trip(&build),
        Command::Build(BuildCommand {
            player: player_id(1),
            master: master_id(70),
            position: Point::new(10.5, 11.5),
            queue_id: genie_core::commands::QueueId::of_data(-1),
            sprite_override: genie_core::commands::SpriteOverride::of_data(0x2c9),
            builders: unit_list(object_ids([9])),
            queued: true,
            commits: false,
            frame: genie_core::objects::Frame::of_data(3),
        })
    );
    let wall = packets::BuildWall {
        commp_id: 1,
        num: 1,
        gap_0x06: [0; 2],
        x1: 1,
        y1: 2,
        x2: 3,
        y2: 4,
        oid: 72,
        qid: -1,
        gap_0x18: [0; 2],
        field_0x1a: 0,
        field_0x1b: 0,
        ids: vec![9],
    }
    .encode()
    .unwrap();
    assert_eq!(
        round_trip(&wall),
        Command::BuildWall(BuildWallCommand {
            player: player_id(1),
            master: master_id(72),
            from: genie_core::map::Tile::new(1, 2),
            to: genie_core::map::Tile::new(3, 4),
            queue_id: genie_core::commands::QueueId::of_data(-1),
            builders: unit_list(object_ids([9])),
            queued: false,
            commits: true,
        })
    );
}

/// QUEUE and RESEARCH keep every field their packet holds.
#[test]
fn queue_and_research_keep_their_fields() {
    let queue = packets::MultiQueue {
        commp_id: 1,
        num: 1,
        gap_0x06: [0; 4],
        bid: 109,
        tid: 83,
        count: 5,
        button_location_id: 2,
        ids: vec![9],
    }
    .encode()
    .unwrap();
    assert_eq!(
        round_trip(&queue),
        Command::Queue(QueueCommand {
            player: player_id(1),
            buildings: unit_list(object_ids([9])),
            building_master: master_id(109),
            unit: master_id(83),
            amount: genie_core::production::QueuedUnits::of_data(5),
            button: QueueButton::of_data(2),
        })
    );
    let research = packets::Research {
        commp_id: 1,
        uid: 90,
        field_0x08: 1,
        tid: 22,
        qid: -1,
        field_0x10: 1,
        ids: vec![90],
    }
    .encode()
    .unwrap();
    assert_eq!(
        round_trip(&research),
        Command::Research(ResearchCommand {
            player: player_id(1),
            building: Some(object_id(90)),
            count: 1,
            buildings: object_ids([90]),
            tech: genie_core::technology::TechId::of_data(22),
            queue_id: genie_core::commands::QueueId::of_data(-1),
            shift: true,
        })
    );
}

/// The commands of one object or player keep their fields.
#[test]
fn the_object_commands_keep_their_fields() {
    let commands = [
        (
            packets::Resign {
                commp_id: 2,
                dropped: 1,
            }
            .encode()
            .unwrap(),
            Command::Resign(ResignCommand {
                player: player_id(2),
                dropped: true,
            }),
        ),
        (
            packets::TransformObject {
                commp_id: 2,
                field_0x04: 83,
                field_0x08: 91,
            }
            .encode()
            .unwrap(),
            Command::TransformObject(TransformObjectCommand {
                player: player_id(2),
                id: object_id(91),
                master: master_id(83),
            }),
        ),
        (
            packets::CancelBuild {
                commp_id: 2,
                uid: 92,
            }
            .encode()
            .unwrap(),
            Command::Delete(DeleteCommand {
                player: player_id(2),
                id: object_id(92),
            }),
        ),
        (
            packets::GoBackToWork {
                commp_id: 2,
                uid: 93,
            }
            .encode()
            .unwrap(),
            Command::GoBackToWork(GoBackToWorkCommand {
                player: player_id(2),
                id: object_id(93),
            }),
        ),
        (
            packets::AllBackToWork {
                commp_id: 2,
                uid: 93,
            }
            .encode()
            .unwrap(),
            Command::AllBackToWork(AllBackToWorkCommand {
                player: player_id(2),
                id: object_id(93),
            }),
        ),
        (
            packets::TownBell {
                commp_id: 2,
                uid: 94,
                activate: 1,
            }
            .encode()
            .unwrap(),
            Command::TownBell(TownBellCommand {
                player: player_id(2),
                id: object_id(94),
                activate: true,
            }),
        ),
        (
            packets::Gate {
                commp_id: 2,
                uid: 95,
            }
            .encode()
            .unwrap(),
            Command::Gate(GateCommand {
                player: player_id(2),
                id: object_id(95),
            }),
        ),
    ];
    for (packet, command) in commands {
        assert_eq!(round_trip(&packet), command);
    }
}

/// FLARE, the trades and GAME keep their fields.
#[test]
fn flare_trade_and_game_keep_their_fields() {
    let commands = [
        (
            packets::Flare {
                commp_id: 2,
                field_0x04: u32::MAX,
                field_0x08: 7.0,
                field_0x0c: 8.0,
                field_0x10: 4,
                gap_0x11: [1],
                field_0x12: 0,
                field_0x13: 1,
                rest: vec![0, 1],
            }
            .encode()
            .unwrap(),
            Command::Flare(FlareCommand {
                player: player_id(2),
                target: None,
                point: Point::new(7.0, 8.0),
                players: 4,
                shown: vec![true, false, true, false, true],
            }),
        ),
        (
            packets::BuyCommodity {
                commp_id: 2,
                attr: 1,
                amt: 2,
                uid: 96,
            }
            .encode()
            .unwrap(),
            Command::Trade(TradeCommand {
                player: player_id(2),
                attribute: genie_core::commands::ResourceWord::of_data(1),
                times: genie_core::commands::TradeLots::of_data(2),
                market: Some(object_id(96)),
                buy: true,
            }),
        ),
        (
            packets::SellCommodity {
                commp_id: 2,
                attr: 0x0102,
                amt: 1,
                uid: 97,
            }
            .encode()
            .unwrap(),
            Command::Trade(TradeCommand {
                player: player_id(2),
                attribute: genie_core::commands::ResourceWord::of_data(0x0102),
                times: genie_core::commands::TradeLots::of_data(1),
                market: Some(object_id(97)),
                buy: false,
            }),
        ),
        (
            packets::Game {
                commp_id: 2,
                field_0x04: 1,
                gap_0x06: [0; 2],
                field_0x08: 3,
                field_0x0a: -1,
                field_0x0c: 1.7,
                field_0x10: 9,
            }
            .encode()
            .unwrap(),
            Command::Game(GameCommand {
                issuer: player_id(2),
                sub: genie_core::commands::GameSub::SPEED,
                player: 3,
                a: -1,
                speed: 1.7,
                value: 9,
            }),
        ),
    ];
    for (packet, command) in commands {
        assert_eq!(round_trip(&packet), command);
    }
}

/// GIVE ATTRIBUTES keeps its four entries in order and the receiver.
#[test]
fn tribute_keeps_its_four_gifts() {
    let tribute = packets::TribeGiveAttributes {
        commp_id: 1,
        amt: 100.0,
        field_0x08: 0.0,
        field_0x0c: 0.0,
        field_0x10: 50.0,
        cost: 30.0,
        field_0x18: 0.0,
        field_0x1c: 0.0,
        field_0x20: 15.0,
        attr: 3,
        field_0x26: 0,
        field_0x28: 0,
        field_0x2a: 1,
        to: 2,
    }
    .encode()
    .unwrap();
    let gift = |attribute, amount, fee| Gift {
        attribute: genie_core::commands::ResourceWord::of_data(attribute),
        amount,
        fee: genie_core::players::TributeFee::of_data(fee),
    };
    assert_eq!(
        round_trip(&tribute),
        Command::Tribute(TributeCommand {
            player: player_id(1),
            to: player_id(2),
            gifts: [
                gift(3, Amount::new(100.0), 30.0),
                gift(0, Amount::ZERO, 0.0),
                gift(0, Amount::ZERO, 0.0),
                gift(1, Amount::new(50.0), 15.0),
            ],
        })
    );
}

/// A packet whose command would not write back its bytes stays raw, with the reason the run
/// stops on: a byte the handler reads as a flag but holds 2, a gap that is not the client's.
#[test]
fn packet_its_command_does_not_write_back_stays_raw() {
    let why = raw(&interact(1, 2, vec![10]));
    assert_eq!(why.kind(), ErrorKind::InvalidInput);
    assert_eq!(
        why.reason(),
        "a command packet its command does not write back"
    );
    let queue = packets::BuildingCommandQueue {
        commp_id: 3,
        tid: 41,
        x: 3.0,
        y: 4.0,
        num: 1,
        gap_0x12: [0; 6],
        ids: vec![32],
    }
    .encode()
    .unwrap();
    assert_eq!(raw(&queue).kind(), ErrorKind::InvalidInput);
}

/// A packet holding a value its command's types refuse stays raw, the reason the type's and
/// the command's name: a negative master to build, −1 for the object a command needs (the one
/// deleted, a FOLLOW's target). Where the command works without one, an object id's −1 is none
/// and decodes: in a unit list (a computer's BUILD without builders lists −1), as a target.
#[test]
fn packet_of_value_its_types_refuse_stays_raw() {
    let build = |oid| {
        packets::Build {
            commp_id: 1,
            num: 1,
            gap_0x06: [0; 2],
            x: 1.0,
            y: 2.0,
            oid,
            qid: -1,
            field_0x18: u32::MAX,
            frame: 0,
            gap_0x1d: [0],
            field_0x1e: 0,
            field_0x1f: 0,
            ids: vec![-1],
        }
        .encode()
        .unwrap()
    };
    let why = raw(&build(-5));
    assert_eq!(why.kind(), ErrorKind::InvalidInput);
    assert_eq!(why.reason(), "a negative master id, which no master has");
    let Command::Build(BuildCommand { builders, .. }) = round_trip(&build(70)) else {
        panic!("a build")
    };
    assert_eq!(builders.ids, [None]);
    let Command::Order(OrderCommand { units, .. }) = round_trip(&interact(2, 1, vec![-1, 5]))
    else {
        panic!("an interact")
    };
    assert_eq!(units.ids, object_ids([-1, 5]));
    let delete = packets::CancelBuild {
        commp_id: 1,
        uid: -1,
    }
    .encode()
    .unwrap();
    let follow = packets::Follow {
        commp_id: 1,
        num: 1,
        gap_0x06: [0; 2],
        target: -1,
        ids: vec![5],
    }
    .encode()
    .unwrap();
    for needed in [delete, follow] {
        let why = raw(&needed);
        assert_eq!(why.kind(), ErrorKind::InvalidInput);
        assert_eq!(why.reason(), "an object id of −1, which names none");
    }
}

/// A kind the engine does not run, and a packet that fits no layout, stay raw; a packet without
/// its player byte is an error.
#[test]
fn an_unmodelled_packet_stays_raw() {
    let pause = packets::Pause {
        commp_id: 5,
        num: 0,
        gap_0x06: [0; 2],
        ids: vec![],
    }
    .encode()
    .unwrap();
    assert_eq!(raw(&pause).kind(), ErrorKind::Unsupported);
    // an INTERACT one byte short of its fixed part
    let mut short = interact(0, 1, vec![]);
    short.truncate(short.len() - 1);
    short[2] -= 1;
    assert_eq!(raw(&short).kind(), ErrorKind::Unsupported);
    assert_eq!(
        decode(&[0]).map_err(|e| e.kind()),
        Err(ErrorKind::InvalidInput)
    );
}

/// A command whose values its packet cannot hold is an error: a FLARE of fewer than three
/// players' bytes, more ids than a packet's size holds (a player is a byte by its type).
#[test]
fn command_its_packet_cannot_hold_is_an_error() {
    let stop = |player, ids: Vec<i32>| {
        Command::Stop(StopCommand {
            player,
            units: unit_list(object_ids(ids)),
        })
    };
    let refused = |c: &Command| encode(c).map_err(|e| e.kind());
    let most = (usize::from(u16::MAX) + 4 - packets::Stop::FIXED) / 4;
    assert_eq!(
        encode(&stop(player_id(1), vec![0; most])).map(|p| p.len()),
        Ok(65536)
    );
    assert_eq!(
        refused(&stop(player_id(1), vec![0; most + 1])),
        Err(ErrorKind::InvalidInput)
    );
    let flare = Command::Flare(FlareCommand {
        player: player_id(1),
        target: None,
        point: Point::new(0.0, 0.0),
        players: 2,
        shown: vec![true, false],
    });
    assert_eq!(refused(&flare), Err(ErrorKind::InvalidInput));
}
