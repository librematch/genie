//! What a short path search returns ([`ShortFound`]).

/// What a short path search returns (its result byte): the old pather's (`FindShortPath`,
/// `FUN_140f85580`), the new one's (`FUN_140fa53d0`) and the movement's around them
/// (`FUN_140f9e540`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShortFound {
    /// 0: the start lies within range of the goal already; no path.
    InRange,
    /// 1: the path reaches the goal's range.
    Reached,
    /// 2: the path ends partway, at the place nearest the goal.
    Partial,
    /// 3: no way out of the start.
    Blocked,
    /// 4: the start or the goal lies off the map, or the new pather's outline failed.
    Invalid,
}

impl ShortFound {
    /// The search's result as the game's code (0 to 4).
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::InRange => 0,
            Self::Reached => 1,
            Self::Partial => 2,
            Self::Blocked => 3,
            Self::Invalid => 4,
        }
    }

    /// The goal's range held: in range from the start or reached (the game's `result < 2`).
    #[must_use]
    pub const fn reaches(self) -> bool {
        matches!(self, Self::InRange | Self::Reached)
    }

    /// A path that leads on: the goal's range reached or a partial path.
    #[must_use]
    pub const fn leads(self) -> bool {
        matches!(self, Self::Reached | Self::Partial)
    }

    /// No path at all: the start blocked or off the map.
    #[must_use]
    pub const fn fails(self) -> bool {
        matches!(self, Self::Blocked | Self::Invalid)
    }
}
