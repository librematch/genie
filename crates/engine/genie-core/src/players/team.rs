//! A player's team.

/// A player's team (the world's `+0x984`.. at the player less one, `playerTeams[i]`, saved as
/// bytes): 1 alone, 2 to 5 a team allied victory joined.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Team(i32);

impl Team {
    /// Alone (1).
    pub const ALONE: Self = Self(1);
    /// The first team players join (2).
    pub const FIRST: Self = Self(2);
    /// The last (5): past it no team is left (`FUN_1415e23f0`).
    pub const LAST: Self = Self(5);

    /// The team of the save's `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// The team after it.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

impl genie_diff::Diff for Team {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::players::Team;

    /// The teams count up from alone.
    #[test]
    fn the_teams_count_up_from_alone() {
        assert_eq!(Team::ALONE.next(), Team::FIRST);
        assert!(Team::LAST < Team::LAST.next());
        assert_eq!(Team::of_data(1), Team::ALONE);
    }
}
