//! `TOWN_BELL` (0x7f): a town centre's bell.

use crate::GenieError;
use crate::objects::ObjectId;
use crate::players::PlayerId;
use crate::world::World;

/// `TOWN_BELL` (0x7f, `do_command` case 0x7f): the town centre `id` rings its owner's bell
/// or ends it ([`World::town_bell`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TownBellCommand {
    /// The player.
    pub player: PlayerId,
    /// The town centre.
    pub id: ObjectId,
    /// Ring or end.
    pub bell: Bell,
}

/// What `TOWN_BELL` does with the bell (its `activate` byte).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bell {
    /// It rings (any byte but 0; the codec writes 1).
    Ring,
    /// It ends (0).
    End,
}

impl Bell {
    /// The bell of the command's byte `value`: ring when it is not 0.
    #[must_use]
    pub const fn of_byte(value: u8) -> Self {
        if value == 0 { Self::End } else { Self::Ring }
    }

    /// The byte the codec writes: 1 to ring, 0 to end.
    #[must_use]
    pub const fn byte(self) -> u8 {
        match self {
            Self::Ring => 1,
            Self::End => 0,
        }
    }
}

impl TownBellCommand {
    /// Its handler ([`World::town_bell`]).
    pub(crate) fn run(self, w: &mut World) -> Result<(), GenieError> {
        w.town_bell(self.player, self.id, self.bell)
    }
}
