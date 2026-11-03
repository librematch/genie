//! What the Hunt's steps answer: whether its tree search found a tree ([`TreeSearch`]) and
//! whether its gathering asks for the drop-off ([`HuntGather`]).

/// Whether the Hunt's search for another tree found one (`FUN_140ac4670`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum TreeSearch {
    /// It found one: the hunt takes it.
    Found,
    /// None counts.
    NoneFound,
}

/// Whether the Hunt's gathering (state 7) ends in the drop-off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum HuntGather {
    /// A full load or an empty target: the caller asks for `set_state(8)`.
    DropOff,
    /// The gathering goes on.
    GoesOn,
}
