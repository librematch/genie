//! The snow under a building ([`Snow`]): its `snow_flag`, `+0xc61`.

use genie_diff::Diff;

use crate::{GenieError, GenieResult};

/// `+0xc61`, `snow_flag`: how a building stands on snow. The terrain cover lays the snow
/// foundation under any snow (`FUN_1411ddd90` tests the byte against 0); the death and the
/// teardowns take back only a completion's (`== 1`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Diff)]
pub enum Snow {
    /// 0: no snow.
    #[default]
    None,
    /// 1: its completion on snow (`FUN_1411dbb70` on a flag of 0) laid the snow decal, which its
    /// death (`FUN_1413b9e20` at `0x1411ddac1`), its recycling (`FUN_1411d5600`) and
    /// `FUN_1411ad250` remove, the flag cleared with it.
    Completed,
    /// 2: a farm or a pasture (masters 50 and 1889) made on snow keeps it (its constructor
    /// `FUN_1411de5f0`; the copy `FUN_1411e5cd0` takes it), and its completion keeps the 2.
    Kept,
}

impl Snow {
    /// The saved byte (`snow_flag`): 0, 1 or 2.
    ///
    /// # Errors
    ///
    /// Any other byte, which no routine of the game writes.
    pub fn of_byte(byte: u8) -> GenieResult<Self> {
        match byte {
            0 => Ok(Self::None),
            1 => Ok(Self::Completed),
            2 => Ok(Self::Kept),
            _ => Err(GenieError::invalid_input("a snow flag above 2").with("snow_flag", byte)),
        }
    }

    /// The building stands on snow, completed or kept: the terrain cover's test (`!= 0`).
    #[must_use]
    pub const fn on_snow(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[cfg(test)]
mod tests {
    use crate::map::Snow;

    /// The saved bytes 0, 1 and 2 are the three snows, any other the save's error; a new
    /// building stands on none, and the cover reads the other two as snow.
    #[test]
    fn snow_of_saved_byte_holds_the_game_three_values() {
        assert_eq!(
            [0, 1, 2].map(|b| Snow::of_byte(b).unwrap()),
            [Snow::None, Snow::Completed, Snow::Kept]
        );
        assert!(Snow::of_byte(3).is_err());
        assert_eq!(Snow::default(), Snow::None);
        assert_eq!(
            [Snow::None, Snow::Completed, Snow::Kept].map(Snow::on_snow),
            [false, true, true]
        );
    }
}
