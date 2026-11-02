//! The missiles' game data: what a ranged master's missile reads of its master (`MissileStats`) and
//! the `.dat` codes of its flight (projectile type, hit mode, arc ratio, miss spread).

pub mod arc_ratio;
pub mod hit_mode;
pub mod miss_spread;
pub mod missile_modes;
pub mod missile_stats;
pub mod projectile_type;
pub use self::arc_ratio::ArcRatio;
pub use self::hit_mode::HitMode;
pub use self::miss_spread::MissSpread;
pub use self::missile_modes::SmartMode;
pub use self::missile_modes::VanishMode;
pub use self::missile_stats::MissileStats;
pub use self::projectile_type::ProjectileType;
