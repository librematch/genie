//! A player section: the player list's load of one player, generated from the game's load code.

use super::generated::world::{Object, Player, PlayerObjects, WorldPlayer};
use genie_error::GenieError;

/// Where a player section starts and ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayerEnds {
    /// The section's first byte.
    pub start: usize,
    /// The byte after its last.
    pub end: usize,
}

/// One player's section of the save.
///
/// The player list's load of one player (`WorldPlayerFactory.cpp`): the class byte, the class's
/// load (`WorldPlayer.cpp` and the class's own fields), then the masters, maps and objects
/// (`WorldPlayer` vtable slot 29).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerSection<'a> {
    /// Where it starts.
    pub start: usize,
    /// The player as read.
    pub player: &'a Player,
    /// The player load of the section's class, found when the section was made.
    world_player: &'a WorldPlayer,
    /// The masters, maps and objects, found likewise.
    objects: &'a PlayerObjects,
}

impl<'a> PlayerSection<'a> {
    /// The section of `player`, read at `start`.
    ///
    /// # Errors
    ///
    /// A player type no creator handles (the game's `InvalidPlayerType`).
    pub fn new(start: usize, player: &'a Player) -> Result<Self, GenieError> {
        let (Some(world_player), Some(objects)) = (player.world_player(), player.objects()) else {
            return Err(GenieError::invalid_input("InvalidPlayerType")
                .with("type", player.kind)
                .with("at", start));
        };
        Ok(Self {
            start,
            player,
            world_player,
            objects,
        })
    }

    /// Where it starts and ends: its end is as many bytes on as it writes back, which takes a
    /// write of the whole section.
    ///
    /// # Errors
    ///
    /// A player that does not write back.
    pub fn ends(&self) -> Result<PlayerEnds, GenieError> {
        Ok(PlayerEnds {
            start: self.start,
            end: self.start + super::written_len(self.player)?,
        })
    }

    /// The player load of the section's class.
    #[must_use]
    pub const fn world_player(&self) -> &'a WorldPlayer {
        self.world_player
    }

    /// The masters, maps and objects.
    #[must_use]
    pub const fn objects(&self) -> &'a PlayerObjects {
        self.objects
    }

    /// The live objects (the list the checkpoint sums over).
    pub fn live_objects(&self) -> impl Iterator<Item = &'a Object> {
        self.objects.objects.objects()
    }

    /// The sleeping objects.
    pub fn sleeping_objects(&self) -> impl Iterator<Item = &'a Object> {
        self.objects.sleeping_objects.objects()
    }
}
