//! The body's frames.

use crate::Error;
use genie_core::world::Millis;
use genie_rec::de;
use genie_rec::de::Op;

/// One frame of the body (FD-007's per-frame record order): the op-2 record that starts a tick, its
/// dt and, every interval, a checkpoint, then the op-1 commands the tick runs after its `SeedAll`.
pub(super) struct Frame<'a> {
    pub(super) dt: Millis,
    pub(super) checkpoint: Option<Box<de::Checkpoint>>,
    pub(super) commands: Vec<&'a [u8]>,
}

/// The chapter's first `limit` frames.
pub(super) fn frames<'a>(chapter: &de::Chapter<'a>, limit: usize) -> Result<Vec<Frame<'a>>, Error> {
    let mut out: Vec<Frame<'a>> = Vec::new();
    for op in chapter.ops() {
        let (dt, checkpoint) = match op? {
            Op::Sync { dt } => (dt, None),
            Op::Checkpoint { dt, checkpoint } => (dt, Some(checkpoint)),
            Op::Command { packet, .. } => {
                out.last_mut()
                    .ok_or(Error::CommandBeforeFirstSync)?
                    .commands
                    .push(packet);
                continue;
            }
            _ => continue,
        };
        if out.len() == limit {
            break;
        }
        out.push(Frame {
            dt: Millis::new(dt),
            checkpoint,
            commands: Vec::new(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
