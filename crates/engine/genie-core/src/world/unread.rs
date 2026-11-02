//! The objects the load could not read whole (FD-045-D61): the import's gap, not game data. The
//! world files each one's reason in its table by id ([`crate::world::parts::Parts::unread`]), so
//! a routine that reaches such an object stops the run where it would read the part.

use crate::GenieError;

/// An object the load could not read whole: the first part of its save the import could not
/// read (its life, unit AI, lists, gather, production or mover, or a garrison's share). The slice
/// cannot run it: it counts as busy, and a routine that needs its actions stops the run with
/// [`Unread::error`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unread(GenieError);

impl Unread {
    /// The object the import could not read for `why`.
    #[must_use]
    pub const fn new(why: GenieError) -> Self {
        Self(why)
    }

    /// The error a routine that reaches the object stops the run with: the import's, at the
    /// place the import raised it.
    #[must_use]
    pub fn error(&self) -> GenieError {
        self.0.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unread_object_stops_with_the_import_s_error() {
        let why = GenieError::unsupported("a saved production list");
        let unread = Unread::new(why.clone());
        assert_eq!(unread.error(), why);
        assert_eq!(unread.error().reason(), "a saved production list");
    }
}
