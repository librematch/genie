//! Each command read through [`Command::read_from`] from its framed bytes; each with a writer
//! written back byte for byte.

use super::*;
use crate::test_support::{floats, words};
use arrayvec::ArrayVec;
use genie_error::ErrorKind;
use genie_support::UnitKindId;

/// A command's frame: its length (the id and the body), the id, the body, a world time.
fn framed(id: u8, body: &[u8]) -> Vec<u8> {
    let len = u32::try_from(body.len() + 1).unwrap();
    let mut bytes = len.to_le_bytes().to_vec();
    bytes.push(id);
    bytes.extend(body);
    bytes.extend(7u32.to_le_bytes());
    bytes
}

/// The command `id` with `body`, read; the whole frame is consumed.
fn read(id: u8, body: &[u8]) -> Command {
    let bytes = framed(id, body);
    let mut input = bytes.as_slice();
    let command = Command::read_from(&mut input).unwrap();
    assert_eq!(input, []);
    command
}

/// Two objects.
fn two() -> ObjectsList {
    ObjectsList::List(vec![ObjectId::from(500u32), ObjectId::from(501u32)])
}

/// Write `$command`, read it back as `Command::$variant` with `$id`, write that again: the
/// same bytes. Gives the command read.
macro_rules! writes_back {
    ($id:expr, $variant:ident, $command:expr) => {{
        let mut body = vec![];
        $command.write_to(&mut body).unwrap();
        let Command::$variant(read) = read($id, &body) else {
            panic!(concat!("not ", stringify!($variant)));
        };
        let mut again = vec![];
        read.write_to(&mut again).unwrap();
        assert_eq!(again, body, stringify!($variant));
        read
    }};
}

/// The orders: order, stop, work, move (with its player), create, add resource and the AI's
/// order (one object in its head, more after it; the target's player).
#[test]
fn the_orders_write_back_what_they_read() {
    let order = writes_back!(
        0x00,
        Order,
        OrderCommand {
            player_id: 2.into(),
            target_id: Some(ObjectId::from(900u32)),
            location: (10.5, 20.5),
            objects: two(),
        }
    );
    assert_eq!((u8::from(order.player_id), order.objects.len()), (2, 2));
    let _ = writes_back!(0x01, Stop, StopCommand { objects: two() });
    let _ = writes_back!(
        0x02,
        Work,
        WorkCommand {
            target_id: None,
            location: (1.0, 2.0),
            objects: two(),
        }
    );
    let moved = writes_back!(
        0x03,
        Move,
        MoveCommand {
            player_id: 3.into(),
            target_id: None,
            location: (4.0, 5.0),
            objects: two(),
        }
    );
    assert_eq!(u8::from(moved.player_id), 3);
    let _ = writes_back!(
        0x04,
        Create,
        CreateCommand {
            player_id: 1.into(),
            unit_kind_id: UnitKindId::from(83u16),
            location: (1.0, 2.0, 3.0),
        }
    );
    let _ = writes_back!(
        0x05,
        AddResource,
        AddResourceCommand {
            player_id: 1.into(),
            resource: 2,
            amount: 100.0,
        }
    );
    for objects in [ObjectsList::List(vec![ObjectId::from(500u32)]), two()] {
        let ai = writes_back!(
            0x0a,
            AiOrder,
            AiOrderCommand {
                player_id: 4.into(),
                issuer: 4.into(),
                objects: objects.clone(),
                order_kind: 0x2d9,
                order_priority: 100,
                target_id: Some(ObjectId::from(77u32)),
                target_player_id: Some(2.into()),
                target_location: (1.0, 2.0, 0.0),
                range: 3.0,
                immediate: true,
                add_to_front: false,
            }
        );
        assert_eq!(ai.objects.len(), objects.len());
        assert_eq!(ai.target_player_id.map(u8::from), Some(2));
    }
}

/// The group commands: resign, waypoint, stance, guard, follow, patrol (its waypoints in two
/// blocks of ten), formation and `UserPatch`'s AI command (its parameters by the length).
#[test]
fn the_group_commands_write_back_what_they_read() {
    let _ = writes_back!(
        0x0b,
        Resign,
        ResignCommand {
            player_id: 2.into(),
            comm_player_id: 2.into(),
            dropped: true,
        }
    );
    let _ = writes_back!(
        0x10,
        GroupWaypoint,
        GroupWaypointCommand {
            player_id: 1.into(),
            location: (3, 4),
            objects: two(),
        }
    );
    let _ = writes_back!(
        0x12,
        UnitAiState,
        UnitAiStateCommand {
            state: 2,
            objects: two(),
        }
    );
    let _ = writes_back!(
        0x13,
        Guard,
        GuardCommand {
            target_id: Some(ObjectId::from(8u32)),
            objects: two(),
        }
    );
    let _ = writes_back!(
        0x14,
        Follow,
        FollowCommand {
            target_id: None,
            objects: two(),
        }
    );
    let mut waypoints = ArrayVec::new();
    waypoints.push((1.0, 2.0));
    waypoints.push((3.0, 4.0));
    let patrol = writes_back!(
        0x15,
        Patrol,
        PatrolCommand {
            waypoints,
            objects: two(),
        }
    );
    assert_eq!(patrol.waypoints.as_slice(), [(1.0, 2.0), (3.0, 4.0)]);
    let _ = writes_back!(
        0x17,
        FormFormation,
        FormFormationCommand {
            player_id: 1.into(),
            formation_kind: 7,
            objects: two(),
        }
    );
    let mut params = ArrayVec::new();
    params.extend([5, 6, 7]);
    let ai = writes_back!(
        0x35,
        UserPatchAi,
        UserPatchAiCommand {
            player_id: 2.into(),
            ai_action: 1,
            params,
        }
    );
    assert_eq!(ai.params.as_slice(), [5, 6, 7]);
}

/// A patrol that says it has more than ten waypoints is an error, not a panic: the command
/// holds ten.
#[test]
fn patrol_above_ten_waypoints_is_an_error() {
    let mut body = vec![0, 11, 0];
    body.extend([0; 80]);
    let error = PatrolCommand::read_from(body.as_slice()).unwrap_err();
    assert_eq!(
        (error.kind(), error.reason()),
        (
            ErrorKind::InvalidInput,
            "a value that does not fit its field"
        )
    );
    assert_eq!(error.context().collect::<Vec<_>>(), [("waypoints", "11")]);
}

/// The building and market commands with writers: make, research, cancel build, attack ground,
/// repair, queue, gather point, sell and buy.
#[test]
fn the_building_commands_write_back_what_they_read() {
    let _ = writes_back!(
        0x64,
        Make,
        MakeCommand {
            player_id: 1.into(),
            building_id: ObjectId::from(700u32),
            unit_kind_id: UnitKindId::from(83u16),
            target_id: None,
        }
    );
    let _ = writes_back!(
        0x65,
        Research,
        ResearchCommand {
            player_id: 1.into(),
            building_id: ObjectId::from(700u32),
            tech_id: genie_support::TechId::from(101u16),
            target_id: Some(ObjectId::from(3u32)),
        }
    );
    let _ = writes_back!(
        0x6a,
        CancelBuild,
        CancelBuildCommand {
            player_id: 1.into(),
            building_id: ObjectId::from(701u32),
        }
    );
    let _ = writes_back!(
        0x6b,
        AttackGround,
        AttackGroundCommand {
            location: (5.0, 6.0),
            objects: two(),
        }
    );
    let _ = writes_back!(
        0x6e,
        Repair,
        RepairCommand {
            target_id: Some(ObjectId::from(702u32)),
            repairers: two(),
        }
    );
    let _ = writes_back!(
        0x77,
        Queue,
        QueueCommand {
            building_id: ObjectId::from(700u32),
            unit_kind_id: UnitKindId::from(4u16),
            amount: 5,
        }
    );
    let _ = writes_back!(
        0x78,
        SetGatherPoint,
        SetGatherPointCommand {
            buildings: two(),
            target_id: None,
            target_kind_id: Some(UnitKindId::from(59u16)),
            location: Some((7.0, 8.0)),
        }
    );
    let market = ObjectId::from(800u32);
    let _ = writes_back!(
        0x7a,
        SellResource,
        SellResourceCommand {
            player_id: 1.into(),
            resource: 1,
            amount: 1,
            market_id: market,
        }
    );
    let _ = writes_back!(
        0x7b,
        BuyResource,
        BuyResourceCommand {
            player_id: 1.into(),
            resource: 2,
            amount: 5,
            market_id: market,
        }
    );
}

/// The commands without writers, from their bytes: build, build wall (one builder of -1 is
/// none), ungarrison and unit order (a point of -1 none), flare, the 0x7f command and back to
/// work.
#[test]
fn the_commands_without_writers_read_their_fields() {
    let mut build = vec![1u8, 2, 0];
    build.extend(floats(&[10.5, 20.5]));
    build.extend([70, 0, 0, 0]);
    build.extend(words(&[-1]));
    build.extend([3, 0, 0, 0]);
    build.extend(words(&[500]));
    let Command::Build(build) = read(0x66, &build) else {
        panic!("not Build")
    };
    assert_eq!(
        (build.frame, build.location, build.builders.len()),
        (3, (10.5, 20.5), 1)
    );

    let mut wall = vec![1u8, 2, 3, 4, 5, 6, 0, 72, 0, 0, 0];
    wall.extend(words(&[-1, -1]));
    let Command::BuildWall(wall) = read(0x69, &wall) else {
        panic!("not BuildWall")
    };
    assert_eq!(
        (wall.start, wall.end, wall.builders.len()),
        ((3, 4), (5, 6), 0)
    );

    let mut ungarrison = vec![1u8, 0, 0];
    ungarrison.extend(floats(&[-1.0, -1.0]));
    ungarrison.extend([4, 0, 0, 0]);
    ungarrison.extend(words(&[-1, 600]));
    let Command::Ungarrison(ungarrison) = read(0x6f, &ungarrison) else {
        panic!("not Ungarrison")
    };
    assert_eq!(
        (
            ungarrison.ungarrison_kind,
            ungarrison.unit_kind_id,
            ungarrison.location,
            ungarrison.objects.len()
        ),
        (4, None, None, 1)
    );
    // the same type (kind 4) of unit type 83
    let mut by_kind = vec![1u8, 0, 0];
    by_kind.extend(floats(&[-1.0, -1.0]));
    by_kind.extend([4, 0, 0, 0]);
    by_kind.extend(words(&[83, 600]));
    let Command::Ungarrison(by_kind) = read(0x6f, &by_kind) else {
        panic!("not Ungarrison")
    };
    assert_eq!(by_kind.unit_kind_id, Some(UnitKindId::from(83u16)));

    let mut flare = vec![0u8; 3];
    flare.extend(words(&[-1]));
    flare.extend([1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    flare.extend(floats(&[5.0, 6.0]));
    flare.extend([2, 2, 0, 0]);
    let Command::Flare(flare) = read(0x73, &flare) else {
        panic!("not Flare")
    };
    assert_eq!(
        (
            flare.recipients[2],
            flare.location,
            u8::from(flare.player_id)
        ),
        (true, (5.0, 6.0), 2)
    );

    let mut order = vec![1u8, 0, 0];
    order.extend(words(&[900]));
    order.extend([3, 0xff, 0, 0]);
    order.extend(floats(&[1.5, 2.5]));
    order.extend(words(&[-1, 501]));
    let Command::UnitOrder(order) = read(0x75, &order) else {
        panic!("not UnitOrder")
    };
    assert_eq!(
        (order.action, order.param, order.location, order.unique_id),
        (3, None, Some((1.5, 2.5)), None)
    );

    let mut unknown = vec![0u8; 3];
    unknown.extend(words(&[42, 9]));
    let Command::Unknown7F(unknown) = read(0x7f, &unknown) else {
        panic!("not Unknown7F")
    };
    assert_eq!((u32::from(unknown.object_id), unknown.value), (42, 9));

    let mut back = vec![0u8; 3];
    back.extend(words(&[700]));
    let Command::BackToWork(back) = read(0x80, &back) else {
        panic!("not BackToWork")
    };
    assert_eq!(u32::from(back.building_id), 700);
}

/// The game commands (0x67) by their sub-command: each takes the player from the first word
/// and its values from the others.
#[test]
fn each_game_command_takes_its_values() {
    let game = |sub: u8, var2: i16, var3: f32, var4: u32| {
        let mut body = vec![sub];
        body.extend(2i16.to_le_bytes());
        body.extend(var2.to_le_bytes());
        body.extend([0, 0]);
        body.extend(var3.to_le_bytes());
        body.extend(var4.to_le_bytes());
        let Command::Game(command) = read(0x67, &body) else {
            panic!("not Game")
        };
        command
    };
    assert!(
        matches!(game(0x01, 0, 1.5, 0), GameCommand::SetGameSpeed { speed, .. } if speed.to_bits() == 1.5f32.to_bits())
    );
    assert!(matches!(
        game(0x02, 3, 20.0, 0),
        GameCommand::Inventory {
            attribute_id: 3,
            ..
        }
    ));
    assert!(matches!(
        game(0x03, 0, 0.0, 0),
        GameCommand::UpgradeTown { .. }
    ));
    assert!(matches!(
        game(0x04, 0, 0.0, 0),
        GameCommand::QuickBuild { .. }
    ));
    assert!(matches!(
        game(0x05, 1, 0.0, 0),
        GameCommand::AlliedVictory { status: true, .. }
    ));
    assert!(matches!(
        game(0x06, 9, 0.0, 0),
        GameCommand::Cheat { cheat_id: 9, .. }
    ));
    assert!(matches!(
        game(0x07, 0, 0.0, 0),
        GameCommand::SharedLos { .. }
    ));
    assert!(matches!(game(0x0a, 0, 0.0, 0), GameCommand::Spies { .. }));
    assert!(matches!(
        game(0x0b, 5, 0.0, 9),
        GameCommand::SetStrategicNumber {
            strategic_number: 5,
            value: 9,
            ..
        }
    ));
    assert!(matches!(
        game(0x0c, 0, 0.0, 0),
        GameCommand::Unknown0x0c { .. }
    ));
    assert!(matches!(
        game(0x0d, 4, 0.0, 0),
        GameCommand::AddFarmReseedQueue { amount: 4, .. }
    ));
    assert!(matches!(
        game(0x0e, 4, 0.0, 0),
        GameCommand::RemoveFarmReseedQueue { amount: 4, .. }
    ));
    assert!(matches!(
        game(0x10, 0, 0.0, 0),
        GameCommand::FarmReseedAutoQueue { .. }
    ));
}

/// An id no command has keeps its id and the bytes after it.
#[test]
fn unknown_unit_command_keeps_its_bytes() {
    let command = Command::read_from(&mut framed(0x06, &[1, 2, 3]).as_slice()).unwrap();
    assert!(matches!(command, Command::Unknown { id: 0x06, ref bytes } if bytes == &[1, 2, 3]));
}

/// So does one among the building commands.
#[test]
fn unknown_building_command_keeps_its_bytes() {
    let command = Command::read_from(&mut framed(0x68, &[]).as_slice()).unwrap();
    assert!(matches!(command, Command::Unknown { id: 0x68, ref bytes } if bytes.is_empty()));
}
