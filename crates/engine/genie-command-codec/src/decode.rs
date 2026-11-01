//! A packet into its command: every field the packet holds, in the command's terms.

use crate::encode::encode;
use crate::names::unmodelled_command;
use genie_core::GenieError;
use genie_core::commands::QueueId;
use genie_core::commands::SpriteOverride;
use genie_core::commands::TradeLots;
use genie_core::commands::{
    AllBackToWorkCommand, AttackMoveCommand, AutoScoutCommand, BuildCommand, BuildWallCommand,
    BuildingQueueCommand, Command, DeleteCommand, FlareCommand, FormFormationCommand, GameCommand,
    GateCommand, GatherPointCommand, GoBackToWork2Command, GoBackToWorkCommand, Order,
    OrderCommand, PatrolCommand, QueueCommand, ResearchCommand, ResignCommand, SeekShelterCommand,
    SetGatherStateCommand, StopCommand, TownBellCommand, TradeCommand, TransformObjectCommand,
    TributeCommand, UnitAiStateCommand, UnitList, UnitOrderAction, UnitOrderCommand, UnloadCommand,
    UnloadPick, UnmodelledCommand, Waypoints,
};
use genie_core::map::Point;
use genie_core::map::Tile;
use genie_core::objects::ObjectId;
use genie_core::objects::masters::{ActionType, MasterId};
use genie_core::orders::{Interact, MoveOrder};
use genie_core::players::Amount;
use genie_core::players::PlayerId;
use genie_core::players::tribute::Gift;
use genie_core::technology::TechId;
use genie_rec::de::command::{self as packets, Command as Packet};

/// The command a packet holds.
///
/// A packet of a kind the engine does not run, one that fits no layout, one holding a value its
/// command's types refuse (a negative master, −1 for an object the command needs), and one
/// whose command would not write back its bytes ([`encode`]) stay raw: an
/// [`UnmodelledCommand`] whose run stops the issuing player.
///
/// # Errors
///
/// A packet too short for its player byte (byte 1).
pub fn decode(packet: &[u8]) -> Result<Command, GenieError> {
    let player = PlayerId::from(*packet.get(1).ok_or(GenieError::invalid_input(
        "a command packet without its player byte",
    ))?);
    let decoded = Packet::decode(packet);
    let command = match typed(&decoded) {
        Ok(Some(command)) => command,
        Ok(None) => {
            let why = GenieError::unsupported(unmodelled_command(&decoded));
            return Ok(raw(player, packet, why));
        }
        Err(why) => {
            let why = why.with("command", unmodelled_command(&decoded));
            return Ok(raw(player, packet, why));
        }
    };
    if encode(&command).is_ok_and(|bytes| bytes == packet) {
        return Ok(command);
    }
    Ok(raw(
        player,
        packet,
        GenieError::invalid_input("a command packet its command does not write back")
            .with("command", unmodelled_command(&decoded)),
    ))
}

/// The packet kept as it is, with why it does not run.
fn raw(player: PlayerId, packet: &[u8], why: GenieError) -> Command {
    Command::Unmodelled(UnmodelledCommand {
        player,
        packet: packet.to_vec(),
        why,
    })
}

/// A word's low short, which a handler reads where the packet holds a word (the masters).
const fn short(word: i32) -> i16 {
    let [low, high, ..] = word.to_le_bytes();
    i16::from_le_bytes([low, high])
}

/// A packet's unit list: its count and every id it holds, none for −1 (the runs read the first
/// `count`).
fn listed(count: u16, ids: &[i32]) -> UnitList {
    UnitList {
        count,
        ids: object_ids(ids),
    }
}

/// A packet's ids, none for −1.
fn object_ids(ids: &[i32]) -> Vec<Option<ObjectId>> {
    ids.iter().map(|&id| ObjectId::of_field(id)).collect()
}

/// The command of a packet the engine runs, None for any other.
fn typed(packet: &Packet) -> Result<Option<Command>, GenieError> {
    for kind in [orders, listed_command, building_command, single_command] {
        if let Some(command) = kind(packet)? {
            return Ok(Some(command));
        }
    }
    Ok(None)
}

/// INTERACT's layout, which WORK and RETREAT share: the target, the point and the flags.
macro_rules! interact {
    ($c:expr) => {
        Interact {
            target: ObjectId::of_field($c.t_id),
            point: Point::new($c.x, $c.y),
            carry_test: $c.field_0x12 != 0,
            queued: $c.field_0x13 != 0,
            direct: $c.field_0x14 != 0,
            human: $c.field_0x15 == 1,
            filter: ActionType::of_data($c.field_0x16.cast_signed()),
        }
    };
}

/// INTERACT, WORK, MOVE and RETREAT, whose units the orders' reader reads (`FUN_140e6b290`),
/// and FOLLOW and GUARD, which list theirs.
fn orders(packet: &Packet) -> Result<Option<Command>, GenieError> {
    let (player, units, order) = match packet {
        Packet::Interact(c) => (
            c.commp_id,
            listed(c.num, &c.ids),
            Order::Interact(interact!(c)),
        ),
        Packet::Work(c) => (c.commp_id, listed(c.num, &c.ids), Order::Work(interact!(c))),
        Packet::Retreat(c) => (
            c.commp_id,
            listed(c.num, &c.ids),
            Order::Retreat(interact!(c)),
        ),
        Packet::Move(c) => {
            let order = MoveOrder {
                target: ObjectId::of_field(c.t_id),
                point: Point::new(c.x, c.y),
                queued: c.field_0x13 != 0,
                direct: c.field_0x14 != 0,
                human: c.field_0x15 == 1,
                control: c.field_0x12 != 0,
            };
            (c.commp_id, listed(c.num, &c.ids), Order::Move(order))
        }
        Packet::Follow(c) => (
            c.commp_id,
            listed(c.num, &c.ids),
            Order::Follow(ObjectId::try_from(c.target)?),
        ),
        Packet::Guard(c) => (
            c.commp_id,
            listed(c.num, &c.ids),
            Order::Guard(ObjectId::try_from(c.target)?),
        ),
        _ => return Ok(None),
    };
    Ok(Some(Command::Order(OrderCommand {
        player: PlayerId::from(player),
        units,
        order,
    })))
}

/// The commands to the objects listed that do not go through the orders' reader.
fn listed_command(packet: &Packet) -> Result<Option<Command>, GenieError> {
    Ok(Some(match packet {
        Packet::Stop(c) => Command::Stop(StopCommand {
            player: PlayerId::from(c.commp_id),
            units: listed(c.num, &c.ids),
        }),
        Packet::UnitAiState(c) => Command::UnitAiState(UnitAiStateCommand {
            player: PlayerId::from(c.commp_id),
            units: listed(c.num, &c.ids),
            state: genie_core::unit_ai::UnitStance::of_byte(c.state),
        }),
        Packet::AutoScout(c) => Command::AutoScout(AutoScoutCommand {
            player: PlayerId::from(c.commp_id),
            units: listed(c.field_0x04, &c.ids),
        }),
        Packet::FormFormation(c) => Command::FormFormation(FormFormationCommand {
            player: PlayerId::from(c.commp_id),
            units: listed(c.num, &c.ids),
            formation: genie_core::groups::FormationKind::from(c.formation),
        }),
        Packet::SetGatherState(c) => Command::SetGatherState(SetGatherStateCommand {
            player: PlayerId::from(c.commp_id),
            units: listed(c.num, &c.ids),
            state: genie_core::commands::GatherPointState::of_byte(c.field_0x08),
        }),
        Packet::SeekShelter(c) => Command::SeekShelter(SeekShelterCommand {
            player: PlayerId::from(c.commp_id),
            units: listed(c.num, &c.ids),
        }),
        Packet::Patrol(c) => patrol(c),
        Packet::AttackMove(c) => attack_move(c),
        Packet::Unload(c) => Command::Unload(UnloadCommand {
            player: PlayerId::from(c.commp_id),
            units: listed(c.num, &c.ids),
            point: Point::new(c.x, c.y),
            pick: UnloadPick::of_packet(c.flag, c.unit_type),
            sound: c.field_0x18 != 0,
        }),
        Packet::UnitOrder(c) => Command::UnitOrder(UnitOrderCommand {
            player: PlayerId::from(c.commp_id),
            units: listed(c.num, &c.ids),
            target: ObjectId::of_field(c.t_id),
            point: Point::new(c.x, c.y),
            q_id: QueueId::of_data(c.q_id),
            through_ai: c.field_0x1d != 0,
            nearby: c.field_0x1e != 0,
            action: UnitOrderAction::of(c.action),
            param: c.param.cast_unsigned(),
            human: c.field_0x20 == 1,
        }),
        Packet::GoBackToWork2(c) => {
            let Some(units) = go_back_to_work_units(&c.rest) else {
                return Ok(None);
            };
            Command::GoBackToWork2(GoBackToWork2Command {
                player: PlayerId::from(c.commp_id),
                units,
            })
        }
        _ => return Ok(None),
    }))
}

/// PATROL (0x15): the units, the waypoints' count (`+0xb`) and slots (x from `+0xc`, each an
/// f32's bits; y from `+0x34`) and `+0x8`.
fn patrol(c: &packets::Patrol) -> Command {
    let xs = [
        c.field_0x0c,
        c.field_0x10,
        c.field_0x14,
        c.field_0x18,
        c.field_0x1c,
        c.field_0x20,
        c.field_0x24,
        c.field_0x28,
        c.field_0x2c,
        c.field_0x30,
    ];
    let ys = [
        c.field_0x34,
        c.field_0x38,
        c.field_0x3c,
        c.field_0x40,
        c.field_0x44,
        c.field_0x48,
        c.field_0x4c,
        c.field_0x50,
        c.field_0x54,
        c.field_0x58,
    ];
    Command::Patrol(PatrolCommand {
        player: PlayerId::from(c.commp_id),
        units: listed(c.num, &c.ids),
        waypoints: Waypoints {
            count: c.waypoints_count,
            points: std::array::from_fn(|i| Point::new(f32::from_bits(xs[i]), ys[i])),
        },
        reset: c.field_0x08 != 0,
    })
}

/// `ATTACK_MOVE` (0x21): PATROL's layout, the first x as an f32's bits and nine more from
/// `+0x10`, the first y at `+0x34` and nine more from `+0x38`; its flags at `+0x8`..`+0xa`.
fn attack_move(c: &packets::AttackMove) -> Command {
    let word = |b: &[u8; 36], i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let points = std::array::from_fn(|i| match i {
        0 => Point::new(f32::from_bits(c.field_0x0c), c.field_0x34),
        _ => Point::new(
            word(&c.gap_0x10, 4 * (i - 1)),
            word(&c.gap_0x38, 4 * (i - 1)),
        ),
    });
    Command::AttackMove(AttackMoveCommand {
        player: PlayerId::from(c.commp_id),
        units: listed(c.num, &c.ids),
        waypoints: Waypoints {
            count: c.waypoints_count,
            points,
        },
        reset: c.field_0x08 != 0,
        queued: c.field_0x09 != 0,
        ignore_pending: c.field_0x0a != 0,
    })
}

/// `GO_BACK_TO_WORK2`'s units: the count is the short at the start of `rest` (+4), the ids
/// follow from +8 (the last bytes of a tail not of whole ids are none, so its command does not
/// write them back); None for a tail too short for the count and the two bytes after it.
fn go_back_to_work_units(rest: &[u8]) -> Option<UnitList> {
    let count = u16::from_le_bytes([*rest.first()?, *rest.get(1)?]);
    let (ids, _) = rest.get(4..)?.as_chunks::<4>();
    let ids: Vec<i32> = ids.iter().map(|b| i32::from_le_bytes(*b)).collect();
    Some(listed(count, &ids))
}

/// The commands that place or queue at buildings: BUILD, `BUILD_WALL`, QUEUE, RESEARCH, SET
/// GATHER POINT and `BUILDING_COMMAND_QUEUE`.
fn building_command(packet: &Packet) -> Result<Option<Command>, GenieError> {
    Ok(Some(match packet {
        Packet::Build(c) => Command::Build(BuildCommand {
            player: PlayerId::from(c.commp_id),
            master: MasterId::try_from(short(c.o_id))?,
            position: Point::new(c.x, c.y),
            q_id: QueueId::of_data(c.q_id),
            sprite_override: SpriteOverride::of_data(c.field_0x18.cast_signed()),
            builders: listed(c.num, &c.ids),
            queued: c.field_0x1e != 0,
            commits: c.field_0x1f == 0,
            frame: genie_core::objects::Frame::of_byte(c.frame),
        }),
        Packet::BuildWall(c) => Command::BuildWall(BuildWallCommand {
            player: PlayerId::from(c.commp_id),
            master: MasterId::try_from(short(c.o_id))?,
            from: Tile::new(i32::from(c.x1), i32::from(c.y1)),
            to: Tile::new(i32::from(c.x2), i32::from(c.y2)),
            q_id: QueueId::of_data(c.q_id),
            builders: listed(c.num, &c.ids),
            queued: c.field_0x1a != 0,
            commits: c.field_0x1b == 0,
        }),
        Packet::MultiQueue(c) => Command::Queue(QueueCommand {
            player: PlayerId::from(c.commp_id),
            buildings: listed(c.num, &c.ids),
            building_master: MasterId::try_from(c.b_id)?,
            unit: MasterId::try_from(c.t_id)?,
            amount: c.count,
            button: genie_core::production::QueueButton::of_data(c.button_location_id),
        }),
        Packet::Research(c) => Command::Research(ResearchCommand {
            player: PlayerId::from(c.commp_id),
            building: ObjectId::of_field(c.u_id),
            count: c.field_0x08.cast_signed(),
            buildings: object_ids(&c.ids),
            tech: TechId::of_data(c.t_id),
            q_id: QueueId::of_data(c.q_id),
            shift: c.field_0x10 != 0,
        }),
        Packet::SetGatherPoint(c) => Command::GatherPoint(GatherPointCommand {
            player: PlayerId::from(c.commp_id),
            units: listed(c.num, &c.ids),
            point: Point::new(c.x, c.y),
            target: ObjectId::of_field(c.t_id),
            target_master: MasterId::of_field(short(c.tm_id))?,
            own: c.field_0x18 != 0,
        }),
        Packet::BuildingCommandQueue(c) => Command::CommandQueue(BuildingQueueCommand {
            player: PlayerId::from(c.commp_id),
            units: listed(c.num, &c.ids),
            target: ObjectId::of_field(c.t_id),
            point: Point::new(c.x, c.y),
        }),
        _ => return Ok(None),
    }))
}

/// The commands of one object, one player or the game.
fn single_command(packet: &Packet) -> Result<Option<Command>, GenieError> {
    Ok(Some(match packet {
        Packet::Resign(c) => Command::Resign(ResignCommand {
            player: PlayerId::from(c.commp_id),
            dropped: c.dropped != 0,
        }),
        Packet::TransformObject(c) => Command::TransformObject(TransformObjectCommand {
            player: PlayerId::from(c.commp_id),
            id: ObjectId::try_from(c.field_0x08.cast_signed())?,
            master: MasterId::try_from(short(c.field_0x04))?,
        }),
        Packet::CancelBuild(c) => Command::Delete(DeleteCommand {
            player: PlayerId::from(c.commp_id),
            id: ObjectId::try_from(c.u_id)?,
        }),
        Packet::GoBackToWork(c) => Command::GoBackToWork(GoBackToWorkCommand {
            player: PlayerId::from(c.commp_id),
            id: ObjectId::try_from(c.u_id)?,
        }),
        Packet::AllBackToWork(c) => Command::AllBackToWork(AllBackToWorkCommand {
            player: PlayerId::from(c.commp_id),
            id: ObjectId::try_from(c.u_id)?,
        }),
        Packet::TownBell(c) => Command::TownBell(TownBellCommand {
            player: PlayerId::from(c.commp_id),
            id: ObjectId::try_from(c.u_id)?,
            activate: c.activate != 0,
        }),
        Packet::Gate(c) => Command::Gate(GateCommand {
            player: PlayerId::from(c.commp_id),
            id: ObjectId::try_from(c.u_id)?,
        }),
        Packet::TribeGiveAttributes(c) => tribute(c),
        Packet::SellCommodity(c) => trade(c.commp_id, c.attr, c.amt, c.u_id, false),
        Packet::BuyCommodity(c) => trade(c.commp_id, c.attr, c.amt, c.u_id, true),
        Packet::Flare(c) => Command::Flare(FlareCommand {
            player: PlayerId::from(c.commp_id),
            target: ObjectId::of_field(c.field_0x04.cast_signed()),
            point: Point::new(c.field_0x08, c.field_0x0c),
            players: c.field_0x10,
            shown: [c.gap_0x11[0], c.field_0x12, c.field_0x13]
                .iter()
                .chain(&c.rest)
                .map(|&b| b != 0)
                .collect(),
        }),
        Packet::Game(g) => Command::Game(GameCommand {
            issuer: PlayerId::from(g.commp_id),
            sub: g.field_0x04,
            player: g.field_0x08,
            a: g.field_0x0a,
            speed: g.field_0x0c,
            value: g.field_0x10,
        }),
        _ => return Ok(None),
    }))
}

/// `SELL_COMMODITY` (0x7a) or `BUY_COMMODITY` (0x7b): the attribute (`+0x4`), the lots
/// (`+0x6`) and the market (`+0x8`).
fn trade(player: u8, attribute: u16, times: u16, market: i32, buy: bool) -> Command {
    Command::Trade(TradeCommand {
        player: PlayerId::from(player),
        attribute,
        times: TradeLots::of_data(times),
        market: ObjectId::of_field(market),
        buy,
    })
}

/// GIVE ATTRIBUTES (0xc4): the four entries' attributes (`+0x24`..), amounts (`+0x04`..) and
/// fees (`+0x14`..), the receiver at `+0x2c`.
fn tribute(c: &packets::TribeGiveAttributes) -> Command {
    let gift = |attribute, amount, fee| Gift {
        attribute,
        amount,
        fee,
    };
    Command::Tribute(TributeCommand {
        player: PlayerId::from(c.commp_id),
        to: PlayerId::from(c.to),
        gifts: [
            gift(c.attr, Amount::new(c.amt), c.cost),
            gift(c.field_0x26, Amount::new(c.field_0x08), c.field_0x18),
            gift(c.field_0x28, Amount::new(c.field_0x0c), c.field_0x1c),
            gift(c.field_0x2a, Amount::new(c.field_0x10), c.field_0x20),
        ],
    })
}
