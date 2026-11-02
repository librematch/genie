//! A volley's size: the firepower (missiles a volley), the missiles counted and the swings that
//! fired.

use std::ops::{Add, AddAssign, MulAssign, Sub, SubAssign};

/// A firepower: the missiles a volley fires (a float: a master's `volley_fire_amount`, `+0x8c0`,
/// which the effects set, add to and scale; an object's `+0xa00`, its master's plus the shares
/// of the units inside it). Each operator is the one `f32` operation the game does; every value
/// is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Firepower(f32);

impl Firepower {
    /// None.
    pub const ZERO: Self = Self(0.0);

    /// The firepower of the data's or the save's float `value`.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// The float, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Add for Firepower {
    type Output = Self;

    /// The `f32` sum.
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for Firepower {
    type Output = Self;

    /// The `f32` difference.
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl AddAssign for Firepower {
    /// A share's or an effect's add (`f32`).
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl SubAssign for Firepower {
    /// A share's removal (`f32`).
    fn sub_assign(&mut self, other: Self) {
        self.0 -= other.0;
    }
}

impl MulAssign<f32> for Firepower {
    /// An effect's multiply (`f32`).
    fn mul_assign(&mut self, factor: f32) {
        self.0 *= factor;
    }
}

impl genie_diff::Diff for Firepower {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// A count of missiles (a byte): a master's most in a volley (`max_attacks_in_volley`,
/// `+0x8c4`, which the effects set, add to and scale) and those of the volley under way an
/// object fired already (`next_volley`, `+0xa04`). Every byte is kept.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MissileCount(u8);

impl MissileCount {
    /// None.
    pub const ZERO: Self = Self(0);

    /// The count of the data's or the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The count plus `amount`, wrapping as the game's byte add.
    #[must_use]
    pub const fn plus(self, amount: u8) -> Self {
        Self(self.0.wrapping_add(amount))
    }

    /// The count as an int.
    #[must_use]
    pub fn counted(self) -> i32 {
        i32::from(self.0)
    }

    /// The byte, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl genie_diff::Diff for MissileCount {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// The swings that fired a missile (`attack_count`, `+0x900`): an object's since its target
/// changed, which its missiles carry. Every value is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttackCount(i32);

impl AttackCount {
    /// None.
    pub const ZERO: Self = Self(0);

    /// The count of the save's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// One swing more.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// The count as a float (`CVTSI2SS`).
    #[must_use]
    pub const fn as_f32(self) -> f32 {
        self.0 as f32
    }

    /// The int, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl genie_diff::Diff for AttackCount {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::{AttackCount, Firepower, MissileCount};

    /// A firepower adds, takes off and scales as `f32`.
    #[test]
    fn firepower_adds_and_scales_as_floats() {
        let mut f = Firepower::new(1.0) + Firepower::new(0.5);
        f += Firepower::new(2.0);
        f -= Firepower::new(1.5);
        f *= 2.0;
        assert_eq!(f, Firepower::new(4.0));
        assert_eq!(f - Firepower::new(1.0), Firepower::new(3.0));
        assert!(Firepower::ZERO < f);
    }

    /// A missile count wraps as a byte; an attack count counts on.
    #[test]
    fn missile_and_attack_counts_count() {
        assert_eq!(MissileCount::of_data(255).plus(2), MissileCount::of_data(1));
        assert_eq!(MissileCount::of_data(3).counted(), 3);
        assert_eq!(AttackCount::ZERO.next().next(), AttackCount::of_data(2));
        assert_eq!(AttackCount::of_data(4).as_f32(), 4.0);
    }
}
