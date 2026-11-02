//! A queued order's target type: the number its writer puts there, a master id or a class, and
//! the two ways the game reads it.

use crate::GenieError;
use crate::objects::masters::{ClassId, MasterId};

/// A queued order's target type (`+0x30`, −1 for none). The writer puts the target's master id
/// there (an order on a target or a move, a follow, an unload, a drop site's order) or its class
/// (the building command queue's, a kept or queued-back current order's, the town bell's way
/// back), and each reader takes the number as one of them: the order on a gone target as a
/// master of the target's owner (`+0x10`), without an owner as a class
/// ([`crate::unit_ai::UnitAi::order_on_target`]); a monastery's keep (`FUN_140c417f0`) as a
/// class whatever the owner. The building command queue's class with an owner is read as a
/// master id there, as the game does. So the type holds the number, 0 or more, with both
/// readings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OrderTargetType(i16);

impl OrderTargetType {
    /// The value a field takes for none.
    const NONE: i32 = -1;

    /// The type of a target of master `master`.
    #[must_use]
    pub fn of_master(master: MasterId) -> Self {
        Self(master.get())
    }

    /// The type of a target of class `class`: None for the −1 of a master without one.
    #[must_use]
    pub fn of_class(class: ClassId) -> Option<Self> {
        class.index().map(|_| Self(class.get()))
    }

    /// The number read as a master id.
    #[must_use]
    pub const fn master(self) -> MasterId {
        MasterId::named(self.0)
    }

    /// The number read as a class.
    #[must_use]
    pub const fn class(self) -> ClassId {
        ClassId::of_master(self.0)
    }

    /// The type of a saved field that holds −1 for none.
    ///
    /// # Errors
    ///
    /// A negative value other than −1 or one past a master's or class's `i16`.
    pub fn of_field(value: i32) -> Result<Option<Self>, GenieError> {
        if value == Self::NONE {
            return Ok(None);
        }
        i16::try_from(value)
            .ok()
            .filter(|v| 0 <= *v)
            .map(|v| Some(Self(v)))
            .ok_or_else(|| {
                GenieError::invalid_input("a target type no master or class has")
                    .with("value", value)
            })
    }

    /// The value of a field that holds −1 for none.
    #[must_use]
    pub fn field(kind: Option<Self>) -> i32 {
        kind.map_or(Self::NONE, |k| i32::from(k.0))
    }
}

impl genie_diff::Diff for OrderTargetType {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    use crate::objects::masters::class;
    use crate::test_ids::master_id;
    use crate::unit_ai::OrderTargetType;

    /// A saved −1 is no type, any other number 0 or more a type that reads as the master and the
    /// class of that number; a negative number or one past `i16` is an error.
    #[test]
    fn saved_target_type_reads_as_master_and_class() {
        assert_eq!(OrderTargetType::of_field(-1), Ok(None));
        let t = OrderTargetType::of_field(42).unwrap().unwrap();
        assert_eq!((t.master(), t.class()), (master_id(42), class::RELIC));
        assert_eq!(OrderTargetType::field(Some(t)), 42);
        assert_eq!(OrderTargetType::field(None), -1);
        for bad in [-2, 40_000] {
            assert_eq!(
                OrderTargetType::of_field(bad).map_err(|e| e.kind()),
                Err(ErrorKind::InvalidInput)
            );
        }
    }

    /// A target's master gives its number; a class gives its number, and a master's −1 none.
    #[test]
    fn type_is_made_of_master_or_class() {
        assert_eq!(
            OrderTargetType::field(Some(OrderTargetType::of_master(master_id(285)))),
            285
        );
        assert_eq!(
            OrderTargetType::of_class(class::FARM).map(OrderTargetType::class),
            Some(class::FARM)
        );
        assert_eq!(OrderTargetType::of_class(class::NO_CLASS), None);
    }
}
