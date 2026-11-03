//! What an object's update steps answer: whether it falls asleep ([`SleepCheck`]), starts its
//! default task ([`DefaultStart`]) or leaves its list ([`ListFate`]), and whether a wake found
//! it asleep ([`Wake`]).

/// Whether an object flagged to sleep falls asleep in its update (`RGE_Static_Object::update`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum SleepCheck {
    /// It moves to its player's sleeping list, and its update ends.
    FallsAsleep,
    /// It is not flagged: its update goes on.
    StaysAwake,
}

/// Whether an idle unit started its master's default task (slot 0x7f0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum DefaultStart {
    /// The task started; its first update comes in the next update.
    Started,
    /// None started: the unit is busy, not alive, or its master has none.
    NotStarted,
}

/// Whether an object leaves its list after its lifecycle's update.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum ListFate {
    /// It leaves its list for good (state 8).
    Leaves,
    /// It stays.
    Stays,
}

/// Whether a wake (`FUN_14119deb0` with 0) found the object asleep.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum Wake {
    /// It left its player's sleeping list for the live one.
    Woke,
    /// No sleeping list holds it.
    NotAsleep,
}
