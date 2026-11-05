//! What a salvage pile holds of six attributes.

/// A combat object's `attr_amt` (`+0x990`..`+0x99a`, six saved shorts): what a salvage pile
/// (class 40) holds of six attributes. Its decay takes one off each (`TRIBE_Combat_Object::update`,
/// [`SalvageAmounts::decay`]); a Gather on the pile takes its load off one
/// (`TRIBE_Action_Gather::slot6`, not ported). The constructors start every object's at 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SalvageAmounts([i16; 6]);

impl SalvageAmounts {
    /// None of the six (the constructors' zeroes).
    pub const NONE: Self = Self([0; 6]);

    /// The amounts of the save's six shorts `values`.
    #[must_use]
    pub const fn of_data(values: [i16; 6]) -> Self {
        Self(values)
    }

    /// The six shorts.
    #[must_use]
    pub const fn get(self) -> [i16; 6] {
        self.0
    }

    /// A decay's step (`DEC word ptr` on each, `0x1412086e5`..`0x141208708`): one less of each,
    /// wrapping as the game's shorts do.
    pub fn decay(&mut self) {
        for amount in &mut self.0 {
            *amount = amount.wrapping_sub(1);
        }
    }
}

impl genie_diff::Diff for SalvageAmounts {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::gathering::SalvageAmounts;

    /// A decay takes one off each amount, a short's least wrapping to its most.
    #[test]
    fn salvage_decay_takes_one_off_each_amount_wrapping() {
        let mut amounts = SalvageAmounts::of_data([5, 0, -1, i16::MIN, 1, 300]);
        amounts.decay();
        assert_eq!(amounts.get(), [4, -1, -2, i16::MAX, 0, 299]);
        assert_eq!(SalvageAmounts::default(), SalvageAmounts::NONE);
    }
}
