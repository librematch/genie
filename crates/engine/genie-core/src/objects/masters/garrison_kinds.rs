//! The kinds of units a building takes in ([`GarrisonKinds`], `garrison_type`).

bitflags::bitflags! {
    /// The kinds of units a building takes in (`garrison_type`, `+0x9bc`), by the classes the
    /// switch of `FUN_14120eb10` gives each bit. Every bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct GarrisonKinds: u8 {
        /// Villagers, trade carts and kings (bit 0: classes 4, 19, 59).
        const VILLAGERS = 1;
        /// Archers and infantry (bit 1: classes 0, 6, 35, 44, 45, 46, 50, 56).
        const INFANTRY = 1 << 1;
        /// Cavalry (bit 2: classes 12, 23, 36, 47, 57).
        const CAVALRY = 1 << 2;
        /// Monks (bit 3: classes 18, 42, 43).
        const MONKS = 1 << 3;
        /// Herdables (bit 4: class 58).
        const HERDABLES = 1 << 4;
        /// Siege (bit 5: classes 13, 51, 54, 55).
        const SIEGE = 1 << 5;
        /// Ships (bit 6: classes 2, 20, 21, 22, 53).
        const SHIPS = 1 << 6;
        /// Any other bit.
        const _ = !0;
    }
}

impl GarrisonKinds {
    /// No kind.
    pub const NONE: Self = Self::empty();
}

impl genie_diff::Diff for GarrisonKinds {
    /// By equality: the flags are one value.
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}
