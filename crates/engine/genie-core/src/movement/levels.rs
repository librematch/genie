//! The levels of a unit's path data by their role: the walked level and its parent.

use crate::movement::PathData;
use crate::{GenieError, GenieResult};

/// A mover's path data without the level its caller walks.
const NO_LEVEL: GenieError =
    GenieError::unreachable("a mover's path data without the level the caller walks");

/// The levels of a unit's path data (`+0x858`, each level's parent at `+8`) by their role, as
/// errors where an index would panic: the walked level is the first, its parent the second.
/// A caller reaches them after it found the path data not empty.
pub(crate) trait Levels {
    /// The walked level.
    ///
    /// # Errors
    ///
    /// Path data without a level.
    fn walked(&self) -> GenieResult<&PathData>;

    /// The walked level, to change.
    ///
    /// # Errors
    ///
    /// Path data without a level.
    fn walked_mut(&mut self) -> GenieResult<&mut PathData>;

    /// The walked level's parent.
    ///
    /// # Errors
    ///
    /// Path data of one level or none.
    fn parent(&self) -> GenieResult<&PathData>;
}

impl Levels for [PathData] {
    fn walked(&self) -> GenieResult<&PathData> {
        self.first().ok_or(NO_LEVEL)
    }

    fn walked_mut(&mut self) -> GenieResult<&mut PathData> {
        self.first_mut().ok_or(NO_LEVEL)
    }

    fn parent(&self) -> GenieResult<&PathData> {
        self.get(1).ok_or(NO_LEVEL)
    }
}
