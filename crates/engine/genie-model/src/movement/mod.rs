//! The movement values a master holds: its speed, turn rate, movement algorithm and what a moving
//! unit reads of it (`MoveStats`).

pub mod move_algorithm;
pub mod move_stats;
pub mod speed;
pub mod turn_rate;
pub use self::move_algorithm::MoveAlgorithm;
pub use self::move_stats::MoveStats;
pub use self::speed::Speed;
pub use self::turn_rate::TurnRate;
