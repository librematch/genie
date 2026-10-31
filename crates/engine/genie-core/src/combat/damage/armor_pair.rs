//! An object's two armor values ([`ArmorPair`]): melee and pierce.

use genie_diff::Diff;

/// An object's melee and pierce armor values, in the game's order (melee first): its armor
/// powerups (`+0xa48`, `+0xa4c`, which the armor slots read as 16 bits) and the armor its
/// charge's hits took off it (`+0x9e4`, `+0x9e6`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct ArmorPair {
    /// The melee armor's.
    pub melee: i16,
    /// The pierce armor's.
    pub pierce: i16,
}

impl ArmorPair {
    /// None of either.
    pub const ZERO: Self = Self::new(0, 0);

    /// `melee` and `pierce`.
    #[must_use]
    pub const fn new(melee: i16, pierce: i16) -> Self {
        Self { melee, pierce }
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::damage::ArmorPair;

    /// The pair holds melee first, as the game's two shorts.
    #[test]
    fn an_armor_pair_holds_melee_then_pierce() {
        let p = ArmorPair::new(3, -4);
        assert_eq!((p.melee, p.pierce), (3, -4));
        assert_eq!(ArmorPair::ZERO, ArmorPair::default());
    }
}
