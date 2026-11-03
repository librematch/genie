//! What production's payments and queues answer: whether a payment was made ([`Payment`]),
//! anything queued ([`QueueOutcome`]) or given back ([`Refund`]), whether researches joined the
//! last entry ([`EntryJoin`]), whether the first entry's production goes on
//! ([`ProductionStart`]) and whether a research completed ([`Research`]).

/// Whether a player paid (`FUN_14162fb60`, `FUN_1413d4370`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum Payment {
    /// The costs came off the player's attributes.
    Paid,
    /// The player cannot pay: nothing was paid.
    Unpaid,
}

/// Whether a queue took anything (QUEUE's and RESEARCH's building part, the farm and fish-trap
/// queues' purchase).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum QueueOutcome {
    /// One or more were queued.
    Queued,
    /// None was.
    NoneQueued,
}

/// Whether the farm and fish-trap queues gave any back (`FUN_141630f10`, `FUN_1416308f0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum Refund {
    /// The queue lost them and the player got their costs back.
    Refunded,
    /// The queue held none.
    NoneQueued,
}

/// Whether researches joined the queue's last entry (`FUN_1411c81a0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum EntryJoin {
    /// They joined it.
    Joined,
    /// They need an entry of their own.
    NewEntry,
}

/// Whether the first queue entry's production goes on this update.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum ProductionStart {
    /// It goes on.
    Goes,
    /// The production stops for this update.
    Stops,
}

/// Whether a research completed (`FUN_1413cdcd0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum Research {
    /// The technology took state 3 and gave its effect.
    Completed,
    /// It was not in state 0 to 2: not done.
    NotDone,
}
