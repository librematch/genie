//! The replay oracle: state read from a recording's header save, checked against the checkpoints
//! the recording stores in its body.
//!
//! The first checkpoint comes 500 syncs into the game; a recording has none at tick 0. Ten of its
//! eleven words per player still equal the start-of-game state ([`SAVE_WORDS`]). Word 9 does not:
//! it sums each object's top action and unit-AI retarget timer, which the engine redraws while the
//! game runs, so reproducing it needs the simulation.

// The port's exceptions to the workspace's lints: the engine reproduces build 158041 bit for bit,
// so it keeps the game's arithmetic and conversions as they are.
#![allow(
    // the game's f32 sums and products, each rounded on its own (no fused multiply-add)
    clippy::suboptimal_flops,
    clippy::imprecise_flops,
    // the game compares floats exactly, in conditions and loops
    clippy::float_cmp,
    clippy::while_float,
    // the game's conversions (cvttss2si, movsx, movzx, truncating stores), written as `as`
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    // the decompiled routines' math names (dx, dy, x0, y1 …), formulas, flag fields and
    // parameters, assignment order, and member functions whose `this` goes unused
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::suspicious_operation_groupings,
    clippy::struct_excessive_bools,
    clippy::fn_params_excessive_bools,
    clippy::useless_let_if_seq,
    clippy::unused_self,
    // the contexts are bundles of references, passed by value
    clippy::needless_pass_by_value,
    // a routine is ported whole, its documentation opens with the routine and its addresses
    clippy::too_many_lines,
    clippy::too_long_first_doc_paragraph,
    // every error is `GenieError`, which names the part the slice does not model
    clippy::missing_errors_doc
)]
#![warn(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::unimplemented,
    clippy::todo
)]

pub mod census;
pub mod groups;
pub use genie_import::dat as masters;
// the `.dat`'s tables in the cache's form (FD-046-D18, D59)
pub use genie_import::cache;
// the `.dat`'s tables from its bytes or, with `fs`, its files through the cache (D60)
pub use genie_import::install;
pub mod survey;
pub mod tick_trace;
pub mod views;

pub use masters::{DefaultAction, MasterTable, TerrainTable};

use genie_core::GenieError;
use genie_core::objects::ObjectId;
use genie_rec::de;

mod command_args;
mod compared_objects;
mod decoder;
mod differences;
mod first_checkpoint;
mod forced_replay;
use genie_import::tables as game_tables;
mod header_runs;
mod map_and_sight;
mod movers;
mod object_parts;
mod pathing_inputs;
mod save_objects;
mod save_runs;
#[cfg(test)]
mod save_tests;
use genie_import::save as saved_masters;
mod saved_unit;
mod setup;
mod step_runs;
mod sub_actions;
mod task_finder;
#[cfg(test)]
mod test_ids;
mod top_actions;

#[cfg(test)]
mod fixtures;

use self::command_args::{Frame, frames, market, saved_move_wait};
use self::compared_objects::header_objects;
pub use self::compared_objects::{object_rows, saved_objects};
use self::decoder::Decoder;
pub use self::differences::{
    Difference, ONLY_IN_THE_GAME, ONLY_IN_THE_SLICE, object_differences, object_field_differences,
    object_field_differences_in,
};
pub use self::first_checkpoint::{
    PlayerComparison, SAVE_WORDS, compare_first_checkpoint, player_state,
};
use self::first_checkpoint::{listed, live, player_totals};
use self::forced_replay::game_state;
pub use self::forced_replay::{
    PatherAnswer, PatherCall, TickReport, Times, command_phase, forced_run, forced_ticks,
    old_pather_paths,
};
pub use self::game_tables::{DropSitesError, UnitLinesError, drop_sites, linked_techs, unit_lines};
#[cfg(test)]
use self::header_runs::checkpoint_action;
pub use self::header_runs::{
    GROUP_COMMANDS, GroupCommand, Word9, Word9Terms, blocks_per_tick, header_run, simulate_word9,
    slice_from_header, trace_commands, word9_terms,
};
use self::header_runs::{header_slice, lane_block, run, top_action, watch_at_load};
use self::map_and_sight::{SavedStep, spatial, tile_differences};
pub use self::map_and_sight::{saved_visibility, terrain_differences, visibility_differences};
use self::movers::{mover, path_level, vec3};
use self::object_parts::{lifecycle, unit_ai};
pub use self::pathing_inputs::{
    mesh_from_dump, mesh_owners_from_dump, planner_from_save, unit_terrain,
};
pub use self::save_objects::{
    Dumps, SliceRun, ghost_pool_from_dump, inside_lists_from_dump, objects_from_save,
    recycle_pools_from_dump, registry_from_dump, tile_lists_from_dump, turns_from_save,
};
use self::save_objects::{group_link, tile_order_differences};
pub use self::save_runs::{
    command_phase_from_save, run_from_save, saved_groups, spatial_from_save, spatial_with_tiles,
};
use self::save_runs::{
    compare_players, hunted_origins, loaded_slice, played_frames, primed, save_header,
};
use self::saved_masters::{
    core_task, master_facets, moving_speed, placement, task_at, work_masters,
};
use self::saved_unit::{OTHER_SUB_ACTION, object};
pub use self::setup::{history, layer, settings};
use self::setup::{saved_ghosts, slice, tech_state};
pub use self::step_runs::{StepSave, steps_from_save};
use self::sub_actions::{
    DefaultStart, build_task, default_task, fly_task, gather_point, production, saved_attack,
    saved_reload, saved_sprite,
};
pub use self::task_finder::{FinderObject, FinderWorld, finder_world};
use self::top_actions::saved_actions;
#[cfg(test)]
use genie_core::objects::unit::Unit;
#[cfg(test)]
use genie_core::ownership::artifact::ARTIFACT_TYPE;
/// The fields a comparison covers ([`object_field_differences_in`]).
pub use genie_diff::Scope;
use genie_import::save::parts::combat_master;
use std::error::Error as StdError;
use std::fmt::{Display, Error as FmtError, Formatter};

/// Why a recording could not be compared.
#[derive(Debug)]
pub enum Error {
    /// The recording could not be read.
    Rec(de::Error),
    /// The recording has no checkpoint (it ends within the first interval).
    NoCheckpoint,
    /// A checkpoint lane names a player the header save does not have.
    NoSuchPlayer(u32),
    /// A command comes before the body's first sync, so no tick runs it.
    CommandBeforeFirstSync,
    /// A mesh dump without its header line.
    BadMeshDump,
    /// A registry dump without its header line, or a line out of shape.
    BadRegistryDump,
    /// A tile list dump's line out of shape.
    BadTileDump,
    /// A recycled pools dump's line out of shape.
    BadPoolDump,
    /// A recording without a chapter.
    NoChapter,
    /// A player the save does not have.
    NoPlayer(usize),
    /// What the slice does not model of the save.
    Unmodelled(GenieError),
}

impl From<de::Error> for Error {
    fn from(e: de::Error) -> Self {
        Self::Rec(e)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        match self {
            Self::Rec(e) => write!(f, "{e}"),
            Self::NoCheckpoint => write!(f, "the recording has no checkpoint"),
            Self::NoSuchPlayer(id) => write!(f, "a checkpoint names player {id}, not in the save"),
            Self::CommandBeforeFirstSync => write!(f, "a command comes before the first sync"),
            Self::BadMeshDump => write!(f, "the mesh dump has no mesh line"),
            Self::BadRegistryDump => write!(f, "the registry dump is out of shape"),
            Self::BadTileDump => write!(f, "the tile list dump is out of shape"),
            Self::BadPoolDump => write!(f, "the recycled pools dump is out of shape"),
            Self::NoChapter => write!(f, "the recording has no chapter"),
            Self::Unmodelled(u) => write!(f, "not modelled: {u}"),
            Self::NoPlayer(p) => write!(f, "no player {p} in the save"),
        }
    }
}

impl StdError for Error {}

/// The ids of a saved list of objects, which names no none (−1).
///
/// # Errors
///
/// An entry of −1.
pub(crate) fn object_ids(ids: &[i32]) -> Result<Vec<ObjectId>, GenieError> {
    ids.iter().map(|&id| ObjectId::try_from(id)).collect()
}

#[cfg(test)]
mod tests;
