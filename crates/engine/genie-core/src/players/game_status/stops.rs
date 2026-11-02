//! A defeated player's objects stop (`FUN_141613ed0`): each object's stop and its top action's
//! halt by kind.

use crate::GenieError;
use crate::gathering::DropSiteId;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::SpriteId;
use crate::objects::masters::ActionType;
use crate::objects::masters::action_type;
use crate::objects::masters::class;
use crate::objects::release::Releaser;
use crate::players::PlayerId;
use crate::players::game_status::{KEEPING_TYPES, UNSTOPPED_BUILDINGS};
use crate::unit_ai::ActionKind;
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::PlayerSlots;

/// A halt of a top action finds the unit without that action.
const NO_TOP_ACTION: GenieError =
    GenieError::unreachable("a halted unit without the top action it halts");

impl World {
    /// `FUN_141613ed0`: a defeated player's objects in its live list's order. A villager, a
    /// fishing boat and an explorer (unit AI action `0x2bd` or 605) empty their ignore lists
    /// (`FUN_141197da0`), stop (slot 0x148, [`World::object_stop`]) and stand (slot 0x560:
    /// the stop of their movement, slot 0x5d0, and their walked level's reset,
    /// `FUN_140f98e80`); a building stops but for a few masters (`FUN_1411e6290`) and gives
    /// back its queue ([`World::cancel_queue`]); every unit AI's orders go (`FUN_140c5e010`). The
    /// slice holds no unit groups (`+0x6d0`).
    pub(super) fn stop_everything(&mut self, p: PlayerId) -> Result<(), GenieError> {
        for idx in self.players.player(p)?.objects.places() {
            let unit = &self.players.live(p, idx)?;
            let master = unit.master(self.master_tables(), p)?;
            let (class, id) = (master.class, master.id);
            let explorer = self.parts.unit_ai.get(unit.id).is_some_and(|ai| {
                matches!(
                    ai.current_action,
                    Some(ActionKind::AutoScout | ActionKind::Explore)
                )
            });
            // The classes a defeat stops besides explorers: villagers (4) and fishing boats (21).
            if matches!(class, class::VILLAGER | class::FISHING_SHIP) || explorer {
                if let Some(m) = self.parts.mover_mut(self.players.live(p, idx)?.id) {
                    m.ignore_units.clear();
                }
                self.object_stop(p, idx)?;
                if let Some(m) = self.parts.mover_mut(self.players.live(p, idx)?.id) {
                    m.stand()?;
                }
            }
            if class == class::BUILDING {
                if !UNSTOPPED_BUILDINGS.contains(&id) {
                    self.object_stop(p, idx)?;
                }
                self.cancel_queue(p, idx)?;
            }
            if let Some(ai) = self.parts.unit_ai.get_mut(self.players.live(p, idx)?.id) {
                ai.orders.clear();
            }
        }
        Ok(())
    }

    /// The object stop (slot 0x148, `FUN_14108c050`; a building's `FUN_1411e6290`) of the object
    /// at `idx` of player `p`: the action object's stop (`FUN_14106fa60`): its top action's
    /// slot 0x38 ([`World::halt_top_action`]); the action list goes ([`World::clear_actions`])
    /// unless that answered true for a unit that is no trade cart with goods; then a unit AI of
    /// a class other than 58 whose action value is not `0x2c2` and whose top action is no trade
    /// (0x6f) with a load stops (slot 0xb0 with 0: the order ends, and stopAction, slot 0x3c0)
    /// and forgets its target and its last target (`FUN_140c4dcb0`).
    pub(crate) fn object_stop(&mut self, p: PlayerId, idx: ListIndex) -> Result<(), GenieError> {
        let unit = &self.players.live(p, idx)?;
        let master = unit.master(self.master_tables(), p)?;
        let class = master.class;
        if unit
            .top_action_kind()
            .is_some_and(|k| KEEPING_TYPES.contains(&k))
        {
            return Err(GenieError::unsupported(
                "a stop of an action of type 0x7c or 0x7d",
            ));
        }
        let keeps = self.halt_top_action(p, idx)?;
        // The livestock's class, whose unit AI a stop leaves alone (`FUN_14108c050`: raw `-0xe72`).
        // The trade carts' class, which keep their actions while they carry goods (`FUN_14106a940`).
        // The trade action's type (`0x6f`), whose load keeps a stopped unit's AI going.
        if keeps && class == class::TRADE_CART {
            return Err(GenieError::unsupported(
                "a trade cart's goods after its stop (+0x7a0, FUN_141188160)",
            ));
        }
        // FUN_14106fa60: the list stays when the top action's slot 0x38 answered true and the
        // unit is no trade cart with goods (FUN_14106a940)
        if !keeps {
            self.clear_actions(p, idx)?;
        }
        let unit = self.players.live_mut(p, idx)?;
        let place = (unit.position().ground(), Some(class));
        // slot 0x438 is the top action's type: a trade (0x6f) with a load held (COMISS 0, held;
        // JNC at 0x14108c11e: a NaN load counts) keeps the unit AI going
        let trading = unit.top_action_kind() == Some(action_type::TRADE)
            && crate::math::fmath::below_or_unordered(0.0, unit.life.amount.get())
            && unit.life.attribute_type.is_some();
        let Some(ai) = self.parts.unit_ai.get(unit.id) else {
            return Ok(());
        };
        if class == class::LIVESTOCK || ai.current_action == Some(ActionKind::HeroPower) || trading
        {
            return Ok(());
        }
        // slot 0xb0 with 0, its stopAction a call on the unit at once
        self.run_ai_calls((p, idx), |ai, side, rng| {
            ai.stop_with(false, Some(place), side, rng)?;
            ai.forget_targets();
            Ok(())
        })
    }

    /// The top action's slot 0x38 before the stop clears the list (`FUN_140a97770`): false
    /// without one, or for a `MoveTo`, an Explore or an Enter (the base's, `FUN_140a7bbe0`). A
    /// human player's Hunt (`FUN_140ac79b0`) goes to state 2 (`0x140ac60e0` case 2): its
    /// sub-actions go, the unit stands with the hunt's sprite (slot 0x68 with
    /// `FUN_140ac5210`'s), its target goes (slot 0x68 with none, [`World::release`]) and its
    /// second target (`FUN_140a7a190` with none), its point and timer clear; then the pasture it
    /// tends lets it go (`FUN_141189ca0` with −1), and the answer is true. The Fly (Bird), the
    /// missile's, the Make and the wait for a foundation keep the base's too; the Gather, the
    /// Build, the Attack and the Artifact halt ([`World::halt_other`]).
    pub(super) fn halt_top_action(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
    ) -> Result<bool, GenieError> {
        let computer = self.players.player(p)?.computer;
        let by = self.releaser(p, idx)?;
        let master_id = self.players.live(p, idx)?.life.master_id;
        let standing = self.master(p, master_id)?.standing_sprite;
        let unit = self.players.live_mut(p, idx)?;
        let Some(kind) = unit.top_action_kind() else {
            return Ok(false);
        };
        match kind {
            crate::movement::move_to::MOVE_TO
            | crate::movement::explore::EXPLORE
            | crate::garrison::ENTER
            | crate::movement::fly::FLY
            | crate::combat::missile::MISSILE
            | crate::gathering::make::MAKE
            | crate::construction::wait_solidify::WAIT_SOLIDIFY => {
                return Ok(false);
            }
            crate::gathering::hunt::HUNT => {}
            _ => return self.halt_other(p, idx, kind),
        }
        if computer {
            return Err(GenieError::unsupported(
                "a computer player's hunt stopped (set_state 1)",
            ));
        }
        let hunt = unit.actions.list.hunt_mut().ok_or(NO_TOP_ACTION)?;
        hunt.tear_down(self.parts.mover_mut(unit.id))?;
        hunt.state = crate::gathering::hunt::HuntState::Idle;
        let sprite = crate::gathering::hunt::stand_sprite(
            &hunt.task,
            standing,
            unit.life.held(),
            &self.graphics,
        );
        // the target, then the second target
        let released = [hunt.target, hunt.carry_to.map(DropSiteId::id)];
        hunt.set_target(None);
        hunt.carry_to = None;
        hunt.point = Position::from([-1.0; 3]);
        hunt.timer = Seconds::ZERO;
        if let Some(g) = sprite {
            crate::objects::sprite::new_sprite(
                &mut unit.sprite,
                self.parts.move_stats_mut(unit.id),
                g,
                self.streams.get(Stream::Sprite),
            );
        }
        if unit.my_pasture.is_some() {
            return Err(GenieError::unsupported(
                "a stopped hunter that tends a pasture",
            ));
        }
        for t in released.into_iter().flatten() {
            self.release(t, by)?;
        }
        Ok(true)
    }

    /// Slot 0x38 of the top actions with their own besides the Hunt's, each true: the Gather's
    /// (`FUN_140a8e1b0`, [`crate::gathering::gather::GatherAction::halt`]: its target and second target go,
    /// [`World::release`]), the Build's (`FUN_140ab0350`, [`crate::construction::build::BuildAction::halt`]:
    /// a counted builder leaves its foundation's builders, [`World::leave_builders`], before the
    /// rest; the foundation goes), the Attack's (`FUN_140a82ac0`,
    /// [`crate::combat::attack::Attack::halt`]) and the Artifact's (`FUN_140aad1a0`,
    /// [`crate::ownership::artifact::Artifact::stop`]). The Gather, the Build and the Artifact show the
    /// master's standing sprite (the Gather the task's carrying one while it carries at least 1 of
    /// the task's resource, `FUN_140a8b6b0`). [`crate::objects::unit::Unit::top_action`] holds no other
    /// kind.
    pub(super) fn halt_other(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        kind: ActionType,
    ) -> Result<bool, GenieError> {
        let by = self.releaser(p, idx)?;
        let master_id = self.players.live(p, idx)?.life.master_id;
        let standing = self.master(p, master_id)?.standing_sprite;
        match kind {
            crate::gathering::gather::GATHER => self.halt_gather(p, idx, standing, by)?,
            action_type::BUILD => self.halt_build(p, idx, by)?,
            crate::construction::repair::REPAIR => self.halt_repair(p, idx)?,
            crate::combat::attack::ATTACK => {
                let unit = self.players.live_mut(p, idx)?;
                let attack = unit.actions.list.attack_mut().ok_or(NO_TOP_ACTION)?;
                attack.halt(
                    self.parts.motion_mut(unit.id),
                    &mut unit.sprite,
                    self.streams.get(Stream::Sprite),
                )?;
            }
            crate::ownership::artifact::ARTIFACT_TYPE => self.halt_artifact(p, idx, by)?,
            _ => {
                return Err(GenieError::unsupported(
                    "a stop of an action whose slot 0x38 is not modelled",
                ));
            }
        }
        Ok(true)
    }

    /// [`World::halt_other`] of a Gather: it halts with `standing` (the task's
    /// carrying one while it carries at least 1 of the task's resource); its target and second
    /// target go, released by `by`. `standing` is the master's standing sprite.
    pub(super) fn halt_gather(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        standing: Option<SpriteId>,
        by: Releaser,
    ) -> Result<(), GenieError> {
        let unit = self.players.live_mut(p, idx)?;
        let gather = unit.actions.list.gather_mut().ok_or(NO_TOP_ACTION)?;
        let stand = crate::gathering::hunt::stand_sprite(
            &gather.task,
            standing,
            unit.life.held(),
            &self.graphics,
        );
        let released = gather.halt(
            self.parts.mover_mut(unit.id),
            stand,
            &mut unit.sprite,
            self.streams.get(Stream::Sprite),
        )?;
        for t in released.into_iter().flatten() {
            self.release(t, by)?;
        }
        Ok(())
    }

    /// [`World::halt_other`] of a Build: a counted builder leaves its foundation's builders, the
    /// unit stands, and the foundation and the second target go, released by `by`.
    pub(super) fn halt_build(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        by: Releaser,
    ) -> Result<(), GenieError> {
        let unit = self.players.live_mut(p, idx)?;
        let build = unit.actions.list.build_mut().ok_or(NO_TOP_ACTION)?;
        let foundation = build.target;
        let (counted, targets) = build.halt(self.parts.mover_mut(unit.id))?;
        if counted {
            self.leave_builders(foundation)?;
        }
        self.standing_sprite(p, idx)?;
        for t in targets.into_iter().flatten() {
            self.release(t, by)?;
        }
        Ok(())
    }

    /// [`World::halt_other`] of a Repair (`FUN_140adaa00`): `set_state(2)`, a counted repairer
    /// out of its target's repairers, the standing sprite; the target stays.
    pub(super) fn halt_repair(&mut self, p: PlayerId, idx: ListIndex) -> Result<(), GenieError> {
        let unit = self.players.live_mut(p, idx)?;
        let repair = unit.actions.list.repair_mut().ok_or(NO_TOP_ACTION)?;
        let target = repair.target;
        if repair.halt(self.parts.mover_mut(unit.id))? {
            self.leave_builders(target)?;
        }
        self.standing_sprite(p, idx)
    }

    /// [`World::halt_other`] of an Artifact: its sub-actions and its target go (released by
    /// `by`, the Enter's first), a walking animal stops, and it stands.
    pub(super) fn halt_artifact(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        by: Releaser,
    ) -> Result<(), GenieError> {
        let unit = self.players.live_mut(p, idx)?;
        let artifact = unit.actions.list.artifact_mut().ok_or(NO_TOP_ACTION)?;
        let (gone, released) = artifact.stop();
        if gone.stops {
            self.parts
                .mover_mut(unit.id)
                .ok_or(GenieError::unsupported(
                    "a walking animal without movement state",
                ))?
                .stop()?;
        }
        for t in [gone.released, released].into_iter().flatten() {
            self.release(t, by)?;
        }
        self.standing_sprite(p, idx)
    }
}
