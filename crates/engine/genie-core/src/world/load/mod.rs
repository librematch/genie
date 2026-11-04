//! The world's load from a save (FD-043-01M6JPBEDA, the review's ORA-01): each saved object's
//! row, its parts, its movement, its actions and sub-actions, as genie-save reads them from a
//! recording's header or a per-tick save.

pub mod movers;
pub mod object;
pub mod object_parts;
pub mod sub_actions;
pub mod top_actions;

#[cfg(test)]
mod fixtures;

use crate::GenieError;
use crate::objects::ObjectId;

/// The ids of a saved list of objects, which names no none (−1).
///
/// # Errors
///
/// An entry of −1.
pub fn object_ids(ids: &[i32]) -> Result<Vec<ObjectId>, GenieError> {
    ids.iter().map(|&id| ObjectId::try_from(id)).collect()
}
