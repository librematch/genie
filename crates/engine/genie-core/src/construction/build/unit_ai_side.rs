//! What the unit AI asks of a build while it runs (through its view, [`UnitView`]): whether the
//! build command takes a foundation, the builder's search for another foundation, and a unit's
//! spot off a foundation's footprint.

use crate::GenieError;
use crate::construction::FoundationId;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Tile;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::action_type;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::task_finder::Actor;
use crate::unit_ai::task_finder::world::{SideWorld, point_terrain, task_search, task_target_with};
use crate::unit_ai::world_view::UnitView;
use crate::unit_ai::world_view::owner_view;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;

impl UnitView<'_, '_> {
    /// Slot 0x380's answer (forced) on `foundation` without its effects, which `BuildObject`
    /// reads before it takes the build ([`crate::unit_ai::UnitSide::builds`]): 0 for a
    /// foundation the world does not hold (`FUN_1415417a0`), 1 with a Build on top (its slot
    /// 0x48), else whether slot 0x820 finds a task ([`UnitView::build_found`]).
    pub(crate) fn builds_on(&self, foundation: FoundationId) -> Result<bool, GenieError> {
        let view = owner_view(
            &self.c.w.players,
            &self.c.w.index,
            self.c.player,
            self.new_owner,
        )?;
        if self.find_in(view, foundation.id()).is_none() {
            return Ok(false);
        }
        // The Build action's type, which word 9 adds.
        if self.unit.top_action_kind() == Some(action_type::BUILD) {
            return Ok(true);
        }
        self.build_found(foundation)
    }

    /// Object `id` as slot 0x380 finds it (`FUN_1415417a0`): the unit, the player's live list,
    /// another player's ([`crate::unit_ai::world_view::Foreign::find`]). The owner's sleeping
    /// list holds only objects of a type below 30, never a building.
    fn find_in<'v>(&'v self, view: OthersRef<'v>, id: ObjectId) -> Option<(PlayerId, &'v Unit)> {
        if id == self.unit.id {
            return Some((self.player, &*self.unit));
        }
        view.find(id)
    }

    /// Slot 0x380's search without a Build on top: the foundation (false when the world does not
    /// hold it), then slot 0x820 with any action type and the switch ([`task_search`] on the
    /// foundation and its main building); false without a task.
    fn build_found(&self, foundation: FoundationId) -> Result<bool, GenieError> {
        let view = owner_view(
            &self.c.w.players,
            &self.c.w.index,
            self.c.player,
            self.new_owner,
        )?;
        let find = |id: ObjectId| self.find_in(view, id);
        let Some((_, f)) = find(foundation.id()) else {
            return Ok(false);
        };
        let (Some(t), Some(general)) = (
            task_target_with(
                foundation.id(),
                1,
                &find,
                &self.c.w.parts.building,
                crate::objects::masters::MasterTables::new(
                    &self.c.w.masters,
                    &self.c.w.parts.own_master,
                ),
            )?,
            task_target_with(
                foundation.id(),
                2,
                &find,
                &self.c.w.parts.building,
                crate::objects::masters::MasterTables::new(
                    &self.c.w.masters,
                    &self.c.w.parts.own_master,
                ),
            )?,
        ) else {
            return Err(crate::commands::FOUNDATION_GONE);
        };
        let masters = self
            .c
            .w
            .masters
            .get(self.player.index())
            .ok_or(crate::world::UNIT_AI_MASTER)?;
        let unit = &*self.unit;
        let own = unit.master(self.c.w.master_tables(), self.player)?;
        let actor = Actor {
            id: unit.id,
            master: own,
            player: self.player,
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
            player: self.player,
            relations: &self.c.w.players.player(self.player)?.relations,
            foreign: owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
                self.new_owner,
            )?
            .players(),
            drop_sites: &self.c.w.drop_sites,
        };
        // slot 0x820's point: the foundation's place
        let terrain = point_terrain(self.c.w.spatial.terrain.as_ref(), f.position());
        Ok(task_search(masters, &actor, &t, &general, terrain, None, &dip)?.is_some())
    }

    /// `FUN_140c1c7c0`: the flood ([`crate::unit_ai::flood_search`]) from the unit's tile over the
    /// larger of its search radius (slot 0x2c8) and its line of sight, truncated, testing each tile
    /// it reaches (`FUN_140c2b950`: the first object of its tile's list, in order, of type 80 or
    /// more in state 0 of the unit's player), if any.
    pub(crate) fn own_foundation_near(&self) -> Result<Option<ObjectId>, GenieError> {
        let unit = &*self.unit;
        let master = unit.master(self.c.w.master_tables(), self.player)?;
        let r = (if master.search_radius <= master.line_of_sight {
            master.line_of_sight
        } else {
            master.search_radius
        })
        .get() as i32;
        let (ux, uy) = (unit.position().x() as i32, unit.position().y() as i32);
        let (player, others, every, spatial) = (
            self.player,
            owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
                self.new_owner,
            )?,
            crate::objects::masters::MasterTables::new(
                &self.c.w.masters,
                &self.c.w.parts.own_master,
            ),
            &self.c.w.spatial,
        );
        let foundation_on = |tile: Tile| -> Result<Option<ObjectId>, GenieError> {
            for (id, p) in spatial.objects_on(tile) {
                if p.player != player {
                    continue;
                }
                let Some(u) = others.own(id) else {
                    continue;
                };
                let kind = u.master(every, player)?.kind;
                if BUILDING <= kind && u.life.state == ObjectState::Foundation {
                    return Ok(Some(id));
                }
            }
            Ok(None)
        };
        crate::unit_ai::flood_search(Tile::new(ux, uy), r, foundation_on)
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
    ) -> Result<crate::unit_ai::OffFootprint, GenieError> {
        let by_id = crate::world::index::ObjectsRef::new(
            &self.c.w.players,
            &self.c.w.index,
            &self.c.w.masters,
            &self.c.w.parts,
        );
        let unit = &*self.unit;
        let master = unit.master(
            crate::objects::masters::MasterTables::new(
                &self.c.w.masters,
                &self.c.w.parts.own_master,
            ),
            self.player,
        )?;
        let (player, others) = (
            self.player,
            owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
                self.new_owner,
            )?,
        );
        let named = |id: ObjectId| {
            let found = if id == unit.id {
                Some((player, unit))
            } else {
                others.find(id)
            };
            found.map(|(p, u)| crate::unit_ai::Named {
                id,
                master: u.life.master_id,
                owner: p,
                vector: Point::new(u.position().x(), u.position().z()),
            })
        };
        let spot = match self.c.w.spatial.get(site) {
            Some(placed) => {
                let mover = unit.body.mover().ok_or(GenieError::unsupported(
                    "a unit sent off without movement state",
                ))?;
                self.c.w.spatial.spot_off(
                    unit.position(),
                    (unit.id, mover.stats.radius.x()),
                    mover.stats.restriction_mask,
                    (placed.position, radius),
                    false,
                    by_id,
                )?
            }
            None => None,
        };
        let target = unit.top_target();
        Ok(crate::unit_ai::OffFootprint {
            spot,
            z: unit.position().z(),
            class: master.class,
            computer: self.computer,
            player,
            site: named(site.id()),
            action_target: target.and_then(named),
        })
    }
}
