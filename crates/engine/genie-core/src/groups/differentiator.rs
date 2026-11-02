//! A group's differentiator (`mDifferentiatorID`): the order splitter's command count.

/// The order splitter's command count (`DAT_143f10134`, a process global): each command that
/// runs the splitter gives it to the groups it forms or orders as their differentiator
/// (`mDifferentiatorID`), then counts up. −1 is a new group's (none). Every value is kept, and
/// the counts compare as the game's signed words.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Differentiator(i32);

impl Differentiator {
    /// −1: a group no command ordered yet.
    pub const NONE: Self = Self(-1);

    /// The count of the save's or the dump's value `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The next count (plus 1, wrapping as the game's add).
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// The value, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::groups::Differentiator;

    /// The count steps by 1 and wraps; −1 is below every count.
    #[test]
    fn count_steps_and_wraps() {
        assert_eq!(Differentiator::NONE.next(), Differentiator::default());
        assert_eq!(Differentiator::of_data(i32::MAX).next().get(), i32::MIN);
        assert!(Differentiator::NONE < Differentiator::of_data(0));
    }
}
