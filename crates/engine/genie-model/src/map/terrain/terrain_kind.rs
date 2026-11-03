//! What the `.dat` says of a terrain: its record as the map's terrain reads see it and its class
//! bit in the restrictions' masks (FD-046: the model's part of the terrain).

use crate::map::TerrainFlags;
use crate::map::TerrainMask;
use serde::Deserialize;
use serde::Serialize;

/// What the `.dat` says of a terrain (its record at map `+0x6238`, 0x1f0 bytes each).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainKind {
    /// The record's first byte (`enabled`) or its `terrain_to_draw` (`+0xf8`) of 0 or more: the
    /// map's terrain reads (`FUN_140f690e0`, `FUN_140f69000`) see the terrain, else terrain 0.
    pub shown: bool,
    /// The `.dat`'s `is_water` byte, which the map's load copies to the terrain table
    /// (`FUN_1413fc340`, `DAT_143cc7b40 + 0x2c`).
    pub flags: TerrainFlags,
    /// A forest (`FUN_140f5efb0`): the densities of its terrain units of class 15 (Gaia's
    /// masters) sum to more than 249.
    pub forest: bool,
}

/// The class bit of a terrain's flags (the `.dat`'s `is_water` byte) in the terrain
/// restrictions' masks (the load's table `FUN_141054630` reads): none for no flag, 1 for water
/// ([`TerrainFlags::ANY_WATER`]), 2 for the shore ([`TerrainFlags::SHORE`]), else land
/// ([`TerrainFlags::LAND`]) as 4.
#[must_use]
pub const fn terrain_class(flags: TerrainFlags) -> TerrainMask {
    TerrainMask::of_bits(if flags.is_empty() {
        0
    } else if flags.intersects(TerrainFlags::ANY_WATER) {
        1
    } else if flags.intersects(TerrainFlags::SHORE) {
        2
    } else {
        (flags.bits() >> 3) & 4
    })
}
