//! The world update's parts ([`crate::world::schedule`] runs them): the command phase, the
//! moving objects, the players' relations and each player's objects' updates.

use crate::GenieError;
use crate::commands::Command;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::PlayersMasters;
use crate::objects::unit::{Outcome, Unit};
use crate::ownership::capture::Captured;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::world::Millis;
use crate::world::World;
use crate::world::slots::PlayerSlots;

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
        self.sync_moving();
        for command in commands {
            tracing::debug!(target: crate::trace::UPDATE, ?command, "command");
            let stopped: Vec<bool> = self
                .players
                .iter()
                .map(|p| p.unsupported.is_some())
                .collect();
            self.run(command);
            // a player the command stopped: its error names the command
            for (p, was) in self.players.iter_mut().zip(stopped) {
                if let (false, Some((_, _, why))) = (was, p.unsupported.as_mut()) {
                    let text = format!("{command:?}");
                    let name = text.split([' ', '{', '(']).next().unwrap_or_default();
                    *why = why.clone().with("command", name);
                }
            }
        }
        self.sync_moving();
    }

    /// The walks read units `ids`' gaits ([`Unit::gait`]) from the spatial index: after a change
    /// to their movers outside their own update (a group update's), each takes its unit's again
    /// (a unit the world does not hold, none).
    ///
    /// [`Unit::gait`]: crate::objects::unit::Unit::gait
    pub(crate) fn refresh_gaits(&mut self, ids: impl IntoIterator<Item = ObjectId>) {
        for id in ids {
            let gait = self.object(id).and_then(|(_, u)| u.gait());
            self.spatial.set_gait(id, gait);
        }
    }

    /// A tick of `dt` seconds passed: every frozen object that moves may have moved its master's
    /// speed times `dt` ([`Spatial::grow_unknown`]), as its master stands now; one of a master
    /// the world does not hold may be anywhere.
    ///
    /// [`Spatial::grow_unknown`]: crate::map::spatial::Spatial::grow_unknown
    pub(crate) fn grow_unknown(&mut self, dt: crate::world::Seconds) {
        let masters = &self.masters;
        self.spatial.grow_unknown(dt, |player, master| {
            crate::objects::masters::player_master(masters, player, master)
                .and_then(|m| m.moving)
                .map_or(crate::movement::Speed::new(f32::INFINITY), |m| m.base)
        });
    }

    /// Tell the world which objects move now and how the players stand to each other, for the
    /// walk's query ([`Spatial::walk_hits`]), as each update does first: for a look at the
    /// world between updates.
    ///
    /// [`Spatial::walk_hits`]: crate::map::spatial::Spatial::walk_hits
    pub fn sync_world(&mut self) {
        self.sync_moving();
    }

    /// Tell the world which objects move now and how the players stand to each other, for the
    /// walk's query ([`Spatial::walk_hits`]).
    ///
    /// [`Spatial::walk_hits`]: crate::map::spatial::Spatial::walk_hits
    pub(super) fn sync_moving(&mut self) {
        for p in &self.players {
            for u in p.objects.iter().chain(&p.sleeping) {
                self.spatial.set_gait(u.id, u.gait());
                self.spatial.set_annex(u.id, u.annex_of);
            }
        }
        if self.spatial.relations.is_empty() {
            self.spatial.relations = self.players.iter().map(|p| p.relations.clone()).collect();
            self.spatial.task_0x99 = self
                .masters
                .iter()
                .map(|m| {
                    m.iter()
                        .filter(|m| {
                            m.tasks
                                .iter()
                                .any(|t| t.action_type == crate::construction::build::CART_TASK)
                        })
                        .map(|m| m.id)
                        .collect()
                })
                .collect();
        }
    }

    /// Some player's attribute 204 is not 0, which a building's completion reads: the players'
    /// attributes as they stand.
    #[must_use]
    pub(crate) fn attribute_204(&self) -> bool {
        self.players
            .iter()
            .any(|q| q.attributes.get(204).is_some_and(|&v| v != Amount::ZERO))
    }
}

impl World {
    /// The updates of player `p`'s objects in its list's order (`FUN_141166550`): each one's
    /// step and what follows it ([`World::step_object`]), then its filing by the step
    /// ([`World::file_step`]). An object whose update is not modelled freezes, and so do all live
    /// objects of a player a command stopped; dead objects still run their state steps, which
    /// draw nothing and depend on nothing the stop changed.
    ///
    /// # Errors
    ///
    /// A player the world does not hold, or a slot of its list the loop's count misses.
    pub(super) fn update_objects(
        &mut self,
        p: PlayerId,
        (dt, turn): (Millis, Option<Millis>),
    ) -> Result<(), GenieError> {
        // the loop's count, which goes on past each object but one that leaves
        let mut i = ListIndex::FIRST;
        while self.players.player(p)?.objects.holds(i) {
            let player = self.players.player(p)?;
            let unit = player.live(i)?;
            let stopped = player.stopped || unit.stopped;
            if stopped && unit.life.is_alive() {
                self.spatial.freeze(unit.id);
                i = i.next();
                continue;
            }
            let (id, master) = (unit.id, unit.life.master_id);
            let _object =
                tracing::trace_span!(target: crate::trace::UPDATE, "object", id = id.get(), master = %master)
                    .entered();
            match self.step_object((p, i), (dt, turn, true), Unit::update) {
                Ok(s) => {
                    if self.file_step(p, s.slot, s.outcome)? {
                        i = i.next();
                    }
                }
                Err(e) => {
                    self.freeze_object(p, (i, id), e)?;
                    i = i.next();
                }
            }
        }
        Ok(())
    }

    /// Files object `i` of player `p` by the step it finished (the list's loop,
    /// `FUN_141166550`): one that leaves goes from its list, a sleeper to the sleeping list, a
    /// captured building or unit to its new owner. Returns whether the loop's count goes on: not
    /// past one that leaves (the game's loop runs its count again, the list's last object in the
    /// slot), past any other (a sleeper's slot holds the last object too, whose update waits
    /// for the next tick).
    ///
    /// # Errors
    ///
    /// A player the world does not hold, or a slot its list does not hold.
    pub(super) fn file_step(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        outcome: Outcome,
    ) -> Result<bool, GenieError> {
        let player = self.players.player_mut(p)?;
        match outcome {
            Outcome::Leave => {
                let gone = player.objects.swap_remove(i)?;
                if let Err(e) = self.leave_world(p, &gone) {
                    crate::trace::player_unsupported(self.ticks, p, &e);
                    let _ = self
                        .players
                        .player_mut(p)?
                        .unsupported
                        .get_or_insert((self.ticks, i, e));
                }
                // the list's last object took its place
                Ok(false)
            }
            Outcome::Sleep => {
                // the list's remove (FUN_1411664a0) puts its last object in the sleeper's place,
                // and the list's loop (0x141166550) goes on past it: that object's update waits
                // for the next tick; the player's remove and add (slots 0x2d8, 0x2d0) file it last
                // in the tracker
                let asleep = player.objects.swap_remove(i)?;
                let k = crate::players::tracker::key(
                    self.masters.of(p),
                    &self.lines,
                    asleep.life.master_id,
                );
                player.tracker.remove(asleep.id, k);
                player.tracker.add(asleep.id, k);
                player.sleeping.push(asleep);
                Ok(true)
            }
            Outcome::Captured(Captured { new, building, .. }) if building => {
                // a building's slot 0x70 (FUN_1411cd860), whole: its annexes, its lists' moves
                // and its new owner's parts
                let id = player.live(i)?.id;
                let watched = player.watched.iter().any(|w| w.id == id);
                if let Err(e) = self.building_changes_hands(p, id, new, watched) {
                    crate::trace::player_unsupported(self.ticks, p, &e);
                    let _ = self
                        .players
                        .player_mut(p)?
                        .unsupported
                        .get_or_insert((self.ticks, i, e));
                }
                Ok(true)
            }
            Outcome::Captured(taken) => {
                self.hand_over_units(p, i, taken)?;
                Ok(true)
            }
            Outcome::Stay => Ok(true),
        }
    }

    /// The units of player `p` a takeover in object `i`'s update captured go to their new owner:
    /// the object itself when the new owner has its master (`main`), and the units taken with it
    /// (`also`). The old owner's list loses them
    /// (`FUN_1411664a0`, the last in each place; the loop goes on past the first) and the new
    /// owner's gains them (`FUN_141616200`), each with its attributes
    /// ([`World::hand_over_attributes`]).
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    fn hand_over_units(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        taken: Captured,
    ) -> Result<(), GenieError> {
        let Captured {
            new, main, also, ..
        } = taken;
        let player = self.players.player_mut(p)?;
        let mut moved = Vec::new();
        if main {
            moved.push(player.objects.swap_remove(i)?);
        }
        for id in also {
            if let Some(k) = player.objects.position_of(id) {
                moved.push(player.objects.swap_remove(k)?);
            }
        }
        let mut failed = None;
        for u in &moved {
            if let Err(e) = self.hand_over_attributes(p, new, u) {
                let _ = failed.get_or_insert(e);
            }
        }
        if let Some(e) = failed {
            crate::trace::player_unsupported(self.ticks, p, &e);
            let _ = self
                .players
                .player_mut(p)?
                .unsupported
                .get_or_insert((self.ticks, i, e));
        }
        // each one's slot 0x70: the old owner's remove (slot 0x2d8), the new owner's add (slot
        // 0x2d0), each tracker by its own masters
        for u in &moved {
            let m = u.life.master_id;
            let old = crate::players::tracker::key(self.masters.of(p), &self.lines, m);
            self.players.player_mut(p)?.tracker.remove(u.id, old);
            let k = crate::players::tracker::key(self.masters.of(new), &self.lines, m);
            self.players.player_mut(new)?.tracker.add(u.id, k);
        }
        self.players.player_mut(new)?.objects.extend(moved);
        Ok(())
    }

    /// Object `id` of player `p`, whose step at the loop's count `i` failed with `e`, freezes:
    /// it stays in its list, stopped, its error in the player's freezes, and the player keeps
    /// its first error with the object's slot (the count, once the step took it from the list).
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    fn freeze_object(
        &mut self,
        p: PlayerId,
        (i, id): (ListIndex, ObjectId),
        e: GenieError,
    ) -> Result<(), GenieError> {
        let player = self.players.player_mut(p)?;
        let slot = player.objects.position_of(id);
        let e = match slot {
            Some(k) => player.live(k)?.in_context(e),
            None => e.with("object", id),
        };
        crate::trace::object_freezes(self.ticks, p, id, &e);
        player.freezes.push((self.ticks, id, e.clone()));
        let first = (self.ticks, slot.unwrap_or(i), e);
        let _ = player.unsupported.get_or_insert(first);
        if let Some(k) = slot {
            player.live_mut(k)?.stopped = true;
        }
        self.spatial.freeze(id);
        Ok(())
    }
}
