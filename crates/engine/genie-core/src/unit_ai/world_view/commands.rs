//! The repair and enter commands of the unit AI and the auto scout's area.

use crate::GenieError;
use crate::construction::repair::{REPAIR, repairers};
use crate::garrison::EnterSlot;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Tile;
use crate::movement::scout_area::ScoutArea;
use crate::objects::ObjectId;
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
use crate::world::slots::PlayerSlots;

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
    ) -> Result<Answer, GenieError> {
        let unit = &*self.unit;
        let (player, others, foreign) = (
            self.player,
            owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
                self.new_owner,
            )?,
            owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
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
        let terrain = point_terrain(self.c.w.spatial.terrain.as_ref(), target_unit.position());
        if self.actions.top_action_kind() == Some(REPAIR) {
            let order = RepairOrder::Retake(target);
            self.c
                .lend_at((&mut *self.unit, &mut *self.actions), |w, p, i| {
                    w.repair_order(p, i, order)
                })?;
            return Ok(Answer::Taken);
        }
        let masters = self
            .c
            .w
            .model
            .masters
            .get(self.player.index())
            .ok_or(UNIT_AI_MASTER)?;
        let own = unit.master(self.c.w.master_tables(), self.player)?;
        let switch = repairers(self.c.w.model.slots.master)
            .iter()
            .find(|&&(group, _)| group == own.action_part().task_by_group)
            .and_then(|&(_, id)| masters.get(id));
        let (Some(t), Some(general)) = (
            task_target_with(
                target,
                1,
                &find,
                &self.c.w.parts,
                MasterTables::new(&self.c.w.model.masters, &self.c.w.parts.own_master),
            )?,
            task_target_with(
                target,
                2,
                &find,
                &self.c.w.parts,
                MasterTables::new(&self.c.w.model.masters, &self.c.w.parts.own_master),
            )?,
        ) else {
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
                .c
                .w
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
            relations: &self.c.w.players.player(self.player)?.relations,
            foreign,
            drop_sites: &self.c.w.model.drop_sites,
        };
        let found = task_search(
            masters,
            &actor,
            &TaskOn {
                target: Some(&t),
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
                switch.map(|m| m.id),
                found.and_then(|(_, m)| m.map(|m| m.id)),
            ],
            task: found.map(|(task, _)| task),
        };
        let repairs = if found.is_some() {
            Answer::Taken
        } else {
            Answer::NotTaken
        };
        self.c
            .lend_at((&mut *self.unit, &mut *self.actions), |w, p, i| {
                w.repair_order(p, i, order)
            })?;
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
    ) -> Result<Answer, GenieError> {
        let unit = &*self.unit;
        let (player, others, foreign) = (
            self.player,
            owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
                self.new_owner,
            )?,
            owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
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
            .c
            .w
            .model
            .masters
            .get(self.player.index())
            .ok_or(UNIT_AI_MASTER)?;
        let own = unit.master(self.c.w.master_tables(), self.player)?;
        let (Some(t), Some(general)) = (
            task_target_with(
                target,
                1,
                &find,
                &self.c.w.parts,
                MasterTables::new(&self.c.w.model.masters, &self.c.w.parts.own_master),
            )?,
            task_target_with(
                target,
                2,
                &find,
                &self.c.w.parts,
                MasterTables::new(&self.c.w.model.masters, &self.c.w.parts.own_master),
            )?,
        ) else {
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
                .c
                .w
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
            relations: &self.c.w.players.player(self.player)?.relations,
            foreign,
            drop_sites: &self.c.w.model.drop_sites,
        };
        // slot 0x820's point: the target's place
        let terrain = point_terrain(self.c.w.spatial.terrain.as_ref(), target_unit.position());
        let Some((task, switch)) = task_search(
            masters,
            &actor,
            &TaskOn {
                target: Some(&t),
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
            switch: switch.map(|m| m.id),
            slot,
        };
        if let Some(n) = self
            .c
            .lend_at((&mut *self.unit, &mut *self.actions), |w, p, i| {
                w.enter_order(p, i, order)
            })?
        {
            self.c.notify_own(self.unit, n)?;
        }
        Ok(Answer::Taken)
    }

    /// What an explorer's searches ask of the world ([`ScoutArea::of`] for the unit), the map's
    /// size and the unit's line of sight.
    pub(crate) fn scout_area(&mut self) -> Result<(ScoutArea<'_>, GridSize, Distance), GenieError> {
        let me = self.unit.me(self.actions);
        ScoutArea::of(self.c.w, self.unit, me, self.player, self.player_kind)
    }
}

/// `FUN_1416597e0` with 0: the player's first town centre, by the player's index of objects by
/// master (`player+0x18`, `FUN_14104f560`: those of master 109, then 444, [`town_centres`]), as
/// its tile. The index's order within a master is not held: two objects of the first master
/// found are not modelled.
pub(crate) fn town_centre(
    units: OthersRef<'_>,
    masters: MasterSlots,
) -> Result<Option<Tile>, GenieError> {
    Ok(town_centre_unit(units, masters)?
        .map(|u| Tile::new(u.position().x() as i32, u.position().y() as i32)))
}

/// [`town_centre`] as the object.
pub(crate) fn town_centre_unit(
    units: OthersRef<'_>,
    masters: MasterSlots,
) -> Result<Option<&Unit>, GenieError> {
    for master in town_centres(masters) {
        let mut found = units.own_iter().filter(|u| u.life.master_id == master);
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
