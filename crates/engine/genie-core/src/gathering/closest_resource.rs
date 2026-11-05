//! `ClosestResourceObject` (`FUN_140c49760`): where it searches from and what it finds; a
//! herded sheep.

use crate::gathering::DropSiteId;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::map::spatial::Spatial;
use crate::math::rng::Stream;
use crate::movement::Mover;
use crate::movement::Planning;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::{MasterTables, OwnerMasters};
use crate::objects::unit::Unit;
use crate::objects::{ObjectState, WorkerCount};
use crate::pathing::zones::{UNKNOWN_RESTRICTION, Zones};
use crate::players::PlayerId;
use crate::unit_ai::world_view::owner_view;
use crate::unit_ai::world_view::{Foreign, UnitView};
use crate::unit_ai::{Gatherable, NO_POINT, ResourceSearch};
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::objects_ref::ObjectsRef;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

/// What `ClosestResourceObject` reads of the world, as the unit's update or its unit AI sees it.
pub(crate) struct ResourceWorld<'world, 'plan> {
    pub(crate) player: PlayerId,
    pub(crate) spatial: &'world mut Spatial,
    pub(crate) zones: &'world mut Zones,
    pub(crate) foreign: Foreign<'world>,
    pub(crate) every: MasterTables<'world>,
    pub(crate) masters: OwnerMasters<'world>,
    pub(crate) plan: Planning<'plan>,
}

/// Where `ClosestResourceObject` (`FUN_140c49760`) searches from, the unit at `unit`: without a
/// site from `point` (at the unit's height), unless both its parts are exactly −1
/// (`DAT_14330ec20`, `UCOMISS`/`JP`/`JNZ`: a part that is not a number differs); with a site
/// (`site`, which replaces the point) and for (−1, −1) from the unit's place.
pub(crate) fn search_origin<T>(point: Point, site: Option<T>, unit: Position) -> Position {
    if site.is_none() && (point.x() != -1.0 || point.y() != -1.0) {
        point.with_height(unit.z())
    } else {
        unit
    }
}

/// [`crate::gathering::gather_search::closest_resource_at`] for `unit` with its mover of gather group `kind` with fewer
/// than `cap` workers (−1 any) from the drop site `site` (if any; one the world does not hold
/// counts as none, `FUN_1415417a0`), its unit AI's current target `current` passed over, below
/// state `min_state` nothing found, and without a site from `point` unless that is (−1, −1)
/// (`param_7`, `param_8`; [`crate::unit_ai::NO_POINT`]: the unit's place, as with a site): the
/// site's place, its master's radii and line of sight, and whether the search starts from its
/// footprint (the site's tile in the unit's zone of its restriction, or a dock: masters 45 and
/// 2172, `0x140c49d17`); the answer and the out argument's steps.
pub(crate) fn closest_resource_of(
    (unit, actions, mover): (&Unit, &Action, Option<&Mover>),
    resource_world: &mut ResourceWorld<'_, '_>,
    others: OthersRef<'_>,
    (kind, cap, site): (GatherGroup, WorkerCount, Option<DropSiteId>),
    (current, min_state, point): (Option<ObjectId>, ObjectState, Point),
    by_id: ObjectsRef<'_>,
    roles: &Roles,
) -> GenieResult<(Option<ObjectId>, Option<i32>)> {
    use crate::gathering::gather_search::closest_resource_at;
    use crate::gathering::gather_search::{self, Candidate, Candidates, ResourceWorld};
    use crate::movement::UnitReach;
    let gather_search_tuning = &by_id.tuning().gather_search;

    let site = match site.filter(|&role_id| resource_world.spatial.get(role_id).is_some()) {
        None => None,
        Some(site) => {
            // the unit itself (a mule cart searches from its own place) or another object
            let (sp, searcher) = if site.id() == unit.id {
                Some((resource_world.player, unit))
            } else {
                others.find(site)
            }
            .ok_or(GenieError::unsupported(
                "a resource search's drop site the slice does not hold",
            ))?;
            let master = searcher.master(resource_world.every, sp)?;
            let footprint =
                roles
                    .of_master(master.id)
                    .contains(MasterRoles::SEARCH_FROM_FOOTPRINT)
                    || {
                        let restriction = mover
                            .ok_or(GenieError::unsupported(
                                "a resource search without movement state",
                            ))?
                            .stats
                            .restriction;
                        let terrain = resource_world.spatial.terrain.as_ref().ok_or(
                            GenieError::unsupported("the map's zones without its terrain"),
                        )?;
                        let values = terrain
                            .restriction(restriction)
                            .ok_or(UNKNOWN_RESTRICTION)?;
                        let zones = resource_world.zones.of(
                            resource_world.spatial.map,
                            &terrain.kinds,
                            values,
                        )?;
                        let zone = |position: Position| {
                            zones.zone(Tile::new(position.x() as i32, position.y() as i32))
                        };
                        zone(searcher.position()) == zone(unit.position())
                    };
            Some(gather_search::SearchSite {
                position: searcher.position(),
                radius: master.radius,
                line_of_sight: master.line_of_sight,
                footprint,
            })
        }
    };
    if resource_world.masters.table.is_none() {
        return Err(GenieError::unsupported(
            "a resource search without its player's masters",
        ));
    }
    let master = unit.master_in(resource_world.masters)?;
    let mover = mover.ok_or(GenieError::unsupported(
        "a resource search without movement state",
    ))?;
    let mask = mover.stats.restriction_mask;
    let player = resource_world.player;
    // the unit out of its slot while it updates, as it stands
    let me = Candidate::of(
        unit.id,
        player,
        unit,
        unit.master(resource_world.every, player)?,
        unit.position(),
    );
    let position = search_origin(point, site, unit.position());
    let seeker = gather_search::Seeker {
        player,
        position,
        line_of_sight: master.line_of_sight,
        state: unit.life.state,
        current_target: current,
        my_pasture: unit.my_pasture,
        min_state,
    };
    closest_resource_at(
        &seeker,
        site.as_ref(),
        kind,
        cap,
        ResourceWorld {
            spatial: resource_world.spatial,
            mask,
            candidates: Candidates {
                objects: by_id,
                me: Some(me),
            },
            reach: UnitReach {
                mover,
                at: unit.position(),
                walker: unit.me(actions),
                master: unit.life.master_id,
                plan: &mut resource_world.plan,
                objects: by_id,
            },
        },
        gather_search_tuning,
    )
}

impl UnitView<'_, '_> {
    /// The unit AI's `ClosestResourceObject` while it runs
    /// ([`crate::unit_ai::world_view::UnitView::closest_resource`]): what it found, with its place
    /// and class.
    pub(crate) fn closest_gatherable(
        &mut self,
        search: ResourceSearch,
    ) -> GenieResult<Option<Gatherable>> {
        let by_id = ObjectsRef::new(
            &self.step.world.players,
            &self.step.world.index,
            &self.step.world.model.masters,
            &self.step.world.parts,
            &self.step.world.model.roles,
            &self.step.world.model.tuning,
        );
        let mut resource_world = ResourceWorld {
            player: self.player,
            spatial: &mut self.step.world.spatial,
            zones: &mut self.step.world.zones,
            foreign: owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?
            .players(),
            every: MasterTables::new(
                &self.step.world.model.masters,
                &self.step.world.parts.own_master,
            ),
            masters: OwnerMasters::new(
                &self.step.world.parts.own_master,
                self.step.world.model.masters.of(self.player),
            ),
            plan: Planning {
                pathing: &mut self.step.world.pathing,
                now: self.step.world.world_time,
                player_kind: self.player_kind,
                object: self.step.world.streams.get_mut(Stream::Object),
                groups: &self.step.world.groups,
            },
        };
        let (id, _) = closest_resource_of(
            (
                self.unit,
                &*self.actions,
                self.step.world.parts.mover_of(self.unit.id),
            ),
            &mut resource_world,
            owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?,
            (search.kind, search.cap, search.site),
            (search.current, search.min_state, search.point),
            by_id,
            &self.step.world.model.roles,
        )?;
        let Some(id) = id else {
            return Ok(None);
        };
        let (owner, unit) = owner_view(
            &self.step.world.players,
            &self.step.world.index,
            self.step.player,
            self.new_owner,
        )?
        .find(id)
        .ok_or(GenieError::unsupported(
            "a resource found that the slice does not hold",
        ))?;
        let class = unit.master(self.step.world.master_tables(), owner)?.class;
        Ok(Some(Gatherable {
            id,
            position: unit.position(),
            class,
        }))
    }
}

impl Unit {
    /// The unit AI's `ClosestResourceObject` (slot 0x80, `FUN_140c49760`) with no drop site, no
    /// point and `param_6` 2 ([`crate::gathering::gather_search::closest_resource`]): the closest object of
    /// gather group `kind` with fewer than `cap` workers (−1 any) the unit reaches (slot 0x3d0
    /// with range 0), from the tiles the player explored that the unit's restriction lets it
    /// stand on; None for none.
    pub(crate) fn resource_search(
        &self,
        actions: &Action,
        step: &mut Step<'_>,
        kind: GatherGroup,
        cap: WorkerCount,
    ) -> GenieResult<Option<ObjectId>> {
        self.resource_search_from(actions, step, kind, cap, None)
    }

    /// [`Unit::resource_search`] from the drop site `site` (if any;
    /// [`crate::gathering::gather_search::closest_resource_at`]): the site's place, its master's radii and
    /// line of sight, and whether the search starts from its footprint (the site's tile in the
    /// unit's zone of its restriction, or a dock: masters 45 and 2172, `0x140c49d17`).
    pub(crate) fn resource_search_from(
        &self,
        actions: &Action,
        step: &mut Step<'_>,
        kind: GatherGroup,
        cap: WorkerCount,
        site: Option<DropSiteId>,
    ) -> GenieResult<Option<ObjectId>> {
        let by_id = ObjectsRef::new(
            &step.world.players,
            &step.world.index,
            &step.world.model.masters,
            &step.world.parts,
            &step.world.model.roles,
            &step.world.model.tuning,
        );
        let others = OthersRef::of(&step.world.players, &step.world.index, step.player)?;
        let current = step
            .world
            .parts
            .unit_ai
            .get(self.id)
            .ok_or(GenieError::unsupported(
                "a resource search without a unit AI",
            ))?
            .current_target;
        let mut resource_world = ResourceWorld {
            player: step.player,
            spatial: &mut step.world.spatial,
            zones: &mut step.world.zones,
            foreign: others.players(),
            every: MasterTables::new(&step.world.model.masters, &step.world.parts.own_master),
            masters: OwnerMasters::new(
                &step.world.parts.own_master,
                step.world.model.masters.of(step.player),
            ),
            plan: Planning {
                pathing: &mut step.world.pathing,
                now: step.world.world_time,
                player_kind: step.world.players.player(step.player)?.kind,
                object: step.world.streams.get_mut(Stream::Object),
                groups: &step.world.groups,
            },
        };
        let search = (current, ObjectState::Alive, NO_POINT);
        closest_resource_of(
            (self, actions, step.world.parts.mover_of(self.id)),
            &mut resource_world,
            others,
            (kind, cap, site),
            search,
            by_id,
            &step.world.model.roles,
        )
        .map(|(id, _)| id)
    }
}
