//! A repairer's Repair action (`TRIBE_Action_Repair`, type 106, vtable `0x142ed7b88`): the
//! action walks to its target (state 4, a `MoveTo` sub-action within the unit's weapon range),
//! faces it (state 6) and repairs it each update (state 7) until the target holds its most hit
//! points or the player cannot pay for more; a target out of reach makes it wait (state 10) or
//! fail (state 13).
//!
//! The work of one update is the unit's work rate times the task's work value times the world's
//! dt, of which each repairer past the first adds half ([`work_amount`]). The target's own repair
//! (a building's slot 0x8c8, `FUN_1411d37d0`; a unit's, `FUN_1412143b0`) pays a share of its
//! master's cost ([`crate::production::pay`]) and heals it (`FUN_141193b30`) over the objects it
//! reaches ([`repair_target`]).

use crate::GenieError;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::Voice;
use crate::objects::WorkRate;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::Task;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::TaskId;
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::world::layer::MasterSlots;
use genie_diff::Diff;

mod action;
pub(crate) mod payment;
mod repair_state;
pub(crate) mod unit;

pub use self::action::work_amount;
pub use self::payment::{
    BUILDING_REPAIR_COST, Payer, RepairOutcome, Repaired, UNIT_REPAIR_COST, repair_building,
    repair_unit,
};
pub use self::repair_state::RepairState;
pub(crate) use self::unit::repair_target;
use crate::construction::{BuilderCount, RepairTargetId};
use crate::map::RestrictionId;
use crate::math::image::repair::ONE as REPAIR_ONE;
use crate::objects::masters::WorkValue;
use crate::unit_ai::UnitStance;

/// The Repair action's type, which word 9 adds, and its tasks' action type.
pub const REPAIR: ActionKind = ActionKind::named(0x6a);

/// The wait of state 10 (`0x3f800000`), and the repairer count's first (`DAT_14330eb8c`).
const ONE: f32 = REPAIR_ONE;

/// The masters a unit of task swap group 1 or 2 (`+0x7b8`) becomes for a repair command (slot
/// 0x378): its player's master table's entries at `+0x4e0` and `+0x6f0`, the repairers 156
/// and 222 (the build's `villager_repairer_male` and `villager_repairer_female`), when the
/// player has them.
#[must_use]
pub const fn repairers(masters: MasterSlots) -> [(TaskGroup, MasterId); 2] {
    [
        (TaskGroup::MALE_VILLAGERS, masters.villager_repairer_male),
        (
            TaskGroup::FEMALE_VILLAGERS,
            masters.villager_repairer_female,
        ),
    ]
}
/// The repair of a ship (its meeting point, `FUN_140ad9120`), which is not modelled.
pub const SHIP_REPAIR: GenieError =
    GenieError::unsupported("the task finder's repair (106) of a ship (FUN_140ad9120)");

/// What a Repair works with: its task's id, work value (`+0x28`, `work_val1`) and the two
/// sprites its states show.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct RepairTask {
    /// The task's id.
    pub id: TaskId,
    /// Its work value.
    pub work_value: WorkValue,
    /// Its moving sprite (`+0x40`), which its `MoveTo`s take and its turn toward the target
    /// (state 6) shows; None for none.
    pub moving: Option<SpriteId>,
    /// Its proceeding sprite (`+0x48`), which its work (state 7) shows; None for none.
    pub proceeding: Option<SpriteId>,
}

impl RepairTask {
    /// The Repair task `task`.
    #[must_use]
    pub const fn of(task: &Task) -> Self {
        Self {
            id: task.id,
            work_value: task.work_val1,
            moving: task.sprites.moving,
            proceeding: task.sprites.proceeding,
        }
    }
}

/// The Repair action of a repairer (`TRIBE_Action_Repair`, type 106).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct RepairAction {
    /// The target's id (`+0x30`).
    pub target: Option<RepairTargetId>,
    /// The action state (`+0x18`), which word 9 adds, with its quiet `MoveTo` sub-action: to
    /// the target (state 4) or to the point (state 11).
    pub state: RepairState,
    /// `+0x38`..`+0x40`: the target's place as the action last took it (the constructor, states
    /// 4, 6 and 7), or the point the unit was sent to.
    pub point: Position,
    /// `+0x44`: state 10's wait.
    pub timer: Seconds,
    /// The task it repairs with (`+0x48`).
    pub task: RepairTask,
    /// The `MoveTo`'s range toward the target: the unit's master's weapon range (`+0x828`).
    pub range: Distance,
    /// `+0x6c`: the action counts among its target's repairers (`+0x9e8` of the object
    /// `FUN_140ad9050` gives: the target, an annex's main building).
    pub counted: bool,
    /// `+0x60` ([`Voice`]): a quiet Repair's end posts no notify 0x1fa.
    pub voice: Voice,
}

/// What a Repair's update reads of its target (the object its id names).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Repairee {
    /// Its state (`+0x640`).
    pub state: ObjectState,
    /// Its hit points (`+0x278`).
    pub hp: HitPoints,
    /// Its owner's id (`player+0x718`).
    pub owner: PlayerId,
    /// The repairer's player counts its owner as its own or an ally (`FUN_141663dc0` with the
    /// ally flag).
    pub allied: bool,
    /// Its place and its master's radii.
    pub position: Position,
    /// Its master's radii.
    pub radius: Footprint,
    /// Its master's class.
    pub class: ClassId,
    /// It moves (`FUN_140fdd080`: a master of type 30 and up with a speed above 0).
    pub moves: bool,
}

/// The object `FUN_140ad9050` counts a Repair's repairers in (the target, an annex's main
/// building) and their count (`+0x9e8`), None without one.
pub(crate) type CountedIn = Option<(ObjectId, BuilderCount)>;

/// What a Repair's update reads as it starts, each constant through the update: its target,
/// the object its repairers count in, and the repairer's own values ([`Unit::repair_reads`]).
///
/// [`Unit::repair_reads`]: crate::objects::unit::Unit::repair_reads
#[derive(Clone, Copy, Debug)]
pub(crate) struct RepairReads {
    /// The target, None when the world holds no object of its id.
    pub(crate) target: Option<Repairee>,
    /// The object `FUN_140ad9050` counts the repairers in and their count.
    pub(crate) counted_in: CountedIn,
    /// The repairer's master's radii.
    pub(crate) radius: Footprint,
    /// The game mode is one of `FUN_141302540`'s ([`crate::world::GameMode::scenario`]).
    pub(crate) scenario: bool,
    /// The repairer's work rate: its `mWorkRatePowerup` (`+0xa30`, slot 0x6e8) plus its
    /// master's `work_rate` (`+0x798`), in that order.
    pub(crate) rate: WorkRate,
    /// The motion step giving up makes the `MoveTo` wait once (a human player's villager).
    pub(crate) waits: bool,
    /// The unit AI's stance (`+0x1a9`), which sets the wait
    /// ([`crate::movement::move_to::MoveWait::give_up`]).
    pub(crate) stance: UnitStance,
    /// The master's standing sprite (`+0x470`), which slot 0x68 shows for none.
    pub(crate) standing: Option<SpriteId>,
    /// The master's terrain restriction, whose mask its quick path takes.
    pub(crate) restriction: Option<RestrictionId>,
}

#[cfg(test)]
mod tests;
