//! What every master and object holds as game data: the masters, the graphics and sprites, hit
//! points, frames and facets, work rates and the unit lines.

pub mod facet;
pub mod facing;
pub mod frame;
pub mod graphics;
pub mod hp;
pub mod lifecycle;
pub mod lines;
pub mod masters;
pub mod regeneration;
pub mod speed_multiplier;
pub mod sprite;
pub mod sprite_flags;
pub mod sprite_id;
pub mod work_rate;
pub mod worker_count;
pub use self::facet::Facet;
pub use self::facet::FacetCount;
pub use self::frame::AnimateState;
pub use self::frame::Frame;
pub use self::graphics::Graphics;
pub use self::hp::HitPoints;
pub use self::hp::MasterHitPoints;
pub use self::lifecycle::ObjectState;
pub use self::regeneration::Regeneration;
pub use self::speed_multiplier::SpeedMultiplier;
pub use self::sprite_flags::SpriteFlags;
pub use self::sprite_id::SpriteId;
pub use self::work_rate::WorkRate;
pub use self::worker_count::WorkerCount;
