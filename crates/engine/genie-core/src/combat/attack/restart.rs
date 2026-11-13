//! The Attack's restart under a deleted head (slot 0x28 with 0, `0x140a7f8c0`): its unit AI and
//! place test, the sprite it stands with, then the start as the list's add runs it.

use crate::GenieResult;
use crate::combat::attack::TARGET_GONE;
use crate::combat::attack::{Attack, AttackReads, AttackSource, AttackState, Fight, ReadyReads};
use crate::combat::attack::{OwnStart, engagement, target_in_sight};
use crate::combat::victims::victim;
use crate::math::rng::Stream;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::masters::Master;
use crate::objects::sprite::new_sprite;
use crate::objects::teardown::HeadStart;
use crate::objects::unit::Unit;
use crate::world::layer::Roles;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::world::{ATTACKER_MASTER, ATTACKER_UNPLACED, NO_STANDING_SPRITE};

impl Unit {
    /// `RGE_Action_Attack`'s slot 0x28 with 0 (`0x140a7f8c0`) once a deletion left `attack` at
    /// the head of the unit's list ([`Unit::restart_head`]), on the unit's step: a unit without
    /// a unit AI (`+0x6b0`) or inside an object (`+0x2f0`) fails it (`set_state(13)`,
    /// [`Attack::finish_in`]: a `MoveTo` it held stops the unit) and nothing more. Any other keeps
    /// its attack count (`+0x900`, which the argument 1 clears), shows the sprite the Attack
    /// stands with (slot 0x68 with `+0x80`), and the start goes on as the list's add's
    /// ([`Attack::start_own`] with [`HeadStart::Restarted`]) on what the Attack reads of the unit
    /// and its target now: readyToAttack, then state 4, 6 or 13. Its notifies go to the unit AI,
    /// in order. The Attack goes back to the list, before an error too (the stop's context reads
    /// the list).
    ///
    /// The unit AI is the world's: while the unit's own AI runs, the world holds none (the task
    /// restart-reads-lent-unit-ai).
    ///
    /// [`Unit::restart_head`]: crate::objects::unit::Unit::restart_head
    ///
    /// # Errors
    ///
    /// An Attack without a target or on one the world does not hold (readyToAttack's part
    /// without a target is not ported), a unit without a standing sprite or a master, and the
    /// start's.
    pub(crate) fn restart_attack(
        &mut self,
        mut attack: Attack,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let restarted = self.restart_listed_attack(&mut attack, actions, step);
        actions.list.set_attack(Some(attack));
        restarted
    }

    /// [`Unit::restart_attack`] on the head's copy `attack`, the list `actions` as it stands.
    fn restart_listed_attack(
        &mut self,
        attack: &mut Attack,
        actions: &Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let sprite_tuning = step.world.model.tuning.sprite;
        if step.world.parts.unit_ai.get(self.id).is_none() || self.life.inside.is_some() {
            return attack.finish_in(
                AttackState::Failed,
                self,
                step.world.parts.motion_mut(self.id),
                step.world.streams.get_mut(Stream::Sprite),
                sprite_tuning,
            );
        }
        let standing = attack.arms.idle_sprite.ok_or(NO_STANDING_SPRITE)?;
        new_sprite(
            &mut self.sprite,
            step.world.parts.move_stats_mut(self.id),
            standing,
            step.world.streams.get_mut(Stream::Sprite),
            sprite_tuning,
        );
        let roles = &step.world.model.roles.clone();
        let target = attack.target.ok_or(TARGET_GONE)?.id();
        let placed = *step.world.spatial.get_known(target)?.ok_or(TARGET_GONE)?;
        let me = *step.world.spatial.get(self.id).ok_or(ATTACKER_UNPLACED)?;
        let master = self.master(step.world.master_tables(), step.player)?;
        let engagement = engagement(
            step.world.parts.unit_ai.get(self.id),
            step.world.top_speed(step.player, self)?,
            master,
            &me,
            &placed,
            step.world.world_time,
            step.world.parts.combat_of(self.id).next_volley,
        )?;
        let target_master = step.world.master_tables().of_placed(target, &placed)?;
        let start = OwnStart {
            target,
            engagement: &engagement,
            visible: target_in_sight(
                &step.world.spatial,
                step.player,
                &placed,
                target_master.fog_visibility,
            )?,
            target_owner: placed.player,
            ready: self.restart_ready_reads(target, master, actions, step, roles)?,
        };
        let posted = attack.start_own(step, self, HeadStart::Restarted, &start, roles)?;
        posted.into_iter().try_for_each(|(kind, params)| {
            self.post_own(&mut step.world.parts.unit_ai, kind, params)
        })
    }

    /// What the restart's `set_state(6)` reads ([`ReadyReads`]): the unit of `master` with its
    /// list `actions` as the Attack's update reads it ([`AttackReads::of`]), the target `target`
    /// as a hit reads it (none when the world does not hold it live) and the unit as a combat
    /// object.
    fn restart_ready_reads(
        &self,
        target: ObjectId,
        master: &Master,
        actions: &Action,
        step: &Step<'_>,
        roles: &Roles,
    ) -> GenieResult<Option<ReadyReads>> {
        let Some(victim) = victim(step, target, roles)? else {
            return Ok(None);
        };
        let own = self
            .own_victim(actions, step, roles)?
            .ok_or(ATTACKER_MASTER)?;
        let reads = AttackReads::of(
            AttackSource {
                combat: step.world.parts.combat_of(self.id),
                top_speed: step.world.top_speed_terms(step.player, self)?,
                master,
                graphics: &step.world.model.graphics,
                ai: step.world.parts.unit_ai.get(self.id),
                player_kind: step.world.players.player(step.player)?.kind,
                now: step.world.world_time,
            },
            roles,
            &step.world.model.tuning.turn,
        );
        Ok(Some(ReadyReads {
            reads,
            fight: Fight { victim, own },
        }))
    }
}

#[cfg(test)]
mod tests;
