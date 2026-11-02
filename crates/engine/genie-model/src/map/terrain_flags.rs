//! A terrain's flags ([`TerrainFlags`], the `.dat`'s `is_water` byte).

bitflags::bitflags! {
    /// A terrain's flags: the `.dat`'s `is_water` byte, which the map's load copies to the
    /// terrain table (`FUN_1413fc340`, `DAT_143cc7b40 + 0x2c`). Named by the terrains of build
    /// 158041's `.dat` that carry each bit; every bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct TerrainFlags: u8 {
        /// Water (bit 0: `Water, Medium`, the normal waters).
        const WATER = 1;
        /// Deep water (bit 1: `Water, Deep`, `Water, Deep Ocean`).
        const DEEP_WATER = 1 << 1;
        /// Shallow water (bit 2: `Water, Shallow`, `Water, Azure`).
        const SHALLOW_WATER = 1 << 2;
        /// Shallows land units wade (bit 3: `Shallows`, `Swamp`, the rice farms).
        const SHALLOWS = 1 << 3;
        /// A beach (bit 4: `Beach`, `Beach, Wet`), where land units and ships meet.
        const BEACH = 1 << 4;
        /// Land (bit 5: `Grass`, `Desert`, the forests).
        const LAND = 1 << 5;
        /// Ice (bit 6, beside land, shallows or beach: `Ice`, `Ice, Beach`).
        const ICE = 1 << 6;
        /// Snow (bit 7, `FUN_1413fc280`).
        const SNOW = 1 << 7;
        /// Any other bit.
        const _ = !0;
    }
}

impl TerrainFlags {
    /// No flag.
    pub const NONE: Self = Self::empty();
    /// The water bits (bits 0 to 2, `FUN_1413fc2b0`).
    pub const ANY_WATER: Self = Self::WATER
        .union(Self::DEEP_WATER)
        .union(Self::SHALLOW_WATER);
    /// The shore's bits, shallows and beach (bits 3 and 4, `FUN_1413fc270`).
    pub const SHORE: Self = Self::SHALLOWS.union(Self::BEACH);
}
