//! An object's step and what follows it ([`World::step_object`]): what its `Step` gathered,
//! applied to the world in the order the game's update makes them.

use crate::GenieError;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::unit::{Outcome, Unit};
use crate::players::PlayerId;
use crate::world::Millis;
use crate::world::World;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

/// What an object's step left for its list's loop ([`World::step_object`]).
pub(super) struct Stepped {
    /// The step.
    pub(super) outcome: Outcome,
    /// The object's slot in its owner's live list after the step: a removal that put the list's
    /// last object in another's slot (`FUN_1411664a0`) may have moved it.
    pub(super) slot: ListIndex,
}

impl World {
    /// An object's step ([`World::run_object`], `f`: the unit's update, or one part of its unit
    /// AI for a command) and what follows it in the game's update: the regeneration, passive
    /// abilities and production of a full update, then a construction start's clearing and the
    /// changed tiles. Returns the step and the object's slot, which it looks up by id after each
    /// part that may move it.
    /// Without `full` (a unit AI run outside the object's update) the parts that only its update
    /// runs (regeneration, passive abilities, production) do not.
    ///
    /// # Errors
    ///
    /// The step's, or the first of what follows it; the object's outputs after it do not apply.
    pub(super) fn step_object(
        &mut self,
        at: (PlayerId, ListIndex),
        timing: (Millis, Option<Millis>, bool),
        f: impl FnOnce(&mut Unit, &mut Action, &mut Step<'_>) -> Result<Outcome, GenieError>,
    ) -> Result<Stepped, GenieError> {
        self.step_answering(at, timing, |unit, actions, c| {
            f(unit, actions, c).map(|outcome| (outcome, ()))
        })
        .map(|(stepped, ())| stepped)
    }

    /// [`World::step_object`] with `f`'s answer `R` beside its outcome: the step and the
    /// answer.
    ///
    /// # Errors
    ///
    /// As [`World::step_object`]'s.
    pub(super) fn step_answering<R>(
        &mut self,
        (p, i): (PlayerId, ListIndex),
        (dt, turn, full): (Millis, Option<Millis>, bool),
        f: impl FnOnce(&mut Unit, &mut Action, &mut Step<'_>) -> Result<(Outcome, R), GenieError>,
    ) -> Result<(Stepped, R), GenieError> {
        let id = self.players.live(p, i)?.id;
        let (outcome, answer) = self.run_object((p, i), (dt, turn), f)?;
        let at = (p, self.slot_of(p, id)?, id);
        self.finish_own_update(at, (dt, full), &outcome)?;
        self.hand_changed_tiles();
        let stepped = Stepped {
            outcome,
            slot: self.slot_of(p, id)?,
        };
        Ok((stepped, answer))
    }

    /// The slot of object `id` in player `p`'s live list, after its step.
    ///
    /// # Errors
    ///
    /// The object left the list in its own step.
    fn slot_of(&self, p: PlayerId, id: ObjectId) -> Result<ListIndex, GenieError> {
        let _ = self.players.player(p)?;
        // the step can move the object (another's removal), and its own removal waits for
        // `World::file_step`
        self.live_index(p, id).ok_or_else(|| {
            GenieError::unreachable("a stepped object left its list in its outcome")
                .with("object", id)
        })
    }

    /// The parts of the object's own update after its actions, once, with the update's unit AI
    /// part (`own_update`): the regeneration, the passive abilities with the hit point powerup
    /// check, and a building's production.
    fn finish_own_update(
        &mut self,
        (p, i, id): (PlayerId, ListIndex, ObjectId),
        (dt, own_update): (Millis, bool),
        outcome: &Outcome,
    ) -> Result<(), GenieError> {
        // TRIBE_Combat_Object::update's regeneration after RGE_Combat_Object::update
        if own_update {
            self.regenerate_object((p, i, id), dt.seconds())?;
        }
        // TRIBE_Combat_Object::update's passive abilities (FUN_1412072f0) as the object's owner
        // after its update; a removal reward's command comes back as the recording's own
        if own_update {
            let (owner, leaves) = match outcome {
                Outcome::Captured(k) if k.main => (k.new, false),
                Outcome::Leave => (p, true),
                _ => (p, false),
            };
            let _ = self.abilities((id, p, i), owner, leaves)?;
            // then its hit point powerup check
            self.clamp_object_powerup((p, i, id))?;
        }
        // TRIBE_Building_Object::update: the production after the combat update
        if matches!(outcome, Outcome::Stay) && own_update {
            self.building_production(p, i, dt.seconds())?;
        }
        Ok(())
    }

    /// The tiles whose static marks the update changed, to the planners.
    fn hand_changed_tiles(&mut self) {
        self.pathing.tiles_changed(self.spatial.take_changes());
    }
}
