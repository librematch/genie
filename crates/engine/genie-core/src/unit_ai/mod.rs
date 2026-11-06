//! The unit AI (`UnitAIModule`): what a unit decides by itself. It takes its orders and the
//! notifies its actions post, picks its targets, answers damage and runs its type's idle checks,
//! with the timers and the AI-stream draws behind checkpoint word 9.
//!
//! 158041: the full update is `UnitAIModule` slot 4 (`0x140c66b10`); slot 73 (`0x140c567c0`) redraws
//! the retarget timer, slot 72 is the idle check (`processIdle`). A boar
//! (factory type 4, `TribeElephantUnitAIModule`, slot 4 `0x140c2c010`) wanders around its origin
//! ([`Home`]); its moves go to the unit as an [`AiMove`]. A hunted animal (type 2,
//! `TribeHuntedAnimalUnitAIModule`: deer, foxes) looks for the closest object of the classes it
//! cares about in its player's sight ([`Search`], answered by the caller) and otherwise wanders
//! around its origin ([`Hunted`]).
//!
//! The unit's actions post notifies to its unit AI ([`UnitAi::post`]), which the full update
//! processes ([`Notify`]); orders wait in a queue the AI runs once its unit can take one
//! ([`QueuedOrder`]), on its owner's turn and between turns ([`UnitAi::off_turn`]). What the AI
//! does to its unit comes back as [`Effect`]s, in order.

mod actions;
mod ai_action;
mod ai_kind;
pub mod ai_kinds;
mod alert_mode;
mod attack_orders;
mod attack_pick;
mod attack_rules;
mod attacked;
mod attackers;
pub mod best_target;
mod cared;
mod damaged;
mod flight;
mod handle;
mod idle;
mod idle_animals;
mod idle_part;
mod ids;
mod issuer;
mod kind_notifies;
mod life;
mod notifies;
mod notify_kind;
mod order_kind;
mod order_kinds;
mod order_outcome;
mod order_priority;
mod process_order;
mod queue_end;
mod retarget;
mod side;
mod soldier_idle;
mod stop_order;
mod surroundings;
mod target_kind;
pub mod task_finder;
pub(crate) mod unit;
mod unit_stance;
mod update;
pub(crate) mod world_view;

pub use self::actions::OrderTest;
pub use self::ai_action::AiAction;
pub use self::ai_kind::UnitAiKind;
pub use self::alert_mode::AlertMode;
use self::attack_pick::point_gap_squared;
pub(crate) use self::attack_pick::spares_friends;
pub use self::attack_pick::{
    AttackAsk, AttackChecks, AttackTargetFlags, Attacker, Memory, Pick, Search, Seeker, Target,
    can_attack, soldier_can_attack, tower_can_attack,
};
pub use self::attack_rules::{
    Attacked, Convertee, Converter, TargetRules, attack_sight, attacks_neutral, can_convert,
    kind_attacks_neutral, priest_attacks_neutral, siege_attacks_neutral, siege_can_attack,
    soldier_attacks_neutral,
};
#[cfg(test)]
use self::attack_rules::{PATROL_FARTHEST, line_distance};
pub(crate) use self::attack_rules::{PATROL_WITHOUT_POINTS, patrol_distance};
use self::attack_rules::{
    back_up, civilian_may_attack, computer_idle_check, defend_patrol_range, lion_may_attack,
};
pub use self::attackers::Attackers;
#[cfg(test)]
pub(crate) use self::cared::cared_classes;
pub use self::cared::{
    ARTIFACT_CARED, BUILDING_CARED, CIVILIAN_CARED, CIVILIAN_SEARCH_GROWTH, FISHING_CARED,
    HUNTED_CARED, LION_CARED, PREDATOR_CARED, PRIEST_CARED, SIEGE_CARED, SIEGE_TOWER_CARED,
    SOLDIER_CARED, SOLDIER_CARED_COMPUTER, TOWER_CARED, TRADE_CART_CARED, TRADE_SHIP_CARED,
    TRANSPORT_CARED, civilian_kind, civilian_search_radius,
};
use self::cared::{
    LIVE_TARGET_ACTIONS, RUN_DRAW_SCALE, RUN_EPSILON, RUN_ONE, adjusted_idle_timeout,
    attack_search_radius,
};
pub(crate) use self::cared::{search_area, search_radius};
use self::ids::{
    AI_MOVE_RANGE, ARTIFACT_IDLE_TIMEOUT, BOAR_HOME, BOAR_LEASH, BOAR_LEASH_DRAW, BOAR_OFFSET,
    BOAR_STEP, BOAR_WANDER, COMPUTER_SOLDIER_IDLE_TIMEOUT, DESIRED_DISTANCE, ENDED_ACTIONS,
    HUNTED_HOME, HUNTED_IDLE_TIMEOUT, HUNTED_SIGHT_GROWTH, HUNTED_STEP, IDLE_TIMEOUT,
    INVALID_POINT, LION_ATTACKED_TIMEOUT, LION_IDLE_TIMEOUT, NO_ANIM_SPEED, OUTSIDE_ORDER,
    PRIEST_IDLE_TIMEOUT, Retarget, RetargetSearch, SIEGE_IDLE_TIMEOUT, SOLDIER_IDLE_TIMEOUT,
    TOWER_IDLE_TIMEOUT, VILLAGER_NOTIFY, WALK_BACK, closest_undiscovered_tile, dodges,
    held_action_state, order_move, retarget_kind, siege_stance, soldier_stance,
};
pub use self::ids::{HUMAN_ORDER_CAP, NewObject, factory_kind, unit_ai_kind};
pub(crate) use self::ids::{alert_owner, helper_answers, searches_on_class};
pub use self::issuer::Issuer;
pub use self::life::{Notifies, OrderHistory};
pub use self::notifies::{AiLayer, Inbox};
pub use self::notify_kind::NotifyKind;
pub use self::order_kind::OrderKind;
pub use self::order_outcome::{
    Answer, IdleOutcome, MiscEnd, OrderOutcome, RetargetCheck, WalkBack,
};
pub use self::order_priority::OrderPriority;
pub(crate) use self::process_order::TakenAs;
pub use self::queue_end::QueueEnd;
pub(crate) use self::side::UNMODELLED_COMMAND;
pub use self::side::{
    Gatherable, NO_POINT, Named, NearestAttacker, OffFootprint, ResourceSearch, TargetFlags,
    UnitCommand,
};
pub use self::stop_order::StopOrder;
pub(crate) use self::surroundings::FOUND_WITHOUT_POSITION;
pub use self::surroundings::{
    AiMove, Duel, Found, GroupState, Home, OrderForce, Pace, Seen, SurroundingFlags, Surroundings,
    TopTarget, UnitSeen, flood_search,
};
use self::surroundings::{LiveTop, group_lets_idle};
pub use self::target_kind::OrderTargetKind;
pub use self::unit_stance::UnitStance;
use crate::groups::FormationKind;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
#[cfg(test)]
use crate::math::rng::Well512;
#[cfg(test)]
use crate::movement::PathAsk;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::players::PlayerId;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WorldTime;
use genie_diff::Diff;

/// A notify to a unit AI (the save's `temp_notify`, 0x18 bytes): from `caller` to `recipient`, of
/// `kind`, with three parameters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct Notify {
    /// The object that posts it; none for a missile's hit whose shooter is gone (its sender −1).
    pub(crate) caller: Option<ObjectId>,
    /// The object whose unit AI takes it.
    pub(crate) recipient: ObjectId,
    /// The kind ([`NotifyKind`]).
    pub(crate) kind: NotifyKind,
    /// Its three parameters.
    pub(crate) params: [i32; 3],
}

impl Notify {
    /// The notify an object posts to itself.
    #[must_use]
    pub const fn own(id: ObjectId, kind: NotifyKind, params: [i32; 3]) -> Self {
        Self {
            caller: Some(id),
            recipient: id,
            kind,
            params,
        }
    }

    /// The record as the save holds it.
    ///
    /// # Errors
    ///
    /// A record shorter than its fields, or a recipient of −1.
    pub fn from_bytes(bytes: &[u8; 24]) -> GenieResult<Self> {
        let word = |offset: usize| le_word(bytes, offset).map(i32::from_le_bytes);
        Ok(Self {
            caller: ObjectId::of_field(word(0)?),
            recipient: ObjectId::try_from(word(4)?)?,
            kind: NotifyKind::from(word(8)?),
            params: [word(12)?, word(16)?, word(20)?],
        })
    }
}

/// The damage case's unit too slow to run away (`DAT_142eb0c60`, [`UnitAi::damaged`]).
const STILL: f32 = UNIT_AI_STILL;

/// What a notify handler (unit-AI slot 0x238) returns: go on with the next notify.
const GO_ON: u32 = 2;
/// "Notify processing returned stop or new action, breaking."
const BREAK: u32 = 3;
/// "I'm dead, purging notifyQueue and returning FALSE."
const DEAD: u32 = 4;
/// "`ProcessIdle` produced an action."
const PRODUCED: u32 = 0xd;
/// The classes of the foundations a fishing ship's builder looks for (`FUN_140c25ca0`, in its
/// order): buildings, walls, gates, towers and farms.
const FISHING_BUILDS: [ClassId; 5] = classes([3, 27, 39, 52, 49]);
/// The fishing ship's builder takes the nearest foundation nearer than this (`FUN_140c25ca0`'s
/// 9999).
const FISHING_BUILD_FAR: WholeDistance = WholeDistance::of_data(9999);

/// The notify queue's capacity of a unit AI of `ai_kind` holding `count` notifies: the
/// factory's (`FUN_141215560`: 5 for the artifact's and the livestock's, types 14 and 19; 10
/// for the others), doubled until they fit.
#[must_use]
pub const fn notify_capacity(ai_kind: UnitAiKind, count: usize) -> usize {
    let mut capacity = if matches!(ai_kind, UnitAiKind::Artifact | UnitAiKind::Livestock) {
        5
    } else {
        10
    };
    while capacity < count {
        capacity *= 2;
    }
    capacity
}

/// A queued order (the save's `temp_order`, 0x34 bytes), which the order queue (`+0x18` count,
/// `+0x20` array) runs once the unit can take it ([`UnitAi::run_orders`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct QueuedOrder {
    /// `+0x0`: the issuing player or object; none for −1 (a guard's defend order).
    pub(crate) issuer: Option<Issuer>,
    /// `+0x4`: the order (a move, an order on a target, …).
    pub(crate) order: OrderKind,
    /// `+0x8`: below 100 it waits while the unit AI has an action.
    pub(crate) priority: OrderPriority,
    /// `+0xc`, `+0x10`: the target and its owner (−1 for none; "T", "`TOwner`" in the log line of
    /// `FUN_140c55c90`).
    pub(crate) target: Option<ObjectId>,
    /// The target's owner.
    pub(crate) target_owner: Option<PlayerId>,
    /// `+0x14`..`+0x1c`: the point; `+0x20`: the range.
    pub(crate) point: Position,
    /// The range.
    pub(crate) range: Distance,
    /// `+0x24`, `+0x28`: the point the unit's slot 0x2d8 shows (not held).
    pub(crate) shown: Point,
    /// `+0x2c` (a byte): 0 leaves the order in the queue.
    pub(crate) ready: bool,
    /// `+0x30`: the target's type, its master id or its class ([`OrderTargetKind`]).
    pub(crate) target_kind: Option<OrderTargetKind>,
}

/// The four bytes at offset `offset` of a saved record.
///
/// # Errors
///
/// A record shorter than the offset.
fn le_word(bytes: &[u8], offset: usize) -> GenieResult<[u8; 4]> {
    bytes
        .get(offset..)
        .and_then(<[u8]>::first_chunk::<4>)
        .copied()
        .ok_or_else(|| {
            GenieError::unreachable("a saved record shorter than its fields").with("offset", offset)
        })
}

impl QueuedOrder {
    /// The record as the save holds it (`+0x2d`..`+0x2f` are padding), in the queue of object
    /// `own` in a world of `players` players ([`Issuer::of_saved`]).
    ///
    /// # Errors
    ///
    /// An order value no order holds ([`OrderKind::try_from`]).
    pub fn from_bytes(bytes: &[u8; 52], own: ObjectId, players: usize) -> GenieResult<Self> {
        let word = |offset: usize| le_word(bytes, offset).map(i32::from_le_bytes);
        let float = |offset: usize| le_word(bytes, offset).map(f32::from_le_bytes);
        Ok(Self {
            issuer: Issuer::of_saved(word(0)?, own, players),
            order: OrderKind::try_from(word(4)?)?,
            priority: OrderPriority::of_data(word(8)?),
            target: ObjectId::of_field(word(12)?),
            target_owner: PlayerId::of_field(word(16)?)?,
            point: Position::new(float(20)?, float(24)?, float(28)?),
            range: Distance::new(float(32)?),
            shown: Point::new(float(36)?, float(40)?),
            ready: le_word(bytes, 44)?[0] != 0,
            target_kind: OrderTargetKind::of_field(word(48)?)?,
        })
    }
}

/// A call the unit AI makes on its unit, at once where the game's AI calls the unit's slot
/// ([`UnitView::effect_now`](crate::unit_ai::world_view::UnitView::effect_now)).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// stopAction (the unit's slot 0x3c0, `FUN_14108c160`): its action list goes
    /// (`FUN_140a98620`, each action's teardown) and a unit in state 2 takes its standing sprite
    /// (slot 0x68 with slot 0x798's).
    Stop,
    /// The AI's move (slot 0x130), which the unit takes (its slot 0x340).
    Move(AiMove),
    /// `FUN_140c41e50`: the unit's last human order time (`+0x650`) is at most this.
    CapHumanOrder(WorldTime),
    /// The queued order on a target (`0x2db`) the AI took: the unit's slot 0x128 on the target,
    /// then the order's priority becomes the AI's (`World::order_on_target`, on the unit back in
    /// its slot); the full update's rest then reads what it gave the AI ([`AfterOrder`]).
    OrderOnTarget(QueuedOrder),
    /// The unit's explore (slot 0x398, `FUN_141084db0`, with its argument 4 set) to the point
    /// (x, y): its Explore takes the point (slot 0x48), else a new Explore to it (`FUN_140a87d30`
    /// at the unit's height) replaces its actions (`FUN_140a98620`, slot 0x808).
    Explore(Point),
    /// The AI's move to an object (slot 0x138, `FUN_140c55490`, with its fourth argument 1): the
    /// unit's slot 0x348 (`FUN_141086440`, forced) replaces its actions (`FUN_140a98620`) with a
    /// `MoveTo` to the target within `range` showing the master's running sprite (the walking one
    /// without it) and starts it (slot 0x808).
    MoveToObject {
        /// The target.
        target: ObjectId,
        /// The target's state as the AI saw it, which decides whether the `MoveTo` holds it
        /// ([`crate::movement::move_to::holds_object`]).
        state: ObjectState,
        /// The range within which the `MoveTo` arrives.
        range: Distance,
    },
    /// The unit's ownership change (its slot 0x70, `FUN_14120f800` for a combat unit,
    /// `FUN_1411cd860` for a building) to the player, which the artifact's processIdle asks of
    /// it (`FUN_140c1e410`); the unit runs it at once.
    ChangeOwner(PlayerId),
    /// A command of the unit's own that the AI gives it ([`AiCommand`]), which the unit runs at
    /// once.
    Command(AiCommand),
    /// `restorePatrolInformation`'s unit part (`FUN_140c5e5b0`): the unit's own waypoints go
    /// (slot 0x450) and become the saved patrol's points (slot 0x448 each), and the unit moves to
    /// its waypoint (slot 0x340: range 0, forced, running).
    RestorePatrol,
}

/// A command of the unit's own that its unit AI gives ([`Effect::Command`]): one of the unit's
/// slots, which the unit runs at once on itself lent back to its slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AiCommand {
    /// Slot 0x148 (`FUN_140c4d0d0`) forced: the unit's slot 0x340 to the point
    /// ([`crate::world::World::move_aside`]).
    MoveAside(AiMove),
    /// `HuntObject`'s unit part (`FUN_140c4e220`), after the AI's stop (slot 0xb0 with 1, the
    /// unit's `stopAction` at once): its hunt command on the target (object slot 0x360,
    /// `FUN_1412151f0`, forced; [`crate::world::World::hunt_command`]).
    HuntObject(ObjectId),
    /// `GatherObject`'s unit part (slot 0xd0, `FUN_140c4dd50`, forced): the unit's gather command
    /// on the target (object slot 0x368, `FUN_141085030`, forced;
    /// [`crate::world::World::gather_command`]).
    GatherObject(ObjectId),
    /// `BuildObject`'s unit part (the AI's slot 0xf0, `FUN_140c47d80`): the unit's build command
    /// (slot 0x380) on the foundation ([`crate::world::World::build_command`]); the AI's part,
    /// the build action and the foundation as its target, is its own ([`UnitAi::build_object`]).
    BuildObject(FoundationId),
    /// The unit's slot 0x7e8 on object `target` at `point` with the retargeting byte 0 and any
    /// task, its `+0xa14` as it stands ([`crate::world::World::order_work`]).
    WorkOn {
        /// The object.
        target: ObjectId,
        /// Its place.
        point: Position,
    },
    /// The unit's slot 0x140 on object `target` at `point`: the same work, then its command
    /// count one more ([`crate::world::World::work_at`]).
    WorkAt {
        /// The object.
        target: ObjectId,
        /// Its place.
        point: Position,
    },
    /// The villager's case 0x21c ([`NotifyKind::OTHER_LOAD`]) with the drop-site table's building
    /// for its load
    /// ([`UnitView::table_drop_site`](crate::unit_ai::world_view::UnitView::table_drop_site)): the
    /// unit's slot 0x128 on it at its place, then its unit AI's order back to the target its top
    /// action had before (slot 0x190), if any ([`crate::world::World::drop_elsewhere`]).
    DropElsewhere {
        /// The drop site.
        site: DropSiteId,
        /// Its place, where the table's search found it.
        at: Position,
        /// The top action's target before the work.
        back: Option<ObjectId>,
    },
}

/// What a full update that took an order on a target ([`Effect::OrderOnTarget`]) leaves for
/// its rest ([`UnitAi::after_order`]): the game runs the order inside the update (slot 0x230),
/// so slot 73 and the idle part read the action, target and order it gave the AI; the idle part
/// reads what the update read of its unit before the order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AfterOrder {
    /// The unit was busy after the notifies (slot 0x430 again, the update's `busy`).
    pub(crate) busy: bool,
    /// The unit's group as the update read it ([`Surroundings::group`]).
    pub(crate) group: Option<GroupState>,
}

/// A unit AI's saved patrol (`+0x1c0`, a `Path` of its own): a copy of its unit's top path
/// (the unit's slot 0x468, `FUN_141157c50`: `FUN_140f95c00` into a new path) with that path's
/// flags (`+0xc`: bit 0 a loop, bit 1 back and forth, `getPatrolLinear` `FUN_140c4df80`).
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct PatrolPath {
    /// The points (x, height, y) and the length.
    pub(crate) path: Path,
    /// The path's flags.
    pub(crate) flags: PathFlags,
}

/// The unit-AI fields the idle update reads and writes, with their offsets in 158041.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct UnitAi {
    /// The factory's type (`has_ai` in the save), which picks the class.
    pub(crate) ai_kind: UnitAiKind,
    /// `+0x3c`, −1 for none.
    pub(crate) current_order: Option<OrderKind>,
    /// `+0x40`, the current order's priority (−1 for none): a queued order sets it.
    pub(crate) order_priority: OrderPriority,
    /// `+0x44`, −1 for none.
    pub(crate) current_action: Option<AiAction>,
    /// `+0x48`.
    pub(crate) current_target: Option<ObjectId>,
    /// `+0x4c`, the target's class (−1 for none), and `+0x50`..`+0x58`, its point.
    pub(crate) target_class: Option<ClassId>,
    /// `+0x50`..`+0x58`: the target's point.
    pub(crate) target_point: Position,
    /// `+0x5c`, `desiredTargetDistance`: how near the follow order keeps the unit to its target
    /// (the constructor's 2).
    pub(crate) desired_distance: Distance,
    /// `+0x60`, `DefendTarget`: the object the defend order keeps the unit near.
    pub(crate) defend_target: Option<ObjectId>,
    /// `+0x64`, `+0x68`, `+0x6c`, `+0x70`: the order, action, target and target class before the
    /// current ones.
    pub(crate) last_order: Option<OrderKind>,
    /// `+0x68`: the action before the current one.
    pub(crate) last_action: Option<AiAction>,
    /// `+0x6c`: the target before the current one.
    pub(crate) last_target: Option<ObjectId>,
    /// `+0x70`: that target's class.
    pub(crate) last_target_class: Option<ClassId>,
    /// `+0x160`.
    pub(crate) idle_timer: Millis,
    /// `+0x164`, the randomised timeout the idle timer is compared with.
    pub(crate) adjusted_idle_timeout: Millis,
    /// `+0x168`, the base the adjusted timeout is drawn around.
    pub(crate) idle_timeout: Millis,
    /// `+0x16c`.
    pub(crate) secondary_timer: Millis,
    /// `+0x170`.
    pub(crate) look_around_timer: Millis,
    /// `+0x174`.
    pub(crate) look_around_timeout: Millis,
    /// `+0xbc`, the world time of the last retarget.
    pub(crate) last_retarget_time: WorldTime,
    /// `+0x1cc`, `mRandomizedRetargetTimer`: the word-9 input.
    pub retarget_timer: Millis,
    /// `+0x1a9`: defensive and stand ground add leash checks to slot 73.
    pub(crate) stance: UnitStance,
    /// `+0x1d4`, `mFormationType`: the formation the unit's group last took for it, which its
    /// next group's type counts (`FUN_14100b940`).
    pub(crate) formation_kind: FormationKind,
    /// `+0x1ac`, `+0x1b0`, `state_position`: where a unit with an order last updated, which a
    /// stance's leash measures from.
    pub(crate) state_position: Point,
    /// `+0x1d6`: non-zero skips the retarget check.
    pub(crate) retarget_disabled: u8,
    /// `+0x1b8`, `alertModeValue`.
    pub(crate) alert_mode: AlertMode,
    /// `+0x1bc`, `alertModeObjectIDValue`: what slot 0x1d8 last named with the alert mode on
    /// ([`UnitAi::watch_alert`]), −1 from the constructor (`FUN_140c2e080`). Only the save
    /// reads it.
    pub(crate) alert_object: Option<ObjectId>,
    /// `+0x1b4`, `time_since_enemy_sighting`: seconds since a search last found enemies; each full
    /// update adds the world's dt.
    pub(crate) time_since_enemy: Seconds,
    /// `+0x1c0`, the saved patrol (the save's `hasPatrolPath` and its path): what
    /// `savePatrolInformation` (`FUN_140c63d20`) kept of the unit's patrol when an attack took it
    /// off it, which `restorePatrolInformation` (`FUN_140c5e5b0`) walks on.
    pub(crate) patrol_path: Option<PatrolPath>,
    /// `+0x1c8`, `mPatrolCurrentWaypoint`: the saved patrol's waypoint the unit walked to.
    pub(crate) patrol_waypoint: WaypointIndex,
    /// `+0x1a8`, `stop_after_target_killed_value`: a move does not let the current action take its
    /// point (`FUN_140c41c10`).
    pub(crate) stop_after_kill: bool,
    /// A boar's own fields (factory type 4).
    pub(crate) home: Option<Home>,
    /// A hunted animal's own fields (factory type 2). Not saved: a load makes the unit AI anew,
    /// its origin the animal's place then, so a replay takes them from the recording's start.
    #[diff(unsaved)]
    pub(crate) hunted: Option<Hunted>,
    /// `+0x15c`, `last_update_time_value`: what `FUN_140c5dc10` last cleared the notifies with
    /// (the owner's time on its turn, the world time on a MOVE or an ownership change).
    pub(crate) last_update_time: Millis,
    /// The order queue, oldest first.
    pub(crate) orders: Vec<QueuedOrder>,
    /// A building's or tower's command queue (`+0x1f0`..`+0x1f8`, 0x34-byte orders the
    /// building command queue fills, saved as `savedOrder`), which each unit it trains takes.
    pub(crate) building_orders: Vec<QueuedOrder>,
    /// The notify queue (`+0x28` count, `+0x30` array), oldest first.
    pub(crate) notifies: Vec<Notify>,
    /// Its capacity (`+0x2c`, not saved): the factory's 10 (5 for the artifact's and the
    /// livestock's, types 14 and 19), doubled by a post that finds it full outside the
    /// processing (`FUN_140c42470`).
    #[diff(unsaved)]
    pub(crate) notify_capacity: usize,
    /// `+0x38`: the notifies are being processed; a post that finds the queue full then goes to
    /// the buffer (`DAT_143e6f930`), which the processing's end flushes into the queue
    /// (`FUN_140c55270`).
    pub(crate) processing: bool,
    /// The buffered notifies (the game's buffer is global; one unit AI processes at a time).
    pub(crate) buffered: Vec<Notify>,
    /// This update's order queue ran an order on a target ([`Effect::OrderOnTarget`]): the update
    /// ends there and its rest runs with what the order gave the AI ([`AfterOrder`]). Not saved:
    /// genie's flag within one update, cleared at its start.
    #[diff(unsaved)]
    pub(crate) ordered_on_target: bool,
    /// `+0xc0`, the attackers ([`Attackers`]).
    pub(crate) attackers: Attackers,
    /// The order history (`+0x78`'s list, the newest at `+0xa8` first, count `+0xb8`; the
    /// save's `ohCount` entries): the orders on a target ([`UnitAi::remember_order`]),
    /// `bestUnitToAttack`'s weights by attack category.
    pub(crate) order_history: Vec<OrderRecord>,
    /// The retarget entries (`+0x1d8` count, `+0x1e0` array): targets `bestUnitToAttack` could
    /// not path to, each with the world time until which it ignores them (`FUN_140c42320`); the
    /// full update lets the expired go ([`UnitAi::expire_retries`]).
    pub(crate) retries: Vec<(ObjectId, WorldTime)>,
    /// `+0x1d0`, `mBestUnitToAttackThisUpdate`: what the damage notify's `bestUnitToAttack`
    /// found this update ([`UnitAi::damaged`]), none before one ran (the constructor's −1, and
    /// the full update's start) or when it found none; the update's later damage notifies take
    /// it.
    pub(crate) best_this_update: Option<ObjectId>,
}

/// An order on a target in a unit AI's history (`FUN_140c42080`, a node of `+0x78`'s list).
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct OrderRecord {
    /// The order (`mOrder`), None for the −1 of an order recorded without one.
    pub(crate) order: Option<OrderKind>,
    /// The action (`mAction`), None for −1.
    pub(crate) action: Option<AiAction>,
    /// The world time it was given (`mTime`).
    pub(crate) time: WorldTime,
    /// The unit's slot 0x520 then (`mPosition`: where it last moved to).
    pub(crate) position: BVector,
    /// The target (`mTargetID`).
    pub(crate) target: Option<ObjectId>,
    /// The target's attack category ([`crate::unit_ai::best_target::category`], `mTargetAttackCategory`).
    pub(crate) category: Option<best_target::AttackCategory>,
    /// The target's slot 0x520 then (`mTargetPosition`).
    pub(crate) target_position: BVector,
}

/// The order history's length (`FUN_140c42080`: a ninth order recycles the oldest).
pub const ORDER_HISTORY: usize = 8;

/// A hunted animal's part of its unit AI (`TribeHuntedAnimalUnitAIModule`, factory type 2), set
/// when the AI is made (`FUN_140c11920`) and not saved: a loaded game makes it from the loaded
/// unit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct Hunted {
    /// `+0x1f0`..`+0x1f8`: the unit's place when its AI was made.
    pub(crate) origin: Position,
    /// `+0x1fc`: the unit's slot 0x7b0 then (`+0xb74`, the work range of its master's last task
    /// of action type 6, `FUN_141206d50`): beyond it the animal goes home.
    pub(crate) leash: Distance,
}

use crate::construction::FoundationId;
use crate::gathering::DropSiteId;
use crate::map::WholeDistance;
use crate::math::fmath::BVector;
use crate::math::image::unit_ai::STILL as UNIT_AI_STILL;
use crate::movement::{PathFlags, WaypointIndex};
use crate::objects::ObjectState;
use crate::pathing::tile_path::Path;
use crate::unit_ai::cared::classes;
/// The engine's error ([`genie_error::GenieError`]), here for the modules' imports.
use crate::{GenieError, GenieResult};

#[cfg(test)]
mod tests;
