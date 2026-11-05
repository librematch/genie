//! A command into its packet: genie-rec's layout of the command's kind, filled from the command.

use genie_core::commands::Departure;
use genie_core::commands::GatherPointState;
use genie_core::commands::{
    AttackMoveCommand, BuildCommand, BuildWallCommand, Command, FlareCommand, GameCommand, Order,
    OrderCommand, PatrolCommand, QueueCommand, ResearchCommand, TradeCommand, TributeCommand,
    UnitList, UnitOrderCommand, UnloadCommand,
};
use genie_core::map::Axis;
use genie_core::objects::ObjectId;
use genie_core::objects::masters::{ActionKind, MasterId};
use genie_core::orders::OrderFlags;
use genie_core::orders::SavedPatrols;
use genie_core::technology::TechId;
use genie_core::{GenieError, GenieResult};
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
pub fn encode(command: &Command) -> GenieResult<Vec<u8>> {
    match command {
        Command::Unmodelled(unmodelled_command) => Ok(unmodelled_command.packet.clone()),
        other => Ok(packet(other)?.encode()?),
    }
}

/// The largest packet: its size after the 4-byte header is a u16 (`+0x2`).
const MOST_BYTES: usize = 4 + u16::MAX as usize;

/// A tail of `fixed` bytes' layout, if its packet's size holds it.
fn within<T>(tail: &[T], fixed: usize) -> GenieResult<&[T]> {
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
fn ids(ids: &[Option<ObjectId>], fixed: usize) -> GenieResult<Vec<i32>> {
    let raw: Vec<i32> = ids.iter().map(|&id| ObjectId::field(id)).collect();
    let _ = within(&raw, fixed)?;
    Ok(raw)
}

/// A command's layout in genie-rec.
fn packet(command: &Command) -> GenieResult<Packet> {
    match command {
        Command::Order(order_command) => order(order_command),
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
fn building(command: &Command) -> GenieResult<Packet> {
    Ok(match command {
        Command::Build(build_command) => build(build_command)?,
        Command::BuildWall(build_wall_command) => build_wall(build_wall_command)?,
        Command::Queue(queue_command) => queue(queue_command)?,
        Command::Research(research_command) => research(research_command)?,
        Command::GatherPoint(gather_point_command) => {
            Packet::SetGatherPoint(packets::SetGatherPoint {
                commp_id: gather_point_command.player.get(),
                num: gather_point_command.units.count,
                gap_0x06: [0; 2],
                x: gather_point_command.point.x(),
                y: gather_point_command.point.y(),
                tid: ObjectId::field(gather_point_command.target),
                tm_id: i32::from(MasterId::field(gather_point_command.target_master)),
                field_0x18: u8::from(gather_point_command.own),
                ids: ids(
                    &gather_point_command.units.ids,
                    packets::SetGatherPoint::FIXED,
                )?,
            })
        }
        Command::CommandQueue(building_queue_command) => {
            Packet::BuildingCommandQueue(packets::BuildingCommandQueue {
                commp_id: building_queue_command.player.get(),
                tid: ObjectId::field(building_queue_command.target),
                x: building_queue_command.point.x(),
                y: building_queue_command.point.y(),
                num: building_queue_command.units.count,
                gap_0x12: COMMAND_QUEUE_GAP,
                ids: ids(
                    &building_queue_command.units.ids,
                    packets::BuildingCommandQueue::FIXED,
                )?,
            })
        }
        other => return elsewhere(other),
    })
}

/// The commands to the objects listed that do not go through the orders' reader.
fn listed(command: &Command) -> GenieResult<Packet> {
    Ok(match command {
        Command::Stop(stop_command) => Packet::Stop(packets::Stop {
            commp_id: stop_command.player.get(),
            num: stop_command.units.count,
            gap_0x06: [0; 2],
            ids: ids(&stop_command.units.ids, packets::Stop::FIXED)?,
        }),
        Command::UnitAiState(unit_ai_state_command) => Packet::UnitAiState(packets::UnitAiState {
            commp_id: unit_ai_state_command.player.get(),
            num: unit_ai_state_command.units.count,
            gap_0x06: [0; 2],
            state: unit_ai_state_command.state.get(),
            gap_0x09: [0; 3],
            ids: ids(
                &unit_ai_state_command.units.ids,
                packets::UnitAiState::FIXED,
            )?,
        }),
        Command::AutoScout(auto_scout_command) => Packet::AutoScout(packets::AutoScout {
            commp_id: auto_scout_command.player.get(),
            field_0x04: auto_scout_command.units.count,
            gap_0x06: [0; 2],
            ids: ids(&auto_scout_command.units.ids, packets::AutoScout::FIXED)?,
        }),
        Command::FormFormation(form_formation_command) => {
            Packet::FormFormation(packets::FormFormation {
                commp_id: form_formation_command.player.get(),
                num: form_formation_command.units.count,
                gap_0x06: [0; 2],
                formation: form_formation_command.formation.get(),
                ids: ids(
                    &form_formation_command.units.ids,
                    packets::FormFormation::FIXED,
                )?,
            })
        }
        Command::SetGatherState(set_gather_state_command) => {
            Packet::SetGatherState(packets::SetGatherState {
                commp_id: set_gather_state_command.player.get(),
                num: set_gather_state_command.units.count,
                gap_0x06: [0; 2],
                field_0x08: set_gather_state_command
                    .state
                    .map_or(0, GatherPointState::get),
                ids: ids(
                    &set_gather_state_command.units.ids,
                    packets::SetGatherState::FIXED,
                )?,
            })
        }
        Command::SeekShelter(seek_shelter_command) => Packet::SeekShelter(packets::SeekShelter {
            commp_id: seek_shelter_command.player.get(),
            num: seek_shelter_command.units.count,
            gap_0x06: [0; 2],
            ids: ids(&seek_shelter_command.units.ids, packets::SeekShelter::FIXED)?,
        }),
        Command::Patrol(patrol_command) => patrol(patrol_command)?,
        Command::AttackMove(attack_move_command) => attack_move(attack_move_command)?,
        Command::Unload(unload_command) => unload(unload_command)?,
        Command::UnitOrder(unit_order_command) => unit_order(unit_order_command)?,
        Command::GoBackToWork2(go_back_to_work2_command) => {
            Packet::GoBackToWork2(packets::GoBackToWork2 {
                commp_id: go_back_to_work2_command.player.get(),
                rest: go_back_to_work_rest(&go_back_to_work2_command.units)?,
            })
        }
        other => return elsewhere(other),
    })
}

/// A command another group's function writes, or an unmodelled one, which keeps its own bytes
/// ([`encode`]): [`packet`] never passes either here.
fn elsewhere(command: &Command) -> GenieResult<Packet> {
    Err(GenieError::unreachable("a command another arm writes")
        .with("command", format!("{command:?}")))
}

/// The commands of one object, one player or the game.
fn single(command: &Command) -> GenieResult<Packet> {
    Ok(match command {
        Command::Resign(resign_command) => Packet::Resign(packets::Resign {
            commp_id: resign_command.player.get(),
            dropped: u8::from(resign_command.departure == Departure::Dropped),
        }),
        Command::TransformObject(transform_object_command) => {
            Packet::TransformObject(packets::TransformObject {
                commp_id: transform_object_command.player.get(),
                field_0x04: i32::from(transform_object_command.master.get()),
                field_0x08: transform_object_command.id.get().cast_unsigned(),
            })
        }
        Command::Delete(delete_command) => Packet::CancelBuild(packets::CancelBuild {
            commp_id: delete_command.player.get(),
            uid: delete_command.id.get(),
        }),
        Command::GoBackToWork(go_back_to_work_command) => {
            Packet::GoBackToWork(packets::GoBackToWork {
                commp_id: go_back_to_work_command.player.get(),
                uid: go_back_to_work_command.id.get(),
            })
        }
        Command::AllBackToWork(all_back_to_work_command) => {
            Packet::AllBackToWork(packets::AllBackToWork {
                commp_id: all_back_to_work_command.player.get(),
                uid: all_back_to_work_command.id.get(),
            })
        }
        Command::TownBell(town_bell_command) => Packet::TownBell(packets::TownBell {
            commp_id: town_bell_command.player.get(),
            uid: town_bell_command.id.get(),
            activate: town_bell_command.bell.byte(),
        }),
        Command::Gate(gate_command) => Packet::Gate(packets::Gate {
            commp_id: gate_command.player.get(),
            uid: gate_command.id.get(),
        }),
        Command::Tribute(tribute_command) => tribute(tribute_command),
        Command::Trade(trade_command) => trade(trade_command),
        Command::Flare(flare_command) => flare(flare_command)?,
        Command::Game(game_command) => game(game_command),
        other => return elsewhere(other),
    })
}

/// INTERACT's layout of an order, which WORK and RETREAT share.
macro_rules! interact_packet {
    ($kind:ident, $player:expr, $units:expr, $o:expr) => {
        Packet::$kind(packets::$kind {
            commp_id: $player,
            tid: ObjectId::field($o.target),
            x: $o.point.x(),
            y: $o.point.y(),
            num: $units.count,
            field_0x12: u8::from($o.disable_retargeting),
            field_0x13: $o.flags.byte(OrderFlags::QUEUED),
            field_0x14: $o.flags.byte(OrderFlags::DIRECT),
            field_0x15: $o.flags.byte(OrderFlags::HUMAN),
            field_0x16: ActionKind::field($o.filter).cast_unsigned(),
            ids: ids(&$units.ids, packets::$kind::FIXED)?,
        })
    };
}

/// INTERACT, WORK, MOVE, RETREAT, FOLLOW and GUARD.
fn order(order_command: &OrderCommand) -> GenieResult<Packet> {
    let (player, units) = (order_command.player.get(), &order_command.units);
    Ok(match order_command.order {
        Order::Interact(interact) => interact_packet!(Interact, player, units, interact),
        Order::Work(order) => interact_packet!(Work, player, units, order),
        Order::Retreat(interact) => interact_packet!(Retreat, player, units, interact),
        Order::Move(move_order) => Packet::Move(packets::Move {
            commp_id: player,
            tid: ObjectId::field(move_order.target),
            x: move_order.point.x(),
            y: move_order.point.y(),
            num: units.count,
            field_0x12: move_order.control.byte(),
            field_0x13: move_order.flags.byte(OrderFlags::QUEUED),
            field_0x14: move_order.flags.byte(OrderFlags::DIRECT),
            field_0x15: move_order.flags.byte(OrderFlags::HUMAN),
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
fn build(build_command: &BuildCommand) -> GenieResult<Packet> {
    Ok(Packet::Build(packets::Build {
        commp_id: build_command.player.get(),
        num: build_command.builders.count,
        gap_0x06: [0; 2],
        x: build_command.position.x(),
        y: build_command.position.y(),
        oid: i32::from(build_command.master.get()),
        qid: build_command.queue_id.get(),
        field_0x18: build_command.sprite_override.get().cast_unsigned(),
        frame: build_command.frame.byte(),
        gap_0x1d: [0; 1],
        field_0x1e: u8::from(build_command.queued),
        field_0x1f: u8::from(!build_command.commits),
        ids: ids(&build_command.builders.ids, packets::Build::FIXED)?,
    }))
}

/// A wall's tile coordinate as the packet's word (`BUILD_WALL`'s `+0x8`..`+0xe`).
///
/// # Errors
///
/// A coordinate outside a word.
fn tile_word(value: i32) -> GenieResult<u16> {
    u16::try_from(value).map_err(|_| {
        GenieError::invalid_input("a wall tile outside the packet's word").with("coordinate", value)
    })
}

/// `BUILD_WALL` (0x69).
fn build_wall(build_wall_command: &BuildWallCommand) -> GenieResult<Packet> {
    Ok(Packet::BuildWall(packets::BuildWall {
        commp_id: build_wall_command.player.get(),
        num: build_wall_command.builders.count,
        gap_0x06: [0; 2],
        x1: tile_word(build_wall_command.from.x())?,
        y1: tile_word(build_wall_command.from.y())?,
        x2: tile_word(build_wall_command.to.x())?,
        y2: tile_word(build_wall_command.to.y())?,
        oid: i32::from(build_wall_command.master.get()),
        qid: build_wall_command.queue_id.get(),
        gap_0x18: [0; 2],
        field_0x1a: u8::from(build_wall_command.queued),
        field_0x1b: u8::from(!build_wall_command.commits),
        ids: ids(&build_wall_command.builders.ids, packets::BuildWall::FIXED)?,
    }))
}

/// QUEUE (0x81, `MULTI_QUEUE`).
fn queue(queue_command: &QueueCommand) -> GenieResult<Packet> {
    Ok(Packet::MultiQueue(packets::MultiQueue {
        commp_id: queue_command.player.get(),
        num: queue_command.buildings.count,
        gap_0x06: [0; 4],
        bid: queue_command.building_master.get(),
        tid: queue_command.unit.get(),
        count: queue_command.amount.get(),
        button_location_id: queue_command.button.get(),
        ids: ids(&queue_command.buildings.ids, packets::MultiQueue::FIXED)?,
    }))
}

/// RESEARCH (0x65).
fn research(research_command: &ResearchCommand) -> GenieResult<Packet> {
    Ok(Packet::Research(packets::Research {
        commp_id: research_command.player.get(),
        uid: ObjectId::field(research_command.building),
        field_0x08: research_command.count.cast_unsigned(),
        tid: TechId::field(research_command.tech),
        qid: research_command.queue_id.get(),
        field_0x10: u8::from(research_command.shift),
        ids: ids(&research_command.buildings, packets::Research::FIXED)?,
    }))
}

/// PATROL (0x15): x as each slot's f32 bits.
fn patrol(patrol_command: &PatrolCommand) -> GenieResult<Packet> {
    let x = |index: usize| patrol_command.waypoints.points[index].x().to_bits();
    let y = |index: usize| patrol_command.waypoints.points[index].y();
    Ok(Packet::Patrol(packets::Patrol {
        commp_id: patrol_command.player.get(),
        num: patrol_command.units.count,
        gap_0x06: [0; 2],
        field_0x08: u8::from(patrol_command.patrols == SavedPatrols::Dropped),
        gap_0x09: [0; 2],
        waypoints_count: patrol_command.waypoints.count,
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
        ids: ids(&patrol_command.units.ids, packets::Patrol::FIXED)?,
    }))
}

/// `ATTACK_MOVE` (0x21): the first x as its f32 bits, the other slots' x and y as bytes.
fn attack_move(attack_move_command: &AttackMoveCommand) -> GenieResult<Packet> {
    let points = &attack_move_command.waypoints.points;
    let tail = |axis: Axis| -> [u8; 36] {
        let mut out = [0; 36];
        for (index, point) in points[1..].iter().enumerate() {
            out[4 * index..4 * index + 4].copy_from_slice(&point.along(axis).to_le_bytes());
        }
        out
    };
    Ok(Packet::AttackMove(packets::AttackMove {
        commp_id: attack_move_command.player.get(),
        num: attack_move_command.units.count,
        gap_0x06: [0; 2],
        field_0x08: u8::from(attack_move_command.patrols == SavedPatrols::Dropped),
        field_0x09: u8::from(attack_move_command.queued),
        field_0x0a: u8::from(attack_move_command.ignore_pending),
        waypoints_count: attack_move_command.waypoints.count,
        field_0x0c: points[0].x().to_bits(),
        gap_0x10: tail(Axis::X),
        field_0x34: points[0].y(),
        gap_0x38: tail(Axis::Y),
        ids: ids(&attack_move_command.units.ids, packets::AttackMove::FIXED)?,
    }))
}

/// UNLOAD (0x6f).
fn unload(unload_command: &UnloadCommand) -> GenieResult<Packet> {
    let (flag, unit_kind) = unload_command.pick.packet();
    Ok(Packet::Unload(packets::Unload {
        commp_id: unload_command.player.get(),
        num: unload_command.units.count,
        gap_0x06: [0; 2],
        x: unload_command.point.x(),
        y: unload_command.point.y(),
        unit_kind,
        flag,
        gap_0x15: [0; 3],
        field_0x18: u8::from(unload_command.sound),
        ids: ids(&unload_command.units.ids, packets::Unload::FIXED)?,
    }))
}

/// `UNIT_ORDER` (0x75).
fn unit_order(unit_order_command: &UnitOrderCommand) -> GenieResult<Packet> {
    Ok(Packet::UnitOrder(packets::UnitOrder {
        commp_id: unit_order_command.player.get(),
        num: unit_order_command.units.count,
        gap_0x06: [0; 2],
        tid: ObjectId::field(unit_order_command.target),
        x: unit_order_command.point.x(),
        y: unit_order_command.point.y(),
        qid: unit_order_command.queue_id.get(),
        param: unit_order_command.param.get().cast_signed(),
        action: unit_order_command.action.byte(),
        field_0x1d: unit_order_command.shift.byte(),
        field_0x1e: unit_order_command.scope.byte(),
        gap_0x1f: UNIT_ORDER_GAP,
        field_0x20: u8::from(unit_order_command.human),
        ids: ids(&unit_order_command.units.ids, packets::UnitOrder::FIXED)?,
    }))
}

/// `GO_BACK_TO_WORK2`'s tail: the count (+4), two bytes no code reads, the ids from +8.
fn go_back_to_work_rest(units: &UnitList) -> GenieResult<Vec<u8>> {
    let mut rest = units.count.to_le_bytes().to_vec();
    rest.extend([0; 2]);
    ids(&units.ids, packets::GoBackToWork2::FIXED + 4)?
        .iter()
        .for_each(|id| rest.extend(id.to_le_bytes()));
    Ok(rest)
}

/// GIVE ATTRIBUTES (0xc4).
fn tribute(tribute_command: &TributeCommand) -> Packet {
    let [first, second, third, fourth] = &tribute_command.gifts;
    Packet::TribeGiveAttributes(packets::TribeGiveAttributes {
        commp_id: tribute_command.player.get(),
        amt: first.amount.get(),
        field_0x08: second.amount.get(),
        field_0x0c: third.amount.get(),
        field_0x10: fourth.amount.get(),
        cost: first.fee.get(),
        field_0x18: second.fee.get(),
        field_0x1c: third.fee.get(),
        field_0x20: fourth.fee.get(),
        attr: first.attribute.get(),
        field_0x26: second.attribute.get(),
        field_0x28: third.attribute.get(),
        field_0x2a: fourth.attribute.get(),
        to: tribute_command.to.get(),
    })
}

/// `SELL_COMMODITY` (0x7a) or `BUY_COMMODITY` (0x7b).
fn trade(trade_command: &TradeCommand) -> Packet {
    let player = trade_command.player.get();
    if trade_command.buy {
        Packet::BuyCommodity(packets::BuyCommodity {
            commp_id: player,
            attr: trade_command.attribute.get(),
            amt: trade_command.times.get(),
            uid: ObjectId::field(trade_command.market),
        })
    } else {
        Packet::SellCommodity(packets::SellCommodity {
            commp_id: player,
            attr: trade_command.attribute.get(),
            amt: trade_command.times.get(),
            uid: ObjectId::field(trade_command.market),
        })
    }
}

/// FLARE (0x73): the players' bytes from `+0x11`, of which the layout's fixed part holds three.
fn flare(flare_command: &FlareCommand) -> GenieResult<Packet> {
    let shown: Vec<u8> = flare_command
        .shown
        .iter()
        .map(|&shown| u8::from(shown))
        .collect();
    let [first, second, third, rest @ ..] = shown.as_slice() else {
        return Err(GenieError::invalid_input(
            "a FLARE's packet holds three players' bytes at least",
        )
        .with("players", flare_command.shown.len()));
    };
    Ok(Packet::Flare(packets::Flare {
        commp_id: flare_command.player.get(),
        field_0x04: ObjectId::field(flare_command.target).cast_unsigned(),
        field_0x08: flare_command.point.x(),
        field_0x0c: flare_command.point.y(),
        field_0x10: flare_command.players,
        gap_0x11: [*first],
        field_0x12: *second,
        field_0x13: *third,
        rest: within(rest, packets::Flare::FIXED)?.to_vec(),
    }))
}

/// GAME (0x67).
fn game(game_command: &GameCommand) -> Packet {
    Packet::Game(packets::Game {
        commp_id: game_command.issuer.get(),
        field_0x04: game_command.sub.get(),
        gap_0x06: [0; 2],
        field_0x08: game_command.player,
        field_0x0a: game_command.argument,
        field_0x0c: game_command.speed,
        field_0x10: game_command.value,
    })
}
