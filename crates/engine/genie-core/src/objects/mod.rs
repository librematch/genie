//! One object and what every object has and runs: the object the engine tracks ([`unit::Unit`])
//! with its update, its actions' holdings and stops, its life and death, its release, its sprite
//! and facing, and the masters it is made from.

mod action_state;
pub mod actions;
mod damaged_timer;
mod facet;
pub mod facing;
mod frame;
pub(crate) mod hit_points;
mod hp;
pub mod layers;
pub mod lifecycle;
pub mod lines;
mod list_index;
pub mod masters;
mod object_id;
pub mod object_list;
mod regeneration;
pub(crate) mod release;
mod relics;
pub mod sprite;
mod sprite_flags;
mod sprite_id;
pub(crate) mod teardown;
pub mod unit;
mod work_rate;
mod worker_count;

pub use self::action_state::ActionState;
pub use self::damaged_timer::DamagedTimer;
pub use self::facet::{Facet, FacetCount};
pub use self::frame::{AnimateState, Frame};
pub use self::hit_points::HpPowerup;
pub use self::hp::{HitPoints, MasterHitPoints};
pub use self::lifecycle::ObjectState;
pub use self::list_index::{ListIndex, Places};
pub use self::object_id::{ObjectId, ObjectKey};
pub use self::regeneration::Regeneration;
pub use self::relics::Relics;
pub use self::sprite_flags::SpriteFlags;
pub use self::sprite_id::SpriteId;
pub use self::work_rate::WorkRate;
pub use self::worker_count::WorkerCount;
