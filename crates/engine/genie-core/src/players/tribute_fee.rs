//! The fee a tribute costs.

/// A tribute entry's fee (`+0x14`.., a float): the share of the amount the giver pays on top
/// (the market's tribute fee); a negative fee counts as 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct TributeFee(f32);

impl TributeFee {
    /// No fee.
    pub const ZERO: Self = Self(0.0);

    /// The fee of the packet's float `value`.
    #[must_use]
    pub const fn of_data(value: f32) -> Self {
        Self(value)
    }

    /// The float.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::players::TributeFee;

    /// The float reads back.
    #[test]
    fn the_fee_reads_back() {
        assert_eq!(TributeFee::of_data(0.3).get(), 0.3);
        assert_eq!(TributeFee::default(), TributeFee::ZERO);
    }
}
