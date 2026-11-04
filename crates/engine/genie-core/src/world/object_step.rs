//! One object's step with its [`Step`] ([`World::run_object`]) and a unit AI run outside its
//! update.

use crate::GenieError;
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

impl World {
    /// One object's step with its `Step` (`f`: the unit's update, or one part of its unit AI
    /// for a command): object `i` of player `p` leaves its list (its stand-in holds the slot)
    /// and its action layer the table ([`Parts::take_actions`]), and `f` gets both and the
    /// `Step`, which holds the world (FD-045-D28; the objects it makes join the list at their
    /// making). It goes back to the slot its stand-in holds then ([`World::put_back_own`]), its
    /// layer to the table. Returns `f`'s result (an error of the put-back, then of the sight's
    /// registry first).
    ///
    /// [`Parts::take_actions`]: crate::world::parts::Parts::take_actions
    #[allow(clippy::too_many_lines)]
    pub(super) fn run_object(
        &mut self,
        (p, i): (PlayerId, ListIndex),
        (dt, turn): (Millis, Option<Millis>),
        f: impl FnOnce(&mut Unit, &mut Action, &mut Step<'_>) -> Result<Outcome, GenieError>,
    ) -> Result<Outcome, GenieError> {
        let mut unit = self.take_out(p, i)?;
        let mut actions = self.parts.take_actions(unit.id);
        let watch = Watch::of(&unit, &actions, self.parts.unit_ai.get(unit.id));
        // the objects a hit's blast may reach about its target (one a hit can read), or about
        // the unit for its blast at death (TRIBE_Combat_Object::update)
        let target = match actions.attack_target() {
            Some(t) if self.victim(t, p)?.is_some() => Some(t),
            _ => None,
        };
        let blasted = match target {
            Some(t) => self.blasted(&unit, p, Some(t))?,
            None if blasts_at_death(
                unit.master(self.master_tables(), p)?,
                &unit.life,
                &self.model.roles,
                &self.model.tuning.turn,
            ) =>
            {
                self.blasted(&unit, p, None)?
            }
            None => Vec::new(),
        };
        let me = unit.id;
        let mut c = Step {
            w: self,
            me,
            dt,
            turn,
            blasted,
            player: p,
            taken_over: None,
            building_handover: false,
            side_notifies: None,
        };
        let mut result = f(&mut unit, &mut actions, &mut c);
        if let Some(w) = watch {
            w.report(
                (&unit, &actions, c.w.parts.unit_ai.get(unit.id)),
                c.w.ticks,
                p,
            );
        }
        if let Some(e) = self.take_sight_unsupported()
            && result.is_ok()
        {
            result = Err(e);
        }
        // back to its stand-in's slot, which a removal in the step may have moved
        self.parts.put_actions(unit.id, actions);
        if let Err(e) = self.put_back_own(p, unit) {
            result = Err(e);
        }
        result
    }

    /// Runs `f` on object `i` of player `p` with its step outside its update: a command's part
    /// the game runs on the object at once (`UNIT_ORDER`'s drop-off), [`World::step_object`]
    /// without the update's own parts. Returns `f`'s answer.
    ///
    /// # Errors
    ///
    /// `f`'s, and what its effects do not model.
    pub(crate) fn run_on_step<R>(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        f: impl FnOnce(&mut Unit, &mut Action, &mut Step<'_>) -> Result<R, GenieError>,
    ) -> Result<R, GenieError> {
        let mut answer = None;
        let _ = self.step_object((p, i), (Millis::ZERO, None, false), |unit, actions, c| {
            answer = Some(f(unit, actions, c)?);
            Ok(Outcome::Stay)
        })?;
        answer.ok_or(GenieError::unreachable(
            "a world routine's step without its answer",
        ))
    }

    /// Runs `f` on the unit AI of object `i` of player `p` for a world routine outside its update
    /// that gives the AI only the unit's place (a stance, a stop of the game's status, a let-out
    /// or an unload, a move's cleared notifies): the object's step ([`World::step_object`]
    /// without the update's own parts) whose only part is the AI's run with its side
    /// ([`Unit::run_calls`]: its calls on the unit at once). Returns `f`'s answer.
    ///
    /// # Errors
    ///
    /// A unit without a unit AI, and what the AI's run and its calls do not model.
    pub(crate) fn run_ai_calls<R>(
        &mut self,
        (p, i): (PlayerId, ListIndex),
        f: impl FnOnce(&mut UnitAi, &mut UnitView<'_, '_>, &mut Well512) -> Result<R, GenieError>,
    ) -> Result<R, GenieError> {
        let mut answer = None;
        let _ = self.step_object((p, i), (Millis::ZERO, None, false), |unit, actions, c| {
            answer = Some(unit.run_calls(actions, c, f)?);
            Ok(Outcome::Stay)
        })?;
        answer.ok_or(GenieError::unreachable(
            "a world routine's unit AI run without its answer",
        ))
    }

    /// Runs `f` on the unit AI of object `i` of player `p` outside its update (a command's
    /// immediate order, `FUN_140c55c90`): the object's step ([`World::step_object`] without the
    /// update's own parts) whose only part is the AI's run with what it reads of the world
    /// ([`Unit::run_ai`]).
    ///
    /// # Errors
    ///
    /// What the AI's run and its effects do not model.
    pub(crate) fn run_unit_ai(
        &mut self,
        (p, i): (PlayerId, ListIndex),
        f: impl FnOnce(
            &mut UnitAi,
            &Surroundings,
            &mut UnitView<'_, '_>,
            PlayerKind,
            &mut Well512,
        ) -> Result<(), GenieError>,
    ) -> Result<(), GenieError> {
        let stepped =
            self.step_object((p, i), (Millis::ZERO, None, false), |unit, actions, c| {
                unit.run_ai(actions, c, f).map(|()| Outcome::Stay)
            })?;
        match stepped.outcome {
            Outcome::Stay => Ok(()),
            _ => Err(GenieError::unsupported(
                "a unit AI run outside its update that takes the unit out of its list",
            )),
        }
    }
}
