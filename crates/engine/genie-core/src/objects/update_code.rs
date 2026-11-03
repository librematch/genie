//! What an action's update answers its list.

/// An action's update code (its slot's return, which the action list's update `FUN_140a992b0`
/// passes on): 1 (done) and 3 (failed) delete it from the list, any other keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UpdateCode {
    /// 0: under way.
    UnderWay,
    /// 1: done; the list deletes it.
    Done,
    /// 2: a unit's own Attack ended after its kill, kept in the list.
    Code2,
    /// 3: failed; the list deletes it.
    Failed,
    /// 4: a `MoveTo`'s target let go (the list's update).
    Code4,
    /// 5: ended and kept, for its owner to read (a `MoveTo` whose target was let go, an
    /// Attack that ended).
    Code5,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`UpdateCode`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unnamed(u8);

impl UpdateCode {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u8) -> Self {
        match value {
            0 => Self::UnderWay,
            1 => Self::Done,
            2 => Self::Code2,
            3 => Self::Failed,
            4 => Self::Code4,
            5 => Self::Code5,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u8 {
        match self {
            Self::UnderWay => 0,
            Self::Done => 1,
            Self::Code2 => 2,
            Self::Failed => 3,
            Self::Code4 => 4,
            Self::Code5 => 5,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The code of the byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self::of_raw(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.raw()
    }

    /// Whether the list deletes the action: 1 or 3 (`(code - 1) & 0xfd == 0`).
    #[must_use]
    pub const fn deletes(self) -> bool {
        matches!(self.raw(), 1 | 3)
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
        assert_eq!(UpdateCode::Code5.get(), 5);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_update_code_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 3, 4, 5, 6] {
            assert_eq!(UpdateCode::of_raw(value).raw(), value);
        }
        assert!(matches!(UpdateCode::of_raw(6), UpdateCode::Other(_)));
    }
}
