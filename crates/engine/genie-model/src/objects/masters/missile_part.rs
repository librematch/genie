//! A master's missile layer: the RGE missile master's values (type 60, the `.dat`'s projectile
//! section; FD-046-D05), and what a missile's flight reads of its master.

use crate::combat::missile::ArcRatio;
use crate::combat::missile::HitMode;
use crate::combat::missile::MissileStats;
use crate::combat::missile::ProjectileKind;
use crate::combat::missile::SmartMode;
use crate::combat::missile::VanishMode;
use crate::objects::masters::Master;
use genie_diff::Diff;
use serde::Deserialize;
use serde::Serialize;

/// The missile layer of a master of type 60 (`RGE_Master_Missile_Object`, `+0x880`..`+0x888`;
/// the `.dat`'s projectile section, genieutils' `Projectile`): its projectile type, smart, hit
/// and vanish modes and its arc. Only a missile holds it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MissilePart {
    /// `projectile_type` (`+0x880`, the `.dat`'s; the save does not hold it).
    pub projectile_kind: ProjectileKind,
    /// `smart_mode` (`+0x881`, the save's `targetting_type`).
    pub smart_mode: SmartMode,
    /// `hit_mode` (`+0x882`, the save's `missile_hit_info`).
    pub hit_mode: HitMode,
    /// `vanish_mode` (`+0x883`, the save's `missile_die_info`).
    pub vanish_mode: VanishMode,
    /// `projectile_arc` (`+0x888`, the save's `ballistics_ratio`).
    pub arc: ArcRatio,
}

impl Master {
    /// What a missile's flight reads of its master ([`MissileStats`]): the missile layer's
    /// values, the animated layer's top speed (slot 0xe8, `+0x740`), the radii
    /// (`+0x5d8`..`+0x5e0`) and the fly mode (`+0x608`); None for a master without the missile
    /// layer.
    #[must_use]
    pub fn missile_stats(&self) -> Option<MissileStats> {
        self.missile.map(|m| MissileStats {
            projectile_kind: m.projectile_kind,
            smart_mode: m.smart_mode,
            hit_mode: m.hit_mode,
            vanish_mode: m.vanish_mode,
            arc: m.arc,
            speed: self.animated_part().max_speed,
            radius: self.radius.with_height(self.height.get()),
            fly_mode: self.fly_mode,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::missile::ArcRatio;
    use crate::combat::missile::MissileStats;
    use crate::combat::missile::ProjectileKind;
    use crate::map::Distance;
    use crate::map::Footprint;
    use crate::map::Radii;
    use crate::movement::Speed;
    use crate::objects::masters::FlyMode;
    use crate::objects::masters::Master;
    use crate::objects::masters::MasterKind;
    use crate::objects::masters::MissilePart;

    /// A missile (60) holds the missile layer; a combat unit (70) and a building (80) do not.
    #[test]
    fn only_missiles_hold_the_missile_layer() {
        assert!(Master::of_kind(MasterKind::MISSILE).missile.is_some());
        assert_eq!(Master::of_kind(MasterKind::COMBAT).missile, None);
        assert_eq!(Master::of_kind(MasterKind::BUILDING).missile, None);
    }

    /// A missile's flight reads the layer with the master's current speed, radii and fly mode, so
    /// an effect on the speed or the size reaches the flight with no copy to update.
    #[test]
    fn the_flight_reads_the_layer_and_the_masters_speed_and_size() {
        let mut m = Master::of_kind(MasterKind::MISSILE);
        let layer = MissilePart {
            projectile_kind: ProjectileKind::of_data(1),
            arc: ArcRatio::new(0.25),
            ..MissilePart::default()
        };
        m.missile = Some(layer);
        m.animated.get_or_insert_with(Default::default).max_speed = Speed::new(7.0);
        (m.radius, m.height) = (Footprint::new(0.1, 0.2), Distance::new(0.3));
        m.fly_mode = FlyMode::of_data(1);
        assert_eq!(
            m.missile_stats(),
            Some(MissileStats {
                projectile_kind: layer.projectile_kind,
                smart_mode: layer.smart_mode,
                hit_mode: layer.hit_mode,
                vanish_mode: layer.vanish_mode,
                arc: layer.arc,
                speed: Speed::new(7.0),
                radius: Radii::new(0.1, 0.2, 0.3),
                fly_mode: FlyMode::of_data(1),
            })
        );
        assert_eq!(Master::of_kind(MasterKind::COMBAT).missile_stats(), None);
    }
}
