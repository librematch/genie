//! `TRIBE_Combat_Object::update` (`0x141208440`), a combat object's or building's own update
//! (types 70 and 80) around `RGE_Combat_Object::update`. Before it: the faith's regeneration, a
//! salvage pile's decay and the special sprite's flag ([`Unit::combat_update_start`]), then the
//! blast at death ([`Unit::death_blast`]). After it: the regeneration
//! ([`Unit::regenerate`]), the charge's refill ([`World::refill_object_charge`]), the passive
//! abilities ([`crate::combat::abilities`]) and the hit point powerup's check
//! ([`Unit::clamp_hp_powerup`]).
//!
//! [`Unit::regenerate`]: crate::objects::unit::Unit::regenerate
//! [`Unit::clamp_hp_powerup`]: crate::objects::unit::Unit::clamp_hp_powerup

use crate::GenieResult;
use crate::combat::{Charge, ChargeRate, SpecialSprite};
use crate::gathering::DecayTimer;
use crate::math::image::combat_update::ONE;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ChargeKind;
use crate::objects::masters::MasterKind;
use crate::objects::masters::SpecialAttack;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::technology::Attributes;
use crate::unit_ai::AlertMode;
use crate::unit_ai::UnitAi;
use crate::world::Seconds;
use crate::world::World;
use crate::world::layer::{ClassRoles, Roles};
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;

/// The faith a priest holds at most: the immediate `0x42c80000` the combat setup
/// (`FUN_14121df60`, `+0x310`) and the faith's regeneration (`0x14120861f`) store.
pub(crate) const FAITH_TOP: Amount = Amount::new(100.0);

impl Unit {
    /// `TRIBE_Combat_Object::update` before `RGE_Combat_Object::update`, for an object whose
    /// master is of kind `kind` (a unit's update calls it first with 70; a building's,
    /// `TRIBE_Building_Object::update`, after its own steps with 80): the faith's regeneration ([`regained_faith`]: a live object of
    /// a faithful class by its master's reload time `+0x838`, one whose master has a conversion
    /// task, `FUN_1415c60b0` with 0x68, by its player's attribute 282; the kind becomes 34), a
    /// salvage pile's decay ([`Unit::salvage_decay`]) and the special sprite's flag
    /// ([`special_sprite`]). The renderer's part of the flag is not the simulation's.
    ///
    /// # Errors
    ///
    /// A master the player has not got, or a player without attribute 282 or 65.
    pub(crate) fn combat_update_start(
        &mut self,
        actions: &Action,
        step: &mut Step<'_>,
        roles: &Roles,
        kind: MasterKind,
    ) -> GenieResult<()> {
        let master = self.master(step.world.master_tables(), step.player)?;
        if master.kind != kind {
            return Ok(());
        }
        let (class, master_id, reload) = (master.class, master.id, master.combat_part().reload);
        let special = master.creatable_part().special_attack == SpecialAttack::RETALIATION
            && master.creatable_part().special_sprite.is_some();
        let slots = step.world.model.slots.attribute;
        let alive = self.life.state == ObjectState::Alive;
        // the class test, then the conversion task's (it runs for a dead priest too)
        let rate = if roles.of_class(class).contains(ClassRoles::FAITHFUL) && alive {
            Some(reload.get())
        } else if step
            .world
            .has_task(step.player, master_id, ActionKind::CONVERT)?
            && alive
        {
            let owner = step.world.players.player(step.player)?;
            Some(Attributes::get(owner, slots.converter_faith_rate)?.get())
        } else {
            None
        };
        if let Some(rate) = rate {
            self.life.amount = regained_faith(
                self.life.amount,
                rate,
                step.dt.seconds(),
                step.world.model.tuning.unit_ai.full_faith,
            );
            self.life.attribute_kind = Some(slots.faith);
        }
        if roles.of_class(class).contains(ClassRoles::SALVAGE_PILE) {
            self.salvage_decay(step)?;
        }
        if special {
            let shows = special_sprite(step.world.parts.unit_ai.get(self.id), actions);
            step.world.parts.combat_mut(self.id)?.special_sprite = shows;
        }
        Ok(())
    }

    /// A salvage pile's decay (class 40, `0x141208698`): while it holds an amount above 0 (not
    /// a NaN), its decay timer loses the world's dt in ms (`+0x7c`, the low 16 bits); at 0 or
    /// below its amount loses 1 (`DAT_14330eb8c`, `SUBSS`), each of its six amounts one
    /// ([`crate::gathering::SalvageAmounts::decay`]) and the timer starts again at its player's
    /// salvage decay rate ([`DecayTimer::of_rate`]).
    ///
    /// # Errors
    ///
    /// A player without attribute 65.
    fn salvage_decay(&mut self, step: &mut Step<'_>) -> GenieResult<()> {
        if 0.0 < self.life.amount.get() {
            let rate = Attributes::get(
                step.world.players.player(step.player)?,
                step.world.model.slots.attribute.salvage_decay_rate,
            )?;
            let combat = step.world.parts.combat_mut(self.id)?;
            combat.decay_timer = combat.decay_timer.less(step.dt);
            if !combat.decay_timer.runs() {
                self.life.amount = Amount::new(self.life.amount.get() - ONE);
                combat.salvage.decay();
                combat.decay_timer = DecayTimer::of_rate(rate);
            }
        }
        Ok(())
    }
}

/// The faith a regeneration leaves of `faith`: its rate `rate` times the world's dt `dt`
/// (`MULSS`) plus the faith (`ADDSS`); at `full` or more (`COMISS`, a NaN is not) it holds
/// [`FAITH_TOP`].
#[must_use]
pub(crate) fn regained_faith(faith: Amount, rate: f32, dt: Seconds, full: f32) -> Amount {
    let regained = rate * dt.get() + faith.get();
    if full <= regained {
        FAITH_TOP
    } else {
        Amount::new(regained)
    }
}

/// The special sprite's flag of an object whose master has one (special attack 2 and a special
/// sprite, `0x1412087ae`): shown while its unit AI `unit_ai` is alert with no attackers and no
/// current action (`FUN_140c42b30`) and its action list `actions` is empty (`FUN_140a98730`),
/// else hidden; an object without a unit AI hides it.
#[must_use]
pub(crate) fn special_sprite(unit_ai: Option<&UnitAi>, actions: &Action) -> SpecialSprite {
    let idle = unit_ai.is_some_and(|unit_ai| {
        unit_ai.alert_mode == AlertMode::Alert
            && unit_ai.attackers.is_empty()
            && unit_ai.current_action.is_none()
    });
    if idle && actions.top_action_kind().is_none() {
        SpecialSprite::Shown
    } else {
        SpecialSprite::Hidden
    }
}

impl World {
    /// `TRIBE_Combat_Object::update`'s charge refill after the regeneration, for the combat
    /// object or building in `slot` ([`World::live_slot`]): below its master's `max_charge`
    /// (`+0x918`, `COMISS`: a NaN refills nothing) its charge (`+0x9e0`) gains the master's
    /// `recharge_rate` (`+0x91c`) times the world's dt ([`refilled`]), unless the master's
    /// charge is a timed ability's (type −3, `FUN_14107d160`) that lasts
    /// ([`crate::combat::AbilityTimer::lasts`]). At its most the local player's interface
    /// flag (`FUN_141663f50`, `DAT_143fb1d40+0x1871`) is the renderer's.
    ///
    /// # Errors
    ///
    /// A slot the player's list does not hold, or a master the player has not got.
    pub(crate) fn refill_object_charge(
        &mut self,
        slot: (PlayerId, ListIndex, ObjectId),
        dt: Seconds,
    ) -> GenieResult<()> {
        let Some((player_id, list_index)) = self.live_slot(slot) else {
            return Ok(());
        };
        let unit = self.players.live(player_id, list_index)?;
        let id = unit.id;
        let master = unit.master(self.master_tables(), player_id)?;
        if !matches!(master.kind, MasterKind::COMBAT | MasterKind::BUILDING) {
            return Ok(());
        }
        let creatable = master.creatable_part();
        let (most, rate, kind) = (
            creatable.max_charge,
            creatable.recharge_rate,
            creatable.charge_kind,
        );
        let now = self.world_time;
        let combat = self.parts.combat_mut(id)?;
        if combat.charge < most && !(kind == ChargeKind::TIMED_ABILITY && combat.ability.lasts(now))
        {
            combat.charge = refilled(combat.charge, rate, dt);
        }
        Ok(())
    }
}

/// A charge's refill: the rate `rate` times the world's dt `dt` (`MULSS`, the rate first) plus
/// the charge `charge` (`ADDSS`).
#[must_use]
pub(crate) fn refilled(charge: Charge, rate: ChargeRate, dt: Seconds) -> Charge {
    let mut filled = rate * dt;
    filled += charge;
    filled
}

#[cfg(test)]
mod tests;
