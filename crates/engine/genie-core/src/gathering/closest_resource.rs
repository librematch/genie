//! `ClosestResourceObject` (`FUN_140c49760`): where it searches from and what it finds; a
//! herded sheep.

use crate::GenieError;
use crate::gathering::DropSiteId;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::map::spatial::BLOCKED;
use crate::map::spatial::Spatial;
use crate::math::rng::Stream;
use crate::movement::Mover;
use crate::movement::Planning;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::{MasterTables, OwnerMasters};
use crate::objects::unit::Unit;
use crate::objects::unit::UnitFlags;
use crate::objects::{ObjectState, WorkerCount};
use crate::ownership::artifact::ARTIFACT_KIND;
use crate::pathing::zones::{UNKNOWN_RESTRICTION, Zones};
use crate::players::PlayerId;
use crate::unit_ai::world_view::owner_view;
use crate::unit_ai::world_view::{Foreign, UnitView};
use crate::unit_ai::{Gatherable, NO_POINT, ResourceSearch};
use crate::world::index::ObjectsRef;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

/// What `ClosestResourceObject` reads of the world, as the unit's update or its unit AI sees it.
pub(crate) struct ResourceWorld<'a, 'b> {
    pub(crate) player: PlayerId,
    pub(crate) spatial: &'a mut Spatial,
    pub(crate) zones: &'a mut Zones,
    pub(crate) foreign: Foreign<'a>,
    pub(crate) every: MasterTables<'a>,
    pub(crate) masters: OwnerMasters<'a>,
    pub(crate) plan: Planning<'b>,
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
    c: &mut ResourceWorld<'_, '_>,
    others: OthersRef<'_>,
    (kind, cap, site): (GatherGroup, WorkerCount, Option<DropSiteId>),
    (current, min_state, point): (Option<ObjectId>, ObjectState, Point),
    by_id: ObjectsRef<'_>,
    roles: &Roles,
) -> Result<(Option<ObjectId>, Option<i32>), GenieError> {
    use crate::gathering::gather_search::closest_resource_at;
    use crate::gathering::gather_search::{self, Candidate, ResourceWorld};
    let gather_search_tuning = &by_id.tuning().gather_search;

    let site = match site.filter(|&s| c.spatial.get(s).is_some()) {
        None => None,
        Some(site) => {
            // the unit itself (a mule cart searches from its own place) or another object
            let (sp, s) = if site.id() == unit.id {
                Some((c.player, unit))
            } else {
                others.find(site)
            }
            .ok_or(GenieError::unsupported(
                "a resource search's drop site the slice does not hold",
            ))?;
            let m = s.master(c.every, sp)?;
            let footprint = roles
                .of_master(m.id)
                .contains(MasterRoles::SEARCH_FROM_FOOTPRINT)
                || {
                    let restriction = mover
                        .ok_or(GenieError::unsupported(
                            "a resource search without movement state",
                        ))?
                        .stats
                        .restriction;
                    let terrain = c.spatial.terrain.as_ref().ok_or(GenieError::unsupported(
                        "the map's zones without its terrain",
                    ))?;
                    let values = terrain
                        .restriction(restriction)
                        .ok_or(UNKNOWN_RESTRICTION)?;
                    let zones = c.zones.of(c.spatial.map, &terrain.kinds, values)?;
                    let zone = |p: Position| zones.zone(Tile::new(p.x() as i32, p.y() as i32));
                    zone(s.position()) == zone(unit.position())
                };
            Some(gather_search::SearchSite {
                position: s.position(),
                radius: m.radius,
                line_of_sight: m.line_of_sight,
                footprint,
            })
        }
    };
    if c.masters.table.is_none() {
        return Err(GenieError::unsupported(
            "a resource search without its player's masters",
        ));
    }
    let master = unit.master_in(c.masters)?;
    let mover = mover.ok_or(GenieError::unsupported(
        "a resource search without movement state",
    ))?;
    let mask = mover.stats.restriction_mask;
    let (player, me_id, every) = (c.player, unit.id, c.every);
    let candidate = |id: ObjectId, owner: PlayerId, u: &Unit, position: Position| {
        let m = u.master(every, owner)?;
        Ok(Candidate {
            id,
            player: owner,
            class: m.class,
            master: m.id,
            state: u.life.state,
            workers: u.workers,
            position,
            pasture: u.pasture,
            untargetable: u.flags.contains(UnitFlags::UNTARGETABLE),
            attribute: u.life.attribute_kind,
            amount: u.life.amount,
            auto_gather: m.auto_gather.is_on(),
            gather_group: m.gather_group,
        })
    };
    let me = candidate(me_id, player, unit, unit.position())?;
    let objects = |spatial: &Spatial, tile: Tile| -> Result<Vec<Candidate>, GenieError> {
        spatial
            .objects_on(tile)
            .map(|(id, p)| {
                if id == me_id {
                    return Ok(me);
                }
                let (owner, u) = others.find(id).ok_or(GenieError::unsupported(
                    "an object on a tile the slice does not hold",
                ))?;
                candidate(id, owner, u, p.position)
            })
            .collect()
    };
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
    let me_walk = unit.me(actions);
    let me_at = unit.position();
    let master_id = unit.life.master_id;
    let plan = &mut c.plan;
    let blocked = mask.blocked() | BLOCKED;
    closest_resource_at(
        &seeker,
        site.as_ref(),
        kind,
        cap,
        ResourceWorld {
            map: c.spatial.map,
            world: c.spatial,
            explored: &|spatial, tile| spatial.explored(player, tile),
            passable: &|spatial, tile| spatial.tiles().word(tile).is_some_and(|w| w & blocked == 0),
            objects: &objects,
            reach: &mut |spatial, id| {
                mover.reaches_object(
                    (me_at, &me_walk),
                    (spatial, by_id),
                    plan,
                    id,
                    Distance::ZERO,
                    master_id,
                )
            },
        },
        gather_search_tuning,
    )
}

/// `ClosestObject`'s "Sheep is moving" (`FUN_140c488b0`): the object's top action is the Artifact
/// (slot 0x530 107) with a point (slot 0x288, the action's `+0x38`, not −1); `by_id` reads its
/// action layer.
pub(crate) fn herded(by_id: ObjectsRef<'_>, id: ObjectId) -> bool {
    by_id.actions(id).is_some_and(|a| {
        a.top_action_kind() == Some(ARTIFACT_KIND)
            && a.list
                .artifact()
                .is_some_and(|a| a.goal.is_some_and(|g| g.x() != -1.0))
    })
}

impl UnitView<'_, '_> {
    /// The unit AI's `ClosestResourceObject` while it runs
    /// ([`crate::unit_ai::UnitSide::closest_resource`]): what it found, with its place and class.
    pub(crate) fn closest_gatherable(
        &mut self,
        search: ResourceSearch,
    ) -> Result<Option<Gatherable>, GenieError> {
        let by_id = ObjectsRef::new(
            &self.c.w.players,
            &self.c.w.index,
            &self.c.w.model.masters,
            &self.c.w.parts,
            &self.c.w.model.roles,
            &self.c.w.model.tuning,
        );
        let mut w = ResourceWorld {
            player: self.player,
            spatial: &mut self.c.w.spatial,
            zones: &mut self.c.w.zones,
            foreign: owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
                self.new_owner,
            )?
            .players(),
            every: MasterTables::new(&self.c.w.model.masters, &self.c.w.parts.own_master),
            masters: OwnerMasters::new(
                &self.c.w.parts.own_master,
                self.c.w.model.masters.of(self.player),
            ),
            plan: Planning {
                pathing: &mut self.c.w.pathing,
                now: self.c.w.world_time,
                player_kind: self.player_kind,
                object: self.c.w.streams.get(Stream::Object),
                groups: &self.c.w.groups,
            },
        };
        let (id, _) = closest_resource_of(
            (
                self.unit,
                &*self.actions,
                self.c.w.parts.mover_of(self.unit.id),
            ),
            &mut w,
            owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
                self.new_owner,
            )?,
            (search.kind, search.cap, search.site),
            (search.current, search.min_state, search.point),
            by_id,
            &self.c.w.model.roles,
        )?;
        let Some(id) = id else {
            return Ok(None);
        };
        let (owner, u) = owner_view(
            &self.c.w.players,
            &self.c.w.index,
            self.c.player,
            self.new_owner,
        )?
        .find(id)
        .ok_or(GenieError::unsupported(
            "a resource found that the slice does not hold",
        ))?;
        let class = u.master(self.c.w.master_tables(), owner)?.class;
        Ok(Some(Gatherable {
            id,
            position: u.position(),
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
        c: &mut Step<'_>,
        kind: GatherGroup,
        cap: WorkerCount,
    ) -> Result<Option<ObjectId>, GenieError> {
        self.resource_search_from(actions, c, kind, cap, None)
    }

    /// [`Unit::resource_search`] from the drop site `site` (if any;
    /// [`crate::gathering::gather_search::closest_resource_at`]): the site's place, its master's radii and
    /// line of sight, and whether the search starts from its footprint (the site's tile in the
    /// unit's zone of its restriction, or a dock: masters 45 and 2172, `0x140c49d17`).
    pub(crate) fn resource_search_from(
        &self,
        actions: &Action,
        c: &mut Step<'_>,
        kind: GatherGroup,
        cap: WorkerCount,
        site: Option<DropSiteId>,
    ) -> Result<Option<ObjectId>, GenieError> {
        let by_id = ObjectsRef::new(
            &c.w.players,
            &c.w.index,
            &c.w.model.masters,
            &c.w.parts,
            &c.w.model.roles,
            &c.w.model.tuning,
        );
        let others = OthersRef::of(&c.w.players, &c.w.index, c.player)?;
        let current =
            c.w.parts
                .unit_ai
                .get(self.id)
                .ok_or(GenieError::unsupported(
                    "a resource search without a unit AI",
                ))?
                .current_target;
        let mut w = ResourceWorld {
            player: c.player,
            spatial: &mut c.w.spatial,
            zones: &mut c.w.zones,
            foreign: others.players(),
            every: MasterTables::new(&c.w.model.masters, &c.w.parts.own_master),
            masters: OwnerMasters::new(&c.w.parts.own_master, c.w.model.masters.of(c.player)),
            plan: Planning {
                pathing: &mut c.w.pathing,
                now: c.w.world_time,
                player_kind: c.w.players.player(c.player)?.kind,
                object: c.w.streams.get(Stream::Object),
                groups: &c.w.groups,
            },
        };
        let search = (current, ObjectState::Alive, NO_POINT);
        closest_resource_of(
            (self, actions, c.w.parts.mover_of(self.id)),
            &mut w,
            others,
            (kind, cap, site),
            search,
            by_id,
            &c.w.model.roles,
        )
        .map(|(id, _)| id)
    }
}
