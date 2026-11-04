//! The unit's own Attack in its update: a lost target's end, and the strike with what
//! follows.

use crate::combat::MissileCount;
use crate::combat::attack::Armament;
use crate::combat::attack::{Attack, AttackState};
use crate::combat::attack::{AttackReads, Fight};
use crate::combat::damage::Victim;
use crate::combat::victims::{KillLeft, file_kill, victim, victim_of};
use crate::combat::volley::armed;
use crate::math::rng::Stream;
use crate::movement::Me;
use crate::objects::layers::Action;
use crate::objects::layers::Combat;
use crate::objects::masters::ClassId;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::PlayersMasters;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::players::PlayerKind;
use crate::world::ATTACKER_MASTER;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::layer::Roles;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl Unit {
    /// The unit's own Attack in its update, `attack` the head's copy: after its target went
    /// ([`crate::combat::attack::Attack::lost`]), or its update (slot 6, `0x140a82ae0`) with the
    /// strike and what follows; codes 1 and 3 delete it (`RGE_Action_Object::update`). `own` is
    /// the unit as a combat object. The strike reads its target and the blast's objects from
    /// the world and writes them back there right after the hit
    /// ([`crate::combat::victims::write_strike_back`]); the unit's own kill by a retaliation is
    /// the caller's after the update.
    pub(crate) fn attack_part(
        &mut self,
        mut attack: Attack,
        actions: &mut Action,
        step: &mut Step<'_>,
        me: Me,
        own: Option<Victim>,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        if attack.target.is_some() {
            return self.attack_strike_part(attack, actions, step, me, own, roles);
        }
        // the lost part reaches no action of the unit's: the copy stands for the head until
        // it goes back, before an error too (the stop's context reads the list)
        let code = self.attack_lost_part(&mut attack, step, me, roles);
        actions.list.set_attack(Some(attack));
        code
    }

    /// The unit's own Attack after its target went
    /// ([`crate::combat::attack::Attack::lost`]): its update without a target, the notify its
    /// disarm posts to the unit AI; codes 1 and 3 delete it.
    fn attack_lost_part(
        &mut self,
        attack: &mut Attack,
        step: &mut Step<'_>,
        me: Me,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        let master = self.master(step.w.master_tables(), step.player)?;
        let armed = armed(
            master,
            step.w.parts.combat_of(self.id).firepower,
            roles,
            &step.w.model.tuning.turn,
        );
        let master_class = master.class;
        let walking = matches!(
            attack.state,
            AttackState::GotoSeen { .. } | AttackState::GotoPoint { .. }
        );
        if walking && armed {
            return self.attack_walks_unseen(attack, step, me, master_class, roles);
        }
        let (code, disarmed) = attack.update_targetless(
            step.dt.seconds(),
            if armed {
                Armament::Armed
            } else {
                Armament::Unarmed
            },
            step.w.parts.motion_mut(self.id),
        )?;
        if disarmed {
            let posted = attack.disarmed(
                &mut self.sprite,
                step.w.parts.motion_mut(self.id),
                step.w.streams.get(Stream::Sprite),
                step.w.model.tuning.sprite,
            )?;
            if let Some((kind, p)) = posted {
                self.post_own(&mut step.w.parts.unit_ai, kind, p)?;
            }
        }
        Ok(HeadCode::of(code))
    }

    /// The unit's own Attack without a target, walking to where it saw it (state 5 or 11): its
    /// update ([`crate::combat::attack::Attack::update_gone`]); codes 1 and 3 delete it.
    fn attack_walks_unseen(
        &mut self,
        attack: &mut Attack,
        step: &mut Step<'_>,
        me: Me,
        master_class: ClassId,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        let master = self.master(step.w.master_tables(), step.player)?;
        let player_kind = step
            .w
            .players
            .get(step.player.index())
            .map_or(PlayerKind::Phantom, |player| player.kind);
        let attack_reads = AttackReads::of(
            step.w.parts.combat_of(self.id),
            master,
            (
                OwnerMasters::new(
                    &step.w.parts.own_master,
                    step.w.model.masters.of(step.player),
                ),
                &step.w.model.graphics,
            ),
            step.w.parts.unit_ai.get(self.id),
            (player_kind, step.w.world_time),
            roles,
            &step.w.model.tuning.turn,
        );
        attack
            .update_gone(step, self, me, &attack_reads, master_class, roles)
            .map(HeadCode::of)
    }

    /// The unit's own Attack's update (slot 6, `0x140a82ae0`) on the step: what it reads of the
    /// unit ([`AttackReads::of`]), its target and the unit as combat objects ([`Fight`]); the
    /// Attack, the head's copy, goes back after it, then the retaliation's changes to the unit
    /// ([`Unit::hit_back`]). Codes 1 and 3 delete it (`RGE_Action_Object::update`).
    fn attack_strike_part(
        &mut self,
        mut attack: Attack,
        actions: &mut Action,
        step: &mut Step<'_>,
        me: Me,
        own: Option<Victim>,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        let master = object_master(
            step.w.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            step.w.model.masters.of(step.player),
        )?;
        let player_kind = step
            .w
            .players
            .get(step.player.index())
            .map_or(PlayerKind::Phantom, |player| player.kind);
        let attack_reads = AttackReads::of(
            step.w.parts.combat_of(self.id),
            master,
            (
                OwnerMasters::new(
                    &step.w.parts.own_master,
                    step.w.model.masters.of(step.player),
                ),
                &step.w.model.graphics,
            ),
            step.w.parts.unit_ai.get(self.id),
            (player_kind, step.w.world_time),
            roles,
            &step.w.model.tuning.turn,
        );
        let victim = victim(step, attack.target, roles)?.ok_or(GenieError::unsupported(
            "an attack's target the slice does not hold live",
        ))?;
        let own = own.ok_or(ATTACKER_MASTER)?;
        let mut fight = Fight { victim, own };
        // written back before an error too: the stop's context reads the list
        let code = attack.update(step, self, actions, me, &mut fight, &attack_reads);
        if let Some(a) = actions.list.attack_mut() {
            *a = attack;
        }
        let code = code?;
        // the retaliation's kill of the unit itself, which follows its step
        if let Some(kill) = self.hit_back(fight.own, step.w.parts.ai_and_combat_mut(self.id)?)? {
            let own = KillLeft::own(self.id, kill);
            file_kill(step, (self, actions), own, roles)?;
        }
        Ok(HeadCode::of(code))
    }
}

/// `RGE_Combat_Object::update`'s timers (`0x14107dfb0`, before the actions) on an object's
/// combat layer `combat`, `dt` the world's step and `now` its time: the attack timer runs down,
/// and a reload world time that passed ends the volley under way.
pub(crate) fn run_down_attack_timers(combat: &mut Combat, dt: Seconds, now: WorldTime) {
    // RGE_Combat_Object::update (0x14107dfb0): the attack timer runs down by dt times
    // `+0x87c` (a terrain look's rate, [`crate::combat::abilities`])
    if Seconds::ZERO < combat.attack_timer {
        combat.attack_timer -= dt * combat.abilities.attack_rate;
        if combat.attack_timer < Seconds::ZERO {
            combat.attack_timer = Seconds::ZERO;
        }
    }
    // then a reload world time past ends the volley under way (`+0xa04`) and is none
    if combat.reload_worldtime != WorldTime::ZERO && combat.reload_worldtime < now {
        combat.next_volley = MissileCount::ZERO;
        combat.reload_worldtime = WorldTime::ZERO;
    }
}

impl Unit {
    /// The hunter or the attacker as a combat object, which its hits read and change: none for a
    /// unit without a Hunt or an Attack, or without its master.
    pub(crate) fn own_victim(
        &self,
        actions: &Action,
        step: &Step<'_>,
        roles: &Roles,
    ) -> GenieResult<Option<Victim>> {
        if actions.list.hunt().is_none() && actions.list.attack().is_none() {
            return Ok(None);
        }
        let master = self.master(step.w.master_tables(), step.player)?;
        let terrain = step.w.spatial.terrain.as_ref();
        Ok(Some(victim_of(
            (
                self,
                step.w.parts.unit_ai.contains(self.id),
                step.w.parts.building_of(self.id),
                step.w.parts.combat_of(self.id),
                actions,
                step.w.parts.unread.get(self.id),
            ),
            master,
            &step.w.players.player(step.player)?.attributes,
            step.player,
            terrain,
            step.w.mode_680,
            roles,
        )))
    }
}
