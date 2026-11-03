//! The repair and enter commands of the unit AI and the auto scout's area.

use crate::GenieError;
use crate::construction::repair::{REPAIR, repairers};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Tile;
use crate::map::spatial::Spatial;
use crate::map::spatial::TileSight;
use crate::math::fmath::sqrt;
use crate::math::rng::Stream;
use crate::movement::explore::Area;
use crate::movement::{Me, Mover, Planning};
use crate::objects::ObjectId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterTables;
use crate::objects::unit::Unit;
use crate::orders::GARRISON;
use crate::ownership::artifact::ARTIFACT_TYPE;
use crate::pathing::zones::{UNKNOWN_RESTRICTION, ZoneId, ZoneMap};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::ai_module::SearchArea;
use crate::players::world_player::town_centres;
use crate::unit_ai::task_finder::Actor;
use crate::unit_ai::task_finder::TaskOn;
use crate::unit_ai::task_finder::world::{SideWorld, point_terrain, task_search, task_target_with};
use crate::unit_ai::world_view::owner_view;
use crate::unit_ai::world_view::{EnterOrder, RepairOrder, UnitView};
use crate::world::UNIT_AI_MASTER;
use crate::world::index::ObjectsRef;
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
    /// [`World::repair_order`]).
    pub(super) fn repair_command(
        &mut self,
        target: ObjectId,
        slots: Slots,
        roles: &Roles,
    ) -> Result<bool, GenieError> {
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
            return Ok(false);
        };
        // slot 0x820's point: the target's place
        let terrain = point_terrain(self.c.w.spatial.terrain.as_ref(), target_unit.position());
        if self.actions.top_action_kind() == Some(REPAIR) {
            let order = RepairOrder::Retake(target);
            self.c
                .lend_at((&mut *self.unit, &mut *self.actions), |w, p, i| {
                    w.repair_order(p, i, order)
                })?;
            return Ok(true);
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
            .find(|&&(group, _)| group == own.task_by_group)
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
            return Ok(false);
        }
        let order = RepairOrder::Start {
            target,
            switches: [
                switch.map(|m| m.id),
                found.and_then(|(_, m)| m.map(|m| m.id)),
            ],
            task: found.map(|(task, _)| task),
        };
        let repairs = found.is_some();
        self.c
            .lend_at((&mut *self.unit, &mut *self.actions), |w, p, i| {
                w.repair_order(p, i, order)
            })?;
        Ok(repairs)
    }

    /// The unit's enter command (slot 0x3a0, `FUN_1410847e0`, forced) on `target`: an object the
    /// world (`FUN_1415417a0`) does not hold is 0; its slot 0x820 with the garrison filter (3)
    /// and the switch ([`task_search`] on the target and its main building) finds a task, whose
    /// Enter (`FUN_140a855d0`) replaces the unit's actions (an Artifact on top would take it as
    /// its sub-action): 1; none: 0. The switch and the Enter are made at once on the unit lent
    /// back to its slot ([`EnterOrder`], [`World::enter_order`]); the notify its install gives
    /// the unit's own AI waits for the side's return.
    pub(super) fn enter_command(
        &mut self,
        target: ObjectId,
        slots: Slots,
        roles: &Roles,
    ) -> Result<bool, GenieError> {
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
            return Ok(false);
        };
        let under_artifact = self.actions.top_action_kind() == Some(ARTIFACT_TYPE);
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
            return Ok(false);
        };
        let order = EnterOrder {
            target,
            task,
            switch: switch.map(|m| m.id),
            under_artifact,
        };
        if let Some(n) = self
            .c
            .lend_at((&mut *self.unit, &mut *self.actions), |w, p, i| {
                w.enter_order(p, i, order)
            })?
        {
            self.c.notify_own(self.unit, n)?;
        }
        Ok(true)
    }

    /// What an explorer's searches ask of the world, the map's size and the unit's line of sight.
    pub(crate) fn scout_area(&mut self) -> Result<(ScoutArea<'_>, GridSize, Distance), GenieError> {
        let unit = &*self.unit;
        let master = unit.master(
            MasterTables::new(&self.c.w.model.masters, &self.c.w.parts.own_master),
            self.player,
        )?;
        let mover = self
            .c
            .w
            .parts
            .mover_of(unit.id)
            .ok_or(GenieError::unsupported(
                "an auto scout without movement state",
            ))?;
        let map = self.c.w.spatial.map;
        let terrain = self
            .c
            .w
            .spatial
            .terrain
            .as_ref()
            .ok_or(GenieError::unsupported(
                "the map's zones without its terrain",
            ))?;
        let values = terrain
            .restriction(mover.stats.restriction)
            .ok_or(UNKNOWN_RESTRICTION)?;
        let zones = self.c.w.zones.of(map, &terrain.kinds, values)?;
        let me = unit.me(self.actions);
        let area = ScoutArea {
            spatial: &mut self.c.w.spatial,
            by_id: ObjectsRef::new(
                &self.c.w.players,
                &self.c.w.index,
                &self.c.w.model.masters,
                &self.c.w.parts,
                &self.c.w.model.roles,
                &self.c.w.model.tuning,
            ),
            zones,
            plan: Planning {
                pathing: &mut self.c.w.pathing,
                now: self.c.w.world_time,
                player_type: self.player_type,
                object: self.c.w.streams.get(Stream::Object),
                groups: &self.c.w.groups,
            },
            mover,
            at: unit.position(),
            me,
            player: self.player,
            master: master.id,
            radius: master.radius,
            reach: master.line_of_sight - Distance::new(self.c.w.model.tuning.explore.los_margin),
            scouted: false,
        };
        Ok((area, map, master.line_of_sight))
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

impl SearchArea for ScoutArea<'_> {
    fn visible_now(&self, tile: Tile) -> bool {
        self.spatial
            .scout_sight(self.player, tile)
            .is_some_and(TileSight::seen)
    }
}

/// What the auto scout's search asks of the world for one unit ([`crate::movement::explore::Area`]).
pub(crate) struct ScoutArea<'a> {
    pub(crate) spatial: &'a mut Spatial,
    /// The world's objects, whose gaits the stand and reach tests read.
    pub(crate) by_id: ObjectsRef<'a>,
    pub(crate) zones: &'a ZoneMap,
    pub(crate) plan: Planning<'a>,
    pub(crate) mover: &'a Mover,
    /// The scout's place (its row's static layer), which the search does not move.
    pub(crate) at: Position,
    pub(crate) me: Me,
    pub(crate) player: PlayerId,
    pub(crate) master: MasterId,
    pub(crate) radius: Footprint,
    /// The reach of slot 0x3d8: the line of sight less a tile.
    pub(crate) reach: Distance,
    /// The search reads the player's scout map for the tiles seen (`FUN_140c4b130` with one).
    pub(crate) scouted: bool,
}

impl Area for ScoutArea<'_> {
    fn explored(&self, tile: Tile) -> Result<bool, GenieError> {
        if self.scouted {
            return self
                .spatial
                .scout_map(self.player)
                .and_then(|m| m.get(tile))
                .map(|v| v == 1)
                .ok_or(GenieError::unsupported("a tile off the player's scout map"));
        }
        self.spatial.explored(self.player, tile)
    }

    fn zone(&mut self, tile: Tile) -> ZoneId {
        self.zones.zone(tile)
    }

    fn own_zone(&mut self) -> ZoneId {
        let (x, y) = (self.at.x(), self.at.y());
        self.zones.zone(Tile::new(x as i32, y as i32))
    }

    fn clear(&mut self, tile: Tile) -> Result<bool, GenieError> {
        let mask = self.mover.stats.restriction_mask;
        self.spatial.point_clear(
            self.me.id,
            tile.corner(),
            Distance::new(self.radius.x()),
            mask,
            self.by_id,
        )
    }

    fn reaches(&mut self, tile: Tile) -> Result<bool, GenieError> {
        let to = tile.corner();
        let _tuning = self.by_id.tuning();
        self.mover.reaches(
            (self.at, &self.me),
            (self.spatial, self.by_id),
            &mut self.plan,
            to,
            self.reach,
            self.master,
        )
    }

    /// `FUN_14118faf0`: the gap from the footprint on each axis (at least 0), then its length.
    fn distance(&self, tile: Tile) -> f32 {
        let gap = |d: f32, r: f32| {
            let g = d.abs() - r;
            if g <= 0.0 { 0.0 } else { g }
        };
        let gx = gap(tile.x() as f32 - self.at.x(), self.radius.x());
        let gy = gap(tile.y() as f32 - self.at.y(), self.radius.y());
        sqrt(gy * gy + gx * gx)
    }
}
