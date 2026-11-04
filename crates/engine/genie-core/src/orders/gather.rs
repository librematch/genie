//! A gather's retarget and take, the quarry, a gather's and a hunt's start.

use crate::combat::attack::{Arms, Engagement};
use crate::combat::attack::{arms as attack_arms, engagement as attack_engagement};
use crate::construction::MainBuildingId;
use crate::gathering::gather::{
    GatherAction, GatherState, MoveTarget, Resource, StopRate, productivity_of,
};
use crate::gathering::gather::{GatherRetarget, GatherStart};
use crate::gathering::hunt::{HuntMove, HuntTask, Quarry};
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::{Speed, footprint_gap};
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::ObjectState;
use crate::objects::lifecycle::Held;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::{ClassId, Master, MasterTables, Task};
use crate::objects::release::Releaser;
use crate::objects::unit::Unit;
use crate::players::AllyTest;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::Answer;
use crate::unit_ai::Notifies;
use crate::unit_ai::OrderKind;
use crate::unit_ai::task_finder::Diplomacy;
use crate::unit_ai::task_finder::world::TaskWorld;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::slots::PlayerSlots;
use crate::world::{ATTACKER_UNPLACED, TARGET_UNPLACED};
use crate::{GenieError, GenieResult};

mod hunt;

/// The Hunt a work command starts ([`World::start_hunt`]'s arguments).
#[derive(Clone, Copy, Debug)]
pub(super) struct HuntOrder {
    /// The hunter's player.
    pub(super) player: PlayerId,
    /// The hunter's index in the player's objects.
    pub(super) index: ListIndex,
    /// The target.
    pub(super) target: ObjectId,
    /// The task.
    pub(super) task: HuntTask,
    /// The command's point.
    pub(super) point: Position,
    /// Whether the work command's notify clears the unit AI's notifies.
    pub(super) notifies: Notifies,
    /// The work command's unit AI notify runs.
    pub(super) notify: bool,
}

impl World {
    /// The Gather's slot 0x48 (`0x140a903a0`) on the work command's new target
    /// ([`crate::gathering::gather::GatherAction::retarget`]): not taken for for a building
    /// (type 80 without object flag 0x20) other than a farm, so the work command makes a new
    /// action. The targets that changed are let go (slot 0x180) and taken (slot 0x178). Not
    /// modelled: a building packing or unpacking (its top action 0x7d), a foundation or master
    /// 1893 or 1897 of the unit's player (slots 0x808 and 0x7e8: its build).
    pub(super) fn retarget_gather(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
    ) -> GenieResult<Answer> {
        let (tp, t) = self.object(target).ok_or(GenieError::unsupported(
            "a gather's target the slice does not hold",
        ))?;
        let tm = t.master(self.master_tables(), tp)?;
        if !self.gatherable_building(player, (tp, t), tm)? {
            return Ok(Answer::NotTaken);
        }
        let owner = self.players.player(player)?;
        let unit = owner.live(idx)?;
        let me = unit.master(self.master_tables(), player)?;
        let resource = Resource {
            id: target,
            position: t.position(),
            class: tm.class,
            master: tm.id,
            player: tp,
            workers: t.workers,
            drop_site: self.drops_at_main(player, idx, target)?,
            gap: footprint_gap(unit.position(), me.radius, t.position(), tm.radius),
            state: t.life.state,
            radius: tm.radius,
        };
        let player_kind = owner.kind;
        let farm_mode = self.settings.farm_mode;
        let by = self.releaser(player, idx)?;
        // a stop farm's time per stop: the gatherer's work rate (its master's plus its powerup),
        // capacity and the task's productivity attribute (none: 1.0)
        let productivity = self
            .parts
            .actions_of(unit.id)
            .list
            .gather()
            .and_then(|g| g.task.productivity);
        let rate = StopRate {
            work_rate: self.parts.combat_of(unit.id).work_rate_powerup + me.action_part().work_rate,
            capacity: Amount::of_master(me.capacity),
            productivity: productivity_of(
                productivity,
                &owner.attributes,
                &self.model.tuning.gather,
            ),
        };
        // the sprites its MoveTos' constructors take for none (FUN_140aa0d30, FUN_140aa0df0)
        let walks = unit.move_walks(
            self.parts.mover_of(unit.id),
            MasterTables::new(&self.model.masters, &self.parts.own_master),
            player,
            (
                &self.model.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
            &self.model.roles,
        )?;
        let unit = self.players.live_mut(player, idx)?;
        let (Some(gather), Some(mover)) = (
            self.parts.actions.of_mut(unit.id)?.list.gather_mut(),
            self.parts.motion.mover_mut(unit.id),
        ) else {
            return Err(GenieError::unsupported(
                "a gatherer without its gather or movement state",
            ));
        };
        let old = gather.targets();
        let rng = self.streams.get(Stream::Action);
        gather.retarget(
            GatherRetarget {
                target: &resource,
                player,
                player_kind,
                walks,
                farm_mode,
                rate,
            },
            mover,
            rng,
            &self.model.roles,
            &self.model.tuning,
        )?;
        let new = gather.targets();
        // a farm another farmer works: taken (slot 0x68) and let go again (set_state 11)
        let waits = matches!(gather.state, GatherState::WalkBack { .. }) && new[0].is_none();
        self.exchange_gather_targets(old, new, by)?;
        if waits && old[0] != Some(target) {
            self.take(target, by)?;
            self.release(target, by)?;
        }
        Ok(Answer::Taken)
    }

    /// The Gather's targets that changed (its target and its second target, `old` and `new`): the
    /// old one is let go (slot 0x180) and the new one taken (slot 0x178), each pair in order.
    fn exchange_gather_targets(
        &mut self,
        old: [Option<ObjectId>; 2],
        new: [Option<ObjectId>; 2],
        by: Releaser,
    ) -> GenieResult<()> {
        for (old, new) in old.into_iter().zip(new) {
            if old != new {
                if let Some(old) = old {
                    self.release(old, by)?;
                }
                if let Some(new) = new {
                    self.take(new, by)?;
                }
            }
        }
        Ok(())
    }

    /// The Gather's slot 0x40 (`0x140a8b740`,
    /// [`crate::gathering::gather::GatherAction::take_move`]) on a move of the gatherer at `idx` to
    /// `point` toward the object `target`: always taken; the targets that changed are let go and
    /// taken.
    pub(crate) fn gather_take_move(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        point: Position,
    ) -> GenieResult<Answer> {
        let (tp, o) = self.object(target).ok_or(GenieError::unsupported(
            "a move to an object the slice does not hold",
        ))?;
        let tm = o.master(self.master_tables(), tp)?;
        let (at, radius) = (o.position(), tm.radius);
        let unit = &self.players.live(player, idx)?;
        let me = unit.master(self.master_tables(), player)?;
        let moved_to = Some(MoveTarget {
            id: target,
            player: tp,
            drop_site: self.drops_at_main(player, idx, target)?,
            position: at,
            gap: footprint_gap(unit.position(), me.radius, at, radius),
        });
        let by = self.releaser(player, idx)?;
        let walks = unit.move_walks(
            self.parts.mover_of(unit.id),
            MasterTables::new(&self.model.masters, &self.parts.own_master),
            player,
            (
                &self.model.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
            &self.model.roles,
        )?;
        let unit = self.players.live_mut(player, idx)?;
        let (Some(gather), Some(mover)) = (
            self.parts.actions.of_mut(unit.id)?.list.gather_mut(),
            self.parts.motion.mover_mut(unit.id),
        ) else {
            return Err(GenieError::unsupported(
                "a gatherer without its gather or movement state",
            ));
        };
        let old = gather.targets();
        gather.take_move(
            moved_to,
            point,
            player,
            mover,
            &walks,
            &self.model.tuning.move_to,
        )?;
        let new = gather.targets();
        self.exchange_gather_targets(old, new, by)?;
        Ok(Answer::Taken)
    }

    /// `FUN_14109d4f0` of the unit at `idx` on `target`, or on a building annex's main building
    /// (`+0xbb8`): the unit's master has that master among its drop sites.
    pub(super) fn drops_at_main(
        &self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
    ) -> GenieResult<bool> {
        let (tp, t) = self.object(target).ok_or(GenieError::unsupported(
            "a hunt's target the slice does not hold",
        ))?;
        let tm = t.master(self.master_tables(), tp)?;
        let main = if tm.kind == MasterKind::BUILDING {
            self.parts
                .building_of(t.id)
                .annex_of
                .map_or(target, MainBuildingId::id)
        } else {
            target
        };
        let main_master = match self.object(main) {
            Some((mp, m)) => m.master(self.master_tables(), mp)?.id,
            None => tm.id,
        };
        let unit = &self.players.live(player, idx)?;
        Ok(unit
            .master(self.master_tables(), player)?
            .action_part()
            .drop_sites
            .contains(&main_master))
    }

    /// The Hunt's slot 0x40 from a move toward the object `target` ([`HuntAction::take_move`])
    /// with `point`: taken when it takes it. The target it had goes (slot 0x68 with none:
    /// [`World::release`]), and a drop site it carries its load to becomes its second target
    /// (`FUN_140a7a190`: [`World::take`]).
    ///
    /// [`HuntAction::take_move`]: crate::gathering::hunt::HuntAction::take_move
    pub(crate) fn hunt_take_move(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        point: Position,
    ) -> GenieResult<Answer> {
        let mut q = self.quarry(player, target)?;
        q.drop_site = self.drops_at_main(player, idx, target)?;
        let quarry = Some(q);
        let gap = self.engagement(player, idx, target)?.gap;
        let by = self.releaser(player, idx)?;
        let unit = self.players.live_mut(player, idx)?;
        let walks = unit.move_walks(
            self.parts.mover_of(unit.id),
            MasterTables::new(&self.model.masters, &self.parts.own_master),
            player,
            (
                &self.model.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
            &self.model.roles,
        )?;
        let my_pasture = unit.my_pasture;
        let (Some(hunt), Some(mover)) = (
            self.parts.actions.of_mut(unit.id)?.list.hunt_mut(),
            self.parts.motion.mover_mut(unit.id),
        ) else {
            return Err(GenieError::unsupported(
                "a hunter without its hunt or movement state",
            ));
        };
        let (old_target, old_second) = (hunt.target, hunt.carry_to);
        if hunt.take_move(
            HuntMove {
                target: quarry.as_ref(),
                point,
                player,
                walks,
                drops_here: quarry.as_ref().is_some_and(|q| q.drop_site),
                my_pasture,
                gap,
            },
            mover,
            &self.model.tuning.move_to,
        )? == Answer::NotTaken
        {
            return Ok(Answer::NotTaken);
        }
        let new_second = hunt.carry_to;
        if let Some(old) = old_target
            && hunt.target.is_none()
        {
            self.release(old.id(), by)?;
        }
        if new_second != old_second
            && let Some(new) = new_second
        {
            if let Some(old) = old_second {
                self.release(old.id(), by)?;
            }
            self.take(new.id(), by)?;
        }
        Ok(Answer::Taken)
    }

    /// What a hunt reads of its target.
    pub(super) fn quarry(&self, player: PlayerId, target: ObjectId) -> GenieResult<Quarry> {
        let (tp, t) = self.object(target).ok_or(GenieError::unsupported(
            "a hunt's target the slice does not hold",
        ))?;
        let target_master = t.master(self.master_tables(), tp)?;
        let world = TaskWorld {
            players: &self.players,
            drop_sites: &self.model.drop_sites,
        };
        Ok(Quarry {
            id: target,
            position: t.position(),
            class: target_master.class,
            kind: target_master.kind,
            state: t.life.state,
            player: tp,
            amount: t.life.amount,
            allied: world.allied(player, tp, AllyTest::WithRelation4),
            top_action: self.parts.actions_of(t.id).top_action_kind(),
            drop_site: false,
        })
    }

    /// The work command's Gather (its task of action type 5, the base factory `FUN_140a97e00` case
    /// 5 with a target): the constructor ([`crate::gathering::gather::GatherAction::new`]) takes
    /// its target (slot 0x178); the unit AI's notify (slot 0x828: the gather order and action, the
    /// target with its class, the work's point; `notify`'s point and clearing, None for the gather
    /// command's, which has none); the install (slot 0x808) clears the action list and starts the
    /// Gather ([`crate::gathering::gather::GatherAction::start`]). A load to its own drop site
    /// takes the target as the second target, then lets it go as the first.
    pub(super) fn start_gather(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        task: &Task,
        notify: Option<(Position, Notifies)>,
    ) -> GenieResult<()> {
        let (tp, t) = self.object(target).ok_or(GenieError::unsupported(
            "a gather's target the slice does not hold",
        ))?;
        let tm = t.master(self.master_tables(), tp)?;
        let unit = &self.players.live(player, idx)?;
        let me = unit.master(self.master_tables(), player)?;
        let resource = Resource {
            id: target,
            position: t.position(),
            class: tm.class,
            master: tm.id,
            player: tp,
            workers: t.workers,
            drop_site: self.drops_at_main(player, idx, target)?,
            gap: footprint_gap(unit.position(), me.radius, t.position(), tm.radius),
            state: t.life.state,
            radius: tm.radius,
        };
        let task = HuntTask {
            id: task.id,
            range: task.work_range,
            resource: task.attribute_kind,
            sprites: task.sprites,
            work_value: task.work_val1,
            productivity: task.attribute_kind2,
            kind4: task.attribute_kind4,
        };
        // the constructor's time per stop: the gatherer's work rate (its master's plus its
        // powerup, slot 0x6e8), capacity and productivity attribute (none: 1.0)
        let rate = StopRate {
            work_rate: self.parts.combat_of(unit.id).work_rate_powerup + me.action_part().work_rate,
            capacity: Amount::of_master(me.capacity),
            productivity: productivity_of(
                task.productivity,
                &self.players.player(player)?.attributes,
                &self.model.tuning.gather,
            ),
        };
        let mut gather = GatherAction::new(
            task,
            &resource,
            self.settings.farm_mode,
            rate,
            &self.model.roles,
        )?;
        self.take(target, self.releaser(player, idx)?)?;
        if let Some(notify) = notify {
            self.notify_gather(player, idx, (target, resource.class), notify)?;
        }
        self.clear_actions(player, idx)?;
        let player_kind = self.players.player(player)?.kind;
        let carry_rule = self.settings.carry_rule();
        let unit = self.players.live_mut(player, idx)?;
        let walks = unit.move_walks(
            self.parts.mover_of(unit.id),
            MasterTables::new(&self.model.masters, &self.parts.own_master),
            player,
            (
                &self.model.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
            &self.model.roles,
        )?;
        let mover = self
            .parts
            .mover_mut(unit.id)
            .ok_or(GenieError::unsupported("a gatherer without movement state"))?;
        let mut held = unit.life.held();
        let rng = self.streams.get(Stream::Action);
        gather.start(
            GatherStart {
                target: &resource,
                player,
                player_kind,
                carry_rule,
                held: &mut held,
                walks,
            },
            mover,
            rng,
            &self.model.roles,
            &self.model.tuning,
        )?;
        Held {
            kind: unit.life.attribute_kind,
            amount: unit.life.amount,
        } = held;
        let drops = gather.target.is_none() && gather.carry_to.key() == Some(target);
        // a farm another farmer works: the start let it go (set_state 11, then slot 0x68)
        let waits = gather.target.is_none() && gather.carry_to.key() != Some(target);
        self.parts
            .actions_mut(unit.id)?
            .list
            .set_gather(Some(gather));
        if drops {
            self.take(target, self.releaser(player, idx)?)?;
        }
        if drops || waits {
            let by = self.releaser(player, idx)?;
            self.release(target, by)?;
        }
        Ok(())
    }

    /// [`World::start_gather`]'s notify of the gatherer's unit AI, where it has one (slot 0x828):
    /// the gather order and action, the target `target` with its class, the work's point and
    /// its clearing (`point`, `notifies`); the AI remembers the order.
    fn notify_gather(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: (ObjectId, ClassId),
        (point, notifies): (Position, Notifies),
    ) -> GenieResult<()> {
        let id = self.players.live(player, idx)?.id;
        let Some(ai) = self.parts.unit_ai.get_mut(id) else {
            return Ok(());
        };
        ai.notify_work(
            Some(OrderKind::Gather),
            Some(AiAction::Gather),
            Some(target),
            point,
            notifies,
            self.streams.get(Stream::Ai),
        )?;
        self.remember_order(
            player,
            idx,
            (Some(OrderKind::Gather), Some(AiAction::Gather)),
        )
    }

    /// [`crate::combat::attack::arms`] of the unit at `idx` for its hunt's Attack (Hunt's
    /// `set_state` 12: the `MoveTo` shows the hunt's walking sprite, [`HuntTask::walk_sprite_id`]).
    pub(super) fn arms(
        &self,
        player: PlayerId,
        idx: ListIndex,
        task: &HuntTask,
    ) -> GenieResult<Arms> {
        let unit = &self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        let moving = task.walk_sprite_id(unit.life.held());
        Ok(attack_arms(
            master,
            &self.model.graphics,
            task.sprites,
            moving,
            &self.model.roles,
        ))
    }

    /// [`crate::combat::attack::engagement`] of the unit at `idx` with `target`.
    pub(super) fn engagement(
        &self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
    ) -> GenieResult<Engagement> {
        let unit = &self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        let me = self.spatial.get(unit.id).ok_or(ATTACKER_UNPLACED)?;
        let them = self.spatial.get_known(target)?.ok_or(TARGET_UNPLACED)?;
        let speed = self
            .parts
            .mover_of(unit.id)
            .map_or(Speed::ZERO, |m| m.stats.speed);
        attack_engagement(
            self.parts.unit_ai.get(unit.id),
            speed,
            master,
            me,
            them,
            self.world_time,
        )
    }

    /// [`World::retarget_gather`]'s test of a target that is a building: whether the gather goes
    /// on (a building it can gather from, or a farm).
    ///
    /// # Errors
    ///
    /// A building with an action, a foundation of the unit's player, or an annex whose main
    /// building is gone: not modelled.
    fn gatherable_building(
        &self,
        player: PlayerId,
        (tp, t): (PlayerId, &Unit),
        tm: &Master,
    ) -> GenieResult<bool> {
        if tm.kind == MasterKind::BUILDING {
            // The action type of a packed siege unit unpacking (an Unpack, 125, `0x7d`), whose target a gather drops
            // (`FUN_140a903a0`).
            if self.parts.actions_of(t.id).top_action_kind() == Some(ActionKind::UNPACK)
                || self.parts.unread.contains(t.id)
            {
                return Err(GenieError::unsupported(
                    "a gather retargeted to a building with an action (packing)",
                ));
            }
            if !tm.object_flags.contains(ObjectFlags::GATHERED_BUILDING) {
                let (mp, main) = match self.parts.building_of(t.id).annex_of {
                    Some(m) => self.object(m).ok_or(GenieError::unsupported(
                        "an annex whose main building is gone",
                    ))?,
                    None => (tp, t),
                };
                let main_master = main.master(self.master_tables(), mp)?.id;
                if (main.life.state < ObjectState::Alive
                    || self
                        .model
                        .roles
                        .of_master(main_master)
                        .intersects(MasterRoles::PASTURE_MANGROVE | MasterRoles::PASTURE))
                    && mp == player
                {
                    return Err(GenieError::unsupported(
                        "a gather retargeted to its player's foundation (its build)",
                    ));
                }
                if !self
                    .model
                    .roles
                    .of_class(tm.class)
                    .contains(ClassRoles::FARM)
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
