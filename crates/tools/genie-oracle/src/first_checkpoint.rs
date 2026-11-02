//! The first checkpoint from the start-of-game save: each player's block against the
//! recording's ([`compare_first_checkpoint`]).

use crate::Error;
use genie_core::GenieError;
use genie_core::map::Position;
use genie_core::objects::ObjectId;
use genie_core::objects::masters::MasterId;
use genie_core::objects::unit::Unit;
use genie_core::players::Amount;
use genie_core::world::checkpoint::{CheckpointObject, PlayerState, player_block};
use genie_rec::de;
use genie_rec::de::Op;
use genie_save::PlayerSection;
use genie_save::generated::world::RgeStaticObject;

/// The words of the first checkpoint that the start-of-game save determines: all but word 9.
pub const SAVE_WORDS: [usize; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10];

/// One player's block, computed from the save and as the recording stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerComparison {
    /// The player's id.
    pub id: u32,
    /// The block computed from the header save.
    pub from_save: [u32; 11],
    /// The block the checkpoint stores.
    pub checkpoint: [u32; 11],
}

/// The static part of each live object, with its object.
pub(super) fn live<'a>(
    section: &PlayerSection<'a>,
) -> impl Iterator<Item = (&'a genie_save::Object, &'a RgeStaticObject)> {
    section
        .live_objects()
        .filter_map(|o| o.static_part().map(|s| (o, s)))
}

/// An object of a player's live list: not the stand-in for a saved object without a static part
/// ([`unit`]). A missile's id is below 0 (the world's reusable ids, `next_reusable_object_id`).
pub(super) fn listed(u: &Unit) -> bool {
    u.id != ObjectId::UNSAVED
}

/// An object saved alive without hit points: its first update kills it, and it leaves the live
/// list within a few ticks (genie-core's lifecycle; a flare's 1 s corpse timer takes 14). The
/// save's PLACEHOLDER2s, FLARE3s and empty town-centre annexes are such objects.
pub(super) fn dies_at_start(s: &RgeStaticObject) -> bool {
    matches!(s.object_state, 0 | 2) && s.hp < 1.0
}

/// What the block reads of a saved object, before the slice changes any of it.
///
/// # Errors
///
/// An id of −1, which names no object; a state past 8.
pub(super) fn object_state(s: &RgeStaticObject) -> Result<CheckpointObject, GenieError> {
    Ok(CheckpointObject {
        id: ObjectId::try_from(s.id)?,
        main_id: MasterId::of_data(s.master_obj)
            .ok_or(GenieError::invalid_input("a live object of no master"))?,
        state: genie_core::objects::ObjectState::of_byte(s.object_state)?,
        amount_held: Amount::new(s.attribute_amount_held),
        position: Position::new(s.tx, s.ty, s.tz),
        top_action: None,
        retarget_timer: None,
    })
}

/// The checkpoint inputs of one player section at the first checkpoint, from the save alone: the
/// objects that die at the start ([`dies_at_start`]) are gone by then. Word 9's inputs stay empty
/// (see the crate docs).
///
/// # Errors
///
/// A live object's id of −1, which names no object.
pub fn player_state(section: &PlayerSection<'_>, id: u32) -> Result<PlayerState, GenieError> {
    Ok(PlayerState {
        live: live(section)
            .map(|(_, s)| s)
            .filter(|s| !dies_at_start(s))
            .map(object_state)
            .collect::<Result<_, _>>()?,
        ..player_totals(section, id)
    })
}

/// A player section's resources and sleeping count, without its live objects.
pub(super) fn player_totals(section: &PlayerSection<'_>, id: u32) -> PlayerState {
    let attributes = &section.world_player().base.attributes;
    let resource = |i: usize| Amount::new(attributes.get(i).copied().unwrap_or_default());
    PlayerState {
        id,
        resources: [resource(0), resource(1), resource(2), resource(3)],
        sleeping_count: section.sleeping_objects().count() as u32,
        live: Vec::new(),
    }
}

/// Compare every player of the first chapter's save with the recording's first checkpoint, in the
/// order of the checkpoint's lanes. Lanes that are all zero hold no player.
pub fn compare_first_checkpoint(recording: &[u8]) -> Result<Vec<PlayerComparison>, Error> {
    let chapter = de::chapters(recording)?[0];
    let header = chapter.header()?;
    let checkpoint = chapter
        .ops()
        .find_map(|op| match op {
            Ok(Op::Checkpoint { checkpoint, .. }) => Some(Ok(checkpoint)),
            Ok(_) => None,
            Err(e) => Some(Err(e)),
        })
        .ok_or(Error::NoCheckpoint)??;
    let players = header.players();
    checkpoint
        .lanes
        .iter()
        .filter(|lane| lane.iter().any(|&w| w != 0))
        .map(|lane| {
            let id = lane[8];
            let section = players.get(id as usize).ok_or(Error::NoSuchPlayer(id))?;
            Ok(PlayerComparison {
                id,
                from_save: player_block(&player_state(section, id).map_err(Error::Unmodelled)?),
                checkpoint: *lane,
            })
        })
        .collect()
}
