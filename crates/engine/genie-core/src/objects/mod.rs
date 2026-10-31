//! One object and what every object has and runs: the object the engine tracks ([`unit::Unit`])
//! with its update, its actions' holdings and stops, its life and death, its release, its sprite
//! and facing, and the masters it is made from.

pub mod actions;
pub mod facing;
pub(crate) mod hit_points;
mod hp;
pub mod layers;
pub mod lifecycle;
pub mod lines;
mod list_index;
pub mod masters;
mod object_id;
pub mod object_list;
pub(crate) mod release;
mod relics;
pub mod sprite;
pub(crate) mod teardown;
pub mod unit;
mod work_rate;

pub use self::hit_points::HpPowerup;
pub use self::hp::HitPoints;
pub use self::lifecycle::ObjectState;
pub use self::list_index::{ListIndex, Places};
pub use self::object_id::{ObjectId, ObjectKey};
pub use self::relics::Relics;
pub use self::work_rate::WorkRate;
