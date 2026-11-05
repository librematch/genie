//! The world update's parts ([`crate::world::schedule`] runs them): the command phase, the
//! moving objects, the players' relations and each player's objects' updates.

use crate::commands::Command;
use crate::construction::build::CART_TASK;
use crate::map::spatial::Spatial;
use crate::movement::MotionBound;
use crate::movement::Speed;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Masters;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::player_master;
use crate::objects::unit::Outcome;
use crate::ownership::capture::Captured;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::tracker::key;
use crate::trace::{UPDATE, object_freezes, player_unsupported};
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};
use std::sync::Arc;

/// Whether the object list's loop counts on past the object it filed ([`World::file_step`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub(super) enum ListCount {
    /// Past it: the slot holds the next object, or the object waits for the next tick.
    GoesOn,
    /// Not past it: it left, and the list's last object took its slot.
    Again,
}

impl World {
    /// One world update of `dt` game milliseconds without commands.
    pub fn tick(&mut self, dt: Millis) {
        self.tick_with(dt, &[]);
    }

    /// An update's start up to the end of its command phase (`BaseWorld::update` up to
    /// `RGE_Command__do_commands`, before `FUN_141552990`): the update's count and the world's
    /// time go on by `dt`, the streams take the frame seed and `commands` run; no player updates.
    /// For a check of the commands against the game's state there (the tick oracle's
    /// `snapshot_at` at `0x141552990`); the schedule's first step
    /// ([`crate::world::schedule::WorldStep::Commands`]).
    pub fn command_phase(&mut self, dt: Millis, commands: &[Command]) {
        self.ticks = self.ticks.next();
        self.world_time = self.world_time.after(dt);
        self.dt_seconds = dt.seconds();
        self.dt = dt;
        self.streams.seed_all(self.frame_seed.get());
        for command in commands {
            tracing::debug!(target: UPDATE, ?command, "command");
            let stopped: Vec<bool> = self
                .players
                .iter()
                .map(|player| player.unsupported.is_some())
                .collect();
            self.run(command);
            // a player the command stopped: its error names the command
            for (player, was) in self.players.iter_mut().zip(stopped) {
                if let (false, Some((_, _, why))) = (was, player.unsupported.as_mut()) {
                    let text = format!("{command:?}");
                    let name = text.split([' ', '{', '(']).next().unwrap_or_default();
                    *why = why.clone().with("command", name);
                }
            }
        }
    }

    /// A tick of `dt` seconds passed, while objects are frozen: what a stopped player's units may
    /// have done rises to their masters as they stand now ([`World::raise_frozen_bounds`]), and
    /// every frozen object that moves may have moved its master's bound of speed times `dt`
    /// ([`MotionBound::speed`], [`Spatial::grow_unknown`]); one of a master the world does not
    /// hold may be anywhere.
    ///
    /// [`Spatial::grow_unknown`]: crate::map::spatial::Spatial::grow_unknown
    ///
    /// # Errors
    ///
    /// An object whose master the world lacks.
    pub(crate) fn grow_unknown(&mut self, dt: Seconds) -> GenieResult<()> {
        if !self.spatial.has_frozen() {
            return Ok(());
        }
        let bound = MotionBound::of(&self.model, &self.parts, &self.players)?;
        let masters = &self.model.masters;
        raise_frozen_bounds(&mut self.spatial, masters, &bound);
        self.spatial.grow_unknown(dt, |player, master| {
            player_master(masters, player, master)
                .map_or(Speed::new(f32::INFINITY), |master| bound.speed(master))
        });
        Ok(())
    }

    /// What a stopped player's units may have done rises to the bounds of the players' unit
    /// masters as they stand ([`raise_frozen_bounds`]): at each new freeze, so the queries of
    /// the same update read them.
    ///
    /// # Errors
    ///
    /// An object whose master the world lacks.
    fn raise_frozen_bounds(&mut self) -> GenieResult<()> {
        let bound = MotionBound::of(&self.model, &self.parts, &self.players)?;
        raise_frozen_bounds(&mut self.spatial, &self.model.masters, &bound);
        Ok(())
    }

    /// The world's table by id ([`World::reindex`]) and the spatial index's view of a world
    /// made from its parts (a load, a test's world): how the players stand to each other (after
    /// it: the diplomacy change, [`World::set_relation`]) and each player's masters with a task
    /// of action type 0x99.
    pub fn sync_world(&mut self) {
        self.reindex();
        self.spatial.relations = self
            .players
            .iter()
            .map(|player| player.relations.clone())
            .collect();
        self.spatial.task_0x99 = self
            .model
            .masters
            .iter()
            .map(|masters| {
                masters
                    .iter()
                    .filter(|master| {
                        master
                            .action_part()
                            .tasks
                            .iter()
                            .any(|task| task.action_kind == CART_TASK)
                    })
                    .map(|master| master.id)
                    .collect()
            })
            .collect();
    }

    /// Some player's attribute 204 is not 0, which a building's completion reads: the players'
    /// attributes as they stand.
    #[must_use]
    pub(crate) fn attribute_204(&self) -> bool {
        self.players.iter().any(|player| {
            player
                .attributes
                .get(204)
                .is_some_and(|&amount| amount != Amount::ZERO)
        })
    }
}

impl World {
    /// The updates of player `player_id`'s objects in its list's order (`FUN_141166550`): each
    /// one's step and what follows it ([`World::step_object`]), then its filing by the step
    /// ([`World::file_step`]). An object whose update is not modelled freezes, and so do all live
    /// objects of a player a command stopped; dead objects still run their state steps, which draw
    /// nothing and depend on nothing the stop changed.
    ///
    /// # Errors
    ///
    /// A player the world does not hold, or a slot of its list the loop's count misses.
    pub(super) fn update_objects(
        &mut self,
        player_id: PlayerId,
        (dt, turn): (Millis, Option<Millis>),
    ) -> GenieResult<()> {
        // the loop's count, which goes on past each object but one that leaves
        let mut list_index = ListIndex::FIRST;
        while self.players.player(player_id)?.objects.holds(list_index) {
            let player = self.players.player(player_id)?;
            let unit = player.live(list_index)?;
            let stopped = player.stopped || unit.stopped;
            if stopped && unit.life.is_alive() {
                let newly = !self.spatial.is_frozen(unit.id);
                self.spatial.freeze(unit.id);
                if newly {
                    self.raise_frozen_bounds()?;
                }
                list_index = list_index.next();
                continue;
            }
            let (id, master) = (unit.id, unit.life.master_id);
            let _object =
                tracing::trace_span!(target: UPDATE, "object", id = id.get(), master = %master)
                    .entered();
            let (roles, slots) = (self.model.roles.clone(), self.model.slots);
            match self.step_object(
                (player_id, list_index),
                (dt, turn, true),
                |unit, action, step| unit.update(action, step, &roles, slots),
            ) {
                Ok(stepped) => {
                    if self.file_step(player_id, stepped.slot, stepped.outcome)?
                        == ListCount::GoesOn
                    {
                        list_index = list_index.next();
                    }
                }
                Err(error) => {
                    self.freeze_object(player_id, (list_index, id), error)?;
                    list_index = list_index.next();
                }
            }
        }
        Ok(())
    }

    /// Files object `list_index` of player `player_id` by the step it finished (the list's loop,
    /// `FUN_141166550`): one that leaves goes from its list, a sleeper to the sleeping list, a
    /// captured building or unit to its new owner. Answers whether the loop's count goes on
    /// ([`ListCount`]): not past one that leaves (the game's loop runs its count again, the list's
    /// last object in the slot), past any other (a sleeper's slot holds the last object too, whose
    /// update waits for the next tick).
    ///
    /// # Errors
    ///
    /// A player the world does not hold, or a slot its list does not hold.
    pub(super) fn file_step(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        outcome: Outcome,
    ) -> GenieResult<ListCount> {
        match outcome {
            Outcome::Leave => {
                let gone = self.swap_remove_live(player_id, list_index)?;
                if let Err(error) = self.leave_world(player_id, &gone) {
                    player_unsupported(self.ticks, player_id, &error);
                    let _ = self
                        .players
                        .player_mut(player_id)?
                        .unsupported
                        .get_or_insert((self.ticks, list_index, error));
                }
                // the list's last object took its place
                Ok(ListCount::Again)
            }
            Outcome::Sleep => {
                // the list's remove (FUN_1411664a0) puts its last object in the sleeper's place,
                // and the list's loop (0x141166550) goes on past it: that object's update waits
                // for the next tick; the player's remove and add (slots 0x2d8, 0x2d0) file it last
                // in the tracker
                let asleep = self.swap_remove_live(player_id, list_index)?;
                let key = key(
                    self.model.masters.of(player_id),
                    &self.model.lines,
                    asleep.unit.life.master_id,
                    &self.model.roles,
                );
                let tracker = &mut self.players.player_mut(player_id)?.tracker;
                tracker.remove(asleep.unit.id, key);
                tracker.add(asleep.unit.id, key);
                let _ = self.push_asleep(player_id, asleep)?;
                Ok(ListCount::GoesOn)
            }
            Outcome::Captured(Captured { new, building, .. }) if building => {
                // a building's slot 0x70 (FUN_1411cd860), whole: its annexes, its lists' moves
                // and its new owner's parts
                let player = self.players.player(player_id)?;
                let id = player.live(list_index)?.id;
                let watched = player.watched.iter().any(|watched| watched.id == id);
                if let Err(error) = self.building_changes_hands(player_id, id, new, watched) {
                    player_unsupported(self.ticks, player_id, &error);
                    let _ = self
                        .players
                        .player_mut(player_id)?
                        .unsupported
                        .get_or_insert((self.ticks, list_index, error));
                }
                Ok(ListCount::GoesOn)
            }
            Outcome::Captured(taken) => {
                self.hand_over_units(player_id, list_index, taken)?;
                Ok(ListCount::GoesOn)
            }
            Outcome::Stay => Ok(ListCount::GoesOn),
        }
    }

    /// The units of player `player_id` a takeover in object `slot`'s update captured go to their
    /// new owner: the object itself when the new owner has its master (`main`), and the units taken
    /// with it (`also`). The old owner's list loses them (`FUN_1411664a0`, the last in each place;
    /// the loop goes on past the first) and the new owner's gains them (`FUN_141616200`), each with
    /// its attributes ([`World::hand_over_attributes`]).
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    fn hand_over_units(
        &mut self,
        player_id: PlayerId,
        slot: ListIndex,
        taken: Captured,
    ) -> GenieResult<()> {
        let Captured {
            new, main, also, ..
        } = taken;
        let mut moved = Vec::new();
        if main {
            moved.push(self.swap_remove_live(player_id, slot)?);
        }
        for id in also {
            if let Some(list_index) = self.live_index(player_id, id) {
                moved.push(self.swap_remove_live(player_id, list_index)?);
            }
        }
        let mut failed = None;
        for object in &moved {
            if let Err(error) =
                self.hand_over_attributes(player_id, new, object.parts.own_master.as_ref())
            {
                let _ = failed.get_or_insert(error);
            }
        }
        if let Some(error) = failed {
            player_unsupported(self.ticks, player_id, &error);
            let _ = self
                .players
                .player_mut(player_id)?
                .unsupported
                .get_or_insert((self.ticks, slot, error));
        }
        // each one's slot 0x70: the old owner's remove (slot 0x2d8), the new owner's add (slot
        // 0x2d0), each tracker by its own masters
        for object in &moved {
            let master_id = object.unit.life.master_id;
            let old = key(
                self.model.masters.of(player_id),
                &self.model.lines,
                master_id,
                &self.model.roles,
            );
            self.players
                .player_mut(player_id)?
                .tracker
                .remove(object.unit.id, old);
            let key = key(
                self.model.masters.of(new),
                &self.model.lines,
                master_id,
                &self.model.roles,
            );
            self.players
                .player_mut(new)?
                .tracker
                .add(object.unit.id, key);
        }
        self.extend_live(new, moved)
    }

    /// Object `id` of player `player_id`, whose step at the loop's count `list_slot` failed with
    /// `error`, freezes: it stays in its list, stopped, its error in the player's freezes, and the
    /// player keeps its first error with the object's slot (the count, once the step took it from
    /// the list).
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    fn freeze_object(
        &mut self,
        player_id: PlayerId,
        (list_slot, id): (ListIndex, ObjectId),
        why: GenieError,
    ) -> GenieResult<()> {
        let slot = self.live_index(player_id, id);
        let player = self.players.player_mut(player_id)?;
        let error = match slot {
            Some(list_index) => player.live(list_index)?.in_context(
                self.parts.actions_of(id),
                self.parts.unit_ai.get(id),
                why,
            ),
            None => why.with("object", id),
        };
        object_freezes(self.ticks, player_id, id, &error);
        player.freezes.push((self.ticks, id, error.clone()));
        let first = (self.ticks, slot.unwrap_or(list_slot), error);
        let _ = player.unsupported.get_or_insert(first);
        if let Some(list_index) = slot {
            player.live_mut(list_index)?.stopped = true;
        }
        self.spatial.freeze(id);
        self.raise_frozen_bounds()
    }
}

/// What a stopped player's building may have made since it froze, and a frozen unit's reach:
/// `spatial`'s bounds rise to the largest `bound` gives the players' unit masters (type 70) of
/// `masters`, Gaia's left out ([`Spatial::raise_frozen_bounds`]).
fn raise_frozen_bounds(spatial: &mut Spatial, masters: &[Arc<Masters>], bound: &MotionBound<'_>) {
    let units = masters
        .iter()
        .skip(1)
        .flat_map(|masters| masters.iter())
        .filter(|master| master.kind == MasterKind::COMBAT);
    for master in units {
        spatial.raise_frozen_bounds(bound.speed(master), bound.reach(master));
    }
}
