//! The levels of a unit's path data by their role: the walked level and its parent.

use crate::GenieError;
use crate::movement::PathData;

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
    fn walked(&self) -> Result<&PathData, GenieError>;

    /// The walked level, to change.
    ///
    /// # Errors
    ///
    /// Path data without a level.
    fn walked_mut(&mut self) -> Result<&mut PathData, GenieError>;

    /// The walked level's parent.
    ///
    /// # Errors
    ///
    /// Path data of one level or none.
    fn parent(&self) -> Result<&PathData, GenieError>;
}

impl Levels for [PathData] {
    fn walked(&self) -> Result<&PathData, GenieError> {
        self.first().ok_or(NO_LEVEL)
    }

    fn walked_mut(&mut self) -> Result<&mut PathData, GenieError> {
        self.first_mut().ok_or(NO_LEVEL)
    }

    fn parent(&self) -> Result<&PathData, GenieError> {
        self.get(1).ok_or(NO_LEVEL)
    }
}
