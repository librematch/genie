//! The unit AI's part of the update: its side and its run, on what it reads of its surroundings
//! ([`Unit::surroundings`]).

use crate::math::rng::{Stream, Well512};
use crate::movement::Speed;
use crate::movement::{Mover, footprint_gap_squared};
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::unit::{Outcome, Unit, object_master};
use crate::ownership::CaptureKind;
use crate::ownership::capture::Captured;
use crate::players::AllyTest;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::unit_ai::task_finder::allied;
use crate::unit_ai::world_view::{OwnerView, UnitView};
use crate::unit_ai::{AfterOrder, Surroundings, UnitAi};
use crate::unit_ai::{Attacker, NotifyKind, Seen, UnitSeen};
use crate::vision::visibility::sight_taker;
use crate::world::UNIT_AI_MASTER;
use crate::world::layer::Roles;
use crate::world::others::OthersRef;
use crate::world::parts::PartTable;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};
use std::mem::{replace, take};

/// A unit that runs its unit AI's side where it has none.
const NO_UNIT_AI: GenieError = GenieError::unreachable("a unit without a unit AI where one runs");

impl Unit {
    /// The end of [`Unit::update`] after the actions (`captured`: the Artifact's capture, `leaves`:
    /// the step its life step left): `RGE_Action_Object::update`'s end (`FUN_141163080`: the
    /// action object's shape), `RGE_Combat_Object::update`'s sight mask, then the unit AI's full
    /// update on its owner's turn, else its check between turns, its calls on the unit at once
    /// ([`Unit::ai_run`]); after an order on a target the full update took, the update's rest
    /// ([`crate::unit_ai::UnitAi::after_order`]), as the game runs both inside the update (slot
    /// 0x230). Then the step the update ends with.
    pub(crate) fn ai_part(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        (captured, leaves): (Option<Captured>, Outcome),
        roles: &Roles,
    ) -> GenieResult<Outcome> {
        self.refresh_pathing(step)?;
        // RGE_Combat_Object::update after the actions: the object's sight mask; a changed mask of
        // an object others take over by seeing it (`+0x924` 1) gives it to the first player that
        // sees it ([`crate::vision::visibility::sight_taker`]) when that one has its master
        let mut captured = captured;
        let combat = MasterKind::COMBAT <= self.master(step.w.master_tables(), step.player)?.kind;
        if combat
            && let Some(mask) = step
                .w
                .spatial
                .combat_sight(self.id, step.w.parts.building_of(self.id).annex_of)?
            && let Some(player_id) = sight_taker(
                mask,
                step.player,
                step.w.settings.game_mode,
                &step.w.players,
                &step.w.allow_gaia_capture,
            )
            && step
                .w
                .model
                .masters
                .get(player_id.index())
                .is_some_and(|masters| masters.get(self.life.master_id).is_some())
        {
            self.take_by_sight(actions, step, player_id, roles)?;
            captured = Some(Captured {
                new: player_id,
                main: true,
                also: Vec::new(),
                building: take(&mut step.building_handover),
            });
        }
        let mut after = self.ai_run(actions, step, captured.as_ref(), None)?;
        while let Some(tail) = after {
            after = self.ai_run(actions, step, captured.as_ref(), Some(tail))?;
        }
        if let Some((new, main)) = step.taken_over.take() {
            captured = Some(Captured {
                new,
                main,
                also: Vec::new(),
                building: take(&mut step.building_handover),
            });
        }
        if let Some(k) = captured {
            return Ok(Outcome::Captured(k));
        }
        Ok(leaves)
    }

    /// Runs `f` on the unit's AI with its side ([`UnitView`]: the unit without its AI, on the
    /// update's step, the owner as `view` names it) and the AI stream, with `attacker` for the
    /// attack tests ([`Unit::attacker_of`]), as player `player` (of `player_type`); the AI is the
    /// unit's again after it and is posted the notifies the side's calls gave it.
    ///
    /// # Panics
    ///
    /// A unit without a unit AI: the callers ask for one.
    ///
    /// # Errors
    ///
    /// `f`'s, a notify the AI's queue does not take, and a draw from the AI stream in a world
    /// routine the side called while the AI held the stream.
    pub(crate) fn with_side<R>(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        view: &OwnerView,
        attacker: Attacker,
        (player, player_kind): (PlayerId, PlayerKind),
        f: impl FnOnce(&mut UnitAi, &mut UnitView<'_, '_>, &mut Well512) -> GenieResult<R>,
    ) -> GenieResult<R> {
        let mut ai = step.w.lend_unit_ai(self.id).ok_or(NO_UNIT_AI)?;
        // the AI stream is the AI's while it runs: no world routine the side calls draws from it
        let held = step.w.streams.get(Stream::Ai).clone();
        let mut ai_stream = held.clone();
        let outer = step.side_notifies.replace(Vec::new());
        let mut side = UnitView {
            c: &mut *step,
            unit: self,
            actions,
            attacker,
            player,
            player_kind,
            new_owner: view.new_owner,
            civ: view.civ,
            ai_settings: view.ai_settings,
        };
        let result = f(&mut ai, &mut side, &mut ai_stream);
        let own = replace(&mut step.side_notifies, outer).unwrap_or_default();
        let posted = own
            .into_iter()
            .try_for_each(|(notify, state)| ai.post(notify, state));
        step.w.return_unit_ai(self.id, ai);
        let stream = step.w.streams.get(Stream::Ai);
        if *stream != held {
            return Err(GenieError::unreachable(
                "a world routine drew from the AI stream while the unit AI held it",
            ));
        }
        *stream = ai_stream;
        let answer = result?;
        posted?;
        Ok(answer)
    }

    /// The unit's AI run outside its update (a command's immediate order): `f` gets the AI, what
    /// it reads of the unit and the world ([`Unit::surroundings`], as the updating player's own
    /// unit), its side ([`Unit::with_side`], whose calls on the unit run at once), the owner's
    /// computer flag and the AI stream.
    ///
    /// # Errors
    ///
    /// A unit without a unit AI, slot 0x2d0 and the surroundings' parts the slice does not model,
    /// and what `f` and its calls do not.
    pub(crate) fn run_ai(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        f: impl FnOnce(
            &mut UnitAi,
            &Surroundings,
            &mut UnitView<'_, '_>,
            PlayerKind,
            &mut Well512,
        ) -> GenieResult<()>,
    ) -> GenieResult<()> {
        // every caller tests the unit for its unit AI first (`World::run_ai_calls`'s and
        // `World::run_unit_ai`'s callers, the capture's stop)
        let Some(ai_kind) = step.w.parts.unit_ai.get(self.id).map(|ai| ai.ai_kind) else {
            return Err(GenieError::unreachable(
                "a unit AI's run of a unit without one (its callers test for one)",
            ));
        };
        let busy = actions.top_action_kind().is_some() || step.w.parts.unread.contains(self.id);
        let master = Some(self.master(step.w.master_tables(), step.player)?);
        let class = master.map(|ms| ms.class);
        let (player, player_kind) = (step.player, step.w.players.player(step.player)?.kind);
        let view = OwnerView::of(step, None)?;
        let interruptible = actions.interruptible(step.w.parts.unread.get(self.id))?;
        let at = self.surroundings(
            actions,
            step,
            &view,
            (master, class),
            (player, busy, interruptible),
        )?;
        let attacker = self.attacker_of(
            actions,
            ai_kind,
            master,
            (player, player_kind),
            step.w.settings.game_mode,
            (
                at.capture,
                step.w.parts.combat_of(self.id).firepower,
                step.w
                    .parts
                    .mover_of(self.id)
                    .map_or(Speed::ZERO, |mover| mover.stats.speed),
            ),
        );
        self.with_side(
            actions,
            step,
            &view,
            attacker,
            (player, player_kind),
            |ai, side, rng| f(ai, &at, side, player_kind, rng),
        )
    }

    /// The unit's AI run by a world routine that gives it only the unit's place (a stance, a
    /// stop of the game's status, a let-out, an unload, a capture's stop): `f` gets the AI, its
    /// side ([`Unit::with_side`], whose calls on the unit run at once, the updating player's) and
    /// the AI stream.
    ///
    /// # Errors
    ///
    /// A unit without a unit AI, and what `f` and its calls do not model.
    pub(crate) fn run_calls<R>(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        f: impl FnOnce(&mut UnitAi, &mut UnitView<'_, '_>, &mut Well512) -> GenieResult<R>,
    ) -> GenieResult<R> {
        // every caller tests the unit for its unit AI first (`World::run_ai_calls`'s and
        // `World::run_unit_ai`'s callers, the capture's stop)
        let Some(ai_kind) = step.w.parts.unit_ai.get(self.id).map(|ai| ai.ai_kind) else {
            return Err(GenieError::unreachable(
                "a unit AI's run of a unit without one (its callers test for one)",
            ));
        };
        let master = Some(self.master(step.w.master_tables(), step.player)?);
        let (player, player_kind) = (step.player, step.w.players.player(step.player)?.kind);
        let view = OwnerView::of(step, None)?;
        let attacker = self.attacker_of(
            actions,
            ai_kind,
            master,
            (player, player_kind),
            step.w.settings.game_mode,
            (
                self.capture_kind(step),
                step.w.parts.combat_of(self.id).firepower,
                step.w
                    .parts
                    .mover_of(self.id)
                    .map_or(Speed::ZERO, |mover| mover.stats.speed),
            ),
        );
        self.with_side(
            actions,
            step,
            &view,
            attacker,
            (player, player_kind),
            |ai, side, rng| f(ai, side, rng),
        )
    }

    /// The capture kind its seer has in the players' sight (0 without one).
    pub(super) fn capture_kind(&self, step: &Step<'_>) -> CaptureKind {
        step.w
            .spatial
            .sight
            .as_ref()
            .and_then(|visibility| visibility.seer(self.id))
            .map_or(CaptureKind::None, |seer| seer.capture)
    }

    /// One pass of the unit AI ([`Unit::ai_part`]) as the player `captured` names (its new
    /// owner's for a main capture, else the owner's): on the owner's turn the full update, or with
    /// `after` its rest after an order on a target ([`crate::unit_ai::UnitAi::after_order`]),
    /// else its check between turns, its calls on the unit at once. Returns the rest an order on
    /// a target the full update took leaves ([`AfterOrder`]).
    ///
    /// # Errors
    ///
    /// The unit AI's, its surroundings' and its calls'.
    fn ai_run(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        captured: Option<&Captured>,
        after: Option<AfterOrder>,
    ) -> GenieResult<Option<AfterOrder>> {
        let busy = actions.top_action_kind().is_some() || step.w.parts.unread.contains(self.id);
        // the unit's master: its own (a captured unit's copy), else its owner's
        let master = object_master(
            step.w.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            step.w.model.masters.of(step.player),
        )
        .ok();
        let class = master.map(|ms| ms.class);
        // a captured unit's unit AI runs as its new owner's (RGE_Combat_Object::update reads the
        // owner then): on that player's turn, with its time as it stands
        let (turn, player, player_kind) = match captured {
            Some(k) if k.main => (
                (k.new == step.w.player_turn)
                    .then(|| step.w.player_time.get(k.new.index()).copied())
                    .flatten(),
                k.new,
                step.w
                    .players
                    .get(k.new.index())
                    .map_or(PlayerKind::Phantom, |q| q.kind),
            ),
            _ => (
                step.turn,
                step.player,
                step.w.players.player(step.player)?.kind,
            ),
        };
        let view = OwnerView::of(step, captured.filter(|k| k.main).map(|k| k.new))?;
        // RGE_Combat_Object::update: on the owner's turn the full update (slot 4), else
        // FUN_140c68980 for a unit not inside another; a dead unit's full update only adds the
        // world's dt to its time since the last enemy sighting (`+0x1b4`) before it returns
        if turn.is_some() && step.w.parts.unit_ai.contains(self.id) && after.is_none() {
            // the full update's start lets the unit AIs' search cache go
            step.w.spatial.forget_search();
        }
        if turn.is_some()
            && after.is_none()
            && !self.life.is_alive()
            && let Some(ai) = step.w.parts.unit_ai.get_mut(self.id)
        {
            ai.time_since_enemy += step.dt.seconds();
        }
        let runs = turn.is_some() || self.life.inside.is_none();
        let Some((orders, ai_kind)) = step
            .w
            .parts
            .unit_ai
            .get(self.id)
            .filter(|_| self.life.is_alive() && runs)
            .map(|ai| (!ai.orders.is_empty(), ai.ai_kind))
        else {
            return Ok(None);
        };
        // slot 0x2d0 matters only to a queue with orders
        let interruptible = match actions.interruptible(step.w.parts.unread.get(self.id)) {
            Ok(i) => i,
            Err(error) if orders => return Err(error),
            Err(_) => false,
        };
        let at = self.surroundings(
            actions,
            step,
            &view,
            (master, class),
            (player, busy, interruptible),
        )?;
        // the unit AI runs on its own; what it asks of its unit comes back through `UnitView`
        let world_time = step.w.world_time;
        let attacker = self.attacker_of(
            actions,
            ai_kind,
            master,
            (player, player_kind),
            step.w.settings.game_mode,
            (
                at.capture,
                step.w.parts.combat_of(self.id).firepower,
                step.w
                    .parts
                    .mover_of(self.id)
                    .map_or(Speed::ZERO, |mover| mover.stats.speed),
            ),
        );
        self.with_side(
            actions,
            step,
            &view,
            attacker,
            (player, player_kind),
            |ai, side, ai_stream| {
                // an order on a target comes only from a full update, on the owner's turn
                match (turn, after) {
                    (Some(_), Some(after)) => ai
                        .after_order(after, world_time, player_kind, &at, ai_stream, side)
                        .map(|()| None),
                    (Some(dt), None) => {
                        ai.update(dt, world_time, player_kind, &at, ai_stream, side)
                    }
                    (None, _) => ai
                        .off_turn(&at, side, player_kind, ai_stream)
                        .map(|()| None),
                }
            },
        )
    }

    /// Its unit AI's target as the AI reads it where it runs (`FUN_1415417a0` on `+0x48`), with
    /// player `viewer`'s map ([`crate::world::World::seen`]): none without one, the unit itself
    /// (out of its slot) when it is its own target (an unload's). The unit's update may have
    /// changed the target before (the Build's work order on a resource).
    pub(super) fn ai_target(&self, step: &Step<'_>, viewer: PlayerId) -> GenieResult<Option<Seen>> {
        match step
            .w
            .parts
            .unit_ai
            .get(self.id)
            .and_then(|ai| ai.current_target)
        {
            None => Ok(None),
            Some(t) if t == self.id => step.w.seen_of(self, step.player, viewer).map(Some),
            Some(object_id) => step.w.seen(Some(object_id), viewer),
        }
    }

    /// The objects its unit AI names besides its target, as the world's table holds them
    /// ([`crate::unit_ai::Surroundings::units`]): its attackers, the callers of its damage
    /// notifies and missile warnings (a warning not dodged is the damage, whose handler looks its
    /// caller up), the units in its way, from its own list or another player's, each with its
    /// master's class, state, its
    /// unit AI's target and its footprint's squared gap to the unit's (`FUN_14118f8c0`).
    pub(crate) fn named_units(
        &self,
        step: &Step<'_>,
        others: OthersRef<'_>,
        view: &OwnerView,
        master: Option<&Master>,
    ) -> GenieResult<Vec<UnitSeen>> {
        let Some(ai) = step.w.parts.unit_ai.get(self.id) else {
            return Ok(Vec::new());
        };
        let damaged = ai
            .notifies
            .iter()
            .filter(|n| matches!(n.kind, NotifyKind::DAMAGED | NotifyKind::MISSILE_WARNING))
            .map(|n| n.caller);
        let in_the_way = ai
            .notifies
            .iter()
            .filter(|n| n.kind == NotifyKind::IN_THE_WAY)
            .map(|notify| ObjectId::of_field(notify.p[0]));
        // none (−1) names no object the world holds
        let mut ids: Vec<ObjectId> = ai
            .attackers
            .iter()
            .chain(damaged)
            .chain(in_the_way)
            .flatten()
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let mut seen = Vec::new();
        for id in ids {
            let found = others.find(id);
            let (Some((owner, unit)), Some(placed)) = (found, step.w.spatial.get(id)) else {
                continue;
            };
            let m = unit.master(step.w.master_tables(), owner)?;
            let me = master.ok_or(UNIT_AI_MASTER)?;
            seen.push(UnitSeen {
                id,
                class: m.class,
                owner,
                combat_level: m.combat_level,
                state: unit.life.state,
                target: step
                    .w
                    .parts
                    .unit_ai
                    .get(unit.id)
                    .and_then(|unit_ai| unit_ai.current_target),
                gap_squared: footprint_gap_squared(
                    self.position(),
                    me.radius,
                    placed.position,
                    placed.radius,
                ),
                position: placed.position,
                hp: unit.life.hp,
                master: unit.life.master_id,
                kind: m.kind,
                main: step.w.parts.building_of(unit.id).annex_of,
                allied: allied(
                    view.relations(step.w)?,
                    view.player,
                    owner,
                    AllyTest::WithRelation4,
                ),
            });
        }
        Ok(seen)
    }

    /// A notify the unit posts to its own unit AI (slot 0x328), its AI in the world's table
    /// `ais`.
    pub(crate) fn post_own(
        &self,
        ais: &mut PartTable<UnitAi>,
        kind: NotifyKind,
        p: [i32; 3],
    ) -> GenieResult<()> {
        ais.inbox(self.id, self.life.state).post(kind, p)
    }

    /// The units in the way the motion step told the unit AI of ([`crate::unit_ai::NotifyKind::IN_THE_WAY`],
    /// [`crate::movement::Mover::in_the_way`] of its `mover`), posted at once in their order
    /// (slot 0x328: the walker is in state 2).
    ///
    /// # Errors
    ///
    /// The notifies'.
    pub(crate) fn post_in_the_way(
        &self,
        mover: Option<&mut Mover>,
        ais: &mut PartTable<UnitAi>,
    ) -> GenieResult<()> {
        let in_the_way = mover.map(|m| take(&mut m.in_the_way)).unwrap_or_default();
        for id in in_the_way {
            self.post_own(ais, NotifyKind::IN_THE_WAY, [id.get(), -1, -1])?;
        }
        Ok(())
    }
}
