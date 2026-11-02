//! The masters' combat values: their charge (`Charge`), firepower and missile counts, and the
//! missiles' data.

pub mod charge;
pub mod firepower;
pub mod missile;
pub use self::charge::Charge;
pub use self::charge::ChargeRate;
pub use self::firepower::AttackCount;
pub use self::firepower::Firepower;
pub use self::firepower::MissileCount;
