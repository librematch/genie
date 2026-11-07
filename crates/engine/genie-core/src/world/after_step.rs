//! An object's step and what follows it ([`World::step_object`]): what its `Step` gathered,
//! applied to the world in the order the game's update makes them.

use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::unit::{Outcome, Unit};
use crate::players::PlayerId;
use crate::world::Millis;
use crate::world::World;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

/// What an object's step left for its list's loop ([`World::step_object`]).
pub(super) struct Stepped {
    /// The step.
    pub(super) outcome: Outcome,
    /// The object's slot in its owner's live list after the step: a removal that put the list's
    /// last object in another's slot (`FUN_1411664a0`) may have moved it.
    pub(super) slot: ListIndex,
}

impl World {
    /// An object's step ([`World::run_object`], `work`: the unit's update, or one part of its unit
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
        work: impl FnOnce(&mut Unit, &mut Action, &mut Step<'_>) -> GenieResult<Outcome>,
    ) -> GenieResult<Stepped> {
        self.step_answering(at, timing, |unit, actions, step| {
            work(unit, actions, step).map(|outcome| (outcome, ()))
        })
        .map(|(stepped, ())| stepped)
    }

    /// [`World::step_object`] with `work`'s answer `R` beside its outcome: the step and the
    /// answer.
    ///
    /// # Errors
    ///
    /// As [`World::step_object`]'s.
    pub(super) fn step_answering<R>(
        &mut self,
        (player_id, list_index): (PlayerId, ListIndex),
        (dt, turn, full): (Millis, Option<Millis>, bool),
        work: impl FnOnce(&mut Unit, &mut Action, &mut Step<'_>) -> GenieResult<(Outcome, R)>,
    ) -> GenieResult<(Stepped, R)> {
        let id = self.players.live(player_id, list_index)?.id;
        let (outcome, answer) = self.run_object((player_id, list_index), (dt, turn), work)?;
        let at = (player_id, self.slot_of(player_id, id)?, id);
        self.finish_own_update(at, (dt, full), &outcome)?;
        self.hand_changed_tiles();
        let stepped = Stepped {
            outcome,
            slot: self.slot_of(player_id, id)?,
        };
        Ok((stepped, answer))
    }

    /// The slot of object `id` in player `player_id`'s live list, after its step.
    ///
    /// # Errors
    ///
    /// The object left the list in its own step.
    fn slot_of(&self, player_id: PlayerId, id: ObjectId) -> GenieResult<ListIndex> {
        let _ = self.players.player(player_id)?;
        // the step can move the object (another's removal), and its own removal waits for
        // `World::file_step`
        self.live_index(player_id, id).ok_or_else(|| {
            GenieError::unreachable("a stepped object left its list in its outcome")
                .with("object", id)
        })
    }

    /// The parts of the object's own update after its actions, once, with the update's unit AI
    /// part (`own_update`): the regeneration, the passive abilities, the powerup update
    /// ([`World::powerup_update`]), the stinger update ([`World::object_stingers`]) and the hit
    /// point powerup check, and a building's production and a pasture's food
    /// ([`World::sum_livestock_food`]).
    fn finish_own_update(
        &mut self,
        (player_id, list_index, id): (PlayerId, ListIndex, ObjectId),
        (dt, own_update): (Millis, bool),
        outcome: &Outcome,
    ) -> GenieResult<()> {
        // TRIBE_Combat_Object::update's regeneration after RGE_Combat_Object::update
        if own_update {
            self.regenerate_object((player_id, list_index, id), dt.seconds())?;
            // then its charge's refill
            self.refill_object_charge((player_id, list_index, id), dt.seconds())?;
        }
        // TRIBE_Combat_Object::update's passive abilities (FUN_1412072f0) as the object's owner
        // after its update; a removal reward's command comes back as the recording's own
        if own_update {
            let (owner, leaves) = match outcome {
                Outcome::Captured(captured) if captured.main => (captured.new, false),
                Outcome::Leave => (player_id, true),
                _ => (player_id, false),
            };
            let _ = self.abilities((id, player_id, list_index), owner, leaves)?;
            // then its powerup update (FUN_141204a10) and its stinger update (FUN_1412092e0)
            self.powerup_update((player_id, list_index, id))?;
            self.object_stingers((player_id, list_index, id))?;
            // then its hit point powerup check
            self.clamp_object_powerup((player_id, list_index, id))?;
        }
        if own_update {
            // TRIBE_Building_Object::update: the production after the combat update
            if matches!(outcome, Outcome::Stay) {
                self.building_production(player_id, list_index, dt.seconds())?;
            }
            // then a pasture's food, whatever the combat update answered
            self.sum_livestock_food(player_id, list_index, id)?;
        }
        Ok(())
    }

    /// The tiles whose static marks the update changed, to the planners.
    fn hand_changed_tiles(&mut self) {
        self.pathing.tiles_changed(self.spatial.take_changes());
    }
}
