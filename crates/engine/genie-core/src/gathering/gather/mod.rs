//! The Gather action (`RGE_Action_Gather`, type 5, vtable `0x142ed4f28`): a forager sent to a
//! resource walks to it (state 4, a quiet `MoveTo`), faces it (6), gathers (7), takes the load to
//! a drop site (8) and walks back to the resource (4).
//!
//! 158041: constructor `FUN_140a88310`, start (slot 5) `0x140a8b3a0`, update (slot 6)
//! `0x140a8e1d0`, `set_state` (slot 16) `0x140a8cd40`; slot 0x40 `0x140a8b740`, slot 0x48
//! `0x140a903a0`.
//!
//! A farm under the game's farm setting has four stops (`mNumberStops`): the farmer walks to a
//! random point on the farm (`set_state(4)`, the Action stream's `"set_state"` draws), gathers
//! there for the time per stop, then walks to the next.
//!
//! The searches (`set_state` 3, 0xf3) and the walks to a point (11, 15, 0xf2) are in
//! `point_walks.rs`; the face, the work, the drop-off's update and the random waits after a failed
//! walk or a drop-off out of reach (the Action stream's `"update"` draws) are in `work.rs`.

use crate::gathering::FarmStops;
use crate::gathering::StopIndex;
use crate::gathering::hunt::{HuntSprites, HuntTask};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::movement::move_to::{MoveDefaults, MoveToAction};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkRate;
use crate::objects::masters::ActionType;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::world::Seconds;
use genie_diff::Diff;

mod gather_state;
mod point_walks;
mod start;
pub(crate) mod unit;
mod update;
mod walks;
mod work;

pub use self::gather_state::GatherState;
pub use self::point_walks::waits_for_farm;
pub use self::start::{
    GatherRetarget, GatherStart, MoveTarget, StopRate, productivity_of, stop_point,
};
pub use self::walks::Walks;
#[cfg(test)]
use crate::GenieError;
#[cfg(test)]
use crate::combat::damage::Victim;
#[cfg(test)]
use crate::gathering::hunt::Dropped;

/// The Gather action's type, which word 9 adds.
pub const GATHER: ActionType = ActionType::named(5);

/// `set_state(3)`'s timer: the search asks again once it counted up to 0 (`0xc1200000`, an
/// immediate in `0x140a8cd40`).
const SEARCH_WAIT: Seconds = Seconds::new(-10.0);
/// `set_state(0xf3)`'s timer: a second (`0xbf800000`, an immediate in `0x140a8cd40`).
const FARM_WAIT_TIME: Seconds = Seconds::new(-1.0);
/// `mGatherX` of none (`DAT_14330ec20`, −1.0, read at `0x140a8d1c6`).
const NO_GATHER_POINT: f32 = crate::math::image::gather::NO_GATHER_POINT;
/// The gatherers whose gather without a target waits for another farm (`set_state(0xf3)`,
/// `0x140a8e1d0`'s cases 3, 5 and 6: masters 259, 214, 1891 and 1892).
const FARM_WAITERS: [MasterId; 4] = [
    MasterId::named(259),
    MasterId::named(214),
    MasterId::named(1891),
    MasterId::named(1892),
];
/// The gatherers that wait so on a target of the farm class (masters 13, 2356 and 2171, the raw
/// `0x60a9`, `0x69d0`, `0x6917` of the same cases).
const TRAP_WAITERS: [MasterId; 3] = [
    MasterId::named(13),
    MasterId::named(2356),
    MasterId::named(2171),
];

/// Below this a target's or a load's remainder counts as nothing (`DAT_142eb5690`).
const REMAINDER: Amount = Amount::new(crate::math::image::gather::REMAINDER);
/// A target's last bit adds its master's first storage amount times this (`DAT_142eb1e9c`).
const LAST_BIT: f32 = crate::math::image::gather::LAST_BIT;
/// A drop-off reaches its drop site within this squared footprint gap (`DAT_14330eb8c`).
const DROP_REACH: f32 = crate::math::image::gather::DROP_REACH;
/// A random wait's shift (`DAT_142e7f384`).
const WAIT_SHIFT: f32 = crate::math::image::gather::WAIT_SHIFT;
/// A failed walk to a target with at most this many workers waits (`0x140a8e1d0` case 3,
/// `DAT_142eb1c98`), as one to a target at least `WAIT_GAP` away (the squared footprint gap,
/// `DAT_142ec9ec0`); else the gatherer searches.
const WAIT_WORKERS: f32 = crate::math::image::gather::WAIT_WORKERS;
const WAIT_GAP: f32 = crate::math::image::gather::WAIT_GAP;
/// The masters a gatherer may gather from while they hold nothing (the combat object's slot
/// 0x2b8, `FUN_141216390`: the farm, the rice farm and master 45).
const REGATHERED: [MasterId; 3] = [
    MasterId::named(50),
    MasterId::named(1187),
    MasterId::named(45),
];
/// Masters the constructor and `set_state(6)` treat apart (357 and 1188, `FUN_140a8b140`).
const SPECIAL_TARGETS: [MasterId; 2] = [MasterId::named(357), MasterId::named(1188)];
/// The farms whose gather goes in stops under the game's farm setting, and whose time per stop
/// each update renews: the farm and the rice farm (the constructor's and the update's masters).
const STOP_FARMS: [MasterId; 2] = [MasterId::named(50), MasterId::named(1187)];
/// A farm's stops under the game's farm setting (the constructor's `mNumberStops`).
const FARM_STOPS: FarmStops = FarmStops::of_data(4);
/// A stop's point lies this far into its tile (`DAT_142e7f384`).
const STOP_CENTRE: f32 = crate::math::image::gather::STOP_CENTRE;
/// The productivity of a task without a productivity attribute (`DAT_14330eb8c`).
const DEFAULT_PRODUCTIVITY: Amount = Amount::new(crate::math::image::gather::DEFAULT_PRODUCTIVITY);
/// The draws a stop's point takes at most (`set_state(4)`'s loop).
const STOP_DRAWS: u32 = 10;
/// The player's attribute that sells a farm's food at once (`player+0x100` at `+0x300`).
pub(crate) const FARM_FOOD_SOLD: usize = 96;

/// The Gather action of a gatherer (`RGE_Action_Gather`, type 5), with the save's fields.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct GatherAction {
    /// The target object (`+0x20`, its id at `+0x30`), −1 for none.
    pub target: Option<ObjectId>,
    /// `+0x28` (its id at `+0x34`), the second target: the drop site the load goes to in state 8,
    /// −1 for none.
    pub carry_to: Option<crate::gathering::DropSiteId>,
    /// The action state (`+0x18`), which word 9 adds.
    pub state: GatherState,
    /// `+0x38`..`+0x40`: where the target stood.
    pub point: Position,
    /// `+0x44`: the state's timer.
    pub timer: Seconds,
    /// `+0x48`: its task.
    pub task: HuntTask,
    /// `+0x68`, `mTargetType`: the target's class.
    pub target_class: Option<ClassId>,
    /// `+0x6c`, `mNumberStops`: 1, or a farm's stops.
    pub stops: FarmStops,
    /// `+0x6d`, `mCurrentStop`.
    pub current_stop: StopIndex,
    /// `+0x70`, `mTimePerStop`.
    pub time_per_stop: Seconds,
    /// `+0x74`, `mTimeAtCurrentStop`.
    pub time_at_stop: Seconds,
    /// `+0x78`, `+0x7c`: `mLastStopX`, `mLastStopY`.
    pub last_stop: Point,
    /// `+0x80`: the gathering started (`set_state(7)`); not saved.
    #[diff(unsaved)]
    pub worked: bool,
    /// `+0x84`, `+0x88`: `mFarmX`, `mFarmY`, a farm's place.
    pub farm: Point,
    /// `+0x8c`, `+0x90`: `mGatherX`, `mGatherY`, where the gathering started (−1 while the
    /// game's farm setting holds them, [`GatherReads::farm_stops`]).
    pub gather_at: Point,
    /// The sub-action of states 4, 8, 11, 15 and 0xf2: the quiet `MoveTo` (the one component,
    /// [`MoveToAction`]) to the target, the drop site or a point, which `set_state` makes
    /// ([`GatherAction::set_walk`], [`GatherAction::set_drop`], [`GatherAction::walk_back`]).
    pub move_to: Option<MoveToAction>,
}

/// What the gather reads of its target when it takes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resource {
    /// The target.
    pub id: ObjectId,
    /// Its place.
    pub position: Position,
    /// Its master's class.
    pub class: ClassId,
    /// Its master's id.
    pub master: MasterId,
    /// Its owner.
    pub player: PlayerId,
    /// How many actions work on it (`+0x644`).
    pub workers: crate::objects::WorkerCount,
    /// The gatherer's master has its master (a building annex's main building's, `+0xbb8`) among
    /// its drop sites (`FUN_14109d4f0`).
    pub drop_site: bool,
    /// The footprint gap between the gatherer and it (`FUN_14118f680`), which the `MoveTo`'s start
    /// reads.
    pub gap: Distance,
    /// Its object state (`+0x640`), which a `MoveTo` to it reads (`FUN_140a7a230`).
    pub state: ObjectState,
    /// Its master's radii (`+0x5d8`, `+0x5dc`), over which a farm's stops spread.
    pub radius: Footprint,
}

impl Resource {
    /// The object a `MoveTo` of the gatherer walks to: its place, the footprint gap and its state.
    #[must_use]
    pub const fn walk_to(&self) -> crate::movement::move_to::WalkTo {
        crate::movement::move_to::WalkTo {
            id: Some(self.id),
            at: self.position,
            gap: self.gap,
            state: Some(self.state),
        }
    }
}

/// What the Gather's update reads of the gatherer and its player as it starts.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GatherReads {
    /// The master's work rate (`+0x798`) plus the unit's powerup (slot 0x6e8, `+0xa30`).
    pub(crate) work_rate: WorkRate,
    /// The master's `attribute_max_amount` (`+0x61c`): what the unit carries at most.
    pub(crate) capacity: Amount,
    /// The player's attribute the task's `attribute_type2` names, when above 0; else 1.
    pub(crate) productivity: Amount,
    /// The sprites the gather shows.
    pub(crate) sprites: HuntSprites,
    /// The unit's master's class: a villager's `MoveTo` waits once when the motion step gives up.
    pub(crate) class: ClassId,
    /// What the `MoveTo`'s constructors take for a sprite when the task gives none
    /// (`FUN_140aa0d30`, `FUN_140aa0df0`).
    pub(crate) defaults: MoveDefaults,
    /// The unit's master: a farmer's gather without a target waits for another farm
    /// (`set_state(0xf3)`, [`FARM_WAITERS`]).
    pub(crate) master: MasterId,
    /// The unit AI's stance (`+0x1a9`), 0 without a unit AI.
    pub(crate) stance: crate::unit_ai::UnitStance,
    /// The game's setting at `FUN_140c87b90()`'s `+0x69` (farmers walk their farm's stops); with
    /// it `set_state(7)` leaves `mGatherX`, `mGatherY` as they are.
    pub(crate) farm_stops: bool,
    /// The gatherer's player is a computer (`FUN_141659cf0`): its moved gatherer is done at the
    /// point.
    pub(crate) computer: bool,
    /// The player's attribute the task's `attribute_type2` names (none: 1.0), as the time per
    /// stop reads it.
    pub(crate) stop_productivity: Amount,
    /// The player's attribute 96: a farm's food is sold at once.
    pub(crate) food_sold: Amount,
}

/// What an update asks of the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    /// Nothing.
    Stay,
    /// `set_state(8)`: the drop site search, then the walk there
    /// ([`GatherAction::set_drop`]).
    Drop,
    /// The update returned 1 (state 1): the action list removes the Gather, whose teardown lets
    /// its targets go.
    Done,
    /// `set_state(6)` on a target not built yet (state below 2): the unit's build command on it
    /// (slot 0x380 forced, `FUN_14120d410`; the caller's, after the update), which replaces the
    /// Gather.
    Build,
    /// `set_state(3)` or its 10 seconds past, or `set_state(0xf3)`: the caller posts the unit
    /// AI's notify 0x202 (the gather action `0x261`, the target type, the drop site: the second
    /// target, else the one `FUN_1411671e0` finds, 0 for none, −1 for a master without drop
    /// sites).
    Search,
    /// State 0xf2's walk ended (`0x140a8e1d0` case 0xf1): the caller reseeds the farm
    /// ([`crate::world::World::reseed`], `FUN_140a8aa60`, which sends the gatherer to build it);
    /// without a reseed (the interface's event 0x7f, 0x9c for a gatherer not a villager, when it
    /// has a farm spot) a walk that arrived leaves a computer player's gatherer done
    /// (`set_state(1)`), a human's idle (`set_state(2)`), and a failed walk idles both
    /// ([`GatherAction::after_reseed`]).
    Reseed {
        /// The walk arrived (codes 1, 2 and 5), else it failed (code 3).
        arrived: bool,
    },
}

#[cfg(test)]
mod tests;
