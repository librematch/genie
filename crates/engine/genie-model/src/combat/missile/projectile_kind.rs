//! A missile master's projectile type.

/// A missile master's projectile type (the `.dat`'s `projectile_type`, `+0x880`, not saved): how
/// its missile flies. Type 0 flies straight along its unit direction, type 3 takes its whole way
/// in one step; types 1 and 2 are not modelled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectileKind(u8);

impl ProjectileKind {
    /// Type 0: straight along its unit direction, its timer counting the way.
    pub const STRAIGHT: Self = Self(0);
    /// Type 3: the whole way in one step.
    pub const WHOLE_WAY: Self = Self(3);

    /// The type of the `.dat`'s byte `value`: every byte is one.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Whether its missile meets the objects on its way (types 0 to 2, `FUN_140a9e6e0`).
    #[must_use]
    pub const fn meets_on_the_way(self) -> bool {
        self.0 <= 2
    }
}

impl genie_diff::Diff for ProjectileKind {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::missile::ProjectileKind;

    /// Types 0 to 2 meet the objects on the way; type 3 does not.
    #[test]
    fn types_up_to_two_meet_the_objects_on_the_way() {
        assert!(ProjectileKind::STRAIGHT.meets_on_the_way());
        assert!(ProjectileKind::of_data(2).meets_on_the_way());
        assert!(!ProjectileKind::WHOLE_WAY.meets_on_the_way());
        assert_eq!(ProjectileKind::of_data(3), ProjectileKind::WHOLE_WAY);
        assert_eq!(ProjectileKind::default().get(), 0);
    }
}
