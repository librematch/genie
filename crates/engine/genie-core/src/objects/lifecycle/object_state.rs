//! An object's state: where it stands between its placing and its leaving.

use crate::GenieError;
use std::fmt::{Display, Error, Formatter};

/// An object's state (`+0x640`, the save's `object_state`, `RGE_Static_Object::set_object_state`):
/// the steps of its update ([`crate::objects::lifecycle::Lifecycle::update`]). The game compares
/// the numbers (`state < 3` alive or not yet built, `7 <= state` on its way out), so the states
/// keep their order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectState {
    /// 0: placed but not built, a foundation; it updates as an alive object does.
    Foundation = 0,
    /// 1: not built either, as the checks read it (`state < 2`); no step modelled here sets it.
    Unfinished = 1,
    /// 2: alive.
    #[default]
    Alive = 2,
    /// 3: dying: its hit points ran out (`set_state(3)`).
    Dying = 3,
    /// 4: playing its dying sprite.
    DyingSprite = 4,
    /// 5: dead, its remains holding what decays.
    Dead = 5,
    /// 6: undead (a master's `+0x490`).
    Undead = 6,
    /// 7: removed: what it held is gone, it leaves its lists (a missile's end, a destroyed
    /// ghost's).
    Removed = 7,
    /// 8: gone: its next update takes it off its player's list.
    Gone = 8,
}

impl ObjectState {
    /// The state of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte past 8, which names no state.
    pub fn of_byte(value: u8) -> Result<Self, GenieError> {
        Ok(match value {
            0 => Self::Foundation,
            1 => Self::Unfinished,
            2 => Self::Alive,
            3 => Self::Dying,
            4 => Self::DyingSprite,
            5 => Self::Dead,
            6 => Self::Undead,
            7 => Self::Removed,
            8 => Self::Gone,
            _ => {
                return Err(
                    GenieError::invalid_input("an object state past 8").with("state", value)
                );
            }
        })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl Display for ObjectState {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.get())
    }
}

impl genie_diff::Diff for ObjectState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::ObjectState;

    /// The bytes 0 to 8 name the states in their order, and read back; 9 is an error.
    #[test]
    fn the_bytes_0_to_8_name_the_states_in_order() {
        let all: Vec<ObjectState> = (0..=8).map(|b| ObjectState::of_byte(b).unwrap()).collect();
        assert!(all.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(
            all.iter().map(|s| s.get()).collect::<Vec<_>>(),
            (0..=8).collect::<Vec<_>>()
        );
        assert_eq!(ObjectState::default(), ObjectState::Alive);
        assert_eq!(ObjectState::Removed.to_string(), "7");
        assert!(ObjectState::of_byte(9).is_err());
    }
}
