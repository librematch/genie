//! Whether a gathering action ends at its drop-off for the unit AI's queued orders.

/// Whether a Gather or a Hunt ends at its drop-off for its unit AI's queued orders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum ActionEnd {
    /// The action ends: its unit AI runs the queued order.
    Ends,
    /// No orders are queued: the action goes on.
    GoesOn,
}
