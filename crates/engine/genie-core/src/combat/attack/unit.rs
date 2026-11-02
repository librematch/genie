//! The unit's own Attack in its update: a lost target's end, and the strike with what
//! follows.

use crate::GenieError;
use crate::combat::MissileCount;
use crate::combat::attack::AttackState;
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
use crate::players::PlayerType;
use crate::world::ATTACKER_MASTER;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::layer::Roles;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

/// The unit's own Attack's update without its Attack: the caller's test.
pub(super) const NO_ATTACK: GenieError =
    GenieError::unreachable("an Attack's update without its Attack");

impl Unit {
    /// The unit's own Attack in its update: after its target went
    /// ([`crate::combat::attack::Attack::lost`]), or its update (slot 6, `0x140a82ae0`) with the
    /// strike and what follows; codes 1 and 3 delete it (`RGE_Action_Object::update`). `own` is
    /// the unit as a combat object. The strike reads its target and the blast's objects from
    /// the world and writes them back there right after the hit
    /// ([`crate::combat::victims::write_strike_back`]); the unit's own kill by a retaliation is
    /// the caller's after the update.
    pub(crate) fn attack_part(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
        own: Option<Victim>,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        match actions.list.attack().map(|a| a.target) {
            None => Ok(HeadCode::Keeps),
            Some(None) => self.attack_lost_part(actions, c, me, roles),
            Some(_) => self.attack_strike_part(actions, c, me, own, roles),
        }
    }

    /// The unit's own Attack after its target went
    /// ([`crate::combat::attack::Attack::lost`]): its update without a target, the notify its
    /// disarm posts to the unit AI; codes 1 and 3 delete it.
    fn attack_lost_part(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        let master = self.master(c.w.master_tables(), c.player)?;
        let armed = armed(master, c.w.parts.combat_of(self.id).firepower, roles);
        let master_class = master.class;
        let walking = actions.list.attack().is_some_and(|a| {
            matches!(
                a.state,
                AttackState::GotoSeen { .. } | AttackState::GotoPoint { .. }
            )
        });
        if walking && armed {
            return self.attack_walks_unseen(actions, c, me, master_class, roles);
        }
        let attack = actions.list.attack_mut().ok_or(NO_ATTACK)?;
        let (code, disarmed) =
            attack.update_targetless(c.dt.seconds(), armed, c.w.parts.motion_mut(self.id))?;
        if disarmed {
            let posted = attack.disarmed(
                &mut self.sprite,
                c.w.parts.motion_mut(self.id),
                c.w.streams.get(Stream::Sprite),
            )?;
            if let Some((kind, p)) = posted {
                self.post_own(&mut c.w.parts.unit_ai, kind, p)?;
            }
        }
        Ok(HeadCode::of(code))
    }

    /// The unit's own Attack without a target, walking to where it saw it (state 5 or 11): its
    /// update ([`crate::combat::attack::Attack::update_gone`]); codes 1 and 3 delete it.
    fn attack_walks_unseen(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
        master_class: ClassId,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        let mut attack = *actions.list.attack().ok_or(NO_ATTACK)?;
        let master = self.master(c.w.master_tables(), c.player)?;
        let player_type =
            c.w.players
                .get(c.player.index())
                .map_or(PlayerType::PHANTOM, |q| q.kind);
        let r = AttackReads::of(
            c.w.parts.combat_of(self.id),
            master,
            (
                OwnerMasters::new(&c.w.parts.own_master, c.w.model.masters.of(c.player)),
                &c.w.model.graphics,
            ),
            c.w.parts.unit_ai.get(self.id),
            (player_type, c.w.world_time),
            roles,
        );
        let code = attack.update_gone(c, self, me, &r, master_class, roles);
        if let Some(a) = actions.list.attack_mut() {
            *a = attack;
        }
        Ok(HeadCode::of(code?))
    }

    /// The unit's own Attack's update (slot 6, `0x140a82ae0`) on the step: what it reads of the
    /// unit ([`AttackReads::of`]), its target and the unit as combat objects ([`Fight`]); the
    /// Attack, copied out of the list, goes back after it, then the retaliation's changes to the
    /// unit ([`Unit::hit_back`]). Codes 1 and 3 delete it (`RGE_Action_Object::update`).
    fn attack_strike_part(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
        own: Option<Victim>,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        let mut attack = *actions.list.attack().ok_or(NO_ATTACK)?;
        let master = object_master(
            c.w.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            c.w.model.masters.of(c.player),
        )?;
        let player_type =
            c.w.players
                .get(c.player.index())
                .map_or(PlayerType::PHANTOM, |q| q.kind);
        let r = AttackReads::of(
            c.w.parts.combat_of(self.id),
            master,
            (
                OwnerMasters::new(&c.w.parts.own_master, c.w.model.masters.of(c.player)),
                &c.w.model.graphics,
            ),
            c.w.parts.unit_ai.get(self.id),
            (player_type, c.w.world_time),
            roles,
        );
        let victim = victim(c, attack.target, roles)?.ok_or(GenieError::unsupported(
            "an attack's target the slice does not hold live",
        ))?;
        let own = own.ok_or(ATTACKER_MASTER)?;
        let mut f = Fight { victim, own };
        // written back before an error too: the stop's context reads the list
        let code = attack.update(c, self, actions, me, &mut f, &r);
        if let Some(a) = actions.list.attack_mut() {
            *a = attack;
        }
        let code = code?;
        // the retaliation's kill of the unit itself, which follows its step
        if let Some(k) = self.hit_back(f.own, c.w.parts.ai_and_combat_mut(self.id)?)? {
            let own = KillLeft::own(self.id, k);
            file_kill(c, (self, actions), own, roles)?;
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
        c: &Step<'_>,
        roles: &Roles,
    ) -> Result<Option<Victim>, GenieError> {
        if actions.list.hunt().is_none() && actions.list.attack().is_none() {
            return Ok(None);
        }
        let master = self.master(c.w.master_tables(), c.player)?;
        let terrain = c.w.spatial.terrain.as_ref();
        Ok(Some(victim_of(
            (
                self,
                c.w.parts.unit_ai.contains(self.id),
                c.w.parts.building_of(self.id),
                c.w.parts.combat_of(self.id),
                actions,
                c.w.parts.unread.get(self.id),
            ),
            master,
            &c.w.players.player(c.player)?.attributes,
            c.player,
            terrain,
            c.w.mode_680,
            roles,
        )))
    }
}
