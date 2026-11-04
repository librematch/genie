//! One object's step with its [`Step`] ([`World::run_object`]) and a unit AI run outside its
//! update.

use crate::construction::building_update::blasts_at_death;
use crate::math::rng::Well512;
use crate::objects::ListIndex;
use crate::objects::layers::Action;
use crate::objects::unit::{Outcome, Unit};
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::trace::Watch;
use crate::unit_ai::world_view::UnitView;
use crate::unit_ai::{Surroundings, UnitAi};
use crate::world::Millis;
use crate::world::World;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl World {
    /// One object's step with its `Step` (`f`: the unit's update, or one part of its unit AI for a
    /// command): object `list_index` of player `player_id` leaves its list (its stand-in holds the
    /// slot) and its action layer the table ([`Parts::take_actions`]), and `f` gets both and the
    /// `Step`, which holds the world (FD-045-D28; the objects it makes join the list at their
    /// making). It goes back to the slot its stand-in holds then ([`World::put_back_own`]), its
    /// layer to the table. Returns `f`'s result (an error of the put-back, then of the sight's
    /// registry first): its outcome and its answer `R`.
    ///
    /// [`Parts::take_actions`]: crate::world::parts::Parts::take_actions
    #[allow(clippy::too_many_lines)]
    pub(super) fn run_object<R>(
        &mut self,
        (player_id, list_index): (PlayerId, ListIndex),
        (dt, turn): (Millis, Option<Millis>),
        f: impl FnOnce(&mut Unit, &mut Action, &mut Step<'_>) -> GenieResult<(Outcome, R)>,
    ) -> GenieResult<(Outcome, R)> {
        let mut unit = self.take_out(player_id, list_index)?;
        let mut actions = self.parts.take_actions(unit.id);
        let watch = Watch::of(&unit, &actions, self.parts.unit_ai.get(unit.id));
        // the objects a hit's blast may reach about its target (one a hit can read), or about
        // the unit for its blast at death (TRIBE_Combat_Object::update)
        let target = match actions.attack_target() {
            Some(t) if self.victim(t, player_id)?.is_some() => Some(t),
            _ => None,
        };
        let blasted = match target {
            Some(object_id) => self.blasted(&unit, player_id, Some(object_id))?,
            None if blasts_at_death(
                unit.master(self.master_tables(), player_id)?,
                &unit.life,
                &self.model.roles,
                &self.model.tuning.turn,
            ) =>
            {
                self.blasted(&unit, player_id, None)?
            }
            None => Vec::new(),
        };
        let me = unit.id;
        let mut step = Step {
            w: self,
            me,
            dt,
            turn,
            blasted,
            player: player_id,
            taken_over: None,
            building_handover: false,
            side_notifies: None,
        };
        let mut result = f(&mut unit, &mut actions, &mut step);
        if let Some(w) = watch {
            w.report(
                (&unit, &actions, step.w.parts.unit_ai.get(unit.id)),
                step.w.ticks,
                player_id,
            );
        }
        if let Some(e) = self.take_sight_unsupported()
            && result.is_ok()
        {
            result = Err(e);
        }
        // back to its stand-in's slot, which a removal in the step may have moved
        self.parts.put_actions(unit.id, actions);
        if let Err(error) = self.put_back_own(player_id, unit) {
            result = Err(error);
        }
        result
    }

    /// Runs `f` on object `list_index` of player `player_id` with its step outside its update: a
    /// command's part the game runs on the object at once (`UNIT_ORDER`'s drop-off),
    /// [`World::step_object`] without the update's own parts. Returns `f`'s answer.
    ///
    /// # Errors
    ///
    /// `f`'s, and what its effects do not model.
    pub(crate) fn run_on_step<R>(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        f: impl FnOnce(&mut Unit, &mut Action, &mut Step<'_>) -> GenieResult<R>,
    ) -> GenieResult<R> {
        self.step_answering(
            (player_id, list_index),
            (Millis::ZERO, None, false),
            |unit, actions, step| Ok((Outcome::Stay, f(unit, actions, step)?)),
        )
        .map(|(_, answer)| answer)
    }

    /// Runs `f` on the unit AI of object `list_index` of player `player_id` for a world routine
    /// outside its update that gives the AI only the unit's place (a stance, a stop of the game's
    /// status, a let-out or an unload, a move's cleared notifies): the object's step
    /// ([`World::step_object`] without the update's own parts) whose only part is the AI's run with
    /// its side ([`Unit::run_calls`]: its calls on the unit at once). Returns `f`'s answer.
    ///
    /// # Errors
    ///
    /// A unit without a unit AI, and what the AI's run and its calls do not model.
    pub(crate) fn run_ai_calls<R>(
        &mut self,
        (player_id, list_index): (PlayerId, ListIndex),
        f: impl FnOnce(&mut UnitAi, &mut UnitView<'_, '_>, &mut Well512) -> GenieResult<R>,
    ) -> GenieResult<R> {
        self.step_answering(
            (player_id, list_index),
            (Millis::ZERO, None, false),
            |unit, actions, step| Ok((Outcome::Stay, unit.run_calls(actions, step, f)?)),
        )
        .map(|(_, answer)| answer)
    }

    /// Runs `f` on the unit AI of object `list_index` of player `player_id` outside its update (a
    /// command's immediate order, `FUN_140c55c90`): the object's step ([`World::step_object`]
    /// without the update's own parts) whose only part is the AI's run with what it reads of the
    /// world ([`Unit::run_ai`]).
    ///
    /// # Errors
    ///
    /// What the AI's run and its effects do not model.
    pub(crate) fn run_unit_ai(
        &mut self,
        (player_id, list_index): (PlayerId, ListIndex),
        f: impl FnOnce(
            &mut UnitAi,
            &Surroundings,
            &mut UnitView<'_, '_>,
            PlayerKind,
            &mut Well512,
        ) -> GenieResult<()>,
    ) -> GenieResult<()> {
        let stepped = self.step_object(
            (player_id, list_index),
            (Millis::ZERO, None, false),
            |unit, actions, step| unit.run_ai(actions, step, f).map(|()| Outcome::Stay),
        )?;
        match stepped.outcome {
            Outcome::Stay => Ok(()),
            _ => Err(GenieError::unsupported(
                "a unit AI run outside its update that takes the unit out of its list",
            )),
        }
    }
}
