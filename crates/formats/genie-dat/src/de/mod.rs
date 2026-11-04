//! The data file of Age of Empires II: Definitive Edition, file versions VER 7.1 to VER 8.9.
//!
//! Ported from genieutils (github.com/Tapsa/genieutils, LGPL-3.0; the library the game ships with
//! Advanced Genie Editor in `Tools_Builds/docs/Source.7z`): every struct follows its class's
//! `serializeObject`, and every version gate compares genieutils' `GameVersion` ordinals, of
//! which the DE files take `GV_C14` (VER 7.1) to `GV_C32` (VER 8.9). Gates that are constant over that
//! range (the `AoE`, `AoK` and SWGB ones) are folded.
//!
//! Counts are kept as the file stores them, so a file reads and writes back byte for byte; a
//! caller that changes a list's length updates its count. The file on disk is raw-deflate
//! compressed: [`DatFile::read_from`] inflates, [`DatFile::to_bytes`] gives the inflated bytes.
//!
//! The errors are [`GenieError`](genie_error::GenieError)s: reading, inflating or writing that
//! fails underneath is [`Io`](genie_error::ErrorKind::Io), a file version other than `VER 7.1`
//! to `VER 8.9` [`Unsupported`](genie_error::ErrorKind::Unsupported), data that does not
//! follow the format [`InvalidInput`](genie_error::ErrorKind::InvalidInput).

// `Seek` stays unimported: binrw's `count` expansion calls `binrw::io::Seek::stream_position`,
// which an imported `Seek` would make an unnecessary qualification.

mod file;
mod graphics;
mod random_maps;
mod techs;
mod terrain;
mod unit_parts;
mod units;

pub use self::file::{DatFile, DebugString, Version};
use self::file::{read_present, write_present};
pub use self::graphics::{
    Graphic, GraphicAngleSound, GraphicDelta, PlayerColour, Sound, SoundItem,
};
pub use self::random_maps::{
    MapElevation, MapInfo, MapInfoHead, MapLand, MapTerrain, MapUnit, RandomMaps,
};
pub use self::techs::{
    BuildingConnection, Connections, Effect, EffectCommand, ResearchConnection, ResearchLocation,
    ResearchResourceCost, Tech, TechTree, TechTreeAge, TechTreeCommon, UnitConnection,
};
pub use self::terrain::{
    FrameData, Terrain, TerrainBlock, TerrainPassGraphic, TerrainRestriction, TileSize,
};
pub use self::unit_parts::{
    AttackOrArmor, Bird, Building, BuildingAnnex, Creatable, DeadFish, Projectile, ResourceCost,
    TrainLocation, Type50,
};
pub use self::units::{Civ, DamageGraphic, ResourceStorage, Task, Unit, UnitHeader};

/// genieutils' unit types (`UT_*`).
pub mod unit_kind {
    /// Flags and other objects that move: the first type with a `speed`.
    pub const FLAG: u8 = 20;
    /// Dead units and fish: the first type with a movement part (`dead_fish`).
    pub const DEAD_FISH: u8 = 30;
    /// Birds: the first type with an action part and tasks (`bird`).
    pub const BIRD: u8 = 40;
    /// Combatants: the first type with a combat part (`type50`).
    pub const COMBATANT: u8 = 50;
    /// Projectiles: the only type with a projectile part.
    pub const PROJECTILE: u8 = 60;
    /// Trainable units: the first type with a creation part (`creatable`).
    pub const CREATABLE: u8 = 70;
    /// Buildings: the only type with a building part.
    pub const BUILDING: u8 = 80;
    /// Age of Empires' trees: despite the high number, only the part every unit has.
    pub const AOE_TREES: u8 = 90;
}
