//! A set of classes as the game tests one: a bit mask over the classes below a bound.

use crate::objects::masters::ClassId;

/// A set of classes as the game's code tests it: a 64-bit mask, bit `class` for class `class`, read
/// only for a class below `below` (the class taken as unsigned, so −1 and the other negative
/// classes are never in it). The game's routines inline each such test with its own mask and
/// bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassMask {
    bits: u64,
    below: u32,
}

impl ClassMask {
    /// The classes of mask `bits` below class `below` (at most 64).
    ///
    /// # Panics
    ///
    /// For a bound above 64, which a constant's evaluation rejects when it compiles.
    #[must_use]
    pub const fn new(bits: u64, below: u32) -> Self {
        assert!(below <= 64, "a class mask reads at most 64 classes");
        Self { bits, below }
    }

    /// Whether `class` is in the set: below the bound (unsigned) and its bit set.
    #[must_use]
    pub const fn contains(self, class: ClassId) -> bool {
        let bit = class.get() as u16 as u32;
        bit < self.below && (self.bits >> bit) & 1 != 0
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::{ClassId, ClassMask};

    /// A class is in the set when its bit is set below the bound; a negative class never is.
    #[test]
    fn class_is_in_the_mask_below_its_bound() {
        let set = ClassMask::new(0x0012_0000_0000_0008, 0x35);
        let has = |class: i16| set.contains(ClassId::of_master(class));
        assert!(has(3) && has(49) && has(52));
        assert!(!has(4) && !has(-1));
        assert!(!ClassMask::new(1 << 52, 52).contains(ClassId::of_master(52)));
    }
}
