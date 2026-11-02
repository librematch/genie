//! The unit AI's part of the update: its surroundings, its side and its run.

use crate::GenieError;
use crate::construction::build::CART_TASK;
use crate::groups::GroupCommandKind;
use crate::map::Distance;
use crate::map::Footprint;
use crate::math::rng::{Stream, Well512};
use crate::movement::Speed;
use crate::movement::{Mover, footprint_gap_squared};
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::masters::ActionKind;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatLevel;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::unit::object_master;
use crate::objects::unit::{Outcome, Unit};
use crate::ownership::CaptureKind;
use crate::ownership::capture::Captured;
use crate::players::Chance;
use crate::players::PlayerId;
use crate::players::PlayerType;
use crate::unit_ai::UnitAiType;
use crate::unit_ai::task_finder::allied;
use crate::unit_ai::world_view::{OwnerView, UnitView, changed_view};
use crate::unit_ai::{AfterOrder, Surroundings, UnitAi};
use crate::unit_ai::{Attacker, GroupState, NotifyKind, Seen, UnitSeen, UnitSide};
use crate::vision::visibility::sight_taker;
use crate::world::UNIT_AI_MASTER;
use crate::world::others::OthersRef;
use crate::world::parts::PartTable;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
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
        c: &mut Step<'_>,
        (captured, leaves): (Option<Captured>, Outcome),
    ) -> Result<Outcome, GenieError> {
        self.refresh_pathing(c)?;
        // RGE_Combat_Object::update after the actions: the object's sight mask; a changed mask of
        // an object others take over by seeing it (`+0x924` 1) gives it to the first player that
        // sees it ([`crate::vision::visibility::sight_taker`]) when that one has its master
        let mut captured = captured;
        let combat = MasterKind::COMBAT <= self.master(c.w.master_tables(), c.player)?.kind;
        if combat
            && let Some(mask) =
                c.w.spatial
                    .combat_sight(self.id, c.w.parts.building_of(self.id).annex_of)?
            && let Some(q) = sight_taker(
                mask,
                c.player,
                c.w.settings.game_mode.standard(),
                &c.w.players,
                &c.w.allow_gaia_capture,
            )
            && c.w
                .model
                .masters
                .get(q.index())
                .is_some_and(|m| m.get(self.life.master_id).is_some())
        {
            self.take_by_sight(actions, c, q)?;
            captured = Some(Captured {
                new: q,
                main: true,
                also: Vec::new(),
                building: take(&mut c.building_handover),
            });
        }
        let mut after = self.ai_run(actions, c, captured.as_ref(), None)?;
        while let Some(tail) = after {
            after = self.ai_run(actions, c, captured.as_ref(), Some(tail))?;
        }
        if let Some((new, main)) = c.taken_over.take() {
            captured = Some(Captured {
                new,
                main,
                also: Vec::new(),
                building: take(&mut c.building_handover),
            });
        }
        if let Some(k) = captured {
            return Ok(Outcome::Captured(k));
        }
        Ok(leaves)
    }

    /// What the unit's AI reads of it and the world as it runs ([`Surroundings`]): the unit,
    /// its master (`master`, its class `class`), its owner `player` (a captured unit's new one,
    /// whose map sees the AI's target), `busy` (an action of its own) and `interruptible` (slot
    /// 0x2d0), through `view` (the owner's side).
    ///
    /// # Errors
    ///
    /// A busy unit of a master the slice does not have, and what the target's block and the
    /// named units do not model.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn surroundings(
        &self,
        actions: &Action,
        c: &Step<'_>,
        view: &OwnerView,
        (master, class): (Option<&Master>, Option<ClassId>),
        (player, busy, interruptible): (PlayerId, bool, bool),
    ) -> Result<Surroundings, GenieError> {
        Ok(Surroundings {
            position: self.position(),
            hp: self.life.hp,
            speed: c
                .w
                .parts
                .mover_of(self.id)
                .map_or(Speed::ZERO, |m| m.stats.speed),
            map: c.w.spatial.map,
            dt_seconds: c.dt.seconds(),
            mode_1: c.w.mode_680.is_one(),
            busy,
            player,
            class: match class {
                Some(class) => Some(class),
                None if busy => {
                    return Err(GenieError::unsupported(
                        "a busy unit of a master the slice does not have",
                    ));
                }
                None => None,
            },
            combat_level: master.map_or(CombatLevel::NONE, |m| m.combat_level),
            attack_priority: master.map_or(AttackPriority::PRIORITY_0, |m| m.attack_priority),
            size: master.map_or(Footprint::ZERO, |m| m.radius),
            los: master.map_or(Distance::ZERO, |m| m.line_of_sight),
            search_radius: master.map_or(Distance::ZERO, |m| m.search_radius),
            weapon_range: master.map_or(Distance::ZERO, |m| m.weapon_range),
            interruptible,
            task_0x99: master.is_some_and(|m| m.tasks.iter().any(|t| t.action_kind == CART_TASK)),
            garrisoned: self.life.inside.is_some(),
            state: self.life.state,
            kind: master.map_or(MasterKind::of_data(0), |m| m.kind),
            ranged: master.is_some_and(|m| m.missile.master().is_some()),
            firepower: c.w.parts.combat_of(self.id).firepower,
            capture: self.capture_kind(c),
            master: self.life.master_id,
            scenario: c.w.settings.game_mode.scenario(),
            task_0x69: master
                .is_some_and(|m| m.tasks.iter().any(|t| t.action_kind == ActionKind::HEAL)),
            // a new owner sees the target with its own map
            target: self.ai_target(c, view.player)?,
            under_attack: self.under_attack,
            id: self.id,
            anim_speed: c.w.parts.anim_speed_of(self.id),
            action_state: if c.w.parts.unread.contains(self.id) {
                None
            } else {
                Some(
                    actions
                        .top_action()
                        .map_or(ActionState::NO_ACTION, |a| a.state),
                )
            },
            attack_listed: actions.list.attack().is_some(),
            units: self.named_units(
                c,
                changed_view(
                    OthersRef::of(&c.w.players, &c.w.index, c.player)?,
                    view.new_owner,
                ),
                view,
                master,
            )?,
            group: (self.group.id)
                .and_then(|id| c.w.groups.get(id))
                .map(|g| GroupState {
                    commanded: g.current_command != GroupCommandKind::NONE,
                    idle: g.is_idle(),
                    patrol: g.patrols(),
                    moving: g.motion.moving(),
                    member_moving: c.w.parts.mover_of(self.id).is_some_and(Mover::is_moving),
                }),
            top_target: actions
                .top_target_seen(c, OthersRef::of(&c.w.players, &c.w.index, c.player)?)?,
            min_range: master.map_or(Distance::ZERO, |m| m.min_range),
            attack_moving: c.w.parts.mover_of(self.id).is_some_and(|m| m.attack_moving),
            moved_at: c
                .w
                .parts
                .mover_of(self.id)
                .map_or_else(|| self.position(), |m| m.moved_at)
                .vector(),
            dodge_chance: c
                .w
                .players
                .get(player.index())
                .map_or(Chance::NONE, |q| q.dodge_chance),
            keep_distance_chance: c
                .w
                .players
                .get(player.index())
                .map_or(Chance::NONE, |q| q.keep_distance_chance),
            disable_tower_priority: view.ai_settings.disable_tower_priority,
            guest_computer: self.garrison.iter().any(|&g| {
                c.w.spatial.get(g).is_some_and(|p| {
                    p.player != player
                        && c.w
                            .players
                            .get(p.player.index())
                            .is_some_and(|q| q.kind.is_computer())
                })
            }),
        })
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
        c: &mut Step<'_>,
        view: &OwnerView,
        attacker: Attacker,
        (player, player_type): (PlayerId, PlayerType),
        f: impl FnOnce(&mut UnitAi, &mut UnitView<'_, '_>, &mut Well512) -> Result<R, GenieError>,
    ) -> Result<R, GenieError> {
        let mut ai = c.w.lend_unit_ai(self.id).ok_or(NO_UNIT_AI)?;
        // the AI stream is the AI's while it runs: no world routine the side calls draws from it
        let held = c.w.streams.get(Stream::Ai).clone();
        let mut ai_stream = held.clone();
        let outer = c.side_notifies.replace(Vec::new());
        let mut side = UnitView {
            c: &mut *c,
            unit: self,
            actions,
            attacker,
            player,
            player_type,
            new_owner: view.new_owner,
            civ: view.civ,
            ai_settings: view.ai_settings,
        };
        let result = f(&mut ai, &mut side, &mut ai_stream);
        let own = replace(&mut c.side_notifies, outer).unwrap_or_default();
        let posted = own.into_iter().try_for_each(|(n, state)| ai.post(n, state));
        c.w.return_unit_ai(self.id, ai);
        let stream = c.w.streams.get(Stream::Ai);
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
        c: &mut Step<'_>,
        f: impl FnOnce(
            &mut UnitAi,
            &Surroundings,
            &mut dyn UnitSide,
            PlayerType,
            &mut Well512,
        ) -> Result<(), GenieError>,
    ) -> Result<(), GenieError> {
        let Some(ai_type) = c.w.parts.unit_ai.get(self.id).map(|ai| ai.ai_type) else {
            return Err(GenieError::unreachable(
                "a unit AI's run of a unit without one",
            ));
        };
        let busy = actions.top_action_kind().is_some() || c.w.parts.unread.contains(self.id);
        let master = Some(self.master(c.w.master_tables(), c.player)?);
        let class = master.map(|ms| ms.class);
        let (player, player_type) = (c.player, c.w.players.player(c.player)?.kind);
        let view = OwnerView::of(c, None)?;
        let interruptible = actions.interruptible(c.w.parts.unread.get(self.id))?;
        let at = self.surroundings(
            actions,
            c,
            &view,
            (master, class),
            (player, busy, interruptible),
        )?;
        let attacker = self.attacker_of(
            actions,
            ai_type,
            master,
            (player, player_type),
            c.w.settings.game_mode.scenario(),
            (
                at.capture,
                c.w.parts.combat_of(self.id).firepower,
                c.w.parts
                    .mover_of(self.id)
                    .map_or(Speed::ZERO, |m| m.stats.speed),
            ),
        );
        self.with_side(
            actions,
            c,
            &view,
            attacker,
            (player, player_type),
            |ai, side, rng| f(ai, &at, side, player_type, rng),
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
        c: &mut Step<'_>,
        f: impl FnOnce(&mut UnitAi, &mut dyn UnitSide, &mut Well512) -> Result<R, GenieError>,
    ) -> Result<R, GenieError> {
        let Some(ai_type) = c.w.parts.unit_ai.get(self.id).map(|ai| ai.ai_type) else {
            return Err(GenieError::unreachable(
                "a unit AI's run of a unit without one",
            ));
        };
        let master = Some(self.master(c.w.master_tables(), c.player)?);
        let (player, player_type) = (c.player, c.w.players.player(c.player)?.kind);
        let view = OwnerView::of(c, None)?;
        let attacker = self.attacker_of(
            actions,
            ai_type,
            master,
            (player, player_type),
            c.w.settings.game_mode.scenario(),
            (
                self.capture_kind(c),
                c.w.parts.combat_of(self.id).firepower,
                c.w.parts
                    .mover_of(self.id)
                    .map_or(Speed::ZERO, |m| m.stats.speed),
            ),
        );
        self.with_side(
            actions,
            c,
            &view,
            attacker,
            (player, player_type),
            |ai, side, rng| f(ai, side, rng),
        )
    }

    /// The capture kind its seer has in the players' sight (0 without one).
    fn capture_kind(&self, c: &Step<'_>) -> CaptureKind {
        c.w.spatial
            .sight
            .as_ref()
            .and_then(|v| v.seer(self.id))
            .map_or(CaptureKind::NONE, |s| s.capture)
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
        c: &mut Step<'_>,
        captured: Option<&Captured>,
        after: Option<AfterOrder>,
    ) -> Result<Option<AfterOrder>, GenieError> {
        let busy = actions.top_action_kind().is_some() || c.w.parts.unread.contains(self.id);
        // the unit's master: its own (a captured unit's copy), else its owner's
        let master = object_master(
            c.w.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            c.w.model.masters.of(c.player),
        )
        .ok();
        let class = master.map(|ms| ms.class);
        // a captured unit's unit AI runs as its new owner's (RGE_Combat_Object::update reads the
        // owner then): on that player's turn, with its time as it stands
        let (turn, player, player_type) = match captured {
            Some(k) if k.main => (
                (k.new == c.w.player_turn)
                    .then(|| c.w.player_time.get(k.new.index()).copied())
                    .flatten(),
                k.new,
                c.w.players
                    .get(k.new.index())
                    .map_or(PlayerType::PHANTOM, |q| q.kind),
            ),
            _ => (c.turn, c.player, c.w.players.player(c.player)?.kind),
        };
        let view = OwnerView::of(c, captured.filter(|k| k.main).map(|k| k.new))?;
        // RGE_Combat_Object::update: on the owner's turn the full update (slot 4), else
        // FUN_140c68980 for a unit not inside another; a dead unit's full update only adds the
        // world's dt to its time since the last enemy sighting (`+0x1b4`) before it returns
        if turn.is_some() && c.w.parts.unit_ai.contains(self.id) && after.is_none() {
            // the full update's start lets the unit AIs' search cache go
            c.w.spatial.forget_search();
        }
        if turn.is_some()
            && after.is_none()
            && !self.life.is_alive()
            && let Some(ai) = c.w.parts.unit_ai.get_mut(self.id)
        {
            ai.time_since_enemy += c.dt.seconds();
        }
        let runs = turn.is_some() || self.life.inside.is_none();
        let Some(orders) =
            c.w.parts
                .unit_ai
                .get(self.id)
                .filter(|_| self.life.is_alive() && runs)
                .map(|ai| !ai.orders.is_empty())
        else {
            return Ok(None);
        };
        // slot 0x2d0 matters only to a queue with orders
        let interruptible = match actions.interruptible(c.w.parts.unread.get(self.id)) {
            Ok(i) => i,
            Err(e) if orders => return Err(e),
            Err(_) => false,
        };
        let at = self.surroundings(
            actions,
            c,
            &view,
            (master, class),
            (player, busy, interruptible),
        )?;
        // the unit AI runs on its own; what it asks of its unit comes back through `UnitView`
        let world_time = c.w.world_time;
        let ai_type =
            c.w.parts
                .unit_ai
                .get(self.id)
                .map_or(UnitAiType::of_value(0), |ai| ai.ai_type);
        let attacker = self.attacker_of(
            actions,
            ai_type,
            master,
            (player, player_type),
            c.w.settings.game_mode.scenario(),
            (
                at.capture,
                c.w.parts.combat_of(self.id).firepower,
                c.w.parts
                    .mover_of(self.id)
                    .map_or(Speed::ZERO, |m| m.stats.speed),
            ),
        );
        self.with_side(
            actions,
            c,
            &view,
            attacker,
            (player, player_type),
            |ai, side, ai_stream| {
                // an order on a target comes only from a full update, on the owner's turn
                match (turn, after) {
                    (Some(_), Some(after)) => ai
                        .after_order(after, world_time, player_type, &at, ai_stream, side)
                        .map(|()| None),
                    (Some(dt), None) => {
                        ai.update(dt, world_time, player_type, &at, ai_stream, side)
                    }
                    (None, _) => ai
                        .off_turn(&at, side, player_type, ai_stream)
                        .map(|()| None),
                }
            },
        )
    }

    /// Its unit AI's target as the AI reads it where it runs (`FUN_1415417a0` on `+0x48`), with
    /// player `viewer`'s map ([`crate::world::World::seen`]): none without one, the unit itself
    /// (out of its slot) when it is its own target (an unload's). The unit's update may have
    /// changed the target before (the Build's work order on a resource).
    fn ai_target(&self, c: &Step<'_>, viewer: PlayerId) -> Result<Option<Seen>, GenieError> {
        match c
            .w
            .parts
            .unit_ai
            .get(self.id)
            .and_then(|ai| ai.current_target)
        {
            None => Ok(None),
            Some(t) if t == self.id => c.w.seen_of(self, c.player, viewer).map(Some),
            Some(t) => c.w.seen(Some(t), viewer),
        }
    }

    /// The objects its unit AI names besides its target, as the world's table holds them
    /// ([`crate::unit_ai::Surroundings::units`]): its attackers and the callers of its damage
    /// notifies, from its own list or another player's, each with its master's class, state, its
    /// unit AI's target and its footprint's squared gap to the unit's (`FUN_14118f8c0`).
    pub(crate) fn named_units(
        &self,
        c: &Step<'_>,
        others: OthersRef<'_>,
        view: &OwnerView,
        master: Option<&Master>,
    ) -> Result<Vec<UnitSeen>, GenieError> {
        let Some(ai) = c.w.parts.unit_ai.get(self.id) else {
            return Ok(Vec::new());
        };
        let damaged = ai
            .notifies
            .iter()
            .filter(|n| n.kind == NotifyKind::DAMAGED)
            .map(|n| n.caller);
        let in_the_way = ai
            .notifies
            .iter()
            .filter(|n| n.kind == NotifyKind::IN_THE_WAY)
            .map(|n| ObjectId::of_field(n.p[0]));
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
            let (Some((owner, u)), Some(p)) = (found, c.w.spatial.get(id)) else {
                continue;
            };
            let m = u.master(c.w.master_tables(), owner)?;
            let me = master.ok_or(UNIT_AI_MASTER)?;
            seen.push(UnitSeen {
                id,
                class: m.class,
                owner,
                combat_level: m.combat_level,
                state: u.life.state,
                target: c.w.parts.unit_ai.get(u.id).and_then(|a| a.current_target),
                gap_squared: footprint_gap_squared(
                    self.position(),
                    me.radius,
                    p.position,
                    p.radius,
                ),
                position: p.position,
                hp: u.life.hp,
                master: u.life.master_id,
                kind: m.kind,
                main: c.w.parts.building_of(u.id).annex_of,
                allied: allied(view.relations(c.w)?, view.player, owner, true),
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
    ) -> Result<(), GenieError> {
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
    ) -> Result<(), GenieError> {
        let in_the_way = mover.map(|m| take(&mut m.in_the_way)).unwrap_or_default();
        for id in in_the_way {
            self.post_own(ais, NotifyKind::IN_THE_WAY, [id.get(), -1, -1])?;
        }
        Ok(())
    }
}
