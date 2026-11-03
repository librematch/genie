//! How a `MoveTo` to a target started.

/// How a `MoveTo` to a target started (its start, `0x140aa2100`): the Attack's walk to its
/// target (`Attack::walk_to_target`) and its start, and the Hunt's chase, start and retarget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub(crate) enum MoveStart {
    /// Within 0.01 of the target's footprint: state 1.
    Arrived,
    /// Walking (state 3, or 13 without speed), or no `MoveTo` (the list deleted it, or none
    /// started).
    UnderWay,
}
