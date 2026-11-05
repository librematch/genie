//! An object's sight powerup (`+0xad4`): the powerup reset takes it off, an aura's powerup
//! raises it, each time between the sight's going and coming.

use crate::GenieResult;
use crate::garrison::ContainerId;
use crate::map::Distance;
use crate::map::spatial::Spatial;
use crate::math::image::powerup::EPSILON as POWERUP_EPSILON;
use crate::objects::ObjectId;
use crate::vision::SightLayer;
use crate::vision::{SightCentre, SightShape};

impl Spatial {
    /// The powerup reset's sight (`FUN_1412066b0`, [`crate::combat::powerup`]): a sight powerup
    /// above `powerup::EPSILON` (`COMISS`: a NaN stays) comes off, its sight going (slot 0x1d8)
    /// and coming again without it (slot 0x1d0).
    ///
    /// # Errors
    ///
    /// What the sight's slots do not model.
    pub fn take_sight_powerup(
        &mut self,
        id: ObjectId,
        container: Option<ContainerId>,
        sight: SightLayer<'_>,
    ) -> GenieResult<()> {
        let held = self
            .sight
            .as_ref()
            .and_then(|visibility| visibility.seer(id))
            .map(|seer| seer.los_powerup);
        match held {
            Some(powerup) if POWERUP_EPSILON < powerup.get() => {
                self.sight_powerup(id, container, Distance::ZERO, sight)
            }
            _ => Ok(()),
        }
    }

    /// An aura's sight powerup (`FUN_141200dd0` case 1): its sight goes (slot 0x1d8), the
    /// powerup takes `amount` (`ADDSS`, the amount first) and the sight comes (slot 0x1d0).
    ///
    /// # Errors
    ///
    /// What the sight's slots do not model.
    pub fn raise_sight_powerup(
        &mut self,
        id: ObjectId,
        container: Option<ContainerId>,
        amount: Distance,
        sight: SightLayer<'_>,
    ) -> GenieResult<()> {
        let held = self
            .sight
            .as_ref()
            .and_then(|visibility| visibility.seer(id))
            .map_or(Distance::ZERO, |seer| seer.los_powerup);
        self.sight_powerup(id, container, amount + held, sight)
    }

    /// The sight powerup of seer `id` becomes `powerup` between slots 0x1d8 and 0x1d0 (each with
    /// the object's player, the circle, its own line of sight and the tile's centre: `0`, `−1`,
    /// `0`). Inside its `container` (`+0x2f0`) the object has no tile and the slots see nothing.
    fn sight_powerup(
        &mut self,
        id: ObjectId,
        container: Option<ContainerId>,
        powerup: Distance,
        sight: SightLayer<'_>,
    ) -> GenieResult<()> {
        let placed = self.objects.get(id).copied();
        let Some(visibility) = self.sight.as_mut() else {
            return Ok(());
        };
        let around = placed.filter(|_| container.is_none());
        if let Some(placed) = around {
            visibility.sight_off(
                id,
                placed.position,
                placed.radius,
                SightShape::Circle,
                SightCentre::Tile,
                sight,
            )?;
        }
        visibility.set_los_powerup(id, powerup);
        if let Some(placed) = around {
            let _ = visibility.sight_on(
                id,
                placed.position,
                placed.radius,
                SightShape::Circle,
                SightCentre::Tile,
                sight,
            )?;
        }
        Ok(())
    }
}
