//! The Attack's `set_state` (`0x140a820c0`): the move, the teardown and prelude, a lost
//! target, one out of sight, the finish and the halt.

use crate::GenieError;
use crate::combat::attack::AttackState;
use crate::combat::attack::update::let_go;
use crate::combat::attack::{
    AiOrder, Attack, AttackReads, Engagement, Fight, HALF, NO_RANGE, NONE, set_walker_anim,
};
use crate::map::Distance;
use crate::map::Position;
use crate::math::rng::{Stream, Well512};
use crate::movement::Mover;
use crate::movement::Speed;
use crate::movement::move_to::MoveToReads;
use crate::objects::UpdateCode;
use crate::objects::layers::MotionMut;
use crate::objects::sprite::ActiveSprite;
use crate::objects::sprite::new_sprite;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::BOAR_AI;
use crate::unit_ai::LION_AI;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitStance;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::step::Step;
use crate::world::{ATTACKER_UNMOVING, NO_STANDING_SPRITE};

impl Attack {
    /// A `MoveTo` of the attack that starts arrived (its `set_state` 1, not quiet): the unit shows
    /// its idle sprite (slot 0x68) and its `+0x7b8` clears.
    pub(crate) fn arrived_sprite(
        &self,
        unit: &mut Unit,
        mut motion: MotionMut<'_>,
        sprite_rng: &mut Well512,
    ) -> Result<(), GenieError> {
        let g = self.arms.idle_sprite.ok_or(NO_STANDING_SPRITE)?;
        new_sprite(&mut unit.sprite, motion.move_stats_mut(), g, sprite_rng);
        set_walker_anim(motion, Speed::new(0.0));
        Ok(())
    }

    /// How far the unit reaches: the range, half a tile more without one.
    pub(super) fn reach(&self) -> Distance {
        if self.range <= Distance::new(NO_RANGE) {
            self.range + Distance::new(HALF)
        } else {
            self.range
        }
    }

    /// `set_state(4)` (`0x140a820c0` case 4): the sub-actions go (a `MoveTo`'s teardown stops the unit,
    /// slot 0x5d0), the range is slot 0x238's again, and a `MoveTo` to the target with that range
    /// starts ([`Attack::moving`]). True when it arrived.
    pub(super) fn set_move(
        &mut self,
        e: &Engagement,
        mover: &mut Mover,
        reads: &MoveToReads<'_>,
    ) -> Result<bool, GenieError> {
        if self.state.take_move_to().is_some() {
            mover.stop()?;
        }
        self.state = AttackState::Move { move_to: None };
        self.moving(e.range, e.target_position, e.gap, reads)
    }

    /// `set_state(4)`'s own part: the range (slot 0x238's), the timer, the `MoveTo` to the target
    /// at `at`, which starts arrived (state 1, true) within 0.01 of a `gap` away
    /// ([`Attack::walk_to_target`]).
    ///
    /// # Errors
    ///
    /// An Attack without a target.
    pub(super) fn moving(
        &mut self,
        range: Distance,
        at: Position,
        gap: Distance,
        reads: &MoveToReads<'_>,
    ) -> Result<bool, GenieError> {
        self.range = range;
        self.walk_to_target(at, gap, reads)
    }

    /// The teardown of the sub-actions (`FUN_140a73fc0` on the action list): a `MoveTo` stops the
    /// unit.
    pub fn tear_down(&mut self, mover: Option<&mut Mover>) -> Result<(), GenieError> {
        if self.state.take_move_to().is_some()
            && let Some(m) = mover
        {
            m.stop()?;
        }
        Ok(())
    }

    /// The start of every `set_state` (`0x140a820c0`): the sub-actions go (a `MoveTo`'s teardown stops
    /// the unit and its animation, `+0x7b8`), then the state.
    pub(super) fn enter(
        &mut self,
        state: AttackState,
        mut motion: MotionMut<'_>,
    ) -> Result<(), GenieError> {
        if self.state.take_move_to().is_some() {
            motion.mover_mut().ok_or(ATTACKER_UNMOVING)?.stop()?;
        }
        self.state = state;
        Ok(())
    }

    /// A unit's own Attack whose target died (the update's start, not quiet: `+0x60` 0): its unit
    /// AI forgets the target at once (`FUN_140c5e1e0`; a villager keeps a Gaia target), the
    /// target goes (slot 0x68 with none: let go at once) and the point; a human player's unit
    /// whose unit AI has an order and an empty queue (or no unit AI) posts 0x1fa with 600 (the
    /// action completed), any other with a unit AI whose queue is empty 0x1fb with 600 and
    /// 0x202; it ends (`set_state` 1) and returns 5, which keeps it in the list. A mover of
    /// stance 0 that is not a villager waits instead where it saw the target (`set_state` 5,
    /// [`Attack::wait_at_seen`]; a unit's own Attack posts 0x1fb with 600 and 0x202 first) and
    /// returns 5 too; its next update has no target ([`Attack::update_gone`]). A dead tree
    /// (class 15) up to state 5, which keeps a non-villager at it, is not modelled.
    pub(super) fn lost(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        f: &Fight,
        r: &AttackReads,
        roles: &Roles,
    ) -> Result<UpdateCode, GenieError> {
        // The villagers' class, whose `MoveTo` waits once when the motion step gives up.
        if r.ai_order.has_ai()
            && (f.victim.player != PlayerId::new(0)
                || !roles.of_class(f.own.class).contains(ClassRoles::VILLAGER))
        {
            forget_target(c.w.parts.unit_ai.get_mut(unit.id));
        }
        let target = self.target.take();
        let_go(c, unit, target)?;
        self.point = Position::from([NONE; 3]);
        let human = f.own.player != PlayerId::new(0) && !r.player_type.is_computer();
        let waits = match r.ai_order {
            AiOrder::Absent => true,
            AiOrder::Idle => false,
            AiOrder::Order(_) => r.queue_empty,
        };
        if human && waits {
            let speed =
                c.w.parts
                    .mover_of(unit.id)
                    .map_or(Speed::ZERO, |m| m.stats.speed);
            if r.stance == UnitStance::AGGRESSIVE
                && Speed::ZERO < speed
                && !roles.of_class(f.own.class).contains(ClassRoles::VILLAGER)
            {
                // a mover of stance 0 that is not a villager waits where it saw the target:
                // 0x1fb with 600 and 0x202 for a unit's own Attack, then `set_state(5)`
                if self.voice.is_loud() {
                    unit.post_own(
                        &mut c.w.parts.unit_ai,
                        NotifyKind::ACTION_INVALIDATED,
                        [600, 0, 0x202],
                    )?;
                }
                self.wait_at_seen(
                    unit,
                    c.w.parts.motion_mut(unit.id),
                    f,
                    r,
                    &c.w.model.graphics,
                )?;
                return Ok(UpdateCode::CODE_5);
            }
            unit.post_own(
                &mut c.w.parts.unit_ai,
                NotifyKind::ACTION_COMPLETED,
                [600, 0, 0],
            )?;
        } else if r.ai_order.has_ai() && r.queue_empty {
            unit.post_own(
                &mut c.w.parts.unit_ai,
                NotifyKind::ACTION_INVALIDATED,
                [600, 0, 0x202],
            )?;
        }
        self.finish(
            unit,
            c.w.parts.motion_mut(unit.id),
            c.w.streams.get(Stream::Sprite),
        )?;
        Ok(UpdateCode::CODE_5)
    }

    /// The update's answer to a target out of sight ([`Attack::update`]): the target goes (slot
    /// 0x68 with none), the attack fails (`set_state` 13, as 1); a unit's own Attack lets the
    /// target go, its unit AI forgets it (`FUN_140c5e1e0`) and a predator's unit AI's slot 0x1f0
    /// runs (the boar's sends it home at its next idle check, `+0x1fd`; the lion's does nothing),
    /// each at once, and with its unit AI's queue empty it posts 0x1f9 with 600; returns 3,
    /// which deletes it. The Hunt's Attack, a sub-action, does none of these (MISSING.md, "The
    /// Hunt's Attack out of sight").
    pub(super) fn out_of_sight(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        f: &Fight,
        r: &AttackReads,
        roles: &Roles,
    ) -> Result<UpdateCode, GenieError> {
        let target = self.target.take();
        self.finish_in(
            AttackState::Failed,
            unit,
            c.w.parts.motion_mut(unit.id),
            c.w.streams.get(Stream::Sprite),
        )?;
        // The predators' class (10), whose unit AI's slot 0x1f0 a failed Attack calls.
        // The boar's unit-AI type, whose slot 0x1f0 (`FUN_140c16d50`) sends it home.
        // The lion's unit-AI type, whose slot 0x1f0 (`FUN_140c16d60`) does nothing.
        if roles.of_class(f.own.class).contains(ClassRoles::PREDATOR)
            && !matches!(r.ai_type, Some(BOAR_AI | LION_AI) | None)
        {
            return Err(GenieError::unsupported(
                "a predator's failed Attack whose unit AI is not a boar's or a lion's (slot 0x1f0)",
            ));
        }
        if self.voice.is_quiet() {
            return Ok(UpdateCode::FAILED);
        }
        let_go(c, unit, target)?;
        if r.ai_order.has_ai() {
            forget_target(c.w.parts.unit_ai.get_mut(unit.id));
        }
        if roles.of_class(f.own.class).contains(ClassRoles::PREDATOR)
            && r.ai_type == Some(BOAR_AI)
            && let Some(home) =
                c.w.parts
                    .unit_ai
                    .get_mut(unit.id)
                    .and_then(|ai| ai.home.as_mut())
        {
            home.go_home = true;
        }
        if r.queue_empty {
            unit.post_own(
                &mut c.w.parts.unit_ai,
                NotifyKind::ACTION_FAILED,
                [600, 0, 0],
            )?;
        }
        Ok(UpdateCode::FAILED)
    }

    /// `set_state(1)` (case 1): the unit's animation runs at speed 1 and it takes the sprite it
    /// stands with.
    pub(super) fn finish(
        &mut self,
        unit: &mut Unit,
        motion: MotionMut<'_>,
        sprite_rng: &mut Well512,
    ) -> Result<(), GenieError> {
        // The states the slice models: 1 done, 2 the target died, 4 on the way, 6 waiting to swing, 7
        // swinging, 13 failed.
        self.finish_in(AttackState::Done, unit, motion, sprite_rng)
    }

    /// Slot 0x38 (`FUN_140a82ac0`) as the unit's stop calls it: `set_state(1)` (the
    /// sub-actions go, a `MoveTo`'s teardown stopping the unit and its animation; the animation
    /// runs at speed 1 and the unit takes the sprite it stands with, [`Attack::finish`]); its
    /// answer is true (the caller's), so the stop keeps the action list.
    ///
    /// # Errors
    ///
    /// An attacker walking without movement state, or without a standing sprite.
    pub(crate) fn halt(
        &mut self,
        mut motion: MotionMut<'_>,
        sprite: &mut Option<ActiveSprite>,
        rng: &mut Well512,
    ) -> Result<(), GenieError> {
        if self.state.take_move_to().is_some() {
            motion.mover_mut().ok_or(ATTACKER_UNMOVING)?.stop()?;
        }
        self.state = AttackState::Done;
        motion.set_anim_speed(Speed::new(1.0));
        let idle = self.arms.idle_sprite.ok_or(GenieError::unsupported(
            "an Attack's unit without a standing sprite",
        ))?;
        new_sprite(sprite, motion.move_stats_mut(), idle, rng);
        Ok(())
    }

    /// `set_state` cases 1, 13 and 14: as [`Attack::finish`], in `state`.
    pub(super) fn finish_in(
        &mut self,
        state: AttackState,
        unit: &mut Unit,
        mut motion: MotionMut<'_>,
        sprite_rng: &mut Well512,
    ) -> Result<(), GenieError> {
        self.enter(state, motion.reborrow())?;
        set_walker_anim(motion.reborrow(), Speed::new(1.0));
        let idle = self.arms.idle_sprite.ok_or(GenieError::unsupported(
            "an Attack's unit without a standing sprite",
        ))?;
        new_sprite(&mut unit.sprite, motion.move_stats_mut(), idle, sprite_rng);
        Ok(())
    }
}

/// `FUN_140c5e1e0` at once: the unit AI forgets its target ([`crate::unit_ai::UnitAi::remove_target`]).
fn forget_target(ai: Option<&mut UnitAi>) {
    if let Some(ai) = ai {
        ai.remove_target();
    }
}
