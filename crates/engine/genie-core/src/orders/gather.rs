//! A gather's retarget and take, the quarry, a gather's and a hunt's start.

use crate::GenieError;
use crate::combat::attack::{Arms, Engagement};
use crate::construction::MainBuildingId;
use crate::gathering::DropSiteId;
use crate::gathering::gather::{GatherRetarget, GatherStart};
use crate::gathering::hunt::{HuntAction, HuntMove, HuntStart, HuntTask, Quarry};
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::{Speed, footprint_gap};
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::ObjectState;
use crate::objects::lifecycle::Held;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::action_type;
use crate::objects::masters::class;
use crate::objects::masters::ids::PASTURE;
use crate::objects::masters::ids::PASTURE_MANGROVE;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::release::Releaser;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::ActionKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::task_finder::Diplomacy;
use crate::unit_ai::task_finder::world::TaskWorld;
use crate::world::World;
use crate::world::slots::PlayerSlots;

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
    /// The install clears the action list.
    pub(super) clears: bool,
    /// The work command's unit AI notify runs.
    pub(super) notify: bool,
}

impl World {
    /// The Gather's slot 0x48 (`0x140a903a0`) on the work command's new target
    /// ([`crate::gathering::gather::GatherAction::retarget`]): false when it does not take it, for a building
    /// (type 80 without object flag 0x20) other than a farm, so the work command makes a new
    /// action. The targets that changed are let go (slot 0x180) and taken (slot 0x178). Not
    /// modelled: a building packing or unpacking (its top action 0x7d), a foundation or master
    /// 1893 or 1897 of the unit's player (slots 0x808 and 0x7e8: its build).
    pub(super) fn retarget_gather(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
    ) -> Result<bool, GenieError> {
        let (tp, t) = self.object(target).ok_or(GenieError::unsupported(
            "a gather's target the slice does not hold",
        ))?;
        let tm = t.master(self.master_tables(), tp)?;
        if !self.gatherable_building(player, (tp, t), tm)? {
            return Ok(false);
        }
        let owner = self.players.player(player)?;
        let unit = owner.live(idx)?;
        let me = unit.master(self.master_tables(), player)?;
        let resource = crate::gathering::gather::Resource {
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
        let computer = owner.computer;
        let farm_stops = self.settings.walkable_farms;
        let by = self.releaser(player, idx)?;
        // a stop farm's time per stop: the gatherer's work rate (its master's plus its powerup),
        // capacity and the task's productivity attribute (none: 1.0)
        let productivity = self
            .parts
            .actions_of(unit.id)
            .list
            .gather()
            .and_then(|g| g.task.productivity);
        let rate = crate::gathering::gather::StopRate {
            work_rate: self.parts.combat_of(unit.id).work_rate_powerup + me.work_rate,
            capacity: Amount::of_master(me.capacity),
            productivity: crate::gathering::gather::productivity_of(
                productivity,
                &owner.attributes,
            ),
        };
        // the sprites its MoveTos' constructors take for none (FUN_140aa0d30, FUN_140aa0df0)
        let walks = unit.move_walks(
            self.parts.mover_of(unit.id),
            crate::objects::masters::MasterTables::new(&self.masters, &self.parts.own_master),
            player,
            (
                &self.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
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
        let old = [gather.target, gather.carry_to.map(DropSiteId::id)];
        let rng = self.streams.get(Stream::Action);
        let took = gather.retarget(
            GatherRetarget {
                target: &resource,
                player,
                computer,
                walks,
                farm_stops,
                rate,
            },
            mover,
            rng,
        )?;
        let new = [gather.target, gather.carry_to.map(DropSiteId::id)];
        // a farm another farmer works: taken (slot 0x68) and let go again (set_state 11)
        let waits = matches!(
            gather.state,
            crate::gathering::gather::GatherState::WalkBack { .. }
        ) && new[0].is_none();
        self.exchange_gather_targets(old, new, by)?;
        if waits && old[0] != Some(target) {
            self.take(target, by)?;
            self.release(target, by)?;
        }
        Ok(took)
    }

    /// The Gather's targets that changed (its target and its second target, `old` and `new`): the
    /// old one is let go (slot 0x180) and the new one taken (slot 0x178), each pair in order.
    fn exchange_gather_targets(
        &mut self,
        old: [Option<ObjectId>; 2],
        new: [Option<ObjectId>; 2],
        by: Releaser,
    ) -> Result<(), GenieError> {
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

    /// The Gather's slot 0x40 (`0x140a8b740`, [`crate::gathering::gather::GatherAction::take_move`]) on a
    /// move of the gatherer at `idx` to `point` toward the object `target`: always taken; the
    /// targets that changed are let go and taken.
    pub(crate) fn gather_take_move(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        point: Position,
    ) -> Result<bool, GenieError> {
        let (tp, o) = self.object(target).ok_or(GenieError::unsupported(
            "a move to an object the slice does not hold",
        ))?;
        let tm = o.master(self.master_tables(), tp)?;
        let (at, radius) = (o.position(), tm.radius);
        let unit = &self.players.live(player, idx)?;
        let me = unit.master(self.master_tables(), player)?;
        let moved_to = Some(crate::gathering::gather::MoveTarget {
            id: target,
            player: tp,
            drop_site: self.drops_at_main(player, idx, target)?,
            position: at,
            gap: footprint_gap(unit.position(), me.radius, at, radius),
        });
        let by = self.releaser(player, idx)?;
        let walks = unit.move_walks(
            self.parts.mover_of(unit.id),
            crate::objects::masters::MasterTables::new(&self.masters, &self.parts.own_master),
            player,
            (
                &self.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
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
        let old = [gather.target, gather.carry_to.map(DropSiteId::id)];
        gather.take_move(moved_to, point, player, mover, &walks)?;
        let new = [gather.target, gather.carry_to.map(DropSiteId::id)];
        self.exchange_gather_targets(old, new, by)?;
        Ok(true)
    }

    /// `FUN_14109d4f0` of the unit at `idx` on `target`, or on a building annex's main building
    /// (`+0xbb8`): the unit's master has that master among its drop sites.
    pub(super) fn drops_at_main(
        &self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
    ) -> Result<bool, GenieError> {
        let (tp, t) = self.object(target).ok_or(GenieError::unsupported(
            "a hunt's target the slice does not hold",
        ))?;
        let tm = t.master(self.master_tables(), tp)?;
        let main = if tm.kind == BUILDING {
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
            .drop_sites
            .contains(&main_master))
    }

    /// The Hunt's slot 0x40 from a move toward the object `target` ([`HuntAction::take_move`])
    /// with `point`: true when it takes it. The target it had goes (slot 0x68 with none:
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
    ) -> Result<bool, GenieError> {
        let mut q = self.quarry(player, target)?;
        q.drop_site = self.drops_at_main(player, idx, target)?;
        let quarry = Some(q);
        let gap = self.engagement(player, idx, target)?.gap;
        let by = self.releaser(player, idx)?;
        let unit = self.players.live_mut(player, idx)?;
        let walks = unit.move_walks(
            self.parts.mover_of(unit.id),
            crate::objects::masters::MasterTables::new(&self.masters, &self.parts.own_master),
            player,
            (
                &self.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
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
        if !hunt.take_move(
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
        )? {
            return Ok(false);
        }
        let new_second = hunt.carry_to;
        if let Some(old) = old_target
            && hunt.target.is_none()
        {
            self.release(old, by)?;
        }
        if new_second != old_second
            && let Some(new) = new_second
        {
            if let Some(old) = old_second {
                self.release(old.id(), by)?;
            }
            self.take(new.id(), by)?;
        }
        Ok(true)
    }

    /// What a hunt reads of its target.
    pub(super) fn quarry(&self, player: PlayerId, target: ObjectId) -> Result<Quarry, GenieError> {
        let (tp, t) = self.object(target).ok_or(GenieError::unsupported(
            "a hunt's target the slice does not hold",
        ))?;
        let target_master = t.master(self.master_tables(), tp)?;
        let world = TaskWorld {
            players: &self.players,
            drop_sites: &self.drop_sites,
        };
        Ok(Quarry {
            id: target,
            position: t.position(),
            class: target_master.class,
            kind: target_master.kind,
            state: t.life.state,
            player: tp,
            amount: t.life.amount,
            allied: world.allied(player, tp, true),
            top_action: self.parts.actions_of(t.id).top_action_kind(),
            drop_site: false,
        })
    }

    /// The work command's Gather (its task of action type 5, the base factory `FUN_140a97e00`
    /// case 5 with a target): the constructor ([`crate::gathering::gather::GatherAction::new`]) takes its
    /// target (slot 0x178); the unit AI's notify (slot 0x828: the gather order and action, the
    /// target with its class, the work's point; `notify`'s point and clearing, None for the gather
    /// command's, which has none); the install (slot 0x808) clears the action list and starts the
    /// Gather ([`crate::gathering::gather::GatherAction::start`]). A load to its own drop site takes the
    /// target as the second target, then lets it go as the first.
    pub(super) fn start_gather(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        task: &crate::objects::masters::Task,
        notify: Option<(Position, bool)>,
    ) -> Result<(), GenieError> {
        let (tp, t) = self.object(target).ok_or(GenieError::unsupported(
            "a gather's target the slice does not hold",
        ))?;
        let tm = t.master(self.master_tables(), tp)?;
        let unit = &self.players.live(player, idx)?;
        let me = unit.master(self.master_tables(), player)?;
        let resource = crate::gathering::gather::Resource {
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
            resource: task.attribute_type,
            sprites: task.sprites,
            work_value: task.work_val1,
            productivity: task.attribute_type2,
            kind4: task.attribute_type4,
        };
        // the constructor's time per stop: the gatherer's work rate (its master's plus its
        // powerup, slot 0x6e8), capacity and productivity attribute (none: 1.0)
        let rate = crate::gathering::gather::StopRate {
            work_rate: self.parts.combat_of(unit.id).work_rate_powerup + me.work_rate,
            capacity: Amount::of_master(me.capacity),
            productivity: crate::gathering::gather::productivity_of(
                task.productivity,
                &self.players.player(player)?.attributes,
            ),
        };
        let mut gather = crate::gathering::gather::GatherAction::new(
            task,
            &resource,
            self.settings.walkable_farms,
            rate,
        )?;
        self.take(target, self.releaser(player, idx)?)?;
        if let Some(notify) = notify {
            self.notify_gather(player, idx, (target, resource.class), notify)?;
        }
        self.clear_actions(player, idx)?;
        let computer = self.players.player(player)?.computer;
        let carry_rule = self.settings.carry_rule();
        let unit = self.players.live_mut(player, idx)?;
        let walks = unit.move_walks(
            self.parts.mover_of(unit.id),
            crate::objects::masters::MasterTables::new(&self.masters, &self.parts.own_master),
            player,
            (
                &self.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
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
                computer,
                carry_rule,
                held: &mut held,
                walks,
            },
            mover,
            rng,
        )?;
        Held {
            kind: unit.life.attribute_type,
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
    /// its clearing (`point`, `clears`); the AI remembers the order.
    fn notify_gather(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: (ObjectId, crate::objects::masters::ClassId),
        (point, clears): (Position, bool),
    ) -> Result<(), GenieError> {
        let id = self.players.live(player, idx)?.id;
        let Some(ai) = self.parts.unit_ai.get_mut(id) else {
            return Ok(());
        };
        ai.notify_work(
            Some(OrderKind::Gather),
            Some(ActionKind::Gather),
            Some(target),
            point,
            clears,
            self.streams.get(Stream::Ai),
        )?;
        self.remember_order(
            player,
            idx,
            (Some(OrderKind::Gather), Some(ActionKind::Gather)),
        )
    }

    /// [`crate::combat::attack::arms`] of the unit at `idx` for its hunt's Attack (Hunt's `set_state` 12:
    /// the `MoveTo` shows the hunt's walking sprite, [`HuntTask::walk_sprite_id`]).
    pub(super) fn arms(
        &self,
        player: PlayerId,
        idx: ListIndex,
        task: &HuntTask,
    ) -> Result<Arms, GenieError> {
        let unit = &self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        let moving = task.walk_sprite_id(unit.life.held());
        Ok(crate::combat::attack::arms(
            master,
            &self.graphics,
            task.sprites,
            moving,
        ))
    }

    /// [`crate::combat::attack::engagement`] of the unit at `idx` with `target`.
    pub(super) fn engagement(
        &self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
    ) -> Result<Engagement, GenieError> {
        let unit = &self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        let me = self
            .spatial
            .get(unit.id)
            .ok_or(crate::world::ATTACKER_UNPLACED)?;
        let them = self
            .spatial
            .get_known(target)?
            .ok_or(crate::world::TARGET_UNPLACED)?;
        let speed = self
            .parts
            .mover_of(unit.id)
            .map_or(Speed::ZERO, |m| m.stats.speed);
        crate::combat::attack::engagement(
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
        (tp, t): (PlayerId, &crate::objects::unit::Unit),
        tm: &crate::objects::masters::Master,
    ) -> Result<bool, GenieError> {
        if tm.kind == BUILDING {
            // The action type of a packed siege unit unpacking (an Unpack, 125, `0x7d`), whose target a gather drops
            // (`FUN_140a903a0`).
            if self.parts.actions_of(t.id).top_action_kind() == Some(action_type::UNPACK)
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
                    || matches!(main_master, PASTURE_MANGROVE | PASTURE))
                    && mp == player
                {
                    return Err(GenieError::unsupported(
                        "a gather retargeted to its player's foundation (its build)",
                    ));
                }
                if tm.class != class::FARM {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// The Hunt from the factory (`FUN_140aca4b0` case 0x6e): the action takes its target (slot
    /// 0x178); the work command's unit AI notify (`FUN_14106e4e0`, `notify`) sets the hunt order;
    /// the install (`FUN_14106e990`) clears the action list and starts the hunt.
    pub(super) fn start_hunt(&mut self, order: HuntOrder) -> Result<(), GenieError> {
        let HuntOrder {
            player,
            index: idx,
            target,
            task,
            point,
            clears,
            notify,
        } = order;
        let mut quarry = self.quarry(player, target)?;
        quarry.drop_site = self.drops_at_main(player, idx, target)?;
        self.take(target, self.releaser(player, idx)?)?;
        let unit = self.players.live_mut(player, idx)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id).filter(|_| notify) {
            ai.notify_work(
                Some(OrderKind::Hunt),
                Some(ActionKind::Hunt),
                Some((target, quarry.class)),
                point,
                clears,
                self.streams.get(Stream::Ai),
            )?;
            self.remember_order(player, idx, (Some(OrderKind::Hunt), Some(ActionKind::Hunt)))?;
        }
        self.clear_actions(player, idx)?;
        self.begin_hunt(player, idx, (task, &quarry))
    }

    /// [`World::start_hunt`]'s start of the Hunt action itself, once the unit's actions are
    /// cleared: the engagement and arms, the action's start and its first sprite, and a load to its
    /// own drop site.
    ///
    /// # Errors
    ///
    /// What the start does not model.
    fn begin_hunt(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (task, quarry): (HuntTask, &Quarry),
    ) -> Result<(), GenieError> {
        let target = quarry.id;
        let engagement = self.engagement(player, idx, target)?;
        let arms = self.arms(player, idx, &task)?;
        let computer = self.players.player(player)?.computer;
        let carry_rule = self.settings.carry_rule();
        let unit = self.players.live_mut(player, idx)?;
        let walks = unit.move_walks(
            self.parts.mover_of(unit.id),
            crate::objects::masters::MasterTables::new(&self.masters, &self.parts.own_master),
            player,
            (
                &self.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
        )?;
        let mover = self
            .parts
            .mover_mut(unit.id)
            .ok_or(crate::world::HUNTER_UNMOVING)?;
        let mut held = unit.life.held();
        let (hunt, arrived) = HuntAction::start(
            HuntStart {
                task,
                target: quarry,
                player,
                computer,
                carry_rule,
                held: &mut held,
                walks,
            },
            &engagement,
            arms,
            mover,
        )?;
        Held {
            kind: unit.life.attribute_type,
            amount: unit.life.amount,
        } = held;
        // an Attack whose MoveTo started arrived: the unit shows the idle sprite (slot 0x68) and
        // its `+0x7b8` clears (`Attack::arrived_sprite`)
        if arrived {
            let g = arms.idle_sprite.ok_or(crate::world::NO_STANDING_SPRITE)?;
            crate::objects::sprite::new_sprite(
                &mut unit.sprite,
                self.parts.move_stats_mut(unit.id),
                g,
                self.streams.get(Stream::Sprite),
            );
            self.parts.set_anim_speed(unit.id, Speed::ZERO);
        }
        let drops = hunt.target.is_none() && hunt.carry_to.key() == Some(target);
        self.parts.actions_mut(unit.id)?.list.set_hunt(Some(hunt));
        // a load to its own drop site: the target taken as the second (FUN_140a7a190), then let
        // go as the first (slot 0x68 with none)
        if drops {
            self.take(target, self.releaser(player, idx)?)?;
            let by = self.releaser(player, idx)?;
            self.release(target, by)?;
        }
        Ok(())
    }
}
