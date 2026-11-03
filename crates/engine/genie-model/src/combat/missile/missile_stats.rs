//! What a missile master's flight reads of its master (FD-046: the model's part of the missile).

use crate::combat::missile::ArcRatio;
use crate::combat::missile::HitMode;
use crate::combat::missile::ProjectileKind;
use crate::combat::missile::SmartMode;
use crate::combat::missile::VanishMode;
use crate::map::Radii;
use crate::movement::Speed;
use crate::objects::masters::FlyMode;
use genie_diff::Diff;
use serde::Deserialize;
use serde::Serialize;

/// What a missile's master holds for the flight: the `.dat`'s `projectile_type` (`+0x880`, not
/// saved), `smart_mode` (`+0x881`, the save's `targetting_type`), `hit_mode` (`+0x882`,
/// `missile_hit_info`), `vanish_mode` (`+0x883`, `missile_die_info`) and `projectile_arc`
/// (`+0x888`, `ballistics_ratio`); its master's speed (slot 0xe8, `+0x740`), radii
/// (`+0x5d8`..`+0x5e0`) and the `.dat`'s `fly_mode` (`+0x608`: 0 holds it on the ground, 1 above
/// it, `FUN_141193a10`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MissileStats {
    /// `projectile_type` (`+0x880`).
    pub projectile_kind: ProjectileKind,
    /// `smart_mode`.
    pub smart_mode: SmartMode,
    /// `hit_mode` (`missile_hit_info`).
    pub hit_mode: HitMode,
    /// `vanish_mode` (`missile_die_info`).
    pub vanish_mode: VanishMode,
    /// `projectile_arc` (`ballistics_ratio`).
    pub arc: ArcRatio,
    /// Its master's speed (slot 0xe8).
    pub speed: Speed,
    /// Its master's radii (`+0x5d8`..`+0x5e0`).
    pub radius: Radii,
    /// The `.dat`'s `fly_mode` (`+0x608`): 0 holds it on the ground, 1 above it.
    pub fly_mode: FlyMode,
}
