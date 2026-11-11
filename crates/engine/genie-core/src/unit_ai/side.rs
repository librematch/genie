//! The values the unit AI's calls on its unit's view
//! ([`UnitView`](crate::unit_ai::world_view::UnitView)) take and answer: the commands it gives its
//! unit, a resource search and its find, a target's flags, a defended unit's nearest attacker and a
//! unit sent off a footprint.

use crate::GenieError;
use crate::gathering::{DropSiteId, PastureId};
use crate::map::Point;
use crate::map::Position;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::{ObjectState, WorkerCount};
use crate::ownership::CaptureKind;
use crate::players::PlayerId;
use crate::players::PlayerKind;

/// What a defend target's unit AI names as its nearest attacker
/// ([`UnitView::defended_attacker`](crate::unit_ai::world_view::UnitView::defended_attacker)): the
/// defender attacks it, or with none runs its idle check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NearestAttacker {
    /// The world does not hold the target, or it has no unit AI: no slot 0x78 to ask.
    NoUnitAi,
    /// Its unit AI holds no attacker the world holds (slot 0x78's −1).
    NoAttacker,
    /// The attacker nearest to it.
    Attacker(ObjectId),
}

/// A unit command the slice holds no action for
/// ([`UnitView::command`](crate::unit_ai::world_view::UnitView::command)).
pub(crate) const UNMODELLED_COMMAND: GenieError = GenieError::unsupported(
    "a unit command the slice does not model (heal, conversion, enter, transport, unload)",
);

/// The unit's commands the unit AI's order slots give
/// ([`UnitView::command`](crate::unit_ai::world_view::UnitView::command)), each the unit's slot of
/// `TRIBE_Combat_Object`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UnitCommand {
    /// The heal of an object (slot 0x108, `FUN_141214040`), forced.
    Heal(ObjectId),
    /// The conversion of an object (slot 0x370, `FUN_141210510`), forced.
    Convert(ObjectId),
    /// The repair of an object (slot 0x378, `FUN_141217c00`), forced.
    Repair(ObjectId),
    /// The enter into an object (slot 0x3a0), forced.
    Enter(ObjectId),
    /// The transport to a point (slot 0x3b8, `FUN_14108ce60`), forced.
    Transport(Position),
    /// The unload at a point at the unit's height (slot 0x3a8, `FUN_14108d050`), or over a wall
    /// (slot 0x3b0, `FUN_14108d560`), forced.
    Unload {
        /// The point.
        to: Position,
        /// Over a wall.
        over_wall: bool,
    },
}

/// What the full update's tail (`0x140c66b10` after the look-around check) tests of the target of
/// its unit's top action
/// ([`UnitView::target_flags`](crate::unit_ai::world_view::UnitView::target_flags)).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TargetFlags {
    /// Its owner's id (`+0x138`, `player+0x718`); Gaia is 0.
    pub(crate) owner: PlayerId,
    /// Its type (`+0xf`).
    pub(crate) kind: MasterKind,
    /// `capture_flag` (`+0x924`, `FUN_14107cc90`).
    pub(crate) capture: CaptureKind,
    /// `+0x790`, `parent_pasture_id`: the pasture it is in.
    pub(crate) pasture: Option<PastureId>,
    /// `+0x689`, `mUnitTargetingFlag`, is 0 (`FUN_141188bb0`).
    pub(crate) untargetable: bool,
    /// `+0x68a`, `unit_attackable_flag` (`FUN_141188b10`).
    pub(crate) attackable: bool,
}

/// What the unit AI asks `ClosestResourceObject` (slot 0x80, `FUN_140c49760`;
/// [`UnitView::closest_resource`](crate::unit_ai::world_view::UnitView::closest_resource)).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResourceSearch {
    /// The gather group (`param_2`).
    pub(crate) kind: GatherGroup,
    /// Fewer workers than this (`param_4`; −1 any).
    pub(crate) cap: WorkerCount,
    /// The drop site the search starts from (`param_3`).
    pub(crate) site: Option<DropSiteId>,
    /// The unit AI's current target (`+0x48`), which the search passes over.
    pub(crate) current: Option<ObjectId>,
    /// The point the search starts from without a site (`param_7`, `param_8`; (−1, −1) for the
    /// unit's place).
    pub(crate) point: Point,
    /// Below this state the unit finds nothing (`param_6`; its −1 is 0 here).
    pub(crate) min_state: ObjectState,
}

/// [`ResourceSearch::point`]'s (−1, −1): the unit's own place (`DAT_14330ec20`). The spawn keeps
/// a constant of its own for its point of none, of the same value: `FUN_140ace630`'s immediates
/// (`NO_POINT` of `production::spawn::new_unit`).
pub const NO_POINT: Point = Point::new(-1.0, -1.0);

/// What the unit AI's `ClosestResourceObject` found
/// ([`UnitView::closest_resource`](crate::unit_ai::world_view::UnitView::closest_resource)): the
/// object, its place and its master's class.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gatherable {
    /// The object.
    pub(crate) id: ObjectId,
    /// Its place.
    pub(crate) position: Position,
    /// Its master's class, or the class the hunter's or gatherer's search took.
    pub(crate) class: ClassId,
}

/// An object as an order in the queue names it: its id, master, owner and the first two parts
/// of its slot 0x520 vector (its x and its height).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Named {
    /// The object.
    pub(crate) id: ObjectId,
    /// Its master.
    pub(crate) master: MasterId,
    /// Its owner.
    pub(crate) owner: PlayerId,
    /// Its slot 0x520 vector's x and second part (the height), which the game's orders read as
    /// a point's x and y.
    pub(crate) vector: Point,
}

/// What a unit sent off a footprint finds (the base handler's case 0x216).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OffFootprint {
    /// The closest spot beside the footprint (`FUN_1411566b0` with 0), None when there is none
    /// (or no foundation).
    pub(crate) spot: Option<Point>,
    /// The unit's height.
    pub(crate) z: f32,
    /// Its master's class.
    pub(crate) class: ClassId,
    /// The owner's type (`player+0x10`; [`PlayerKind::is_computer`] is `FUN_141659cf0`).
    pub(crate) player_kind: PlayerKind,
    /// Its owner's id.
    pub(crate) player: PlayerId,
    /// The foundation, while the world holds it.
    pub(crate) site: Option<Named>,
    /// The target of its current action (`FUN_140a986f0`, `FUN_140a78450`).
    pub(crate) action_target: Option<Named>,
}
