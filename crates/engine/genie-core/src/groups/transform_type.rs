//! How a formation's transform places its slots (`mTransformType`).

/// How a formation's transform places its slots (`mTransformType`, `FormationTransform`'s
/// switch, `FUN_14103a1f0`); every value is kept, one not named here places none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TransformType(i32);

impl TransformType {
    /// 0: line by line, each at its distance back along the group's stored positions (the
    /// phalanx's lines).
    pub const BY_LINES: Self = Self(0);
    /// 1: every slot from the group's position, its offset turned to the facing (the box, a
    /// user-defined formation).
    pub const TURNED: Self = Self(1);
    /// 2: every slot from the target's original position, its offset as it is.
    pub const AT_TARGET: Self = Self(2);
    /// 3: every slot from the group's position, its offset as it is (the horde).
    pub const UNTURNED: Self = Self(3);

    /// The transform type of the save's value `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// Whether a slot's offset is not turned to the facing (types 2 and 3, `FUN_14103cfe0`).
    #[must_use]
    pub const fn unturned(self) -> bool {
        matches!(self, Self::AT_TARGET | Self::UNTURNED)
    }

    /// The value, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::groups::TransformType;

    /// Types 2 and 3 leave the offsets unturned; any value is kept.
    #[test]
    fn the_unturned_types_are_2_and_3() {
        let unturned: Vec<i32> = (0..5)
            .map(TransformType::of_data)
            .filter(|t| t.unturned())
            .map(TransformType::get)
            .collect();
        assert_eq!(unturned, [2, 3]);
        assert_eq!(TransformType::default(), TransformType::BY_LINES);
    }
}
