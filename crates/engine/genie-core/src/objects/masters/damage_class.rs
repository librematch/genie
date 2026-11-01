//! An armor or attack entry's class as one type, and the entry: the `.dat`'s armor and attack
//! classes.

/// An armor or attack class (the `.dat`'s `class` of an armor or attack entry; an attribute
/// effect's amount's high byte): a hit's weapon of a class meets the victim's armor of the same
/// class. The simulation's tests name melee (4), pierce (3) and the classes the garrison's
/// firepower sums (31, 38); every other value is a class the tests do not tell apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DamageClass(i16);

impl DamageClass {
    /// Pierce (3).
    pub const PIERCE: Self = Self(3);
    /// Melee (4).
    pub const MELEE: Self = Self(4);
    /// Class 31, which a master's own weapons add to the firepower.
    pub const CLASS_31: Self = Self(31);
    /// Class 38, which the missiles add to the firepower.
    pub const CLASS_38: Self = Self(38);

    /// Its value.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl From<i16> for DamageClass {
    /// The class a `.dat` entry or an effect's amount holds: every value is one.
    fn from(value: i16) -> Self {
        Self(value)
    }
}

impl genie_diff::Diff for DamageClass {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

/// One armor or attack entry (`+0x808` and `+0x818`, 4 bytes each): its class and amount.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ClassAmount {
    /// The class.
    pub class: DamageClass,
    /// The amount.
    pub amount: crate::objects::masters::DamageAmount,
}

impl ClassAmount {
    /// The entry of `class` and `amount` as the `.dat` and the save hold them.
    #[must_use]
    pub const fn new(class: i16, amount: i16) -> Self {
        Self {
            class: DamageClass(class),
            amount: crate::objects::masters::DamageAmount::new(amount),
        }
    }
}

impl genie_diff::Diff for ClassAmount {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::{ClassAmount, DamageClass};

    /// An entry keeps its class and amount; the named classes are the game's values.
    #[test]
    fn an_entry_holds_its_class_and_amount() {
        let e = ClassAmount::new(4, -2);
        assert_eq!((e.class, e.amount.get()), (DamageClass::MELEE, -2));
        assert_eq!(DamageClass::PIERCE.get(), 3);
        assert_eq!(DamageClass::from(38), DamageClass::CLASS_38);
    }
}
