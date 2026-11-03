//! What the Hunt's steps answer: whether its tree search found a tree ([`TreeSearch`]),
//! whether its gathering asks for the drop-off ([`HuntGather`]) and whether the drop-off's
//! queued orders end it ([`HuntEnd`]).

/// Whether the Hunt's search for another tree found one (`FUN_140ac4670`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum TreeSearch {
    /// It found one: the hunt takes it.
    Found,
    /// None counts.
    NoneFound,
}

/// Whether the Hunt's gathering (state 7) ends in the drop-off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum HuntGather {
    /// A full load or an empty target: the caller asks for `set_state(8)`.
    DropOff,
    /// The gathering goes on.
    GoesOn,
}

/// Whether the Hunt ends at its drop-off for the unit AI's queued orders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum HuntEnd {
    /// The hunt ends: its unit AI runs the queued order.
    Ends,
    /// No orders are queued: the hunt goes on.
    GoesOn,
}
