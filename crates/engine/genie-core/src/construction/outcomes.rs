//! What a foundation's work answers: whether its construction goes on ([`BuildProgress`]) and
//! whether it began ([`ConstructionStart`]).

/// Whether a foundation's construction goes on after a work (`FUN_1411cc430`'s 0 or 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum BuildProgress {
    /// It goes on: the build points stay below the build time (the routine's 0).
    Ongoing,
    /// It ended: the work reached the build time, or the foundation was not in state 0 or
    /// below one hit point before it.
    Ended,
}

/// Whether a foundation's construction began (`FUN_1411e5670`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum ConstructionStart {
    /// The desolid foundation began: its caller gives it its volley and its owner's watch.
    Begun,
    /// It was not desolid: nothing changed.
    NotDesolid,
}
