//! The build layer genie-oracle's import reads, and the slice: genie-core's world from a save
//! with that layer ([`saved_world`], [`loaded_world`]).

use crate::masters::MasterTable;
use genie_core::GenieError;
use genie_core::world::World;
use genie_core::world::load::world::loaded::loaded_world;
use genie_core::world::load::world::saved_world;
use genie_model::layer::BuildLayer;
use genie_nickel::layer::build_layer;
use genie_save::Header;
use std::sync::LazyLock;

/// The build genie-oracle's import reads, its saves' and its `.dat`'s: its layer applies.
const LAYER_BUILD: u32 = 158_041;

/// The build layer, evaluated once from its Nickel source (FD-046-D08, D09's step 2).
static LAYER: LazyLock<Result<BuildLayer, GenieError>> = LazyLock::new(|| build_layer(LAYER_BUILD));

/// The build layer genie-oracle's import reads: build 158041's, from `builds/158041.ncl`.
///
/// # Errors
///
/// Its Nickel source does not evaluate or breaks its contracts.
pub fn layer() -> Result<&'static BuildLayer, GenieError> {
    LAYER.as_ref().map_err(Clone::clone)
}

/// The slice set up from the header save: genie-core's [`saved_world`] with build 158041's
/// layer. Only for a save whose run-time state then comes from the tick oracle's dumps; a run
/// from a recording's header takes [`header_slice`].
///
/// # Errors
///
/// The build layer does not evaluate ([`layer`]).
pub(super) fn slice(header: &Header, masters: Option<&MasterTable>) -> Result<World, GenieError> {
    saved_world(header, masters, layer()?)
}

/// The slice as the recording's header load leaves it: genie-core's [`loaded_world`] with build
/// 158041's layer.
///
/// # Errors
///
/// The build layer does not evaluate ([`layer`]).
pub(super) fn header_slice(
    header: &Header,
    masters: Option<&MasterTable>,
) -> Result<World, GenieError> {
    loaded_world(header, masters, layer()?)
}
