//! The repair and enter commands of the unit AI and the auto scout's area.

use crate::construction::repair::{REPAIR, repairers};
use crate::garrison::EnterSlot;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Tile;
use crate::movement::scout_area::ScoutArea;
use crate::objects::ObjectId;
use crate::objects::actions::Action;
use crate::objects::masters::MasterTables;
use crate::objects::unit::Unit;
use crate::orders::GARRISON;
use crate::players::Amount;
use crate::players::world_player::town_centres;
use crate::unit_ai::Answer;
use crate::unit_ai::task_finder::Actor;
use crate::unit_ai::task_finder::TaskOn;
use crate::unit_ai::task_finder::world::{SideWorld, point_terrain, task_search, task_target_with};
use crate::unit_ai::world_view::owner_view;
use crate::unit_ai::world_view::{EnterOrder, RepairOrder, UnitView};
use crate::world::UNIT_AI_MASTER;
use crate::world::layer::MasterSlots;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

impl UnitView<'_, '_> {
    /// The unit's repair command (slot 0x378, `FUN_141217c00`, forced) on `target`: an object the
    /// world (`FUN_1415417a0`) does not hold is 0. A Repair on top takes it (its slot 0x48,
    /// `FUN_140adb480`): 1. Else a unit of task swap group 1 or 2 switches to its player's
    /// repairer ([`crate::construction::repair::repairers`], slot 0x90 with 0), and its slot 0x820 with the
    /// repair's action type (`0x6a`) and the switch ([`task_search`] on the target and its main
    /// building, the new master's list first) finds a task: a new Repair on it
    /// (`FUN_140ad7fa0`) replaces the unit's actions (slot 0x808): 1; none: 0. The switches and
    /// the Repair are made at once on the unit lent back to its slot ([`RepairOrder`],
    /// [`World::repair_order`](crate::world::World::repair_order)).
    pub(super) fn repair_command(
        &mut self,
        target: ObjectId,
        slots: Slots,
        roles: &Roles,
    ) -> GenieResult<Answer> {
        let unit = &*self.unit;
        let (player, others, foreign) = (
            self.player,
            owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?,
            owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?
            .players(),
        );
        let find = |id: ObjectId| {
            if id == unit.id {
                return Some((player, unit));
            }
            others.find(id)
        };
        let Some((_, target_unit)) = find(target) else {
            return Ok(Answer::NotTaken);
        };
        // slot 0x820's point: the target's place
        let terrain = point_terrain(
            self.step.world.spatial.terrain.as_ref(),
            target_unit.position(),
        );
        if let Some(Action::Repair(repair)) = self.actions.list.head() {
            let order = RepairOrder::Retake {
                target,
                repair: *repair,
            };
            self.step.lend_at(
                (&mut *self.unit, &mut *self.actions),
                |world, player_id, list_index| world.repair_order(player_id, list_index, order),
            )?;
            return Ok(Answer::Taken);
        }
        let masters = self
            .step
            .world
            .model
            .masters
            .get(self.player.index())
            .ok_or(UNIT_AI_MASTER)?;
        let own = unit.master(self.step.world.master_tables(), self.player)?;
        let switch = repairers(self.step.world.model.slots.master)
            .iter()
            .find(|&&(group, _)| group == own.action_part().task_by_group)
            .and_then(|&(_, id)| masters.get(id));
        let (Some(task_target), Some(general)) = (
            task_target_with(
                target,
                1,
                &find,
                &self.step.world.parts,
                MasterTables::new(
                    &self.step.world.model.masters,
                    &self.step.world.parts.own_master,
                ),
            )?,
            task_target_with(
                target,
                2,
                &find,
                &self.step.world.parts,
                MasterTables::new(
                    &self.step.world.model.masters,
                    &self.step.world.parts.own_master,
                ),
            )?,
        ) else {
            // the target is held and an annex's link to its main building clears as the main
            // building dies (`Unit::died`'s annexes)
            return Err(GenieError::unreachable(
                "a repair command's target the world holds without its task target",
            ));
        };
        let actor = Actor {
            id: unit.id,
            master: switch.unwrap_or(own),
            player,
            held: unit.life.held(),
            attribute_95: self
                .step
                .world
                .players
                .player(self.player)?
                .attributes
                .get(95)
                .copied()
                .unwrap_or(Amount::ZERO),
            civ: self.civ,
        };
        let dip = SideWorld {
            player,
            relations: &self.step.world.players.player(self.player)?.relations,
            foreign,
            drop_sites: &self.step.world.model.drop_sites,
        };
        let found = task_search(
            masters,
            &actor,
            &TaskOn {
                target: Some(&task_target),
                general: Some(&general),
                terrain,
            },
            Some(REPAIR),
            &dip,
            slots,
            roles,
        )?;
        if switch.is_none() && found.is_none() {
            return Ok(Answer::NotTaken);
        }
        let order = RepairOrder::Start {
            target,
            switches: [
                switch.map(|master| master.id),
                found.and_then(|(_, found)| found.map(|master| master.id)),
            ],
            task: found.map(|(task, _)| task),
        };
        let repairs = if found.is_some() {
            Answer::Taken
        } else {
            Answer::NotTaken
        };
        self.step.lend_at(
            (&mut *self.unit, &mut *self.actions),
            |world, player_id, list_index| world.repair_order(player_id, list_index, order),
        )?;
        Ok(repairs)
    }

    /// The unit's enter command (slot 0x3a0, `FUN_1410847e0`, forced) on `target`: an object the
    /// world (`FUN_1415417a0`) does not hold is 0; its slot 0x820 with the garrison filter (3) and
    /// the switch ([`task_search`] on the target and its main building) finds a task, whose Enter
    /// (`FUN_140a855d0`) replaces the unit's actions (an Artifact on top would take it as its
    /// sub-action): 1; none: 0. The switch and the Enter are made at once on the unit lent back to
    /// its slot ([`EnterOrder`], [`World::enter_order`](crate::world::World::enter_order)); the
    /// notify its install gives the unit's own AI waits for the side's return.
    pub(super) fn enter_command(
        &mut self,
        target: ObjectId,
        slots: Slots,
        roles: &Roles,
    ) -> GenieResult<Answer> {
        let unit = &*self.unit;
        let (player, others, foreign) = (
            self.player,
            owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?,
            owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?
            .players(),
        );
        let find = |id: ObjectId| {
            if id == unit.id {
                return Some((player, unit));
            }
            others.find(id)
        };
        let Some((_, target_unit)) = find(target) else {
            return Ok(Answer::NotTaken);
        };
        let slot = EnterSlot::under(self.actions.top_action_kind());
        let masters = self
            .step
            .world
            .model
            .masters
            .get(self.player.index())
            .ok_or(UNIT_AI_MASTER)?;
        let own = unit.master(self.step.world.master_tables(), self.player)?;
        let (Some(task_target), Some(general)) = (
            task_target_with(
                target,
                1,
                &find,
                &self.step.world.parts,
                MasterTables::new(
                    &self.step.world.model.masters,
                    &self.step.world.parts.own_master,
                ),
            )?,
            task_target_with(
                target,
                2,
                &find,
                &self.step.world.parts,
                MasterTables::new(
                    &self.step.world.model.masters,
                    &self.step.world.parts.own_master,
                ),
            )?,
        ) else {
            // the target is held and an annex's link to its main building clears as the main
            // building dies (`Unit::died`'s annexes)
            return Err(GenieError::unreachable(
                "an enter command's target the world holds without its task target",
            ));
        };
        let actor = Actor {
            id: unit.id,
            master: own,
            player,
            held: unit.life.held(),
            attribute_95: self
                .step
                .world
                .players
                .player(self.player)?
                .attributes
                .get(95)
                .copied()
                .unwrap_or(Amount::ZERO),
            civ: self.civ,
        };
        let dip = SideWorld {
            player,
            relations: &self.step.world.players.player(self.player)?.relations,
            foreign,
            drop_sites: &self.step.world.model.drop_sites,
        };
        // slot 0x820's point: the target's place
        let terrain = point_terrain(
            self.step.world.spatial.terrain.as_ref(),
            target_unit.position(),
        );
        let Some((task, switch)) = task_search(
            masters,
            &actor,
            &TaskOn {
                target: Some(&task_target),
                general: Some(&general),
                terrain,
            },
            Some(GARRISON),
            &dip,
            slots,
            roles,
        )?
        else {
            return Ok(Answer::NotTaken);
        };
        let order = EnterOrder {
            target,
            task,
            switch: switch.map(|master| master.id),
            slot,
        };
        if let Some(notify) = self.step.lend_at(
            (&mut *self.unit, &mut *self.actions),
            |world, player_id, list_index| world.enter_order(player_id, list_index, order),
        )? {
            self.step.notify_own(self.unit, notify)?;
        }
        Ok(Answer::Taken)
    }

    /// What an explorer's searches ask of the world ([`ScoutArea::of`] for the unit, its unit
    /// AI's target `ai_target`), the map's size and the unit's line of sight.
    ///
    /// # Errors
    ///
    /// A unit whose master the world lacks, and what the area does not model.
    pub(crate) fn scout_area(
        &mut self,
        ai_target: Option<ObjectId>,
    ) -> GenieResult<(ScoutArea<'_>, GridSize, Distance)> {
        let top_speed = self
            .step
            .world
            .top_speed_terms_with(self.player, self.unit, ai_target)?;
        let me = self.unit.me(self.actions, top_speed);
        ScoutArea::of(
            self.step.world,
            self.unit,
            me,
            self.player,
            self.player_kind,
        )
    }
}

/// `FUN_1416597e0` with 0: the player's first town centre, by the player's index of objects by
/// master (`player+0x18`, `FUN_14104f560`: those of master 109, then 444, [`town_centres`]), as
/// its tile. The index's order within a master is not held: two objects of the first master
/// found are not modelled.
pub(crate) fn town_centre(units: OthersRef<'_>, masters: MasterSlots) -> GenieResult<Option<Tile>> {
    Ok(town_centre_unit(units, masters)?
        .map(|unit| Tile::new(unit.position().x() as i32, unit.position().y() as i32)))
}

/// [`town_centre`] as the object.
pub(crate) fn town_centre_unit(
    units: OthersRef<'_>,
    masters: MasterSlots,
) -> GenieResult<Option<&Unit>> {
    for master in town_centres(masters) {
        let mut found = units
            .own_iter()
            .filter(|unit| unit.life.master_id == master);
        let Some(first) = found.next() else { continue };
        if found.next().is_some() {
            return Err(GenieError::unsupported(
                "two town centres (the order of the player's index)",
            ));
        }
        return Ok(Some(first));
    }
    Ok(None)
}
