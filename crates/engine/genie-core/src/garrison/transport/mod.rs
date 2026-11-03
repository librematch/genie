//! The Transport action (`RGE_Action_Transport`, type 12, vtable `0x142ed61e8`): a transport's
//! sail to a point and its unload there. The unit's unload command (slot 0x3a8, `FUN_14108d050`,
//! which the unit AI's unload order 0x2d1 runs) makes one at the unit's place and gives it the
//! order's point as its work ([`TransportAction::work`]): near enough it unloads at once (state
//! 6), else it sails toward the point (state 4, a quiet `MoveTo` sub-action to the last point of
//! the planner's path that a unit can cross, [`Shore::shore_point`]) and unloads where that ends.
//! The unload (`FUN_140aa3ba0`, the caller's) puts the units inside ashore where a drop location
//! is free (`World::drop_location`); when none goes, the action sails to a random point about
//! the old one ([`retry_point`]) and tries again.
//!
//! 158041: the constructor `FUN_140aa3ab0`, `first_in_stack` `0x140aa3e10` (slot 5), update
//! `0x140aa4640` (slot 6), stop `0x140aa4620` (slot 7), `move_to` `0x140aa3e40` (slot 8), work
//! `0x140aa4830` (slot 9), `set_state` `0x140aa42a0` (slot 16), the shore search `FUN_140aa3f40`
//! (HD's `next_to_drop_off_terrain`).

use crate::GenieError;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::TerrainFlags;
use crate::map::TerrainId;
use crate::map::Tile;
use crate::movement::move_to::{MoveDefaults, MoveToReads};
use crate::objects::SpriteId;
use crate::objects::UpdateCode;
use crate::objects::masters::ActionKind;
use crate::objects::sprite::Graphic;
use crate::pathing::zones::ZoneId;
use genie_diff::Diff;

mod action;
mod shore;
mod transport_mode;
mod transport_state;
mod unit;
mod unload;
mod work;

pub use self::shore::Around;
pub(crate) use self::shore::MapShore;
pub(crate) use self::shore::{DropDraws, in_zone, retry_point};
pub use self::transport_mode::TransportMode;
pub use self::transport_state::TransportState;
use crate::map::RestrictionId;
use crate::math::image::transport::{DRAW_SCALE as TRANSPORT_DRAW_SCALE, NONE as TRANSPORT_NONE};
#[cfg(test)]
use crate::math::rng::Well512;
use crate::objects::masters::TaskId;

/// The Transport action's type, which word 9 adds.
pub const TRANSPORT: ActionKind = ActionKind::named(12);
/// A Transport's update without its action in the unit's list.
pub(crate) const NO_TRANSPORT: GenieError =
    GenieError::unreachable("a Transport's update without its action");

/// The action's point while it has none (`DAT_14330ec20`).
pub const NONE: f32 = TRANSPORT_NONE;
/// The divisor that cuts a 15-bit draw to 0..10 tiles (`FUN_140aa3ba0`'s `0xccc`).
const RETRY_STEP: u32 = 0xccc;
/// The new point's draws (`FUN_140aa3ba0`).
const RETRIES: i32 = 100;
/// The terrains a new point must leave: 22, 23 and 1, the waters (`FUN_140aa3ba0`).
const DEEP_WATER: TerrainId = TerrainId::named(0x16);
const WATER: TerrainId = TerrainId::named(1);
/// A drop location's draw scale (`DAT_142eb69cc`) and the gap past the footprints
/// (`DAT_142eb99cc`).
const DRAW_SCALE: f32 = TRANSPORT_DRAW_SCALE;
/// The draws `World::drop_location` makes (`FUN_141192d10`: 50).
const DROP_TRIES: u16 = 50;

/// A drop location's first draw of side 4 (a draw of `0x7fff`), which keeps the place the
/// stack holds: not modelled.
pub const STACK_PLACE: GenieError =
    GenieError::unsupported("a drop location's first draw of side 4 (the place the stack holds)");

/// Who calls a Transport's start (slot 5, [`TransportAction::start`]): the action list's add,
/// which passes 1, or another caller (0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartCall {
    /// The action list's add.
    Added,
    /// Another call.
    Other,
}

/// What a Transport's work order (slot 9, [`TransportAction::work`]) names: the ship itself,
/// as the unload command's does (out of reach it sails to unload), or another object or none
/// (it sails).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportTarget {
    /// The ship itself.
    Ship,
    /// Another object, or none.
    Other,
}

/// A unit's Transport action.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct TransportAction {
    /// The action state (`+0x18`), which word 9 adds, with its sail's `MoveTo` sub-action to
    /// the point within `TransportTuning::move_range`: quiet, with the sprite [`Ship::given`] (`set_state` 4
    /// and 11).
    pub state: TransportState,
    /// `+0x34` (the base's `+0x30` pair's second): how it was last ordered.
    pub mode: TransportMode,
    /// `+0x38`..`+0x40`: where it sails or unloads; x [`NONE`] for nowhere.
    pub point: Position,
    /// `+0x68`: the unload found no drop location and the action sails to a new point.
    pub retarget: bool,
    /// `+0x48`: the task it works on, none from the unload command (the work command's
    /// `LAB_1410706a8` retakes only a Transport on the task it finds).
    pub task: Option<TaskId>,
}

/// What the Transport reads of its unit.
#[derive(Clone, Copy, Debug)]
pub struct Ship<'a> {
    /// The unit's place.
    pub position: Position,
    /// Its master's radii (x, y).
    pub radius: Footprint,
    /// An object is inside it (`+0x2f8` holds one).
    pub loaded: bool,
    /// The map's size in tiles (the owner's world's map, `+0x18`, `+0x1c`).
    pub map: GridSize,
    /// What a `MoveTo` that `set_state` makes reads of the unit ([`MoveToReads`]): its speed
    /// (slot 0x208: without one the `MoveTo` starts failed) and the sprites' defaults.
    pub walk: MoveToReads<'a>,
    /// The sprite its maker gives a `MoveTo` (`+0x58`): the task's moving one, without a task
    /// the master's walking sprite ([`TransportAction::given_sprite`]).
    pub given: Option<SpriteId>,
}

/// What the Transport reads of the map about its unit (the caller's).
pub trait Shore {
    /// `FUN_1411958c0`: the unit's zone (its restriction's zone map, `FUN_1411931c0`) at `tile`:
    /// 0 for a tile below 0, or without a zone map.
    ///
    /// # Errors
    ///
    /// A map the caller cannot read.
    fn zone(&mut self, tile: Tile) -> Result<ZoneId, GenieError>;
    /// `FUN_1411a7570`: a tile of zone `zone` lies within `reach` of the unit's tile (its place
    /// cut by `CVTTSS2SI`, [`crate::pathing::zones::ZoneMap::near`]); no without a zone map.
    ///
    /// # Errors
    ///
    /// A map the caller cannot read.
    fn near(&mut self, zone: ZoneId, reach: Distance) -> Result<bool, GenieError>;
    /// The terrain table's flags of `tile`, its x and y shorts
    /// ([`crate::map::terrain::Terrain::flags_at`]).
    fn flags(&self, tile: Tile) -> TerrainFlags;
    /// `FUN_140f7c390` with the planner of the unit's restriction (`FUN_140f7cb30`, none without
    /// a master): the last point of the planner's path from `from` to `to` whose tile has an edge
    /// ([`crate::pathing::tile_path::Planner::last_open_point`]); None without a planner or such a point.
    ///
    /// # Errors
    ///
    /// What the planner's searches fail on.
    fn shore_point(&mut self, from: Point, to: Point) -> Result<Option<Point>, GenieError>;
}

/// What a state change leaves to its caller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changed {
    /// A `MoveTo` sub-action went (`FUN_140a98620`): its teardown stops the unit (slot 0x5d0).
    pub stopped: bool,
    /// State 2: the unit shows its master's standing sprite (slot 0x68 with `+0x470`).
    pub stands: bool,
}

/// What the Transport's update reads of its unit's master ([`TransportAction::update`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TransportReads {
    /// The master's radii (x, y).
    pub(crate) radius: Footprint,
    /// The master's terrain restriction (`+0x606`).
    pub(crate) restriction: Option<RestrictionId>,
    /// The sprites a `MoveTo` the update's `set_state` makes takes when its maker gives none.
    pub(crate) defaults: MoveDefaults,
    /// The sprite its maker gives it ([`Ship::given`]).
    pub(crate) given: Option<SpriteId>,
    /// The master's standing sprite (`+0x470`), which state 2 shows.
    pub(crate) standing: Option<Graphic>,
}

/// What a state of the update asks of its caller ([`TransportAction::state_update`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Updated {
    /// The update's code: 1 done, 3 failed (the action list removes it), else 0.
    Code(UpdateCode),
    /// State 6 unloads (`FUN_140aa3ba0`, the caller's after the actions), then
    /// [`TransportAction::unloaded`]; its code is 0.
    Unload,
}

#[cfg(test)]
mod tests;
