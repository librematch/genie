//! The history's period: a number of its updates, as the `.dat` gives it (`time_slice`).

pub mod history_updates;

pub use self::history_updates::HISTORY_PERIOD;
pub use self::history_updates::HistoryUpdates;
