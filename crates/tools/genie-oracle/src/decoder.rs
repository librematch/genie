//! The recorded commands the slice runs ([`Decoder`]): genie-command-codec's commands, and the
//! last commanded units a run from a mid-game save starts with.

use crate::Frame;
use genie_core::commands::{Command, Order, OrderCommand};
use genie_core::objects::ObjectId;
use std::collections::BTreeMap;

/// Decodes the recorded commands, in their order ([`genie_command_codec::decode`]). An order's
/// count of `0xffff` names the comm player's last commanded units, which the world keeps as the
/// game's unit-list reader (`FUN_140e6b290`) does.
///
/// The player byte is the comm player, which the handlers map to a world player through the lobby
/// (`FUN_140e6b210`); the slice takes the two as equal.
#[derive(Default)]
pub(super) struct Decoder {
    /// Each comm player's last own list of an INTERACT, WORK, MOVE or RETREAT (its count's
    /// ids), without the world's checks: what a run from a mid-game save starts its world's
    /// last commanded units with ([`Decoder::prime`]).
    pub(super) last_commanded: BTreeMap<usize, Vec<Option<ObjectId>>>,
}

impl Decoder {
    /// The commands of the frames a save already played: decoded for the unit lists they leave
    /// (a later command's 'last commanded units'), not run. Without the world of those updates
    /// the commands' checks do not run: a list a check would have refused is left too.
    pub(super) fn prime(&mut self, frames: &[Frame<'_>]) {
        for frame in frames {
            for p in &frame.commands {
                let _ = self.command(p);
            }
        }
    }

    /// The command of a packet, none for one without its player byte; an INTERACT, WORK, MOVE
    /// or RETREAT of its own list notes the list for [`Decoder::prime`].
    pub(super) fn command(&mut self, packet: &[u8]) -> Option<Command> {
        let command = genie_command_codec::decode(packet).ok()?;
        if let Command::Order(OrderCommand {
            player,
            units,
            order: Order::Interact(_) | Order::Work(_) | Order::Move(_) | Order::Retreat(_),
        }) = &command
            && !units.names_last_commanded()
        {
            let _ = self
                .last_commanded
                .insert(player.index(), units.first(usize::MAX).to_vec());
        }
        Some(command)
    }
}

#[cfg(test)]
mod tests;
