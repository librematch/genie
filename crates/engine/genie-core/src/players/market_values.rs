//! The market's values: a commodity's price, its swing and its net trades.

/// A commodity's price in gold (a float of the world, `+0x868`..`+0x880`), and the world's least
/// and most price.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Price(f32);

impl Price {
    /// The price of the save's or the image's float `value`.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// The float.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }

    /// The price a purchase's step raises it to (one `f32` add, the price first).
    #[must_use]
    pub fn raised(self, step: f32) -> Self {
        Self(self.0 + step)
    }

    /// The price a sale's step lowers it to (one `f32` subtract).
    #[must_use]
    pub fn lowered(self, step: f32) -> Self {
        Self(self.0 - step)
    }
}

/// A commodity's swing (the save's `old_*_price_adj`, a float), which every trade grows by 2.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct PriceSwing(f32);

impl PriceSwing {
    /// None (the swing after the net trades came back to 0).
    pub const ZERO: Self = Self(0.0);

    /// The swing of the save's float `value`.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// The float.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

/// A commodity's purchases less its sales (the save's `*_price_delta`, an int).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetTrades(i32);

impl NetTrades {
    /// None.
    pub const ZERO: Self = Self(0);

    /// The count of the save's int `value`.
    #[must_use]
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// One more purchase, or one more sale (wrapping as the game's int).
    #[must_use]
    pub const fn traded(self, buy: bool) -> Self {
        Self(if buy {
            self.0.wrapping_add(1)
        } else {
            self.0.wrapping_sub(1)
        })
    }
}

impl genie_diff::Diff for Price {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

impl genie_diff::Diff for PriceSwing {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

impl genie_diff::Diff for NetTrades {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// The lot a market trade moves (`commodity_volumn_delta`, `+0x140c`, an int, saved; 100): a
/// sale needs that much and a lot is worth that many times the price. None below 1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LotVolume(i32);

impl LotVolume {
    /// The volume of the save's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// Whether a trade moves anything: 1 and up.
    #[must_use]
    pub const fn any(self) -> bool {
        0 < self.0
    }

    /// The volume as a float (`CVTSI2SS`).
    #[must_use]
    pub const fn as_f32(self) -> f32 {
        self.0 as f32
    }

    /// The volume negated as a float (the sale's change, `NEG` then `CVTSI2SS`).
    #[must_use]
    pub const fn negated_f32(self) -> f32 {
        self.0.wrapping_neg() as f32
    }
}

impl genie_diff::Diff for LotVolume {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::players::{LotVolume, NetTrades, Price, PriceSwing};

    /// A step raises or lowers a price by one `f32` operation; the net trades count both ways.
    #[test]
    fn the_market_values_move_by_the_game_operations() {
        let p = Price::new(1.3);
        assert_eq!(p.raised(0.02).get().to_bits(), (1.3_f32 + 0.02).to_bits());
        assert_eq!(p.lowered(0.02).get().to_bits(), (1.3_f32 - 0.02).to_bits());
        assert!(Price::new(0.2) < p);
        assert_eq!(NetTrades::ZERO.traded(true).traded(false), NetTrades::ZERO);
        assert_eq!(
            NetTrades::new(i32::MIN).traded(false),
            NetTrades::new(i32::MAX)
        );
        assert_eq!(PriceSwing::default(), PriceSwing::ZERO);
    }

    /// A volume trades from 1; its floats are the int's and its negation's.
    #[test]
    fn volume_trades_from_one() {
        assert!(LotVolume::of_data(1).any() && !LotVolume::of_data(0).any());
        assert_eq!(
            LotVolume::of_data(100).as_f32().to_bits(),
            100.0f32.to_bits()
        );
        assert_eq!(
            LotVolume::of_data(100).negated_f32().to_bits(),
            (-100.0f32).to_bits()
        );
    }
}
