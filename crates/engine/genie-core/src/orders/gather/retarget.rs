//! The Gather's slot 0x48 on a command's new target: a walk to a point without one, the target it
//! keeps, its player's foundation it builds, a building other than a farm it refuses, else the
//! Gather's retarget.

use crate::GenieResult;
use crate::construction::MAIN_BUILDING_GONE;
use crate::gathering::gather::GATHERER_STATE_LACKING;
use crate::gathering::gather::unit::WalkSprites;
use crate::gathering::gather::{GatherRetarget, GatherState, Resource, StopRate, productivity_of};
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::footprint_gap;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MasterTables;
use crate::objects::masters::ObjectFlags;
use crate::orders::Work;
use crate::orders::gather::GATHER_TARGET_NOT_HELD;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::Answer;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::slots::PlayerSlotsExt;

/// What slot 0x48's building test (`0x140a90531`) makes of its target.
#[derive(Clone, Copy, Debug, PartialEq)]
enum GatheredBuilding {
    /// Its player's foundation or pasture (an annex's main building), which the gatherer builds
    /// at its place.
    Builds(ObjectId, Position),
    /// A building whose main building is no farm: not taken.
    Refused,
    /// A target the Gather takes. The gatherer's drop site test (`FUN_14109d4f0`) reads this
    /// master: a building's main building's, else the target's own.
    Gathers(MasterId),
}

impl World {
    /// The Gather's slot 0x48 (`0x140a903a0`, `RGE_Action_Gather`'s slot 9) on the new target
    /// `target` of the gatherer at `idx`, with `point` (the gather command's target's place, the
    /// work command's point): the gathering is not started any more (`+0x80` cleared first,
    /// whatever follows). Without a target, or on a building under its Unpack
    /// ([`World::unpacked`]), the Gather walks to `point` ([`World::gather_to_point`]): 1. A
    /// target it keeps ([`GatherAction::keeps`]) changes nothing: 1. Its player's foundation or
    /// pasture is built ([`World::gathered_building`], [`World::build_gathered`]): 1; a building
    /// whose main building is no farm is refused: 0, and the work command makes a new action.
    /// Else the Gather takes the target ([`World::retarget_gather_to`]): 1.
    ///
    /// [`GatherAction::keeps`]: crate::gathering::gather::GatherAction::keeps
    ///
    /// # Errors
    ///
    /// A gatherer without its Gather or its mover; a target the world does not hold; what the
    /// Gather's walks and the work command do not model.
    pub(in crate::orders) fn retarget_gather(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: Option<ObjectId>,
        point: Position,
    ) -> GenieResult<Answer> {
        let id = self.players.live(player, idx)?.id;
        self.parts
            .actions
            .of_mut(id)?
            .list
            .gather_mut()
            .ok_or(GATHERER_STATE_LACKING)?
            .worked = false;
        let Some(target) = self.unpacked(target)? else {
            return self.gather_to_point(player, idx, point);
        };
        let player_kind = self.players.player(player)?.kind;
        if self
            .parts
            .actions_of(id)
            .list
            .gather()
            .is_some_and(|gather_action| gather_action.keeps(target, player_kind))
        {
            return Ok(Answer::Taken);
        }
        match self.gathered_building(player, target)? {
            GatheredBuilding::Builds(main, at) => self.build_gathered(player, idx, main, at),
            GatheredBuilding::Refused => Ok(Answer::NotTaken),
            GatheredBuilding::Gathers(drops_at) => {
                self.retarget_gather_to(player, idx, target, drops_at)
            }
        }
    }

    /// Slot 0x48's first test (`0x140a903ef`): `target`, none for none and for a building
    /// (`TRIBE_Building_Object`, type 80) under its Unpack (its action list's head of type
    /// `0x7d`, [`ActionKind::UNPACK`]; the game logs "`obj_in=%d` is packing/unpacking").
    ///
    /// # Errors
    ///
    /// A target the world does not hold; a building the load could not read whole, which may be
    /// under its Unpack (its [`crate::world::Unread::error`]).
    fn unpacked(&self, target: Option<ObjectId>) -> GenieResult<Option<ObjectId>> {
        let Some(target) = target else {
            return Ok(None);
        };
        let (owner, unit) = self.object(target).ok_or(GATHER_TARGET_NOT_HELD)?;
        if unit.master(self.master_tables(), owner)?.kind != MasterKind::BUILDING {
            return Ok(Some(target));
        }
        if let Some(unread) = self.parts.unread.get(unit.id) {
            return Err(unread.error());
        }
        let unpacking =
            self.parts.actions_of(unit.id).top_action_kind() == Some(ActionKind::UNPACK);
        Ok((!unpacking).then_some(target))
    }

    /// Slot 0x48 without a target or on a building under its Unpack (`0x140a90440`): the
    /// Gather of the gatherer at `idx` walks to `point`
    /// ([`crate::gathering::gather::GatherAction::walk_to_moved_point`]); the target it had is
    /// let go. 1.
    ///
    /// # Errors
    ///
    /// A gatherer without its Gather or its mover; a sprite the table lacks.
    fn gather_to_point(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        point: Position,
    ) -> GenieResult<Answer> {
        let by = self.releaser(player, idx)?;
        let unit = self.players.live(player, idx)?;
        let id = unit.id;
        let walks = unit.move_walks(
            self.parts.mover_of(id),
            self.top_speed_terms(player, unit)?,
            WalkSprites {
                every: MasterTables::new(&self.model.masters, &self.parts.own_master),
                player,
                graphics: &self.model.graphics,
                look: self.parts.combat_of(id).abilities.look.as_ref(),
            },
            &self.model.roles,
        )?;
        let (Some(gather), Some(mover)) = (
            self.parts.actions.of_mut(id)?.list.gather_mut(),
            self.parts.motion.mover_mut(id),
        ) else {
            return Err(GATHERER_STATE_LACKING);
        };
        let old = gather.targets();
        gather.walk_to_moved_point(point, mover, &walks, &self.model.tuning.move_to)?;
        let new = gather.targets();
        self.exchange_gather_targets(old, new, by)?;
        Ok(Answer::Taken)
    }

    /// Slot 0x48's building test (`0x140a90531`) of `target` for a gatherer of `player`: a
    /// building (type 80) without object flag 0x20 ([`ObjectFlags::GATHERED_BUILDING`]) is
    /// tested by its main building (an annex's `+0xbb8`, else itself). Its player's (`+0x138`)
    /// foundation (below state 2) or pasture (masters 1893 and 1897) the gatherer builds; one
    /// whose master is no farm (class 49) it refuses.
    ///
    /// # Errors
    ///
    /// A target the world does not hold; an annex whose main building it does not hold.
    fn gathered_building(
        &self,
        player: PlayerId,
        target: ObjectId,
    ) -> GenieResult<GatheredBuilding> {
        let (target_player, unit) = self.object(target).ok_or(GATHER_TARGET_NOT_HELD)?;
        let target_master = unit.master(self.master_tables(), target_player)?;
        if target_master.kind != MasterKind::BUILDING
            || target_master
                .object_flags
                .contains(ObjectFlags::GATHERED_BUILDING)
        {
            return Ok(GatheredBuilding::Gathers(target_master.id));
        }
        let (main_player, main) = match self.parts.building_of(unit.id).annex_of {
            Some(main_id) => self.object(main_id).ok_or(MAIN_BUILDING_GONE)?,
            None => (target_player, unit),
        };
        let main_master = main.master(self.master_tables(), main_player)?;
        let roles = &self.model.roles;
        if (main.life.state < ObjectState::Alive
            || roles
                .of_master(main_master.id)
                .intersects(MasterRoles::PASTURE_MANGROVE | MasterRoles::PASTURE))
            && main_player == player
        {
            return Ok(GatheredBuilding::Builds(main.id, main.position()));
        }
        if !roles.of_class(main_master.class).contains(ClassRoles::FARM) {
            return Ok(GatheredBuilding::Refused);
        }
        Ok(GatheredBuilding::Gathers(main_master.id))
    }

    /// Slot 0x48 on its player's foundation or pasture `main` at `at` (`0x140a905f7`): slot 0x808
    /// with none empties the gatherer's action list, its Gather too ([`World::clear_actions`]),
    /// and slot 0x7e8 works `main` at its place with the flag 1, any task (`0xffff`) and the
    /// byte 0 ([`World::order_work`]). 1.
    ///
    /// # Errors
    ///
    /// What the teardowns and the work command do not model.
    fn build_gathered(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        main: ObjectId,
        at: Position,
    ) -> GenieResult<Answer> {
        self.clear_actions(player, idx)?;
        self.order_work(player, idx, &Work::on(Some(main), at))?;
        Ok(Answer::Taken)
    }

    /// Slot 0x48 on a `target` the Gather takes (`0x140a90761`,
    /// [`crate::gathering::gather::GatherAction::retarget`]), the gatherer's drop site test on
    /// master `drops_at`. The targets that changed are let go (slot 0x180) and taken (slot
    /// 0x178). 1.
    ///
    /// # Errors
    ///
    /// A target the world does not hold; a gatherer without its Gather or its mover; what the
    /// Gather's walks do not model.
    fn retarget_gather_to(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        drops_at: MasterId,
    ) -> GenieResult<Answer> {
        let (target_player, target_unit) = self.object(target).ok_or(GATHER_TARGET_NOT_HELD)?;
        let target_master = target_unit.master(self.master_tables(), target_player)?;
        let owner = self.players.player(player)?;
        let unit = owner.live(idx)?;
        let me = unit.master(self.master_tables(), player)?;
        let resource = Resource {
            id: target,
            position: target_unit.position(),
            class: target_master.class,
            master: target_master.id,
            player: target_player,
            workers: target_unit.workers,
            drop_site: me.action_part().drop_sites.contains(&drops_at),
            gap: footprint_gap(
                unit.position(),
                me.radius,
                target_unit.position(),
                target_master.radius,
            ),
            state: target_unit.life.state,
            radius: target_master.radius,
        };
        let farm_mode = self.settings.farm_mode;
        let by = self.releaser(player, idx)?;
        // a stop farm's time per stop: the gatherer's work rate (its master's plus its powerup),
        // capacity and the task's productivity attribute (none: 1.0)
        let productivity = self
            .parts
            .actions_of(unit.id)
            .list
            .gather()
            .and_then(|gather_action| gather_action.task.productivity);
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
            self.top_speed_terms(player, unit)?,
            WalkSprites {
                every: MasterTables::new(&self.model.masters, &self.parts.own_master),
                player,
                graphics: &self.model.graphics,
                look: self.parts.combat_of(unit.id).abilities.look.as_ref(),
            },
            &self.model.roles,
        )?;
        let id = unit.id;
        let (Some(gather), Some(mover)) = (
            self.parts.actions.of_mut(id)?.list.gather_mut(),
            self.parts.motion.mover_mut(id),
        ) else {
            return Err(GATHERER_STATE_LACKING);
        };
        let old = gather.targets();
        let rng = self.streams.get_mut(Stream::Action);
        gather.retarget(
            GatherRetarget {
                target: &resource,
                player,
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
}

#[cfg(test)]
mod tests;
