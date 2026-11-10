//! What the unit AI asks of a build while it runs (through its view, [`UnitView`]): whether the
//! build command takes a foundation, the builder's search for another foundation, and a unit's
//! spot off a foundation's footprint.

use crate::GenieResult;
use crate::commands::FOUNDATION_GONE;
use crate::construction::FoundationId;
use crate::construction::build::SENT_OFF_UNMOVING;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Tile;
use crate::map::spatial::SpotPick;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MasterTables;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::task_finder::Actor;
use crate::unit_ai::task_finder::TaskOn;
use crate::unit_ai::task_finder::world::{SideWorld, point_terrain, task_search, task_target_with};
use crate::unit_ai::world_view::UnitView;
use crate::unit_ai::world_view::owner_view;
use crate::unit_ai::{Named, OffFootprint, flood_search};
use crate::world::UNIT_AI_MASTER;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::objects_ref::ObjectsRef;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlotsExt;

impl UnitView<'_, '_> {
    /// Slot 0x380's answer (forced) on `foundation` without its effects, which `BuildObject`
    /// reads before it takes the build ([`crate::unit_ai::world_view::UnitView::builds`]): 0 for a
    /// foundation the world does not hold (`FUN_1415417a0`), 1 with a Build on top (its slot
    /// 0x48), else whether slot 0x820 finds a task ([`UnitView::build_found`]).
    pub(crate) fn builds_on(
        &self,
        foundation: FoundationId,
        slots: Slots,
        roles: &Roles,
    ) -> GenieResult<bool> {
        let view = owner_view(
            &self.step.world.players,
            &self.step.world.index,
            self.step.player,
            self.new_owner,
        )?;
        if self.find_in(view, foundation.id()).is_none() {
            return Ok(false);
        }
        // The Build action's type, which word 9 adds.
        if self.actions.top_action_kind() == Some(ActionKind::BUILD) {
            return Ok(true);
        }
        self.build_found(foundation, slots, roles)
    }

    /// Object `id` as slot 0x380 finds it (`FUN_1415417a0`): the unit, the player's live list,
    /// another player's. The owner's sleeping
    /// list holds only objects of a type below 30, never a building.
    fn find_in<'world>(
        &'world self,
        view: OthersRef<'world>,
        id: ObjectId,
    ) -> Option<(PlayerId, &'world Unit)> {
        if id == self.unit.id {
            return Some((self.player, &*self.unit));
        }
        view.find(id)
    }

    /// Slot 0x380's search without a Build on top: the foundation (false when the world does not
    /// hold it), then slot 0x820 with any action type and the switch ([`task_search`] on the
    /// foundation and its main building); false without a task.
    fn build_found(
        &self,
        foundation: FoundationId,
        slots: Slots,
        roles: &Roles,
    ) -> GenieResult<bool> {
        let view = owner_view(
            &self.step.world.players,
            &self.step.world.index,
            self.step.player,
            self.new_owner,
        )?;
        let find = |id: ObjectId| self.find_in(view, id);
        let Some((_, foundation_unit)) = find(foundation.id()) else {
            return Ok(false);
        };
        let (Some(task_target), Some(general)) = (
            task_target_with(
                foundation.id(),
                1,
                &find,
                &self.step.world.parts,
                MasterTables::new(
                    &self.step.world.model.masters,
                    &self.step.world.parts.own_master,
                ),
            )?,
            task_target_with(
                foundation.id(),
                2,
                &find,
                &self.step.world.parts,
                MasterTables::new(
                    &self.step.world.model.masters,
                    &self.step.world.parts.own_master,
                ),
            )?,
        ) else {
            return Err(FOUNDATION_GONE);
        };
        let masters = self
            .step
            .world
            .model
            .masters
            .get(self.player.index())
            .ok_or(UNIT_AI_MASTER)?;
        let unit = &*self.unit;
        let own = unit.master(self.step.world.master_tables(), self.player)?;
        let actor = Actor {
            id: unit.id,
            master: own,
            player: self.player,
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
            player: self.player,
            relations: &self.step.world.players.player(self.player)?.relations,
            foreign: owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?
            .players(),
            treaty: self.step.world.treaty.as_ref(),
            drop_sites: &self.step.world.model.drop_sites,
        };
        // slot 0x820's point: the foundation's place
        let terrain = point_terrain(
            self.step.world.spatial.terrain.as_ref(),
            foundation_unit.position(),
        );
        Ok(task_search(
            masters,
            &actor,
            &TaskOn {
                target: Some(&task_target),
                general: Some(&general),
                terrain,
            },
            None,
            &dip,
            slots,
            roles,
        )?
        .is_some())
    }

    /// `FUN_140c1c7c0`: the flood ([`crate::unit_ai::flood_search`]) from the unit's tile over the
    /// larger of its search radius (slot 0x2c8) and its line of sight, truncated, testing each tile
    /// it reaches (`FUN_140c2b950`: the first object of its tile's list, in order, of type 80 or
    /// more in state 0 of the unit's player), if any.
    pub(crate) fn own_foundation_near(&self) -> GenieResult<Option<ObjectId>> {
        let unit = &*self.unit;
        let master = unit.master(self.step.world.master_tables(), self.player)?;
        let radius = (if master.action_part().search_radius <= master.line_of_sight {
            master.line_of_sight
        } else {
            master.action_part().search_radius
        })
        .get() as i32;
        let (ux, uy) = (unit.position().x() as i32, unit.position().y() as i32);
        let (player, others, every, spatial) = (
            self.player,
            owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?,
            MasterTables::new(
                &self.step.world.model.masters,
                &self.step.world.parts.own_master,
            ),
            &self.step.world.spatial,
        );
        let foundation_on = |tile: Tile| -> GenieResult<Option<ObjectId>> {
            for (id, placed) in spatial.objects_on(tile) {
                if placed.player != player {
                    continue;
                }
                let Some(unit) = others.own(id) else {
                    continue;
                };
                let kind = unit.master(every, player)?.kind;
                if MasterKind::BUILDING <= kind && unit.life.state == ObjectState::Foundation {
                    return Ok(Some(id));
                }
            }
            Ok(None)
        };
        flood_search(Tile::new(ux, uy), radius, foundation_on)
    }

    /// The base handler's case 0x216: the unit's closest spot beside the foundation `site`
    /// ([`Spatial::spot_off`] with the notify's radii; none without the foundation), its height
    /// and class, its owner, the foundation and its current action's target as orders name them.
    ///
    /// [`Spatial::spot_off`]: crate::map::spatial::Spatial::spot_off
    pub(crate) fn spot_off_footprint(
        &mut self,
        site: FoundationId,
        radius: Footprint,
    ) -> GenieResult<OffFootprint> {
        let by_id = ObjectsRef::new(
            &self.step.world.players,
            &self.step.world.index,
            &self.step.world.model.masters,
            &self.step.world.parts,
            &self.step.world.model.roles,
            &self.step.world.model.tuning,
            self.step.world.treaty.as_ref(),
        );
        let unit = &*self.unit;
        let master = unit.master(
            MasterTables::new(
                &self.step.world.model.masters,
                &self.step.world.parts.own_master,
            ),
            self.player,
        )?;
        let (player, others) = (
            self.player,
            owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?,
        );
        let named = |id: ObjectId| {
            let found = if id == unit.id {
                Some((player, unit))
            } else {
                others.find(id)
            };
            found.map(|(player_id, unit)| Named {
                id,
                master: unit.life.master_id,
                owner: player_id,
                vector: Point::new(unit.position().x(), unit.position().z()),
            })
        };
        let spot = match self.step.world.spatial.get(site) {
            Some(placed) => {
                let mover = self
                    .step
                    .world
                    .parts
                    .mover_of(unit.id)
                    .ok_or(SENT_OFF_UNMOVING)?;
                self.step.world.spatial.spot_off(
                    unit.position(),
                    (unit.id, mover.stats.radius.x()),
                    mover.stats.restriction_mask,
                    (placed.position, radius),
                    SpotPick::Nearest,
                    by_id,
                )?
            }
            None => None,
        };
        let target = self.actions.top_target();
        Ok(OffFootprint {
            spot,
            z: unit.position().z(),
            class: master.class,
            player_kind: self.player_kind,
            player,
            site: named(site.id()),
            action_target: target.and_then(named),
        })
    }
}
