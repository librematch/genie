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

use crate::GenieError;
use crate::gathering::FarmStops;
use crate::gathering::StopIndex;
use crate::gathering::hunt::{HuntSprites, HuntTask};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::math::rng::Well512;
use crate::movement::MoveStats;
use crate::movement::TopSpeed;
use crate::movement::move_to::MoveDefaults;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkRate;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::sprite::ActiveSprite;
use crate::objects::sprite::Graphic;
use crate::objects::sprite::new_sprite;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::world::Seconds;
use crate::world::layer::generated::SpriteTuning;
use crate::world::settings::FarmMode;
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
use crate::combat::damage::Victim;
#[cfg(test)]
use crate::gathering::hunt::Dropped;
use crate::gathering::{DropSiteId, GatherTargetId};
use crate::math::image::gather::{
    LAST_BIT as GATHER_LAST_BIT, NO_GATHER_POINT as GATHER_NO_GATHER_POINT,
    REMAINDER as GATHER_REMAINDER,
};
use crate::movement::move_to::WalkTo;
use crate::objects::WorkerCount;
use crate::unit_ai::UnitStance;

/// What a gatherer's stand reads and writes (`FUN_140a8b6b0`'s sprite, shown with
/// [`new_sprite`]): the standing sprite (none keeps the one shown), the unit's sprite, the Sprite
/// stream that draws a new sprite's first frame and the sprite group's tuning values.
pub(crate) struct Stand<'world> {
    pub(crate) graphic: Option<Graphic>,
    pub(crate) sprite: &'world mut Option<ActiveSprite>,
    pub(crate) rng: &'world mut Well512,
    pub(crate) sprite_tuning: &'world SpriteTuning,
}

impl Stand<'_> {
    /// The unit stands: [`new_sprite`] with the standing sprite, if any, its top speed `stats`
    /// read with it.
    fn show(self, stats: Option<&mut MoveStats>) {
        if let Some(graphic) = self.graphic {
            new_sprite(self.sprite, stats, graphic, self.rng, *self.sprite_tuning);
        }
    }
}

/// The Gather action's type, which word 9 adds.
pub const GATHER: ActionKind = ActionKind::named(5);

/// `set_state(3)`'s timer: the search asks again once it counted up to 0 (`0xc1200000`, an
/// immediate in `0x140a8cd40`).
const SEARCH_WAIT: Seconds = Seconds::new(-10.0);
/// `set_state(0xf3)`'s timer: a second (`0xbf800000`, an immediate in `0x140a8cd40`).
const FARM_WAIT_TIME: Seconds = Seconds::new(-1.0);
/// `mGatherX` of none (`DAT_14330ec20`, −1.0, read at `0x140a8d1c6`).
const NO_GATHER_POINT: f32 = GATHER_NO_GATHER_POINT;

/// Below this a target's or a load's remainder counts as nothing (`DAT_142eb5690`).
const REMAINDER: Amount = Amount::new(GATHER_REMAINDER);
/// A target's last bit adds its master's first storage amount times this (`DAT_142eb1e9c`).
const LAST_BIT: f32 = GATHER_LAST_BIT;

/// A farm's stops under the game's farm setting (the constructor's `mNumberStops`).
const FARM_STOPS: FarmStops = FarmStops::of_data(4);
/// The draws a stop's point takes at most (`set_state(4)`'s loop).
const STOP_DRAWS: u32 = 10;
/// The player's attribute that sells a farm's food at once (`player+0x100` at `+0x300`).
pub(crate) const FARM_FOOD_SOLD: usize = 96;

/// A gatherer without movement state, which the Gather's walks and turns and its drop-off need.
pub(crate) const GATHERER_UNMOVING: GenieError =
    GenieError::unsupported("a gatherer without movement state");
/// A gatherer without its Gather or its movement state, which its orders, its walks and its
/// drop-off need.
pub(crate) const GATHERER_STATE_LACKING: GenieError =
    GenieError::unsupported("a gatherer without its gather or movement state");

/// The Gather action of a gatherer (`RGE_Action_Gather`, type 5), with the save's fields.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct GatherAction {
    /// The target object (`+0x20`, its id at `+0x30`), −1 for none.
    pub(crate) target: Option<GatherTargetId>,
    /// `+0x28` (its id at `+0x34`), the second target: the drop site the load goes to in state 8,
    /// −1 for none.
    pub(crate) carry_to: Option<DropSiteId>,
    /// The action state (`+0x18`), which word 9 adds.
    pub(crate) state: GatherState,
    /// `+0x38`..`+0x40`: where the target stood.
    pub(crate) point: Position,
    /// `+0x44`: the state's timer.
    pub(crate) timer: Seconds,
    /// `+0x48`: its task.
    pub(crate) task: HuntTask,
    /// `+0x68`, `mTargetType`: the target's class.
    pub(crate) target_class: Option<ClassId>,
    /// `+0x6c`, `mNumberStops`: 1, or a farm's stops.
    pub(crate) stops: FarmStops,
    /// `+0x6d`, `mCurrentStop`.
    pub(crate) current_stop: StopIndex,
    /// `+0x70`, `mTimePerStop`.
    pub(crate) time_per_stop: Seconds,
    /// `+0x74`, `mTimeAtCurrentStop`.
    pub(crate) time_at_stop: Seconds,
    /// `+0x78`, `+0x7c`: `mLastStopX`, `mLastStopY`.
    pub(crate) last_stop: Point,
    /// `+0x80`: the gathering started (`set_state(7)`); not saved.
    #[diff(unsaved)]
    pub(crate) worked: bool,
    /// `+0x84`, `+0x88`: `mFarmX`, `mFarmY`, a farm's place.
    pub(crate) farm: Point,
    /// `+0x8c`, `+0x90`: `mGatherX`, `mGatherY`, where the gathering started (−1 while the
    /// game's farm setting is walkable, [`GatherReads::farm_mode`]).
    pub(crate) gather_at: Point,
}

impl GatherAction {
    /// Its target and its second target as any objects' ids, −1 for none: what the releases
    /// compare and let go (slot 0x180).
    #[must_use]
    pub const fn targets(&self) -> [Option<ObjectId>; 2] {
        [
            match self.target {
                Some(role_id) => Some(role_id.id()),
                None => None,
            },
            match self.carry_to {
                Some(role_id) => Some(role_id.id()),
                None => None,
            },
        ]
    }
}

/// What the gather reads of its target when it takes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resource {
    /// The target.
    pub(crate) id: ObjectId,
    /// Its place.
    pub(crate) position: Position,
    /// Its master's class.
    pub(crate) class: ClassId,
    /// Its master's id.
    pub(crate) master: MasterId,
    /// Its owner.
    pub(crate) player: PlayerId,
    /// How many actions work on it (`+0x644`).
    pub(crate) workers: WorkerCount,
    /// The gatherer's master has its master (a building annex's main building's, `+0xbb8`) among
    /// its drop sites (`FUN_14109d4f0`).
    pub(crate) drop_site: bool,
    /// The footprint gap between the gatherer and it (`FUN_14118f680`), which the `MoveTo`'s start
    /// reads.
    pub(crate) gap: Distance,
    /// Its object state (`+0x640`), which a `MoveTo` to it reads (`FUN_140a7a230`).
    pub(crate) state: ObjectState,
    /// Its master's radii (`+0x5d8`, `+0x5dc`), over which a farm's stops spread.
    pub(crate) radius: Footprint,
}

impl Resource {
    /// The object a `MoveTo` of the gatherer walks to: its place, the footprint gap and its state.
    #[must_use]
    pub const fn walk_to(&self) -> WalkTo {
        WalkTo {
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
    /// (`set_state(0xf3)`,
    /// [`MasterRoles::FARM_WAITER`](crate::world::layer::MasterRoles::FARM_WAITER)).
    pub(crate) master: MasterId,
    /// The unit AI's stance (`+0x1a9`), 0 without a unit AI.
    pub(crate) stance: UnitStance,
    /// The game's setting at `FUN_140c87b90()`'s `+0x69` (farmers walk their farm's stops); with
    /// it `set_state(7)` leaves `mGatherX`, `mGatherY` as they are.
    pub(crate) farm_mode: FarmMode,
    /// The gatherer's player's type (`player+0x10`): a computer player's (`FUN_141659cf0`)
    /// moved gatherer is done at the point.
    pub(crate) player_kind: PlayerKind,
    /// The player's attribute the task's `attribute_type2` names (none: 1.0), as the time per
    /// stop reads it.
    pub(crate) stop_productivity: Amount,
    /// The player's attribute 96: a farm's food is sold at once.
    pub(crate) food_sold: Amount,
    /// What the gatherer's slot 0x208 reads besides its mover's own speed and its place
    /// ([`Walks::top_speed`](crate::gathering::gather::Walks::top_speed)).
    pub(crate) top_speed: TopSpeed,
}

/// What an update asks of the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
        /// How the walk ended.
        end: WalkEnd,
    },
}

/// How State 0xf2's walk ([`Next::Reseed`]) ended: it arrived (codes 1, 2 and 5) or failed (3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WalkEnd {
    /// It arrived.
    Arrived,
    /// It failed.
    Failed,
}

#[cfg(test)]
mod tests;
