//! The Empire Wars start: in the game's first update every idle villager is put to work
//! (`BaseWorld::update`, `0x14154fc70`, its per-player part before the player's objects update).
//!
//! The pass runs once (the world's `+0x6c6`) under the Empire Wars rule, the game mode 13 or in
//! another mode the sub-mode's bit 0 (`FUN_141324b60`, [`crate::world::settings::Settings::empire_wars`];
//! 426422314's random map game): each object of the player's live list, in its order, that is a villager (class 4) or a
//! fishing ship (class 21), may be interrupted with an empty order queue (slot 0x2d0 with 1) and
//! has a unit AI takes work. Under the game's farm setting a farm on its tile comes first (the
//! map tile's objects in their order: the first of class 49; a fishing ship's must be a fish
//! trap, master 199); else the closest resource of the five gather groups by the unit AI's
//! `ClosestResourceObject` (slot 0x80, [`crate::gathering::gather_search::closest_resource_at`]: no drop
//! site, any workers, the least distance wins, the first group at a tie). Slot 0x140
//! (`FUN_141070c90`) then gives the unit its work on the target at the target's place
//! ([`World::work_at`]: slot 0x7e8, then the command count). Any other
//! object that is a building (master type 80) whose master has a task of action type 0x99 (a
//! mule cart), may be interrupted as above and has a unit AI runs that AI's slot 0x1e8, the
//! cart's own search for resources ([`World::cart_work_search`]).

use crate::construction::build::CART_TASK;
use crate::gathering::closest_resource::SEARCHER_AI_LACKING;
use crate::gathering::closest_resource::SEARCHER_UNMOVING;
use crate::gathering::gather_search::TILE_OBJECT_NOT_HELD;
use crate::gathering::gather_search::closest_resource_at;
use crate::gathering::gather_search::{self, Candidates, ResourceWorld};
use crate::map::Rect;
use crate::map::Tile;
use crate::math::rng::Stream;
use crate::movement::UnitReach;
use crate::movement::{Me, Planning};
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::masters::ClassId;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MasterTables;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::objects_ref::ObjectsRef;
use crate::world::settings::FarmMode;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

/// The gather groups the resource search tries, in order (0 to 4).
const GROUPS: [GatherGroup; 5] = [
    GatherGroup::WOOD,
    GatherGroup::FORAGE,
    GatherGroup::FISH,
    GatherGroup::STONE,
    GatherGroup::GOLD,
];
/// The distance a group's answer must be below (the pass's 99).
const NO_DISTANCE: i32 = 99;

/// What the pass gives an object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Start {
    /// A villager's or fishing ship's work on its farm or its closest resource.
    Work,
    /// A mule cart's own search for resources (its unit AI's slot 0x1e8).
    CartSearch,
}

/// Whether a master has a task of action type 0x99 (`FUN_1415c60b0`), a mule cart's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum CartTask {
    /// It has one.
    Held,
    /// It has none.
    Lacking,
}

/// The pass's choice for an object of the decoded `class` and master type `kind`: a villager or
/// fishing ship works, else a building (master type 80) whose master has a task of action type 0x99
/// (`cart_task`, `FUN_1415c60b0`) searches. The caller then tests that it is ready (slot 0x2d0 with
/// 1: its top action may be interrupted and its unit AI holds no order) and has a unit AI.
fn start_of(class: ClassId, kind: MasterKind, cart_task: CartTask, roles: &Roles) -> Option<Start> {
    // The classes the pass puts to work: villagers and fishing ships (decoded 4 and 21).
    // The farm class a unit's tile is searched for, and the fish trap a fishing ship takes.
    if roles
        .of_class(class)
        .intersects(ClassRoles::VILLAGER | ClassRoles::FISHING_SHIP)
    {
        return Some(Start::Work);
    }
    if kind == MasterKind::BUILDING && cart_task == CartTask::Held {
        return Some(Start::CartSearch);
    }
    None
}

impl World {
    /// The pass for player `player_id` ([`crate::orders::start_work`]).
    ///
    /// # Errors
    ///
    /// The pass without the game's farm setting (the player's nearest unworked farm), a fishing
    /// ship's own search (its unit AI's slot 0x80), and what the work command does not model.
    pub(crate) fn start_work(&mut self, player_id: PlayerId) -> GenieResult<()> {
        let mut list_index = ListIndex::FIRST;
        while self.players.player(player_id)?.objects.holds(list_index) {
            let unit = &self.players.live(player_id, list_index)?;
            let master = unit.master(self.master_tables(), player_id)?;
            let class = master.class;
            let cart_task = if master.kind == MasterKind::BUILDING
                && self.has_task(player_id, unit.life.master_id, CART_TASK)?
            {
                CartTask::Held
            } else {
                CartTask::Lacking
            };
            let Some(start) = start_of(class, master.kind, cart_task, &self.model.roles) else {
                list_index = list_index.next();
                continue;
            };
            let idle = self
                .parts
                .unit_ai
                .get(unit.id)
                .is_some_and(|ai| ai.orders.is_empty());
            if !idle
                || !self
                    .parts
                    .actions_of(unit.id)
                    .interruptible(self.parts.unread.get(unit.id))?
            {
                list_index = list_index.next();
                continue;
            }
            if start == Start::CartSearch {
                self.cart_work_search(player_id, list_index)?;
                list_index = list_index.next();
                continue;
            }
            if self.settings.farm_mode == FarmMode::Fixed {
                return Err(GenieError::unsupported(
                    "the Empire Wars start without the farm setting (the nearest unworked farm)",
                ));
            }
            let target = match self.farm_under(player_id, list_index, class)? {
                Some(farm) => Some(farm),
                None if self
                    .model
                    .roles
                    .of_class(class)
                    .contains(ClassRoles::FISHING_SHIP) =>
                {
                    return Err(GenieError::unsupported(
                        "the Empire Wars start of a fishing ship (its unit AI's slot 0x80)",
                    ));
                }
                None => self.closest_of_groups(player_id, list_index)?,
            };
            let own = self.players.live(player_id, list_index)?.id;
            if let Some(target) = target.filter(|&object_id| object_id != own) {
                let point = self
                    .spatial
                    .get_known(target)?
                    .ok_or(GenieError::unsupported(
                        "the Empire Wars start's target without a place",
                    ))?
                    .position;
                self.work_at(player_id, list_index, Some(target), point)?;
            }
            list_index = list_index.next();
        }
        Ok(())
    }

    /// The first farm in the object list of the unit's tile (a fish trap for a fishing ship).
    fn farm_under(
        &self,
        player_id: PlayerId,
        list_index: ListIndex,
        class: ClassId,
    ) -> GenieResult<Option<ObjectId>> {
        let at = self.players.live(player_id, list_index)?.position();
        let (x, y) = (at.x() as i32, at.y() as i32);
        self.spatial
            .check_known(Rect::new(at.x(), at.y(), at.x(), at.y()))?;
        for (id, _) in self.spatial.objects_on(Tile::new(x, y)) {
            let (owner, unit) = self.object(id).ok_or(TILE_OBJECT_NOT_HELD)?;
            let master = unit.master(self.master_tables(), owner)?;
            if self
                .model
                .roles
                .of_class(master.class)
                .contains(ClassRoles::FARM)
                && (!self
                    .model
                    .roles
                    .of_class(class)
                    .contains(ClassRoles::FISHING_SHIP)
                    || self
                        .model
                        .roles
                        .of_master(master.id)
                        .contains(MasterRoles::FISH_TRAP))
            {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    /// The unit AI's `ClosestResourceObject` for each gather group (no drop site, any workers,
    /// `param_6` 2, no point): the answer at the least distance, the first group at a tie.
    fn closest_of_groups(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
    ) -> GenieResult<Option<ObjectId>> {
        let mut best = (NO_DISTANCE, None);
        for kind in GROUPS {
            let (id, distance) = self.resource_search_from(player_id, list_index, kind)?;
            if let Some(gap) = distance.filter(|&gap| gap < best.0) {
                best = (gap, id);
            }
        }
        Ok(best.1)
    }

    /// The seeker and the walk's view of the unit `unit` of player `player_id` for
    /// [`World::resource_search_from`].
    ///
    /// # Errors
    ///
    /// A master the world does not hold, or a unit without a unit AI.
    fn search_seeker(
        &self,
        player_id: PlayerId,
        unit: &Unit,
    ) -> GenieResult<(gather_search::Seeker, Me)> {
        let master = unit.master(self.master_tables(), player_id)?;
        let ai = self.parts.unit_ai.get(unit.id).ok_or(SEARCHER_AI_LACKING)?;
        let seeker = gather_search::Seeker {
            player: player_id,
            position: unit.position(),
            line_of_sight: master.line_of_sight,
            state: unit.life.state,
            current_target: ai.current_target,
            my_pasture: unit.my_pasture,
            min_state: ObjectState::Alive,
        };
        let top_speed = self.top_speed_terms_with(player_id, unit, ai.current_target)?;
        let me_walk = unit.me(self.parts.actions_of(unit.id), top_speed);
        Ok((seeker, me_walk))
    }

    /// [`gather_search::closest_resource_at`] for the unit at `list_index` of player `player_id`
    /// outside its update: the tiles the player explored that its restriction lets it stand on, the
    /// objects the world holds on them, its reach by the planners (slot 0x3d0 with the range 0).
    fn resource_search_from(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        kind: GatherGroup,
    ) -> GenieResult<(Option<ObjectId>, Option<i32>)> {
        let owner = self.players.player(player_id)?;
        let unit = owner.live(list_index)?;
        let mover = self.parts.mover_of(unit.id).ok_or(SEARCHER_UNMOVING)?;
        let (seeker, me_walk) = self.search_seeker(player_id, unit)?;
        let (mask, master_id, at) = (
            mover.stats.restriction_mask,
            unit.life.master_id,
            unit.position(),
        );
        let (players, masters, index) = (
            &self.players,
            MasterTables::new(&self.model.masters, &self.parts.own_master),
            &self.index,
        );
        let by_id = ObjectsRef::of_tables(
            players,
            index,
            masters,
            (
                &self.parts.building,
                &self.parts.combat,
                &self.parts.actions,
                &self.parts.unread,
                self.parts.motion.unlent(),
            ),
            &self.model.roles,
            &self.model.tuning,
        );
        let mut plan = Planning {
            pathing: &mut self.pathing,
            now: self.world_time,
            player_kind: owner.kind,
            object: self.streams.get_mut(Stream::Object),
            groups: &self.groups,
        };
        closest_resource_at(
            &seeker,
            None,
            kind,
            WorkerCount::NO_CAP,
            ResourceWorld {
                spatial: &mut self.spatial,
                mask,
                candidates: Candidates {
                    objects: by_id,
                    me: None,
                },
                reach: UnitReach {
                    mover,
                    at,
                    walker: me_walk,
                    master: master_id,
                    plan: &mut plan,
                    objects: by_id,
                },
            },
            &self.model.tuning.gather_search,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::class;
    use crate::testing::roles;

    /// `BaseWorld::update`'s first update per object: a villager (4) or a fishing ship (21)
    /// works; else a building whose master has a task of action type 0x99 (a mule cart,
    /// 431096717's four carts of player 2) searches; anything else takes nothing.
    #[test]
    fn the_start_pass_works_villagers_and_lets_mule_carts_search() {
        assert_eq!(
            start_of(
                class::VILLAGER,
                MasterKind::COMBAT,
                CartTask::Lacking,
                &roles()
            ),
            Some(Start::Work)
        );
        assert_eq!(
            start_of(
                class::FISHING_SHIP,
                MasterKind::COMBAT,
                CartTask::Lacking,
                &roles()
            ),
            Some(Start::Work)
        );
        assert_eq!(
            start_of(
                ClassId::of_master(99),
                MasterKind::BUILDING,
                CartTask::Held,
                &roles()
            ),
            Some(Start::CartSearch)
        );
        assert_eq!(
            start_of(
                ClassId::of_master(99),
                MasterKind::BUILDING,
                CartTask::Lacking,
                &roles()
            ),
            None
        );
        assert_eq!(
            start_of(
                ClassId::of_master(99),
                MasterKind::COMBAT,
                CartTask::Held,
                &roles()
            ),
            None
        );
        assert_eq!(
            start_of(
                ClassId::of_master(99),
                MasterKind::COMBAT,
                CartTask::Lacking,
                &roles()
            ),
            None
        );
    }
}
