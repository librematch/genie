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
    // the decompiled routines' math names (dx, dy, x0, y1 …), formulas and assignment order
    clippy::many_single_char_names,
    clippy::suspicious_operation_groupings,
    clippy::useless_let_if_seq,
    // the contexts are bundles of references, passed by value
    clippy::needless_pass_by_value,
    // a port's documentation opens with the routine and its addresses
    clippy::too_long_first_doc_paragraph,
    // every error is `GenieError`, which names the part the slice does not model
    clippy::missing_errors_doc,
    // its comparison tables are lookups by id outside the simulation, which holds no hash
    // container (CODING_STYLE.md section 12)
    clippy::disallowed_types
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
pub mod errors;
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

mod command_args;
mod compared_objects;
mod decoder;
mod differences;
mod first_checkpoint;
mod forced_replay;
use genie_import::tables as game_tables;
mod header_runs;
mod map_and_sight;
mod pathing_inputs;
mod save_objects;
mod save_runs;
#[cfg(test)]
mod save_tests;
use genie_import::save as saved_masters;
mod setup;
mod step_runs;
mod task_finder;
#[cfg(test)]
mod test_ids;

#[cfg(test)]
mod fixtures;

use self::command_args::{Frame, frames};
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
pub use self::game_tables::{drop_sites, linked_techs, unit_lines};
#[cfg(test)]
use self::header_runs::checkpoint_action;
pub use self::header_runs::{
    CommandTrace, GROUP_COMMANDS, GroupCommand, Word9, Word9Terms, blocks_per_tick, header_run,
    simulate_word9, slice_from_header, trace_commands, word9_terms,
};
use self::header_runs::{lane_block, run, top_action};
use self::map_and_sight::{SavedStep, tile_differences};
pub use self::map_and_sight::{saved_visibility, terrain_differences, visibility_differences};
pub use self::pathing_inputs::{
    mesh_from_dump, mesh_owners_from_dump, planner_from_save, unit_terrain,
};
use self::save_objects::tile_order_differences;
pub use self::save_objects::{
    Dumps, SliceRun, ghost_pool_from_dump, inside_lists_from_dump, objects_from_save,
    recycle_pools_from_dump, registry_from_dump, tile_lists_from_dump, turns_from_save,
};
pub use self::save_runs::{
    command_phase_from_save, run_from_save, saved_groups, spatial_from_save, spatial_with_tiles,
};
use self::save_runs::{compare_players, loaded_slice, played_frames, primed, save_header};
use self::saved_masters::work_masters;
pub use self::setup::layer;
use self::setup::{header_slice, slice};
pub use self::step_runs::{StepSave, steps_from_save};
pub use self::task_finder::{FinderObject, FinderWorld, finder_world};
#[cfg(test)]
use genie_core::objects::unit::Unit;
#[cfg(test)]
use genie_core::ownership::artifact::ARTIFACT_KIND;
use genie_core::world::load::object::object;
pub use genie_core::world::load::world::options::settings;
pub use genie_core::world::load::world::players::history;
use genie_core::world::load::world::players::tech_state;
use genie_core::world::load::world::saved_ghosts;
/// The fields a comparison covers ([`object_field_differences_in`]).
pub use genie_diff::Scope;

#[cfg(test)]
mod tests;
