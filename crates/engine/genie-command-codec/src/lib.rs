//! The command packets of a recording (genie-rec's op 1) as genie-core's commands, both ways
//! (FD-045 D17): [`decode()`] reads a packet into its command, [`encode()`] writes a command's
//! packet. A command keeps every field its packet holds; a packet whose command would not write
//! back its bytes, or of a kind the engine does not run, stays raw
//! ([`genie_core::commands::UnmodelledCommand`]). So `encode(decode(p)) == p` for every packet.
//!
//! The packets' layouts are genie-rec's (generated from build 158041's command formatters and
//! the tick oracle's field map); the commands are the game's handlers' view of them.

mod decode;
mod encode;
mod names;

pub use self::decode::decode;
pub use self::encode::encode;

#[cfg(test)]
mod tests;
