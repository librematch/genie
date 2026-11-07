//! What `RGE_Action_Object::update` (`0x14106ac90`) does after the action list: a villager's
//! idle statistics, then the drop of a speed-up building that is gone or out of range.

use crate::GenieResult;
use crate::movement::footprint_gap_squared;
use crate::objects::ObjectState;
use crate::objects::layers::{Action, IdleCount, Speedup};
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::world::NO_ATTRIBUTE;
use crate::world::layer::ClassRoles;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::world::{Millis, WorldTime};

impl Unit {
    /// `RGE_Action_Object::update`'s end before `FUN_141163080`: a villager's idle statistics
    /// ([`Unit::idle_statistics`]), then the speed-up building's check ([`Unit::speedup_check`]).
    ///
    /// # Errors
    ///
    /// The unit's master, slot 0x2d0's and the attributes'.
    pub(crate) fn action_update_end(
        &self,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        self.idle_statistics(actions, step)?;
        self.speedup_check(actions, step)
    }

    /// A villager's (class 4) idle time: while it may be interrupted in state 2 (slot 0x2d0
    /// with 1, [`crate::world::World::unit_may_interrupt`]) an idle begins once (`+0x898` 0:
    /// the count `+0x8a0` one more, `+0x898` the world's time); otherwise a begun idle ends into
    /// its milliseconds (`+0x89c`). Past state 2, idle milliseconds go to its owner's
    /// statistics: attribute 250 takes the seconds (`CVTSI2SS` of the unsigned milliseconds,
    /// `DIVSS` by 1000) and attribute 249 the count (`CVTDQ2PS`), and both fields go to 0. The
    /// game first counts a begun idle up to now there and begins it again; past state 2 the
    /// branch before has ended every idle, so that test always finds `+0x898` 0. The telemetry's
    /// flush of the same fields (`FUN_14106fc60`, from `FUN_14160b920`), where the test can
    /// find one, is out of scope.
    fn idle_statistics(&self, actions: &mut Action, step: &mut Step<'_>) -> GenieResult<()> {
        let master = self.master(step.world.master_tables(), step.player)?;
        if !step
            .world
            .model
            .roles
            .of_class(master.class)
            .contains(ClassRoles::VILLAGER)
        {
            return Ok(());
        }
        let interruptible = step.world.unit_may_interrupt(step.player, self, actions)?;
        let now = step.world.world_time;
        let idle = &mut actions.idle;
        if interruptible && self.life.state == ObjectState::Alive {
            if idle.since == WorldTime::ZERO {
                idle.count = idle.count.next();
                idle.since = now;
            }
        } else if idle.since != WorldTime::ZERO {
            idle.idle = idle.idle.wrapping_add(now.since(idle.since));
            idle.since = WorldTime::ZERO;
        }
        if self.life.state <= ObjectState::Alive || idle.idle == Millis::ZERO {
            return Ok(());
        }
        let slots = step.world.model.slots.attribute;
        let owner = step.world.players.player_mut(step.player)?;
        owner.add_attribute(
            slots.villager_idle_seconds,
            Amount::new(idle.idle.seconds().get()),
        );
        owner.add_attribute(slots.villager_idles, Amount::new(idle.count.as_f32()));
        idle.idle = Millis::ZERO;
        idle.count = IdleCount::ZERO;
        Ok(())
    }

    /// The speed-up building (`+0x890`, by the owner's world: `FUN_1415417a0`) stays while its
    /// squared footprint gap to the unit (`FUN_14118f8c0`) is below the square of the owner's
    /// attribute 256 (`COMISS`, `JC`: a NaN gap drops it); otherwise `+0x890` takes −1 and
    /// `+0x894` 0, as for none.
    fn speedup_check(&self, actions: &mut Action, step: &Step<'_>) -> GenieResult<()> {
        let keeps = match actions.speedup.building {
            Some(building) => match step.world.spatial.get_known(building)? {
                Some(placed) => {
                    // the game reads the attribute before the lookup; only a building held uses it
                    let range = step
                        .world
                        .players
                        .player(step.player)?
                        .attribute(step.world.model.slots.attribute.speedup_range)
                        .ok_or(NO_ATTRIBUTE)?
                        .get();
                    let radius = self.master(step.world.master_tables(), step.player)?.radius;
                    let gap = footprint_gap_squared(
                        self.place.position(),
                        radius,
                        placed.position,
                        placed.radius,
                    );
                    gap.get() < range * range
                }
                None => false,
            },
            None => false,
        };
        if !keeps {
            actions.speedup = Speedup::NONE;
        }
        Ok(())
    }
}
