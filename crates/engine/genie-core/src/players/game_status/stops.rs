//! A defeated player's objects stop (`FUN_141613ed0`): each object's stop and its top action's
//! halt by kind.

use crate::construction::FoundationId;
use crate::construction::build::BuildAction;
use crate::construction::repair::RepairAction;
use crate::construction::repair::RepairerCount;
use crate::gathering::DropSiteId;
use crate::gathering::HuntTargetId;
use crate::gathering::gather::GatherAction;
use crate::gathering::gather::Stand;
use crate::gathering::hunt::{HuntState, stand_sprite};
use crate::map::Position;
use crate::math::fmath::below_or_unordered;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::SpriteId;
use crate::objects::actions::Action;
use crate::objects::masters::ActionKind;
use crate::objects::release::Releaser;
use crate::objects::sprite::new_sprite;
use crate::ownership::artifact::ANIMAL_UNMOVING;
use crate::ownership::artifact::Artifact;
use crate::players::PlayerId;
use crate::players::game_status::KEEPING_KINDS;
use crate::unit_ai::AiAction;
use crate::unit_ai::StopOrder;
use crate::world::Seconds;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

/// Whether the stop keeps the unit's action list (its top action's slot 0x38,
/// [`World::halt_top_action`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub(super) enum StopList {
    /// The slot answered 1: the list stays.
    Keeps,
    /// The list is cleared.
    Clears,
}

impl World {
    /// `FUN_141613ed0`: a defeated player's objects in its live list's order. A villager, a
    /// fishing boat and an explorer (unit AI action `0x2bd` or 605) empty their ignore lists
    /// (`FUN_141197da0`), stop (slot 0x148, [`World::object_stop`]) and stand (slot 0x560:
    /// the stop of their movement, slot 0x5d0, and their walked level's reset,
    /// `FUN_140f98e80`); a building stops but for a few masters (`FUN_1411e6290`) and gives
    /// back its queue ([`World::cancel_queue`]); every unit AI's orders go (`FUN_140c5e010`). The
    /// slice holds no unit groups (`+0x6d0`).
    pub(super) fn stop_everything(&mut self, player_id: PlayerId) -> GenieResult<()> {
        for idx in self.players.player(player_id)?.objects.places() {
            let unit = &self.players.live(player_id, idx)?;
            let master = unit.master(self.master_tables(), player_id)?;
            let (class, id) = (master.class, master.id);
            let explorer = self.parts.unit_ai.get(unit.id).is_some_and(|ai| {
                matches!(
                    ai.current_action,
                    Some(AiAction::AutoScout | AiAction::Explore)
                )
            });
            // The classes a defeat stops besides explorers: villagers (4) and fishing boats (21).
            if self
                .model
                .roles
                .of_class(class)
                .intersects(ClassRoles::VILLAGER | ClassRoles::FISHING_SHIP)
                || explorer
            {
                if let Some(mover) = self.parts.mover_mut(self.players.live(player_id, idx)?.id) {
                    mover.ignore_units.clear();
                }
                self.object_stop(player_id, idx)?;
                if let Some(mover) = self.parts.mover_mut(self.players.live(player_id, idx)?.id) {
                    mover.stand()?;
                }
            }
            if self
                .model
                .roles
                .of_class(class)
                .contains(ClassRoles::BUILDING)
            {
                if !self
                    .model
                    .roles
                    .of_master(id)
                    .contains(MasterRoles::UNSTOPPED)
                {
                    self.object_stop(player_id, idx)?;
                }
                self.cancel_queue(player_id, idx)?;
            }
            if let Some(ai) = self
                .parts
                .unit_ai
                .get_mut(self.players.live(player_id, idx)?.id)
            {
                ai.orders.clear();
            }
        }
        Ok(())
    }

    /// The object stop (slot 0x148, `FUN_14108c050`; a building's `FUN_1411e6290`) of the object
    /// at `idx` of player `player_id`: the action object's stop (`FUN_14106fa60`): its top action's
    /// slot 0x38 ([`World::halt_top_action`]); the action list goes ([`World::clear_actions`])
    /// unless that answered true for a unit that is no trade cart with goods; then a unit AI of
    /// a class other than 58 whose action value is not `0x2c2` and whose top action is no trade
    /// (0x6f) with a load stops (slot 0xb0 with 0: the order ends, and stopAction, slot 0x3c0)
    /// and forgets its target and its last target (`FUN_140c4dcb0`).
    pub(crate) fn object_stop(&mut self, player_id: PlayerId, idx: ListIndex) -> GenieResult<()> {
        let unit = &self.players.live(player_id, idx)?;
        let master = unit.master(self.master_tables(), player_id)?;
        let class = master.class;
        if self
            .parts
            .actions_of(unit.id)
            .top_action_kind()
            .is_some_and(|action_kind| KEEPING_KINDS.contains(&action_kind))
        {
            return Err(GenieError::unsupported(
                "a stop of an action of type 0x7c or 0x7d",
            ));
        }
        let list = self.halt_top_action(player_id, idx)?;
        // The livestock's class, whose unit AI a stop leaves alone (`FUN_14108c050`: raw `-0xe72`).
        // The trade carts' class, which keep their actions while they carry goods (`FUN_14106a940`).
        // The trade action's type (`0x6f`), whose load keeps a stopped unit's AI going.
        if list == StopList::Keeps
            && self
                .model
                .roles
                .of_class(class)
                .contains(ClassRoles::TRADE_CART)
        {
            return Err(GenieError::unsupported(
                "a trade cart's goods after its stop (+0x7a0, FUN_141188160)",
            ));
        }
        // FUN_14106fa60: the list stays when the top action's slot 0x38 answered true and the
        // unit is no trade cart with goods (FUN_14106a940)
        if list == StopList::Clears {
            self.clear_actions(player_id, idx)?;
        }
        let unit = self.players.live_mut(player_id, idx)?;
        let place = (unit.position().ground(), Some(class));
        // slot 0x438 is the top action's type: a trade (0x6f) with a load held (COMISS 0, held;
        // JNC at 0x14108c11e: a NaN load counts) keeps the unit AI going
        let trading = self.parts.actions_of(unit.id).top_action_kind() == Some(ActionKind::TRADE)
            && below_or_unordered(0.0, unit.life.amount.get())
            && unit.life.attribute_kind.is_some();
        let Some(ai) = self.parts.unit_ai.get(unit.id) else {
            return Ok(());
        };
        if self
            .model
            .roles
            .of_class(class)
            .contains(ClassRoles::LIVESTOCK)
            || ai.current_action == Some(AiAction::HeroPower)
            || trading
        {
            return Ok(());
        }
        // slot 0xb0 with 0, its stopAction a call on the unit at once
        let roles = self.model.roles.clone();
        self.run_ai_calls((player_id, idx), |ai, side, rng| {
            ai.stop_with(StopOrder::End, Some(place), side, rng, &roles)?;
            ai.forget_targets();
            Ok(())
        })
    }

    /// The top action's slot 0x38 before the stop clears the list (`FUN_140a97770`):
    /// [`StopList::Clears`] without one, or for a `MoveTo`, an Explore or an Enter (the base's,
    /// `FUN_140a7bbe0`). A human player's Hunt (`FUN_140ac79b0`) goes to state 2 (`0x140ac60e0`
    /// case 2): its sub-actions go, the unit stands with the hunt's sprite (slot 0x68 with
    /// `FUN_140ac5210`'s), its target goes (slot 0x68 with none, [`World::release`]) and its second
    /// target (`FUN_140a7a190` with none), its point and timer clear; then the pasture it tends
    /// lets it go (`FUN_141189ca0` with −1), and the answer is [`StopList::Keeps`]. The Fly (Bird),
    /// the missile's, the Make and the wait for a foundation keep the base's too; the Gather, the
    /// Build, the Attack and the Artifact halt ([`World::halt_other`]).
    pub(super) fn halt_top_action(
        &mut self,
        player_id: PlayerId,
        idx: ListIndex,
    ) -> GenieResult<StopList> {
        let player_kind = self.players.player(player_id)?.kind;
        let by = self.releaser(player_id, idx)?;
        let master_id = self.players.live(player_id, idx)?.life.master_id;
        let standing = self.master(player_id, master_id)?.standing_sprite;
        let unit = self.players.live_mut(player_id, idx)?;
        let Some(head) = self.parts.actions_of(unit.id).list.head().cloned() else {
            return Ok(StopList::Clears);
        };
        // each halt changes the head's copy and writes it back before it calls the world
        let mut hunt = match head {
            Action::MoveTo(_)
            | Action::Explore(_)
            | Action::Enter(_)
            | Action::Fly(_)
            | Action::Missile(_)
            | Action::Make(_)
            | Action::WaitSolidify(_) => return Ok(StopList::Clears),
            Action::Hunt(hunt) => hunt,
            other => return self.halt_other(player_id, idx, other),
        };
        if player_kind.is_computer() {
            return Err(GenieError::unsupported(
                "a computer player's hunt stopped (set_state 1)",
            ));
        }
        if let Err(error) = hunt.tear_down(self.parts.motion.mover_mut(unit.id)) {
            self.parts
                .actions
                .of_mut(unit.id)?
                .list
                .set_hunt(Some(hunt));
            return Err(error);
        }
        hunt.state = HuntState::Idle;
        let sprite = stand_sprite(&hunt.task, standing, unit.life.held(), &self.model.graphics);
        // the target, then the second target
        let released = [
            hunt.target.map(HuntTargetId::id),
            hunt.carry_to.map(DropSiteId::id),
        ];
        hunt.set_target(None);
        hunt.carry_to = None;
        hunt.point = Position::from([-1.0; 3]);
        hunt.timer = Seconds::ZERO;
        self.parts
            .actions
            .of_mut(unit.id)?
            .list
            .set_hunt(Some(hunt));
        if let Some(graphic) = sprite {
            new_sprite(
                &mut unit.sprite,
                self.parts.move_stats_mut(unit.id),
                graphic,
                self.streams.get_mut(Stream::Sprite),
                self.model.tuning.sprite,
            );
        }
        if unit.my_pasture.is_some() {
            return Err(GenieError::unsupported(
                "a stopped hunter that tends a pasture",
            ));
        }
        for object_id in released.into_iter().flatten() {
            self.release(object_id, by)?;
        }
        Ok(StopList::Keeps)
    }

    /// Slot 0x38 of the top actions with their own besides the Hunt's, each [`StopList::Keeps`]:
    /// the Gather's (`FUN_140a8e1b0`, [`crate::gathering::gather::GatherAction::halt`]: its target
    /// and second target go, [`World::release`]), the Build's (`FUN_140ab0350`,
    /// [`crate::construction::build::BuildAction::halt`]: a counted builder leaves its foundation's
    /// builders, [`World::leave_builders`], before the rest; the foundation goes), the Attack's
    /// (`FUN_140a82ac0`, [`crate::combat::attack::Attack::halt`]), the Artifact's
    /// (`FUN_140aad1a0`, [`crate::ownership::artifact::Artifact::stop`]) and the Transport's
    /// (`FUN_140aa4620`, [`World::halt_transport`]). The Gather, the Build and
    /// the Artifact show the master's standing sprite (the Gather the task's carrying one while it
    /// carries at least 1 of the task's resource, `FUN_140a8b6b0`).
    /// [`Action::list`](crate::objects::layers::Action::list) holds no other kind.
    pub(super) fn halt_other(
        &mut self,
        player_id: PlayerId,
        idx: ListIndex,
        head: Action,
    ) -> GenieResult<StopList> {
        let by = self.releaser(player_id, idx)?;
        let master_id = self.players.live(player_id, idx)?.life.master_id;
        let standing = self.master(player_id, master_id)?.standing_sprite;
        match head {
            Action::Gather(gather) => self.halt_gather((player_id, idx), gather, standing, by)?,
            Action::Build(build) => self.halt_build((player_id, idx), build, by)?,
            Action::Repair(repair) => self.halt_repair((player_id, idx), repair)?,
            Action::Attack(mut attack) => {
                let unit = self.players.live_mut(player_id, idx)?;
                let halted = attack.halt(
                    self.parts.motion.layer_mut(unit.id),
                    &mut unit.sprite,
                    self.streams.get_mut(Stream::Sprite),
                    self.model.tuning.sprite,
                );
                self.parts
                    .actions
                    .of_mut(unit.id)?
                    .list
                    .set_attack(Some(attack));
                halted?;
            }
            Action::Artifact(artifact) => self.halt_artifact((player_id, idx), artifact, by)?,
            Action::Transport(transport) => self.halt_transport(player_id, idx, transport)?,
            Action::MoveTo(_)
            | Action::Explore(_)
            | Action::Enter(_)
            | Action::Fly(_)
            | Action::Missile(_)
            | Action::Make(_)
            | Action::WaitSolidify(_)
            | Action::Hunt(_) => {
                return Err(GenieError::unreachable(
                    "a stop's slot 0x38 that World::halt_top_action answers itself",
                )
                .with("action", head.kind()));
            }
        }
        Ok(StopList::Keeps)
    }

    /// [`World::halt_other`] of a Gather: it halts with `standing` (the task's
    /// carrying one while it carries at least 1 of the task's resource); its target and second
    /// target go, released by `by`. `standing` is the master's standing sprite.
    pub(super) fn halt_gather(
        &mut self,
        (player_id, idx): (PlayerId, ListIndex),
        mut gather: GatherAction,
        standing: Option<SpriteId>,
        by: Releaser,
    ) -> GenieResult<()> {
        let unit = self.players.live_mut(player_id, idx)?;
        let stand = stand_sprite(
            &gather.task,
            standing,
            unit.life.held(),
            &self.model.graphics,
        );
        let halted = gather.halt(
            self.parts.motion.mover_mut(unit.id),
            Stand {
                graphic: stand,
                sprite: &mut unit.sprite,
                rng: self.streams.get_mut(Stream::Sprite),
                sprite_tuning: &self.model.tuning.sprite,
            },
        );
        self.parts
            .actions
            .of_mut(unit.id)?
            .list
            .set_gather(Some(gather));
        let released = halted?;
        for object_id in released.into_iter().flatten() {
            self.release(object_id, by)?;
        }
        Ok(())
    }

    /// [`World::halt_other`] of a Build: a counted builder leaves its foundation's builders, the
    /// unit stands, and the foundation and the second target go, released by `by`.
    pub(super) fn halt_build(
        &mut self,
        (player_id, idx): (PlayerId, ListIndex),
        mut build: BuildAction,
        by: Releaser,
    ) -> GenieResult<()> {
        let unit = self.players.live_mut(player_id, idx)?;
        let foundation = build.target;
        let halted = build.halt(self.parts.motion.mover_mut(unit.id));
        self.parts
            .actions
            .of_mut(unit.id)?
            .list
            .set_build(Some(build));
        let (counted, target, site) = halted?;
        if counted {
            self.leave_builders(foundation)?;
        }
        self.standing_sprite(player_id, idx)?;
        let targets = [target.map(FoundationId::id), site.map(DropSiteId::id)];
        for object_id in targets.into_iter().flatten() {
            self.release(object_id, by)?;
        }
        Ok(())
    }

    /// [`World::halt_other`] of a Repair (`FUN_140adaa00`): `set_state(2)`, a counted repairer
    /// out of its target's repairers, the standing sprite; the target stays.
    pub(super) fn halt_repair(
        &mut self,
        (player_id, idx): (PlayerId, ListIndex),
        mut repair: RepairAction,
    ) -> GenieResult<()> {
        let unit = self.players.live_mut(player_id, idx)?;
        let target = repair.target;
        let halted = repair.halt(self.parts.motion.mover_mut(unit.id));
        self.parts
            .actions
            .of_mut(unit.id)?
            .list
            .set_repair(Some(repair));
        if halted? == RepairerCount::Counted {
            self.leave_builders(target)?;
        }
        self.standing_sprite(player_id, idx)
    }

    /// [`World::halt_other`] of an Artifact: its sub-actions and its target go (released by
    /// `by`, the Enter's first), a walking animal stops, and it stands.
    pub(super) fn halt_artifact(
        &mut self,
        (player_id, idx): (PlayerId, ListIndex),
        mut artifact: Artifact,
        by: Releaser,
    ) -> GenieResult<()> {
        let unit = self.players.live_mut(player_id, idx)?;
        let (gone, released) = artifact.stop();
        self.parts
            .actions_mut(unit.id)?
            .list
            .set_artifact(Some(artifact));
        if gone.stops {
            self.parts
                .mover_mut(unit.id)
                .ok_or(ANIMAL_UNMOVING)?
                .stop()?;
        }
        for object_id in [gone.released, released].into_iter().flatten() {
            self.release(object_id, by)?;
        }
        self.standing_sprite(player_id, idx)
    }
}
