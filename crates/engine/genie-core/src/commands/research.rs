//! RESEARCH (0x65): a technology at the player's buildings.

use crate::GenieError;
use crate::commands::QueueId;
use crate::objects::ObjectId;
use crate::players::PlayerId;
use crate::technology::TechId;
use crate::world::World;

/// RESEARCH (0x65, `0x140e7e270`): the technology `tech` at the building `building`
/// ([`World::research`]). `count` (`+0x8`) counts the buildings the packet lists, `queue_id`
/// (`+0xc`, the formatter's `qId`) is −1 for a research to run, and `shift` (`+0x10`) asks for five at once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResearchCommand {
    /// The player who researches.
    pub player: PlayerId,
    /// The building of the packet.
    pub building: Option<ObjectId>,
    /// `+0x8`: how many buildings the packet lists.
    pub count: i16,
    /// `+0x11..`: the buildings the packet lists, none for −1.
    pub buildings: Vec<Option<ObjectId>>,
    /// `+0xa`: the technology, none for −1 (or any id below 0).
    pub tech: Option<TechId>,
    /// `+0xc`, the formatter's `qId`: −1 for a research to run.
    pub queue_id: QueueId,
    /// `+0x10`: five at once.
    pub shift: bool,
}

impl ResearchCommand {
    /// `do_command`'s case checks the buildings first ([`World::research_check`]), then the
    /// research runs in those left ([`World::research`]).
    pub(crate) fn run(&self, w: &mut World) -> Result<(), GenieError> {
        let Some(buildings) = w.research_check(self)? else {
            return Ok(());
        };
        w.research(self, &buildings)
    }
}
