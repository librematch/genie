//! The game commands (`0x67`), one variant per sub-command.

use crate::PlayerId;
use crate::fits;
use byteorder::{LE, ReadBytesExt};
use genie_error::GenieResult;
use std::io::Read;

/// Commands affecting the game (command `0x67`), one variant per sub-command.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum GameCommand {
    /// Change the game speed (sub-command 1).
    SetGameSpeed {
        /// The player who sent the command.
        player_id: PlayerId,
        /// The new game speed.
        speed: f32,
    },
    /// Not used in game, but implemented.
    Inventory {
        /// The player who sent the command.
        player_id: PlayerId,
        /// The attribute (resource) to change.
        attribute_id: i16,
        /// The amount.
        amount: f32,
    },
    /// Not implemented in game.
    UpgradeTown {
        /// The player who sent the command.
        player_id: PlayerId,
    },
    /// The quick build cheat (sub-command 4).
    QuickBuild {
        /// The player who sent the command.
        player_id: PlayerId,
    },
    /// Set whether the player wants an allied victory (sub-command 5).
    AlliedVictory {
        /// The player who sent the command.
        player_id: PlayerId,
        /// Whether allied victory is turned on.
        status: bool,
    },
    /// A cheat code (sub-command 6).
    Cheat {
        /// The player who sent the command.
        player_id: PlayerId,
        /// The ID of the cheat.
        cheat_id: i16,
    },
    /// Not implemented in game.
    SharedLos {
        /// The player who sent the command.
        player_id: PlayerId,
    },
    /// Spies, or Treason in later versions (sub-command 0x0a): the player pays to reveal other
    /// players' units.
    Spies {
        /// The player who sent the command.
        player_id: PlayerId,
    },
    /// Set an AI strategic number (sub-command 0x0b), sent by AI players.
    SetStrategicNumber {
        /// The player who sent the command.
        player_id: PlayerId,
        /// The ID of the strategic number.
        strategic_number: i16,
        /// The new value of the strategic number.
        value: i32,
    },
    /// Appears to be unused.
    Unknown0x0c {
        /// The player who sent the command.
        player_id: PlayerId,
    },
    /// Queue farm reseeds at the mill (sub-command 0x0d).
    AddFarmReseedQueue {
        /// The player who sent the command.
        player_id: PlayerId,
        /// The number of reseeds to queue.
        amount: i16,
    },
    /// Remove queued farm reseeds (sub-command 0x0e).
    RemoveFarmReseedQueue {
        /// The player who sent the command.
        player_id: PlayerId,
        /// The number of reseeds to remove.
        amount: i16,
    },
    /// Toggle automatic farm reseeding (sub-command 0x10).
    FarmReseedAutoQueue {
        /// The player who sent the command.
        player_id: PlayerId,
        // TODO unknown vars
    },
    /// A sub-command this reader does not name, its fixed fields kept as the file holds them.
    Other {
        /// The sub-command.
        kind: u8,
        /// The first word (a player in the named sub-commands).
        var1: i16,
        /// The second word.
        var2: i16,
        /// The float.
        var3: f32,
        /// The last double word.
        var4: u32,
    },
}

#[derive(Debug)]
pub(super) struct RawGameCommand {
    pub(super) game_command: u8,
    pub(super) var1: i16,
    pub(super) var2: i16,
    pub(super) var3: f32,
    pub(super) var4: u32,
}

impl RawGameCommand {
    pub fn read_from(mut input: impl Read) -> GenieResult<Self> {
        let game_command = input.read_u8()?;
        let var1 = input.read_i16::<LE>()?;
        let var2 = input.read_i16::<LE>()?;
        let _padding = input.read_u16::<LE>()?;
        let var3 = input.read_f32::<LE>()?;
        let var4 = input.read_u32::<LE>()?;
        Ok(Self {
            game_command,
            var1,
            var2,
            var3,
            var4,
        })
    }
}

impl GameCommand {
    /// Read a game command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if the player ID of a named sub-command is negative or above 255, or if a
    /// strategic number's value is above `i32::MAX`; a sub-command no variant names reads as
    /// [`GameCommand::Other`].
    pub fn read_from(input: impl Read) -> GenieResult<Self> {
        use GameCommand::{
            AddFarmReseedQueue, AlliedVictory, Cheat, FarmReseedAutoQueue, Inventory, QuickBuild,
            RemoveFarmReseedQueue, SetGameSpeed, SetStrategicNumber, SharedLos, Spies, Unknown0x0c,
            UpgradeTown,
        };

        let RawGameCommand {
            game_command,
            var1,
            var2,
            var3,
            var4,
        } = RawGameCommand::read_from(input)?;

        if !matches!(game_command, 0x01..=0x07 | 0x0a..=0x0e | 0x10) {
            return Ok(Self::Other {
                kind: game_command,
                var1,
                var2,
                var3,
                var4,
            });
        }
        let player_id = fits(var1)?;
        match game_command {
            0x01 => Ok(SetGameSpeed {
                player_id,
                speed: var3,
            }),
            0x02 => Ok(Inventory {
                player_id,
                attribute_id: var2,
                amount: var3,
            }),
            0x03 => Ok(UpgradeTown { player_id }),
            0x04 => Ok(QuickBuild { player_id }),
            0x05 => Ok(AlliedVictory {
                player_id,
                status: var2 != 0,
            }),
            0x06 => Ok(Cheat {
                player_id,
                cheat_id: var2,
            }),
            0x07 => Ok(SharedLos { player_id }),
            0x0a => Ok(Spies { player_id }),
            0x0b => Ok(SetStrategicNumber {
                player_id,
                strategic_number: var2,
                value: fits(var4)?,
            }),
            0x0c => Ok(Unknown0x0c { player_id }),
            0x0d => Ok(AddFarmReseedQueue {
                player_id,
                amount: var2,
            }),
            0x0e => Ok(RemoveFarmReseedQueue {
                player_id,
                amount: var2,
            }),
            0x10 => Ok(FarmReseedAutoQueue { player_id }),
            kind => Ok(Self::Other {
                kind,
                var1,
                var2,
                var3,
                var4,
            }),
        }
    }
}
