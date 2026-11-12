//! A gather's retarget and take, the quarry, a gather's and a hunt's start.

use crate::combat::attack::{Arms, Engagement};
use crate::combat::attack::{arms as attack_arms, engagement as attack_engagement};
use crate::construction::MainBuildingId;
use crate::gathering::gather::GATHERER_STATE_LACKING;
use crate::gathering::gather::GATHERER_UNMOVING;
use crate::gathering::gather::unit::WalkSprites;
use crate::gathering::gather::{GatherAction, MoveTarget, Resource, StopRate, productivity_of};
use crate::gathering::gather::{GatherOpening, GatherStart};
use crate::gathering::hunt::HUNTER_STATE_LACKING;
use crate::gathering::hunt::{HuntMove, HuntTask, Quarry};
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::footprint_gap;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::lifecycle::Held;
use crate::objects::masters::MasterKind;
use crate::objects::masters::{ClassId, MasterTables, Task};
use crate::objects::release::Releaser;
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
use crate::world::slots::PlayerSlotsExt;
use crate::world::{ATTACKER_UNPLACED, TARGET_UNPLACED};
use crate::{GenieError, GenieResult};

/// A gather's target the slice does not hold.
const GATHER_TARGET_NOT_HELD: GenieError =
    GenieError::unsupported("a gather's target the slice does not hold");
/// A hunt's target the slice does not hold.
const HUNT_TARGET_NOT_HELD: GenieError =
    GenieError::unsupported("a hunt's target the slice does not hold");

mod hunt;
mod retarget;

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
        let (tp, target_unit) = self.object(target).ok_or(GenieError::unsupported(
            "a move to an object the slice does not hold",
        ))?;
        let tm = target_unit.master(self.master_tables(), tp)?;
        let (at, radius) = (target_unit.position(), tm.radius);
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
            self.top_speed_terms(player, unit)?,
            WalkSprites {
                every: MasterTables::new(&self.model.masters, &self.parts.own_master),
                player,
                graphics: &self.model.graphics,
                look: self.parts.combat_of(unit.id).abilities.look.as_ref(),
            },
            &self.model.roles,
        )?;
        let unit = self.players.live_mut(player, idx)?;
        let (Some(gather), Some(mover)) = (
            self.parts.actions.of_mut(unit.id)?.list.gather_mut(),
            self.parts.motion.mover_mut(unit.id),
        ) else {
            return Err(GATHERER_STATE_LACKING);
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
        let (tp, target_unit) = self.object(target).ok_or(HUNT_TARGET_NOT_HELD)?;
        let tm = target_unit.master(self.master_tables(), tp)?;
        let main = if tm.kind == MasterKind::BUILDING {
            self.parts
                .building_of(target_unit.id)
                .annex_of
                .map_or(target, MainBuildingId::id)
        } else {
            target
        };
        let main_master = match self.object(main) {
            Some((mp, unit)) => unit.master(self.master_tables(), mp)?.id,
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
        let mut quarry = self.quarry(player, target)?;
        quarry.drop_site = self.drops_at_main(player, idx, target)?;
        let quarry = Some(quarry);
        let gap = self.engagement(player, idx, target)?.gap;
        let by = self.releaser(player, idx)?;
        let top_speed = self.top_speed_terms(player, self.players.live(player, idx)?)?;
        let unit = self.players.live_mut(player, idx)?;
        let walks = unit.move_walks(
            self.parts.mover_of(unit.id),
            top_speed,
            WalkSprites {
                every: MasterTables::new(&self.model.masters, &self.parts.own_master),
                player,
                graphics: &self.model.graphics,
                look: self.parts.combat_of(unit.id).abilities.look.as_ref(),
            },
            &self.model.roles,
        )?;
        let my_pasture = unit.my_pasture;
        let (Some(hunt), Some(mover)) = (
            self.parts.actions.of_mut(unit.id)?.list.hunt_mut(),
            self.parts.motion.mover_mut(unit.id),
        ) else {
            return Err(HUNTER_STATE_LACKING);
        };
        let (old_target, old_second) = (hunt.target, hunt.carry_to);
        if hunt.take_move(
            HuntMove {
                target: quarry.as_ref(),
                point,
                player,
                walks,
                drops_here: quarry.as_ref().is_some_and(|quarry| quarry.drop_site),
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
        let (tp, unit) = self.object(target).ok_or(HUNT_TARGET_NOT_HELD)?;
        let target_master = unit.master(self.master_tables(), tp)?;
        let world = TaskWorld {
            players: &self.players,
            treaty: self.treaty.as_ref(),
            drop_sites: &self.model.drop_sites,
        };
        Ok(Quarry {
            id: target,
            position: unit.position(),
            class: target_master.class,
            kind: target_master.kind,
            state: unit.life.state,
            player: tp,
            amount: unit.life.amount,
            allied: world.allied(player, tp, AllyTest::WithRelation4),
            top_action: self.parts.actions_of(unit.id).top_action_kind(),
            drop_site: false,
        })
    }

    /// The work command's Gather (its task of action type 5, the base factory `FUN_140a97e00` case
    /// 5 with a target): the constructor ([`crate::gathering::gather::GatherAction::new`]) takes
    /// its target (slot 0x178) where it holds it (none past state 6); the unit AI's notify (slot
    /// 0x828: the gather order and action, the target with its class, the work's point;
    /// `notify`'s point and clearing, None for the gather command's, which has none); the
    /// install (slot 0x808, [`World::set_only_action`]) clears the action list and starts the
    /// Gather ([`crate::gathering::gather::GatherAction::start`]), or deletes it unstarted for a
    /// gatherer in state 3 or later (the target it took let go). A load to its own drop site
    /// takes the target as the second target, then lets it go as the first.
    pub(super) fn start_gather(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        task: &Task,
        notify: Option<(Position, Notifies)>,
    ) -> GenieResult<()> {
        let (tp, target_unit) = self.object(target).ok_or(GATHER_TARGET_NOT_HELD)?;
        let tm = target_unit.master(self.master_tables(), tp)?;
        let unit = &self.players.live(player, idx)?;
        let me = unit.master(self.master_tables(), player)?;
        let resource = Resource {
            id: target,
            position: target_unit.position(),
            class: tm.class,
            master: tm.id,
            player: tp,
            workers: target_unit.workers,
            drop_site: self.drops_at_main(player, idx, target)?,
            gap: footprint_gap(
                unit.position(),
                me.radius,
                target_unit.position(),
                tm.radius,
            ),
            state: target_unit.life.state,
            radius: tm.radius,
        };
        let task = HuntTask::of(task);
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
        let gather = GatherAction::new(
            task,
            &resource,
            self.settings.farm_mode,
            rate,
            &self.model.roles,
        );
        if gather.target.is_some() {
            self.take(target, self.releaser(player, idx)?)?;
        }
        if let Some(notify) = notify {
            self.notify_gather(player, idx, (target, resource.class), notify)?;
        }
        let Some(gather) = self.set_only_action(player, idx, Some(gather))? else {
            return Ok(());
        };
        self.install_gather(player, idx, target, gather, &resource)
    }

    /// [`World::start_gather`]'s start after the install (slot 0x808) cleared the list: the
    /// `gather`'s start on `resource` with the gatherer's walks (its top speed's terms, slot
    /// 0x208) and load; a load to its own drop site takes `target` as the second target, then
    /// lets it go as the first, as a farm another farmer works is let go. A Gather without a
    /// target searches on the gatherer's step ([`Unit::gather_begin_search`]).
    ///
    /// [`Unit::gather_begin_search`]: crate::objects::unit::Unit::gather_begin_search
    ///
    /// # Errors
    ///
    /// A gatherer without movement state, and what the start does not model.
    fn install_gather(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        mut gather: GatherAction,
        resource: &Resource,
    ) -> GenieResult<()> {
        let player_kind = self.players.player(player)?.kind;
        let carry_rule = self.settings.carry_rule();
        let top_speed = self.top_speed_terms(player, self.players.live(player, idx)?)?;
        let unit = self.players.live_mut(player, idx)?;
        let walks = unit.move_walks(
            self.parts.mover_of(unit.id),
            top_speed,
            WalkSprites {
                every: MasterTables::new(&self.model.masters, &self.parts.own_master),
                player,
                graphics: &self.model.graphics,
                look: self.parts.combat_of(unit.id).abilities.look.as_ref(),
            },
            &self.model.roles,
        )?;
        let mover = self.parts.mover_mut(unit.id).ok_or(GATHERER_UNMOVING)?;
        let mut held = unit.life.held();
        let rng = self.streams.get_mut(Stream::Action);
        let opening = gather.start(
            GatherStart {
                target: resource,
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
        if opening == GatherOpening::Searches {
            return self.run_on_step(player, idx, |unit, actions, step| {
                unit.gather_begin_search(gather, actions, step)
            });
        }
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
            self.streams.get_mut(Stream::Ai),
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
        let speed = self.top_speed(player, unit)?;
        attack_engagement(
            self.parts.unit_ai.get(unit.id),
            speed,
            master,
            me,
            them,
            self.world_time,
        )
    }
}
