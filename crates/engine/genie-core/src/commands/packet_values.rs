//! Values the command packets carry: a command's queue id, a trade's lots, a BUILD's sprite
//! override and a tribute's or trade's resource word.

use crate::players::attribute::AttributeId;

/// A command's queue id (the formatter's `qId`, an int): −1 runs the command now; any other
/// value the commands that read it leave alone or queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct QueueId(i32);

impl QueueId {
    /// None (−1): the command runs now.
    pub const NONE: Self = Self(-1);

    /// The id of the packet's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// Whether it names a queue: not −1.
    #[must_use]
    pub const fn is_queued(self) -> bool {
        self.0 != Self::NONE.0
    }
}

/// How many lots a trade buys or sells (a `SELL_COMMODITY` or `BUY_COMMODITY` packet's `+0x6`,
/// a short).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TradeLots(u16);

impl TradeLots {
    /// The lots of the packet's short `value`.
    #[must_use]
    pub const fn of_data(value: u16) -> Self {
        Self(value)
    }

    /// The short.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// The sprite a BUILD packet asks its foundation to show (`+0x18`, an int): −1 for none; the
/// handler keeps 0x2c9 to 0x2ce for a house of civ 12 ([`crate::commands::BuildCommand`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpriteOverride(i32);

impl SpriteOverride {
    /// None (−1).
    pub const NONE: Self = Self(-1);
    /// The first override the handler keeps (0x2c9).
    const FIRST_KEPT: i32 = 0x2c9;
    /// The overrides it keeps past the first (5).
    const KEPT_PAST_FIRST: u32 = 5;

    /// The override of the packet's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// Whether it is one the handler keeps: not −1 and 0x2c9 to 0x2ce (`FUN_141189e70`'s
    /// unsigned compare).
    #[must_use]
    pub const fn kept(self) -> bool {
        self.0 != Self::NONE.0
            && self.0.wrapping_sub(Self::FIRST_KEPT).cast_unsigned() <= Self::KEPT_PAST_FIRST
    }
}

/// A packet's resource word (TRIBUTE's attributes `+0x24`.., a trade's commodity `+0x4`): an
/// attribute id its handler checks, which a recording can hold out of range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ResourceWord(u16);

impl ResourceWord {
    /// The word of the packet's `value`.
    #[must_use]
    pub const fn of_data(value: u16) -> Self {
        Self(value)
    }

    /// The word.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    /// The resource a tribute gives: food, wood, stone or gold for 0 to 3, none for any other.
    #[must_use]
    pub fn tributed(self) -> Option<AttributeId> {
        AttributeId::resources().get(usize::from(self.0)).copied()
    }

    /// The commodity a trade takes: food, wood or stone for a low byte of 0 to 2
    /// (`do_command`'s 0x7a and 0x7b), none for another.
    #[must_use]
    pub fn traded(self) -> Option<AttributeId> {
        let commodity = self.0.to_le_bytes()[0];
        (commodity <= 2).then(|| AttributeId::named(i16::from(commodity)))
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::{QueueId, ResourceWord, SpriteOverride, TradeLots};
    use crate::testing::attribute;

    /// −1 runs at once; the handler keeps the six overrides from 0x2c9.
    #[test]
    fn the_packet_values_keep_their_tests() {
        assert!(!QueueId::NONE.is_queued() && QueueId::of_data(71).is_queued());
        assert!(SpriteOverride::of_data(0x2c9).kept() && SpriteOverride::of_data(0x2ce).kept());
        assert!(!SpriteOverride::of_data(0x2cf).kept() && !SpriteOverride::of_data(0x2c8).kept());
        assert!(!SpriteOverride::NONE.kept());
        assert_eq!(TradeLots::of_data(3).get(), 3);
    }

    /// A tribute gives the four resources, a trade the three commodities of its low byte.
    #[test]
    fn the_resource_word_names_the_resources() {
        assert_eq!(ResourceWord::of_data(3).tributed(), Some(attribute::GOLD));
        assert_eq!(ResourceWord::of_data(4).tributed(), None);
        assert_eq!(
            ResourceWord::of_data(0x102).traded(),
            Some(attribute::STONE)
        );
        assert_eq!(ResourceWord::of_data(3).traded(), None);
        assert_eq!(ResourceWord::of_data(9).get(), 9);
    }
}
