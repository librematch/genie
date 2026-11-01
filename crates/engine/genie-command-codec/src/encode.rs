//! A command into its packet: genie-rec's layout of the command's kind, filled from the command.

use genie_core::GenieError;
use genie_core::commands::{
    AttackMoveCommand, BuildCommand, BuildWallCommand, Command, FlareCommand, GameCommand, Order,
    OrderCommand, PatrolCommand, QueueCommand, ResearchCommand, TradeCommand, TributeCommand,
    UnitList, UnitOrderCommand, UnloadCommand,
};
use genie_core::map::Axis;
use genie_core::objects::ObjectId;
use genie_core::objects::masters::{ActionType, MasterId};
use genie_rec::de::command::{self as packets, Command as Packet};

/// The bytes the client writes at `BUILDING_COMMAND_QUEUE`'s `+0x12..+0x18`, which no code
/// reads (every recording holds them).
const COMMAND_QUEUE_GAP: [u8; 6] = [0, 1, 1, 0, 0, 0];
/// The byte the client writes at `UNIT_ORDER`'s `+0x1f`, which no code reads (every recording
/// holds it).
const UNIT_ORDER_GAP: [u8; 1] = [1];

/// The packet of a command: its kind's layout, the bytes no code reads as the client writes
/// them; an unmodelled command's own bytes.
///
/// # Errors
///
/// A value its packet's field cannot hold: a unit list or a FLARE's players too long for the
/// packet.
pub fn encode(command: &Command) -> Result<Vec<u8>, GenieError> {
    match command {
        Command::Unmodelled(c) => Ok(c.packet.clone()),
        other => Ok(packet(other)?.encode()?),
    }
}

/// The largest packet: its size after the 4-byte header is a u16 (`+0x2`).
const MOST_BYTES: usize = 4 + u16::MAX as usize;

/// A tail of `fixed` bytes' layout, if its packet's size holds it.
fn within<T>(tail: &[T], fixed: usize) -> Result<&[T], GenieError> {
    if fixed + size_of_val(tail) <= MOST_BYTES {
        Ok(tail)
    } else {
        Err(
            GenieError::invalid_input("a command too long for its packet's size")
                .with("bytes", fixed + size_of_val(tail)),
        )
    }
}

/// The ids after a fixed part of `fixed` bytes, if the packet's size holds them: the packet's
/// values, −1 for none.
fn ids(ids: &[Option<ObjectId>], fixed: usize) -> Result<Vec<i32>, GenieError> {
    let raw: Vec<i32> = ids.iter().map(|&id| ObjectId::field(id)).collect();
    let _ = within(&raw, fixed)?;
    Ok(raw)
}

/// A command's layout in genie-rec.
fn packet(command: &Command) -> Result<Packet, GenieError> {
    match command {
        Command::Order(c) => order(c),
        Command::Build(_)
        | Command::BuildWall(_)
        | Command::Queue(_)
        | Command::Research(_)
        | Command::GatherPoint(_)
        | Command::CommandQueue(_) => building(command),
        Command::Stop(_)
        | Command::UnitAiState(_)
        | Command::AutoScout(_)
        | Command::SeekShelter(_)
        | Command::SetGatherState(_)
        | Command::FormFormation(_)
        | Command::Patrol(_)
        | Command::AttackMove(_)
        | Command::Unload(_)
        | Command::UnitOrder(_)
        | Command::GoBackToWork2(_) => listed(command),
        other => single(other),
    }
}

/// The commands that place or queue at buildings: BUILD, `BUILD_WALL`, QUEUE, RESEARCH, SET
/// GATHER POINT and `BUILDING_COMMAND_QUEUE`.
fn building(command: &Command) -> Result<Packet, GenieError> {
    Ok(match command {
        Command::Build(c) => build(c)?,
        Command::BuildWall(c) => build_wall(c)?,
        Command::Queue(c) => queue(c)?,
        Command::Research(c) => research(c)?,
        Command::GatherPoint(c) => Packet::SetGatherPoint(packets::SetGatherPoint {
            commp_id: c.player.get(),
            num: c.units.count,
            gap_0x06: [0; 2],
            x: c.point.x(),
            y: c.point.y(),
            t_id: ObjectId::field(c.target),
            tm_id: i32::from(MasterId::field(c.target_master)),
            field_0x18: u8::from(c.own),
            ids: ids(&c.units.ids, packets::SetGatherPoint::FIXED)?,
        }),
        Command::CommandQueue(c) => Packet::BuildingCommandQueue(packets::BuildingCommandQueue {
            commp_id: c.player.get(),
            t_id: ObjectId::field(c.target),
            x: c.point.x(),
            y: c.point.y(),
            num: c.units.count,
            gap_0x12: COMMAND_QUEUE_GAP,
            ids: ids(&c.units.ids, packets::BuildingCommandQueue::FIXED)?,
        }),
        other => return elsewhere(other),
    })
}

/// The commands to the objects listed that do not go through the orders' reader.
fn listed(command: &Command) -> Result<Packet, GenieError> {
    Ok(match command {
        Command::Stop(c) => Packet::Stop(packets::Stop {
            commp_id: c.player.get(),
            num: c.units.count,
            gap_0x06: [0; 2],
            ids: ids(&c.units.ids, packets::Stop::FIXED)?,
        }),
        Command::UnitAiState(c) => Packet::UnitAiState(packets::UnitAiState {
            commp_id: c.player.get(),
            num: c.units.count,
            gap_0x06: [0; 2],
            state: c.state.get(),
            gap_0x09: [0; 3],
            ids: ids(&c.units.ids, packets::UnitAiState::FIXED)?,
        }),
        Command::AutoScout(c) => Packet::AutoScout(packets::AutoScout {
            commp_id: c.player.get(),
            field_0x04: c.units.count,
            gap_0x06: [0; 2],
            ids: ids(&c.units.ids, packets::AutoScout::FIXED)?,
        }),
        Command::FormFormation(c) => Packet::FormFormation(packets::FormFormation {
            commp_id: c.player.get(),
            num: c.units.count,
            gap_0x06: [0; 2],
            formation: c.formation.get(),
            ids: ids(&c.units.ids, packets::FormFormation::FIXED)?,
        }),
        Command::SetGatherState(c) => Packet::SetGatherState(packets::SetGatherState {
            commp_id: c.player.get(),
            num: c.units.count,
            gap_0x06: [0; 2],
            field_0x08: c
                .state
                .map_or(0, genie_core::commands::GatherPointState::get),
            ids: ids(&c.units.ids, packets::SetGatherState::FIXED)?,
        }),
        Command::SeekShelter(c) => Packet::SeekShelter(packets::SeekShelter {
            commp_id: c.player.get(),
            num: c.units.count,
            gap_0x06: [0; 2],
            ids: ids(&c.units.ids, packets::SeekShelter::FIXED)?,
        }),
        Command::Patrol(c) => patrol(c)?,
        Command::AttackMove(c) => attack_move(c)?,
        Command::Unload(c) => unload(c)?,
        Command::UnitOrder(c) => unit_order(c)?,
        Command::GoBackToWork2(c) => Packet::GoBackToWork2(packets::GoBackToWork2 {
            commp_id: c.player.get(),
            rest: go_back_to_work_rest(&c.units)?,
        }),
        other => return elsewhere(other),
    })
}

/// A command another group's function writes, or an unmodelled one, which keeps its own bytes
/// ([`encode`]): [`packet`] never passes either here.
fn elsewhere(command: &Command) -> Result<Packet, GenieError> {
    Err(GenieError::unreachable("a command another arm writes")
        .with("command", format!("{command:?}")))
}

/// The commands of one object, one player or the game.
fn single(command: &Command) -> Result<Packet, GenieError> {
    Ok(match command {
        Command::Resign(c) => Packet::Resign(packets::Resign {
            commp_id: c.player.get(),
            dropped: u8::from(c.dropped),
        }),
        Command::TransformObject(c) => Packet::TransformObject(packets::TransformObject {
            commp_id: c.player.get(),
            field_0x04: i32::from(c.master.get()),
            field_0x08: c.id.get().cast_unsigned(),
        }),
        Command::Delete(c) => Packet::CancelBuild(packets::CancelBuild {
            commp_id: c.player.get(),
            u_id: c.id.get(),
        }),
        Command::GoBackToWork(c) => Packet::GoBackToWork(packets::GoBackToWork {
            commp_id: c.player.get(),
            u_id: c.id.get(),
        }),
        Command::AllBackToWork(c) => Packet::AllBackToWork(packets::AllBackToWork {
            commp_id: c.player.get(),
            u_id: c.id.get(),
        }),
        Command::TownBell(c) => Packet::TownBell(packets::TownBell {
            commp_id: c.player.get(),
            u_id: c.id.get(),
            activate: u8::from(c.activate),
        }),
        Command::Gate(c) => Packet::Gate(packets::Gate {
            commp_id: c.player.get(),
            u_id: c.id.get(),
        }),
        Command::Tribute(c) => tribute(c),
        Command::Trade(c) => trade(c),
        Command::Flare(c) => flare(c)?,
        Command::Game(c) => game(c),
        other => return elsewhere(other),
    })
}

/// INTERACT's layout of an order, which WORK and RETREAT share.
macro_rules! interact_packet {
    ($kind:ident, $player:expr, $units:expr, $o:expr) => {
        Packet::$kind(packets::$kind {
            commp_id: $player,
            t_id: ObjectId::field($o.target),
            x: $o.point.x(),
            y: $o.point.y(),
            num: $units.count,
            field_0x12: u8::from($o.carry_test),
            field_0x13: u8::from($o.queued),
            field_0x14: u8::from($o.direct),
            field_0x15: u8::from($o.human),
            field_0x16: ActionType::field($o.filter).cast_unsigned(),
            ids: ids(&$units.ids, packets::$kind::FIXED)?,
        })
    };
}

/// INTERACT, WORK, MOVE, RETREAT, FOLLOW and GUARD.
fn order(c: &OrderCommand) -> Result<Packet, GenieError> {
    let (player, units) = (c.player.get(), &c.units);
    Ok(match c.order {
        Order::Interact(o) => interact_packet!(Interact, player, units, o),
        Order::Work(o) => interact_packet!(Work, player, units, o),
        Order::Retreat(o) => interact_packet!(Retreat, player, units, o),
        Order::Move(o) => Packet::Move(packets::Move {
            commp_id: player,
            t_id: ObjectId::field(o.target),
            x: o.point.x(),
            y: o.point.y(),
            num: units.count,
            field_0x12: u8::from(o.control),
            field_0x13: u8::from(o.queued),
            field_0x14: u8::from(o.direct),
            field_0x15: u8::from(o.human),
            gap_0x16: [0; 2],
            ids: ids(&units.ids, packets::Move::FIXED)?,
        }),
        Order::Follow(target) => Packet::Follow(packets::Follow {
            commp_id: player,
            num: units.count,
            gap_0x06: [0; 2],
            target: target.get(),
            ids: ids(&units.ids, packets::Follow::FIXED)?,
        }),
        Order::Guard(target) => Packet::Guard(packets::Guard {
            commp_id: player,
            num: units.count,
            gap_0x06: [0; 2],
            target: target.get(),
            ids: ids(&units.ids, packets::Guard::FIXED)?,
        }),
    })
}

/// BUILD (0x66).
fn build(c: &BuildCommand) -> Result<Packet, GenieError> {
    Ok(Packet::Build(packets::Build {
        commp_id: c.player.get(),
        num: c.builders.count,
        gap_0x06: [0; 2],
        x: c.position.x(),
        y: c.position.y(),
        o_id: i32::from(c.master.get()),
        q_id: c.q_id,
        field_0x18: c.sprite_override.cast_unsigned(),
        frame: c.frame,
        gap_0x1d: [0; 1],
        field_0x1e: u8::from(c.queued),
        field_0x1f: u8::from(!c.commits),
        ids: ids(&c.builders.ids, packets::Build::FIXED)?,
    }))
}

/// `BUILD_WALL` (0x69).
fn build_wall(c: &BuildWallCommand) -> Result<Packet, GenieError> {
    Ok(Packet::BuildWall(packets::BuildWall {
        commp_id: c.player.get(),
        num: c.builders.count,
        gap_0x06: [0; 2],
        x1: c.from[0],
        y1: c.from[1],
        x2: c.to[0],
        y2: c.to[1],
        o_id: i32::from(c.master.get()),
        q_id: c.q_id,
        gap_0x18: [0; 2],
        field_0x1a: u8::from(c.queued),
        field_0x1b: u8::from(!c.commits),
        ids: ids(&c.builders.ids, packets::BuildWall::FIXED)?,
    }))
}

/// QUEUE (0x81, `MULTI_QUEUE`).
fn queue(c: &QueueCommand) -> Result<Packet, GenieError> {
    Ok(Packet::MultiQueue(packets::MultiQueue {
        commp_id: c.player.get(),
        num: c.buildings.count,
        gap_0x06: [0; 4],
        b_id: c.building_master.get(),
        t_id: c.unit.get(),
        count: c.amount,
        button_location_id: c.button.get(),
        ids: ids(&c.buildings.ids, packets::MultiQueue::FIXED)?,
    }))
}

/// RESEARCH (0x65).
fn research(c: &ResearchCommand) -> Result<Packet, GenieError> {
    Ok(Packet::Research(packets::Research {
        commp_id: c.player.get(),
        u_id: ObjectId::field(c.building),
        field_0x08: c.count.cast_unsigned(),
        t_id: c.tech,
        q_id: c.q_id,
        field_0x10: u8::from(c.shift),
        ids: ids(&c.buildings, packets::Research::FIXED)?,
    }))
}

/// PATROL (0x15): x as each slot's f32 bits.
fn patrol(c: &PatrolCommand) -> Result<Packet, GenieError> {
    let x = |i: usize| c.waypoints.points[i].x().to_bits();
    let y = |i: usize| c.waypoints.points[i].y();
    Ok(Packet::Patrol(packets::Patrol {
        commp_id: c.player.get(),
        num: c.units.count,
        gap_0x06: [0; 2],
        field_0x08: u8::from(c.reset),
        gap_0x09: [0; 2],
        waypoints_count: c.waypoints.count,
        field_0x0c: x(0),
        field_0x10: x(1),
        field_0x14: x(2),
        field_0x18: x(3),
        field_0x1c: x(4),
        field_0x20: x(5),
        field_0x24: x(6),
        field_0x28: x(7),
        field_0x2c: x(8),
        field_0x30: x(9),
        field_0x34: y(0),
        field_0x38: y(1),
        field_0x3c: y(2),
        field_0x40: y(3),
        field_0x44: y(4),
        field_0x48: y(5),
        field_0x4c: y(6),
        field_0x50: y(7),
        field_0x54: y(8),
        field_0x58: y(9),
        ids: ids(&c.units.ids, packets::Patrol::FIXED)?,
    }))
}

/// `ATTACK_MOVE` (0x21): the first x as its f32 bits, the other slots' x and y as bytes.
fn attack_move(c: &AttackMoveCommand) -> Result<Packet, GenieError> {
    let points = &c.waypoints.points;
    let tail = |axis: Axis| -> [u8; 36] {
        let mut out = [0; 36];
        for (i, p) in points[1..].iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&p.along(axis).to_le_bytes());
        }
        out
    };
    Ok(Packet::AttackMove(packets::AttackMove {
        commp_id: c.player.get(),
        num: c.units.count,
        gap_0x06: [0; 2],
        field_0x08: u8::from(c.reset),
        field_0x09: u8::from(c.queued),
        field_0x0a: u8::from(c.ignore_pending),
        waypoints_count: c.waypoints.count,
        field_0x0c: points[0].x().to_bits(),
        gap_0x10: tail(Axis::X),
        field_0x34: points[0].y(),
        gap_0x38: tail(Axis::Y),
        ids: ids(&c.units.ids, packets::AttackMove::FIXED)?,
    }))
}

/// UNLOAD (0x6f).
fn unload(c: &UnloadCommand) -> Result<Packet, GenieError> {
    let (flag, unit_type) = c.pick.packet();
    Ok(Packet::Unload(packets::Unload {
        commp_id: c.player.get(),
        num: c.units.count,
        gap_0x06: [0; 2],
        x: c.point.x(),
        y: c.point.y(),
        unit_type,
        flag,
        gap_0x15: [0; 3],
        field_0x18: u8::from(c.sound),
        ids: ids(&c.units.ids, packets::Unload::FIXED)?,
    }))
}

/// `UNIT_ORDER` (0x75).
fn unit_order(c: &UnitOrderCommand) -> Result<Packet, GenieError> {
    Ok(Packet::UnitOrder(packets::UnitOrder {
        commp_id: c.player.get(),
        num: c.units.count,
        gap_0x06: [0; 2],
        t_id: ObjectId::field(c.target),
        x: c.point.x(),
        y: c.point.y(),
        q_id: c.q_id,
        param: c.param.cast_signed(),
        action: c.action.byte(),
        field_0x1d: u8::from(c.through_ai),
        field_0x1e: u8::from(c.nearby),
        gap_0x1f: UNIT_ORDER_GAP,
        field_0x20: u8::from(c.human),
        ids: ids(&c.units.ids, packets::UnitOrder::FIXED)?,
    }))
}

/// `GO_BACK_TO_WORK2`'s tail: the count (+4), two bytes no code reads, the ids from +8.
fn go_back_to_work_rest(units: &UnitList) -> Result<Vec<u8>, GenieError> {
    let mut rest = units.count.to_le_bytes().to_vec();
    rest.extend([0; 2]);
    ids(&units.ids, packets::GoBackToWork2::FIXED + 4)?
        .iter()
        .for_each(|id| rest.extend(id.to_le_bytes()));
    Ok(rest)
}

/// GIVE ATTRIBUTES (0xc4).
fn tribute(c: &TributeCommand) -> Packet {
    let [first, second, third, fourth] = &c.gifts;
    Packet::TribeGiveAttributes(packets::TribeGiveAttributes {
        commp_id: c.player.get(),
        amt: first.amount.get(),
        field_0x08: second.amount.get(),
        field_0x0c: third.amount.get(),
        field_0x10: fourth.amount.get(),
        cost: first.fee,
        field_0x18: second.fee,
        field_0x1c: third.fee,
        field_0x20: fourth.fee,
        attr: first.attribute,
        field_0x26: second.attribute,
        field_0x28: third.attribute,
        field_0x2a: fourth.attribute,
        to: c.to.get(),
    })
}

/// `SELL_COMMODITY` (0x7a) or `BUY_COMMODITY` (0x7b).
fn trade(c: &TradeCommand) -> Packet {
    let player = c.player.get();
    if c.buy {
        Packet::BuyCommodity(packets::BuyCommodity {
            commp_id: player,
            attr: c.attribute,
            amt: c.times,
            u_id: ObjectId::field(c.market),
        })
    } else {
        Packet::SellCommodity(packets::SellCommodity {
            commp_id: player,
            attr: c.attribute,
            amt: c.times,
            u_id: ObjectId::field(c.market),
        })
    }
}

/// FLARE (0x73): the players' bytes from `+0x11`, of which the layout's fixed part holds three.
fn flare(c: &FlareCommand) -> Result<Packet, GenieError> {
    let shown: Vec<u8> = c.shown.iter().map(|&s| u8::from(s)).collect();
    let [first, second, third, rest @ ..] = shown.as_slice() else {
        return Err(GenieError::invalid_input(
            "a FLARE's packet holds three players' bytes at least",
        )
        .with("players", c.shown.len()));
    };
    Ok(Packet::Flare(packets::Flare {
        commp_id: c.player.get(),
        field_0x04: ObjectId::field(c.target).cast_unsigned(),
        field_0x08: c.point.x(),
        field_0x0c: c.point.y(),
        field_0x10: c.players,
        gap_0x11: [*first],
        field_0x12: *second,
        field_0x13: *third,
        rest: within(rest, packets::Flare::FIXED)?.to_vec(),
    }))
}

/// GAME (0x67).
fn game(c: &GameCommand) -> Packet {
    Packet::Game(packets::Game {
        commp_id: c.issuer.get(),
        field_0x04: c.sub,
        gap_0x06: [0; 2],
        field_0x08: c.player,
        field_0x0a: c.a,
        field_0x0c: c.speed,
        field_0x10: c.value,
    })
}
