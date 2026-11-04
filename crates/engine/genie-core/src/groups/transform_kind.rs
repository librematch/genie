//! How a formation's transform places its slots (`mTransformType`).

/// How a formation's transform places its slots (`mTransformType`, `FormationTransform`'s
/// switch, `FUN_14103a1f0`); every value is kept, one not named here places none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TransformKind {
    /// 0: line by line, each at its distance back along the group's stored positions (the
    /// phalanx's lines).
    #[default]
    ByLines,
    /// 1: every slot from the group's position, its offset turned to the facing (the box, a
    /// user-defined formation).
    Turned,
    /// 2: every slot from the target's original position, its offset as it is.
    AtTarget,
    /// 3: every slot from the group's position, its offset as it is (the horde).
    Unturned,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`TransformKind`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Unnamed(i32);

impl TransformKind {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: i32) -> Self {
        match value {
            0 => Self::ByLines,
            1 => Self::Turned,
            2 => Self::AtTarget,
            3 => Self::Unturned,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> i32 {
        match self {
            Self::ByLines => 0,
            Self::Turned => 1,
            Self::AtTarget => 2,
            Self::Unturned => 3,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The transform type of the save's value `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self::of_raw(value)
    }

    /// Whether a slot's offset is not turned to the facing (types 2 and 3, `FUN_14103cfe0`).
    #[must_use]
    pub const fn unturned(self) -> bool {
        matches!(self, Self::AtTarget | Self::Unturned)
    }

    /// The value, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.raw()
    }
}

#[cfg(test)]
mod tests {
    use crate::groups::TransformKind;

    /// Types 2 and 3 leave the offsets unturned; any value is kept.
    #[test]
    fn the_unturned_kinds_are_2_and_3() {
        let unturned: Vec<i32> = (0..5)
            .map(TransformKind::of_data)
            .filter(|transform_kind| transform_kind.unturned())
            .map(TransformKind::get)
            .collect();
        assert_eq!(unturned, [2, 3]);
        assert_eq!(TransformKind::default(), TransformKind::ByLines);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_transform_kind_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 3, 4] {
            assert_eq!(TransformKind::of_raw(value).raw(), value);
        }
        assert!(matches!(TransformKind::of_raw(4), TransformKind::Other(_)));
    }
}
