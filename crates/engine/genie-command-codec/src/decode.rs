//! A packet into its command: every field the packet holds, in the command's terms.

use crate::encode::encode;
use crate::names::unmodelled_command;
use genie_core::commands::Bell;
use genie_core::commands::Departure;
use genie_core::commands::GameSub;
use genie_core::commands::GatherPointState;
use genie_core::commands::QueueId;
use genie_core::commands::ResourceWord;
use genie_core::commands::Shift;
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
use genie_core::garrison::GarrisonScope;
use genie_core::groups::FormationKind;
use genie_core::map::Point;
use genie_core::map::Tile;
use genie_core::objects::Frame;
use genie_core::objects::ObjectId;
use genie_core::objects::masters::{ActionKind, MasterId};
use genie_core::orders::OrderFlags;
use genie_core::orders::SavedPatrols;
use genie_core::orders::{ControlKey, Interact, MoveOrder};
use genie_core::players::Amount;
use genie_core::players::PlayerId;
use genie_core::players::TributeFee;
use genie_core::players::tribute::Gift;
use genie_core::production::{QueueButton, QueuedUnits, UnitIndex};
use genie_core::technology::TechId;
use genie_core::unit_ai::UnitStance;
use genie_core::{GenieError, GenieResult};
use genie_rec::de::command::{self as packets, Command as Packet};
use std::array::from_fn;

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
pub fn decode(packet: &[u8]) -> GenieResult<Command> {
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
fn typed(packet: &Packet) -> GenieResult<Option<Command>> {
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
            target: ObjectId::of_field($c.tid),
            point: Point::new($c.x, $c.y),
            carry_test: $c.field_0x12 != 0,
            flags: OrderFlags::of_bytes($c.field_0x13, $c.field_0x14, $c.field_0x15),
            filter: ActionKind::of_data($c.field_0x16.cast_signed()),
        }
    };
}

/// INTERACT, WORK, MOVE and RETREAT, whose units the orders' reader reads (`FUN_140e6b290`),
/// and FOLLOW and GUARD, which list theirs.
fn orders(packet: &Packet) -> GenieResult<Option<Command>> {
    let (player, units, order) = match packet {
        Packet::Interact(interact) => (
            interact.commp_id,
            listed(interact.num, &interact.ids),
            Order::Interact(interact!(interact)),
        ),
        Packet::Work(work) => (
            work.commp_id,
            listed(work.num, &work.ids),
            Order::Work(interact!(work)),
        ),
        Packet::Retreat(retreat) => (
            retreat.commp_id,
            listed(retreat.num, &retreat.ids),
            Order::Retreat(interact!(retreat)),
        ),
        Packet::Move(c) => {
            let order = MoveOrder {
                target: ObjectId::of_field(c.tid),
                point: Point::new(c.x, c.y),
                flags: OrderFlags::of_bytes(c.field_0x13, c.field_0x14, c.field_0x15),
                control: ControlKey::of_byte(c.field_0x12),
            };
            (c.commp_id, listed(c.num, &c.ids), Order::Move(order))
        }
        Packet::Follow(follow) => (
            follow.commp_id,
            listed(follow.num, &follow.ids),
            Order::Follow(ObjectId::try_from(follow.target)?),
        ),
        Packet::Guard(guard) => (
            guard.commp_id,
            listed(guard.num, &guard.ids),
            Order::Guard(ObjectId::try_from(guard.target)?),
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
fn listed_command(packet: &Packet) -> GenieResult<Option<Command>> {
    Ok(Some(match packet {
        Packet::Stop(stop) => Command::Stop(StopCommand {
            player: PlayerId::from(stop.commp_id),
            units: listed(stop.num, &stop.ids),
        }),
        Packet::UnitAiState(unit_ai_state) => Command::UnitAiState(UnitAiStateCommand {
            player: PlayerId::from(unit_ai_state.commp_id),
            units: listed(unit_ai_state.num, &unit_ai_state.ids),
            state: UnitStance::of_byte(unit_ai_state.state),
        }),
        Packet::AutoScout(auto_scout) => Command::AutoScout(AutoScoutCommand {
            player: PlayerId::from(auto_scout.commp_id),
            units: listed(auto_scout.field_0x04, &auto_scout.ids),
        }),
        Packet::FormFormation(form_formation) => Command::FormFormation(FormFormationCommand {
            player: PlayerId::from(form_formation.commp_id),
            units: listed(form_formation.num, &form_formation.ids),
            formation: FormationKind::from(form_formation.formation),
        }),
        Packet::SetGatherState(set_gather_state) => {
            Command::SetGatherState(SetGatherStateCommand {
                player: PlayerId::from(set_gather_state.commp_id),
                units: listed(set_gather_state.num, &set_gather_state.ids),
                state: GatherPointState::of_byte(set_gather_state.field_0x08),
            })
        }
        Packet::SeekShelter(seek_shelter) => Command::SeekShelter(SeekShelterCommand {
            player: PlayerId::from(seek_shelter.commp_id),
            units: listed(seek_shelter.num, &seek_shelter.ids),
        }),
        Packet::Patrol(c) => patrol(c),
        Packet::AttackMove(c) => attack_move(c),
        Packet::Unload(unload) => Command::Unload(UnloadCommand {
            player: PlayerId::from(unload.commp_id),
            units: listed(unload.num, &unload.ids),
            point: Point::new(unload.x, unload.y),
            pick: UnloadPick::of_packet(unload.flag, unload.unit_kind),
            sound: unload.field_0x18 != 0,
        }),
        Packet::UnitOrder(unit_order) => Command::UnitOrder(UnitOrderCommand {
            player: PlayerId::from(unit_order.commp_id),
            units: listed(unit_order.num, &unit_order.ids),
            target: ObjectId::of_field(unit_order.tid),
            point: Point::new(unit_order.x, unit_order.y),
            queue_id: QueueId::of_data(unit_order.qid),
            shift: Shift::of_byte(unit_order.field_0x1d),
            scope: GarrisonScope::of_byte(unit_order.field_0x1e),
            action: UnitOrderAction::of(unit_order.action),
            param: UnitIndex::of_data(unit_order.param.cast_unsigned()),
            human: unit_order.field_0x20 == 1,
        }),
        Packet::GoBackToWork2(go_back_to_work2) => {
            let Some(units) = go_back_to_work_units(&go_back_to_work2.rest) else {
                return Ok(None);
            };
            Command::GoBackToWork2(GoBackToWork2Command {
                player: PlayerId::from(go_back_to_work2.commp_id),
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
            points: from_fn(|i| Point::new(f32::from_bits(xs[i]), ys[i])),
        },
        patrols: if c.field_0x08 != 0 {
            SavedPatrols::Dropped
        } else {
            SavedPatrols::Kept
        },
    })
}

/// `ATTACK_MOVE` (0x21): PATROL's layout, the first x as an f32's bits and nine more from
/// `+0x10`, the first y at `+0x34` and nine more from `+0x38`; its flags at `+0x8`..`+0xa`.
fn attack_move(c: &packets::AttackMove) -> Command {
    let word = |b: &[u8; 36], i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let points = from_fn(|i| match i {
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
        patrols: if c.field_0x08 != 0 {
            SavedPatrols::Dropped
        } else {
            SavedPatrols::Kept
        },
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
fn building_command(packet: &Packet) -> GenieResult<Option<Command>> {
    Ok(Some(match packet {
        Packet::Build(build) => Command::Build(BuildCommand {
            player: PlayerId::from(build.commp_id),
            master: MasterId::try_from(short(build.oid))?,
            position: Point::new(build.x, build.y),
            queue_id: QueueId::of_data(build.qid),
            sprite_override: SpriteOverride::of_data(build.field_0x18.cast_signed()),
            builders: listed(build.num, &build.ids),
            queued: build.field_0x1e != 0,
            commits: build.field_0x1f == 0,
            frame: Frame::of_byte(build.frame),
        }),
        Packet::BuildWall(build_wall) => Command::BuildWall(BuildWallCommand {
            player: PlayerId::from(build_wall.commp_id),
            master: MasterId::try_from(short(build_wall.oid))?,
            from: Tile::new(i32::from(build_wall.x1), i32::from(build_wall.y1)),
            to: Tile::new(i32::from(build_wall.x2), i32::from(build_wall.y2)),
            queue_id: QueueId::of_data(build_wall.qid),
            builders: listed(build_wall.num, &build_wall.ids),
            queued: build_wall.field_0x1a != 0,
            commits: build_wall.field_0x1b == 0,
        }),
        Packet::MultiQueue(multi_queue) => Command::Queue(QueueCommand {
            player: PlayerId::from(multi_queue.commp_id),
            buildings: listed(multi_queue.num, &multi_queue.ids),
            building_master: MasterId::try_from(multi_queue.bid)?,
            unit: MasterId::try_from(multi_queue.tid)?,
            amount: QueuedUnits::of_data(multi_queue.count),
            button: QueueButton::of_data(multi_queue.button_location_id),
        }),
        Packet::Research(research) => Command::Research(ResearchCommand {
            player: PlayerId::from(research.commp_id),
            building: ObjectId::of_field(research.uid),
            count: research.field_0x08.cast_signed(),
            buildings: object_ids(&research.ids),
            tech: TechId::of_data(research.tid),
            queue_id: QueueId::of_data(research.qid),
            shift: research.field_0x10 != 0,
        }),
        Packet::SetGatherPoint(set_gather_point) => Command::GatherPoint(GatherPointCommand {
            player: PlayerId::from(set_gather_point.commp_id),
            units: listed(set_gather_point.num, &set_gather_point.ids),
            point: Point::new(set_gather_point.x, set_gather_point.y),
            target: ObjectId::of_field(set_gather_point.tid),
            target_master: MasterId::of_field(short(set_gather_point.tm_id))?,
            own: set_gather_point.field_0x18 != 0,
        }),
        Packet::BuildingCommandQueue(building_command_queue) => {
            Command::CommandQueue(BuildingQueueCommand {
                player: PlayerId::from(building_command_queue.commp_id),
                units: listed(building_command_queue.num, &building_command_queue.ids),
                target: ObjectId::of_field(building_command_queue.tid),
                point: Point::new(building_command_queue.x, building_command_queue.y),
            })
        }
        _ => return Ok(None),
    }))
}

/// The commands of one object, one player or the game.
fn single_command(packet: &Packet) -> GenieResult<Option<Command>> {
    Ok(Some(match packet {
        Packet::Resign(resign) => Command::Resign(ResignCommand {
            player: PlayerId::from(resign.commp_id),
            departure: if resign.dropped != 0 {
                Departure::Dropped
            } else {
                Departure::Resigned
            },
        }),
        Packet::TransformObject(transform_object) => {
            Command::TransformObject(TransformObjectCommand {
                player: PlayerId::from(transform_object.commp_id),
                id: ObjectId::try_from(transform_object.field_0x08.cast_signed())?,
                master: MasterId::try_from(short(transform_object.field_0x04))?,
            })
        }
        Packet::CancelBuild(cancel_build) => Command::Delete(DeleteCommand {
            player: PlayerId::from(cancel_build.commp_id),
            id: ObjectId::try_from(cancel_build.uid)?,
        }),
        Packet::GoBackToWork(go_back_to_work) => Command::GoBackToWork(GoBackToWorkCommand {
            player: PlayerId::from(go_back_to_work.commp_id),
            id: ObjectId::try_from(go_back_to_work.uid)?,
        }),
        Packet::AllBackToWork(all_back_to_work) => Command::AllBackToWork(AllBackToWorkCommand {
            player: PlayerId::from(all_back_to_work.commp_id),
            id: ObjectId::try_from(all_back_to_work.uid)?,
        }),
        Packet::TownBell(town_bell) => Command::TownBell(TownBellCommand {
            player: PlayerId::from(town_bell.commp_id),
            id: ObjectId::try_from(town_bell.uid)?,
            bell: Bell::of_byte(town_bell.activate),
        }),
        Packet::Gate(gate) => Command::Gate(GateCommand {
            player: PlayerId::from(gate.commp_id),
            id: ObjectId::try_from(gate.uid)?,
        }),
        Packet::TribeGiveAttributes(tribe_give_attributes) => tribute(tribe_give_attributes),
        Packet::SellCommodity(sell_commodity) => trade(
            sell_commodity.commp_id,
            sell_commodity.attr,
            sell_commodity.amt,
            sell_commodity.uid,
            false,
        ),
        Packet::BuyCommodity(buy_commodity) => trade(
            buy_commodity.commp_id,
            buy_commodity.attr,
            buy_commodity.amt,
            buy_commodity.uid,
            true,
        ),
        Packet::Flare(flare) => Command::Flare(FlareCommand {
            player: PlayerId::from(flare.commp_id),
            target: ObjectId::of_field(flare.field_0x04.cast_signed()),
            point: Point::new(flare.field_0x08, flare.field_0x0c),
            players: flare.field_0x10,
            shown: [flare.gap_0x11[0], flare.field_0x12, flare.field_0x13]
                .iter()
                .chain(&flare.rest)
                .map(|&b| b != 0)
                .collect(),
        }),
        Packet::Game(game) => Command::Game(GameCommand {
            issuer: PlayerId::from(game.commp_id),
            sub: GameSub::of_data(game.field_0x04),
            player: game.field_0x08,
            a: game.field_0x0a,
            speed: game.field_0x0c,
            value: game.field_0x10,
        }),
        _ => return Ok(None),
    }))
}

/// `SELL_COMMODITY` (0x7a) or `BUY_COMMODITY` (0x7b): the attribute (`+0x4`), the lots
/// (`+0x6`) and the market (`+0x8`).
fn trade(player: u8, attribute: u16, times: u16, market: i32, buy: bool) -> Command {
    Command::Trade(TradeCommand {
        player: PlayerId::from(player),
        attribute: ResourceWord::of_data(attribute),
        times: TradeLots::of_data(times),
        market: ObjectId::of_field(market),
        buy,
    })
}

/// GIVE ATTRIBUTES (0xc4): the four entries' attributes (`+0x24`..), amounts (`+0x04`..) and
/// fees (`+0x14`..), the receiver at `+0x2c`.
fn tribute(tribe_give_attributes: &packets::TribeGiveAttributes) -> Command {
    let gift = |attribute, amount, fee| Gift {
        attribute: ResourceWord::of_data(attribute),
        amount: Amount::new(amount),
        fee: TributeFee::of_data(fee),
    };
    Command::Tribute(TributeCommand {
        player: PlayerId::from(tribe_give_attributes.commp_id),
        to: PlayerId::from(tribe_give_attributes.to),
        gifts: [
            gift(
                tribe_give_attributes.attr,
                tribe_give_attributes.amt,
                tribe_give_attributes.cost,
            ),
            gift(
                tribe_give_attributes.field_0x26,
                tribe_give_attributes.field_0x08,
                tribe_give_attributes.field_0x18,
            ),
            gift(
                tribe_give_attributes.field_0x28,
                tribe_give_attributes.field_0x0c,
                tribe_give_attributes.field_0x1c,
            ),
            gift(
                tribe_give_attributes.field_0x2a,
                tribe_give_attributes.field_0x10,
                tribe_give_attributes.field_0x20,
            ),
        ],
    })
}
