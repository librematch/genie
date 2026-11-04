//! A missile's landing (`missile_lands`): on its target, or on the ground, where it ends and,
//! unless its master's vanish mode keeps it, is destroyed.

use crate::combat::missile::MissileState;
use crate::combat::missile::ShooterId;
use crate::combat::missile::StrikeResult;
use crate::combat::missile::TargetOwner;
use crate::combat::missile::VanishMode;
use crate::combat::missile::{Missile, MissileAction, MissileStats, MissileTargetId, on_target};
use crate::combat::victims::victim;
use crate::combat::volley::Fired;
use crate::map::Distance;
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::lifecycle::death_parts::forget_resource;
use crate::objects::masters::MasterTables;
use crate::objects::masters::player_master;
use crate::objects::unit::Unit;
use crate::world::layer::Roles;
use crate::world::lets::release;
use crate::world::others::OthersRef;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl Unit {
    /// `FUN_140a9e6e0`'s landing: a missile still flying toward a target it did not meet lands on
    /// it when it lies on the target's box ([`crate::combat::missile::on_target`]) and its master's hit
    /// mode is 0 or 2, or 1 against another player's target: the shooter's hit
    /// ([`Unit::missile_strike`]), which ends it unless a shield blocked it. A missile that still
    /// flies after that hits the ground: unless its last hit was blocked, the blast of the object
    /// it was fired from there (slot 0x848, `FUN_14107f690`: nothing without an area effect; one
    /// is not modelled); it ends (`set_state` 1) and, unless its master's vanish mode has
    /// [`crate::combat::missile::VanishMode::STAYS`], is
    /// destroyed (slot 0xd0, `FUN_14106c420`: its action list goes, which lets its target and
    /// shooter go; `FUN_14118f1e0`: its hit points 0 and state 7).
    pub(crate) fn missile_lands(
        &mut self,
        actions: &mut Action,
        missile: &Missile,
        slot: &mut Option<MissileAction>,
        stats: &MissileStats,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        let fired = Fired {
            charge: missile.charge,
            by: missile.fired_from,
        };
        let charge = fired.charge;
        let Some(action) = slot.as_mut() else {
            return Ok(());
        };
        self.missile_lands_on_target(actions, action, fired, stats, step, roles)?;
        if action.state == MissileState::Done {
            return Ok(());
        }
        if action.result != StrikeResult::Shielded {
            let blaster = if !charge && action.fired_from.is_some() {
                OthersRef::of(&step.w.players, &step.w.index, step.player)?
                    .own(action.fired_from)
                    .map(|unit| unit.life.master_id)
            } else {
                Some(self.life.master_id)
            };
            let area = blaster
                .and_then(|id| player_master(&step.w.model.masters, step.player, id))
                .map(|master| master.combat_part().area_effect)
                .ok_or(GenieError::unsupported(
                    "a landed missile's blast of a master the slice does not have",
                ))?;
            if Distance::ZERO < area {
                return Err(GenieError::unsupported("a landed missile's blast"));
            }
        }
        self.end_missile(action);
        if !stats.vanish_mode.contains(VanishMode::STAYS) {
            self.missile_destroyed(slot, step)?;
        }
        Ok(())
    }

    /// [`Unit::missile_lands`] on a target it did not meet yet: it lands when it lies on the
    /// target's box and its master's hit mode is 0 or 2, or 1 against another player's target;
    /// the shooter's hit ([`Unit::missile_strike`]) ends it unless a shield blocked it.
    fn missile_lands_on_target(
        &mut self,
        actions: &mut Action,
        action: &mut MissileAction,
        fired: Fired,
        stats: &MissileStats,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        let Some(target) = action.target else {
            return Ok(());
        };
        if action.state == MissileState::Done || action.met.contains(&target.id()) {
            return Ok(());
        }
        let victim = victim(step, action.target, roles)?.ok_or(GenieError::unsupported(
            "a missile's target the slice does not hold live",
        ))?;
        let owner = if step.player == victim.player {
            TargetOwner::Own
        } else {
            TargetOwner::Other
        };
        if stats.hit_mode.strikes_on_landing(owner)
            && on_target(
                self.position(),
                stats.radius,
                victim.position,
                victim.radius,
            )
        {
            let result = self.missile_strike(actions, action, fired, step, roles)?;
            action.result = result;
            if result != StrikeResult::Shielded {
                self.end_missile(action);
            }
        }
        Ok(())
    }

    /// A landed missile's destruction (slot 0xd0, `FUN_14106c420`): its action list goes,
    /// which lets its target and shooter go ([`crate::world::lets::release`]); then
    /// `FUN_14118f1e0`: its hit points 0 and state 7, out of the visible resources below state
    /// 4. One inside another is not modelled.
    fn missile_destroyed(
        &mut self,
        slot: &mut Option<MissileAction>,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        if let Some(missile_action) = slot.take() {
            let by = self.releaser(step)?;
            for object_id in [
                missile_action.target.map(MissileTargetId::id),
                missile_action.fired_from.map(ShooterId::id),
            ]
            .into_iter()
            .flatten()
            {
                release(step, self, object_id, by)?;
            }
        }
        if self.life.state < ObjectState::Removed {
            if self.life.inside.is_some() {
                return Err(GenieError::unsupported(
                    "a destroyed object inside another (slot 0x1a8)",
                ));
            }
            let before = self.life.state;
            self.life.hp = HitPoints::ZERO;
            self.life.goto_sleep = false;
            self.life.state = ObjectState::Removed;
            step.w.spatial.sight_state(
                self.id,
                ObjectState::Removed,
                &step.w.model.roles,
                step.w.model.tuning.visibility,
            )?;
            if before < ObjectState::DyingSprite {
                let master = self.master(
                    MasterTables::new(&step.w.model.masters, &step.w.parts.own_master),
                    step.player,
                )?;
                forget_resource(&mut step.w.spatial, Some(master), self.id, self.position())?;
            }
        }
        Ok(())
    }
}
