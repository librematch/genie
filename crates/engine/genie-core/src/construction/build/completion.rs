//! A foundation's completion on the world (`FUN_1411dbb70` from state 0 to 2): each of its parts
//! is the call the game makes, where the game makes it.

use crate::GenieError;
use crate::construction::build::construction::{
    completion_counts, foundation_laid, hand_storage, lay_foundation,
};
use crate::construction::build::{NO_CONSTRUCTION, NOT_HELD, StackUnit, held, show_built};
use crate::map::spatial::Placed;
use crate::math::rng::Stream;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::MasterId;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::{BuildingMaster, Master};
use crate::players::PlayerId;
use crate::players::diplomacy::REVEAL_UNITS;
use crate::world::Seconds;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::slots::PlayerSlots;
use genie_model::model::Model;

impl World {
    /// The building `id`'s `set_state(2)` from state 0 (`FUN_1411dbb70`), after its annexes'
    /// ([`crate::world::World::set_state`] runs them first), in the game's order: the
    /// completion's first part ([`crate::world::World::completion_part`], `FUN_1411c6bd0`), then,
    /// still in state 0, its construction's start with its owner's watch
    /// ([`crate::world::World::construction_starts`], `FUN_1411e5670`); then the block for state
    /// 2: its storage ([`crate::world::World::completion_storage`]); its sight and standing
    /// sprite ([`crate::world::World::completion_shown`]); its stack unit
    /// ([`crate::world::World::completion_stack`]); its ground, history event and counts
    /// ([`crate::world::World::completion_ground`]); then the combat object's and the base
    /// `set_state` (`FUN_14121cc40`, `FUN_14119d710`) put it in state 2. Last its master's
    /// technology is researched (`FUN_1413cdcd0` with `param_3` 0 and no building,
    /// [`crate::world::World::research_built`]) and a master that disappears when built
    /// (`+0x984`) is destroyed (slot 0xd0, [`crate::world::World::destroy`]; its hit points are 0
    /// already, as the builder's test after the work reads them). `FUN_14152bcb0`'s part for
    /// players with attribute 204 is not modelled; a master with object flag 0x40 and a
    /// foundation terrain (its tiles' classes again, `FUN_141055790`) is not either, and a wall's
    /// (`adjacent_mode`) facets, its own and its neighbours' (`FUN_1411ce890`), are not held.
    /// Not held either: the local player's sound (slot 0x4c0), the event 0x69
    /// (`FUN_140c898a0`), the owner's AI (slot 0x388) and the snow decal.
    ///
    /// # Errors
    ///
    /// What the completion does not model, and a building without its master or place.
    pub(crate) fn complete_building(&mut self, id: ObjectId) -> Result<(), GenieError> {
        if self.attribute_204() {
            return Err(REVEAL_UNITS);
        }
        let (p, u) = self.object(id).ok_or(NOT_HELD)?;
        let master = u.master(self.master_tables(), p)?.clone();
        let building = master.building.ok_or(GenieError::unsupported(
            "a completion of a master that is no building",
        ))?;
        self.completion_part(p, id, &master)?;
        // FUN_1411e5670 while still in state 0 (2 lies within 1 to 6)
        if self
            .object(id)
            .is_some_and(|(_, u)| u.life.state == ObjectState::Foundation)
        {
            self.construction_starts(p, id)?;
        }
        if building.foundation_terrain.is_some()
            && master.object_flags.contains(ObjectFlags::TILE_CLASSES)
        {
            return Err(GenieError::unsupported(
                "a completion of a master of object flag 0x40 (its tiles' classes, FUN_141055790)",
            ));
        }
        self.completion_storage(p, id, &master)?;
        let placed = *self.spatial.get(id).ok_or(GenieError::unsupported(
            "a completed building without a place",
        ))?;
        self.completion_shown(id, &master, &placed)?;
        self.completion_stack(p, id, &master, &building, &placed)?;
        self.completion_ground(p, id, &master, &building, &placed)?;
        let (_, u) = self.object_mut(id).ok_or(NOT_HELD)?;
        u.life.state = ObjectState::Alive;
        if building.disappears_when_built {
            u.life.hp = HitPoints::ZERO;
        }
        self.spatial
            .sight_state(id, ObjectState::Alive, &self.model.roles)?;
        if let Some(tech) = building.tech {
            self.research_built(p, tech)?;
        }
        if building.disappears_when_built {
            self.destroy(p, id)?;
        }
        Ok(())
    }

    /// The block for state 2's start: the building `id`'s build points hold its build time, and
    /// its master's storage ([`hand_storage`]) goes to its owner `p` (modes 4, 8 and 64: slot 0xf8
    /// at once) and to its own store (mode 16).
    ///
    /// # Errors
    ///
    /// A building the world does not hold or without its construction, and an attribute its
    /// owner lacks.
    fn completion_storage(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        master: &Master,
    ) -> Result<(), GenieError> {
        let (_, layer) = self.object_and_layer_mut(id)?.ok_or(NOT_HELD)?;
        let c = layer.construction.as_mut().ok_or(NO_CONSTRUCTION)?;
        c.progress = Seconds::of_master(c.build_time);
        let mut gains = Vec::new();
        hand_storage(
            master,
            &mut gains,
            Some(&mut self.parts.combat_mut(id)?.store),
        );
        self.players.player_mut(p)?.add_attributes(gains)
    }

    /// The block for state 2's sight and sprite: the building `id` at `placed` swaps its sight
    /// from its footprint to its line of sight (slots 0x1d8, 0x1d0; not for classes 51 and 54,
    /// whose `+0xca1` the constructor set), then takes its master's standing sprite
    /// ([`show_built`], slot 0x68) on the sprite stream.
    ///
    /// # Errors
    ///
    /// A building the world does not hold.
    fn completion_shown(
        &mut self,
        id: ObjectId,
        master: &Master,
        placed: &Placed,
    ) -> Result<(), GenieError> {
        if !self
            .model
            .roles
            .of_class(master.class)
            .intersects(ClassRoles::PACKED_TREBUCHET | ClassRoles::UNPACKED_TREBUCHET)
            && let Some(v) = self.spatial.sight.as_mut()
        {
            v.construction_completed(id, placed.position, placed.radius, &self.model.roles)?;
        }
        let place = self.place(id).ok_or(NOT_HELD)?;
        let Self {
            players,
            streams,
            parts,
            model: Model { graphics, .. },
            ..
        } = self;
        let u = held(players, place)?;
        show_built(
            &mut u.sprite,
            parts.motion.move_stats_mut(u.id),
            master,
            parts.building.of(id).build_frame,
            graphics,
            streams.get(Stream::Sprite),
        );
        Ok(())
    }

    /// The block for state 2's stack unit (`+0x986`; master 1889 on mangrove or rice terrain,
    /// `+0xcd0`, makes [`PASTURE_MANGROVE`]): the building `id` at `placed` drops its record and mesh
    /// shape (`FUN_141055b40`), lets its tiles go (`FUN_140f65580`, a building that is no annex)
    /// and sets `+0x686`; then its owner `p` makes the stack unit at its place, which loses the
    /// hit points the building lacked (slot 0x1c0, [`crate::world::World::stack_unit`]).
    ///
    /// # Errors
    ///
    /// A building the world does not hold, and what the stack unit's making does not model.
    fn completion_stack(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        master: &Master,
        building: &BuildingMaster,
        placed: &Placed,
    ) -> Result<(), GenieError> {
        if building.stack_unit.is_none() {
            return Ok(());
        }
        self.pathing.removed(id);
        self.spatial.drop_record(id);
        self.pathing.tiles_changed(self.spatial.take_changes());
        let layer = self.parts.building_of(id);
        let (main, rice) = (layer.annex_of.is_some(), layer.rice);
        let (_, u) = self.object_mut(id).ok_or(NOT_HELD)?;
        let hp = u.life.hp;
        u.roo_called = true;
        if !main && let Some(v) = self.spatial.sight.as_mut() {
            v.unlink_building(placed.position, master.radius)?;
        }
        let stack = StackUnit {
            master: stack_master(master, building, rice, &self.model.roles, self.model.slots)
                .ok_or(GenieError::unreachable(
                    "a completed stack unit without a master",
                ))?,
            at: placed.position,
            hp_lost: HitPoints::of_master(master.hit_points) - hp,
        };
        self.stack_unit(p, id, stack)
    }

    /// The block for state 2's ground and counts: the building `id` at `placed` lays its
    /// foundation ([`lay_foundation`]: its snow flag, its foundation terrain over its clearance);
    /// then its owner `p` records the castle's or the wonder's event in its history
    /// (`FUN_141560ae0` with its own id, `+0x718`, as `data1`) and gains its counts
    /// ([`completion_counts`], slot 0xf8 at once).
    ///
    /// # Errors
    ///
    /// A building the world does not hold or without its construction, a map without its
    /// terrain, and an attribute its owner lacks.
    fn completion_ground(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        master: &Master,
        building: &BuildingMaster,
        placed: &Placed,
    ) -> Result<(), GenieError> {
        let mode_680 = self.mode_680;
        let place = self.place(id).ok_or(NOT_HELD)?;
        let Self {
            players,
            spatial,
            parts,
            ..
        } = self;
        let u = held(players, place)?;
        let layer = parts.building_mut(u.id)?;
        let (main, rice) = (layer.annex_of.is_some(), layer.rice);
        let c = layer.construction.as_mut().ok_or(NO_CONSTRUCTION)?;
        let laid = foundation_laid(
            building,
            stack_master(master, building, rice, &self.model.roles, self.model.slots),
            &self.model.roles,
        );
        lay_foundation(
            c,
            spatial,
            placed,
            master,
            laid,
            mode_680,
            &self.model.roles,
        )?;
        let mut counts = Vec::new();
        let history = completion_counts(
            master,
            building,
            main,
            &mut counts,
            &self.model.roles,
            self.model.slots,
        );
        if let Some(kind) = history {
            let now = self.world_time;
            self.players
                .player_mut(p)?
                .history
                .add_event(kind, p.index() as f32, 0.0, 0, now);
        }
        self.players.player_mut(p)?.add_attributes(counts)
    }
}

/// The master a completed building's stack unit takes (`+0x986`): [`PASTURE_MANGROVE`] for master
/// 1889 on mangrove or rice terrain (`rice`, `+0xcd0`), else its own.
fn stack_master(
    master: &Master,
    building: &BuildingMaster,
    rice: bool,
    roles: &Roles,
    slots: Slots,
) -> Option<MasterId> {
    // The buildings' master type (80).
    // The masters and the class the end of the work treats apart: 621 and 1889 among the
    // foundations, class 39 among the builders, 1893 and 1897 (`id - 0x765 & !4 == 0`), the fish
    // trap 199 and farms (class 49, `FUN_141194b20`).
    if roles
        .of_master(master.id)
        .contains(MasterRoles::PASTURE_1889)
        && building.stack_unit.is_some()
        && rice
    {
        Some(slots.master.pasture_mangrove)
    } else {
        building.stack_unit
    }
}
