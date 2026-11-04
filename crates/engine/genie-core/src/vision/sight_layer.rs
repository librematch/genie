//! What a sight change reads of the build layer.

use crate::world::layer::Roles;
use crate::world::layer::generated::VisibilityTuning;

/// What a sight change reads of the build layer (FD-046-D45, D49): the roles (the capture kinds
/// and the classes a sight tests) and the visibility's tuning values.
#[derive(Clone, Copy, Debug)]
pub struct SightLayer<'a> {
    /// The build's roles.
    pub(crate) roles: &'a Roles,
    /// The visibility's tuning values.
    pub(crate) visibility_tuning: VisibilityTuning,
}
