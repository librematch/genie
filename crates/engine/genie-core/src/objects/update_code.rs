//! What an action's update answers its list.

/// An action's update code (its slot's return, which the action list's update `FUN_140a992b0`
/// passes on): 1 (done) and 3 (failed) delete it from the list, any other keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UpdateCode(u8);

impl UpdateCode {
    /// 0: under way.
    pub const UNDER_WAY: Self = Self(0);
    /// 1: done; the list deletes it.
    pub const DONE: Self = Self(1);
    /// 2: a unit's own Attack ended after its kill, kept in the list.
    pub const CODE_2: Self = Self(2);
    /// 3: failed; the list deletes it.
    pub const FAILED: Self = Self(3);
    /// 4: a `MoveTo`'s target let go (the list's update).
    pub const CODE_4: Self = Self(4);
    /// 5: ended and kept, for its owner to read (a `MoveTo` whose target was let go, an
    /// Attack that ended).
    pub const CODE_5: Self = Self(5);

    /// The code of the byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Whether the list deletes the action: 1 or 3 (`(code - 1) & 0xfd == 0`).
    #[must_use]
    pub const fn deletes(self) -> bool {
        matches!(self.0, 1 | 3)
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::UpdateCode;

    /// Codes 1 and 3 alone delete the action.
    #[test]
    fn codes_1_and_3_delete() {
        let deleting: Vec<u8> = (0..=u8::MAX)
            .filter(|&c| UpdateCode::of_data(c).deletes())
            .collect();
        assert_eq!(deleting, [1, 3]);
        assert_eq!(UpdateCode::CODE_5.get(), 5);
    }
}
