//! The Enter's restart under a deleted head (its slot 0x28 with 0): its start again, by where the
//! unit is and what its target has room for.

use crate::GenieResult;
use crate::garrison::ContainerId;
use crate::garrison::EnterAction;
use crate::map::Point;
use crate::movement::move_to::{MoveDefaults, MoveToReads, SpriteReads};
use crate::objects::layers::Action;
use crate::objects::unit::Unit;
use crate::orders::object_gone;
use crate::world::others::OthersRef;
use crate::world::step::Step;

impl Unit {
    /// The Enter's slot 0x28 (`0x140a86f20`, which reads no argument), as the action list's
    /// deletion restarts it at the head (`FUN_140a98e20`): a unit inside an object (`+0x2f0`)
    /// fails it (`set_state(13)`, [`Unit::enter_failed`]); any other starts it again on its
    /// target ([`Unit::restart_walk`]), the copy written back after. Nothing without an Enter.
    ///
    /// # Errors
    ///
    /// The failure's and the start's.
    pub(crate) fn restart_enter(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        if self.life.inside.is_some() {
            return self.enter_failed(actions, step);
        }
        let Some(mut enter) = actions.list.enter().copied() else {
            return Ok(());
        };
        let restarted = self.restart_walk(&mut enter, actions, step);
        if let Some(enter_action) = actions.list.enter_mut() {
            *enter_action = enter;
        }
        restarted
    }

    /// The Enter's start past its inside test (`0x140a86f20`): a target without room for the
    /// unit ([`Unit::roomless`]) gives the player's message (`FUN_140a864d0`: the screen's
    /// `FUN_140c85f00`, nothing simulated) and walks to the target's place (`set_state(11)`,
    /// [`EnterAction::walk_to_point`]). A target with room walks again or enters at once
    /// (`set_state(4)`, [`EnterAction::again`]) with what it reads of the target
    /// ([`Unit::enter_view_of`]); an Enter without a target walks to its point
    /// ([`EnterAction::walk_unheld`]). Each `set_state` lets its sub-actions go first: a
    /// `MoveTo` the Enter walked stops the unit (slot 0x5d0). The `MoveTo`s read the unit's top
    /// speed (slot 0x208) and its master's sprites for none.
    ///
    /// # Errors
    ///
    /// A target the world does not hold, a unit whose master it lacks, and the walks', the
    /// stop's and the enter's.
    fn restart_walk(
        &mut self,
        enter: &mut EnterAction,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let roomless = self.roomless(step, enter.target)?;
        let defaults = MoveDefaults::of(
            self.master(step.world.master_tables(), step.player)?,
            step.world.parts.combat_of(self.id).abilities.look.as_ref(),
            &step.world.model.roles,
        );
        if roomless.is_none() {
            if let Some((enter_target, start)) = self.enter_view_of(step, enter)? {
                return enter
                    .again(step, self, actions, &enter_target, start, defaults)
                    .map(|_| ());
            }
            if let Some(target) = enter.target {
                return Err(object_gone(target));
            }
        }
        if enter.state.take_move_to().is_some()
            && let Some(mover) = step.world.parts.mover_mut(self.id)
        {
            mover.stop()?;
        }
        let top = step.world.top_speed(step.player, self)?;
        let sprites = SpriteReads {
            defaults,
            graphics: &step.world.model.graphics,
        };
        let reads =
            MoveToReads::with_sprites(sprites, self, step.world.parts.mover_of(self.id), top);
        let move_to_tuning = &step.world.model.tuning.move_to;
        match roomless {
            Some(meet) => enter.walk_to_point(meet, &reads, move_to_tuning),
            None => enter.walk_unheld(&reads, move_to_tuning),
        }
    }

    /// The place of the Enter's target `target` when it has no room for this unit (slot 0x198
    /// with the unit, [`World::has_room`]: `0x140a86f20`'s test); None for room and for a
    /// target the world does not hold.
    ///
    /// # Errors
    ///
    /// The room test's.
    ///
    /// [`World::has_room`]: crate::world::World::has_room
    fn roomless(&self, step: &Step<'_>, target: Option<ContainerId>) -> GenieResult<Option<Point>> {
        let others = OthersRef::of(&step.world.players, &step.world.index, step.player)?;
        let Some((tp, building)) = others.find(target) else {
            return Ok(None);
        };
        Ok((!step.world.has_room(tp, building, Some(self.id))?)
            .then(|| building.position().ground()))
    }
}
