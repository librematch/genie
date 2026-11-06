//! `ClosestObject` (`FUN_140c488b0`): the search's pick of the closest object it found, and its tests
//! of each one.

use crate::GenieResult;
use crate::map::Distance;
use crate::map::spatial::search::NO_SIGHT;
use crate::map::spatial::{ClosestChecks, Filter, Placed, Reach, Spatial};
use crate::math::fmath::below_or_unordered;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObjectFlags;
use crate::ownership::artifact::ARTIFACT_KIND;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::players::{QueryRelation, UnitRelation};
use crate::unit_ai::world_view::NOT_HELD;
use crate::vision::registry::Record;
use crate::vision::visibility::Seer;
use crate::world::NO_ATTRIBUTE;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::objects_ref::ObjectsRef;
use std::fmt::Debug;

impl Spatial {
    /// `ClosestObject` (`FUN_140c488b0`) for object `me` over what its search `found`, its
    /// reachability check on (argument 8, every caller's): each record by relation and order:
    /// itself, an object that is gone and one dying (state 7 and up) are passed, so is one the
    /// [`Filter`] leaves out; then one the unit reaches within `SpatialTuning::closest_range` (slot 0x3d0,
    /// `reach`) and nearer than the nearest so far (by the search's distance; a tie goes to the
    /// lower id) is the nearest. Livestock (class 58) counts when the searcher's player owns it or
    /// it is not in state 2, and not while its top action is the Artifact with a point
    /// ([`herded`]: slot 0x288 not −1 and slot 0x530 107, "Sheep is moving"; from type 70 on). A
    /// searcher of class
    /// 10 passes the objects [`shuns`] names (of master 440 or a master with object flag 0x10; the
    /// game's `+0x6b4` of 1, not met, would let it take them). The found object's workers
    /// (`+0x644`) meet the cap (argument 6) and a search for state 0's unbuilt farm test
    /// ([`closest_kept`]), and a farm the searcher gathers from ([`farm_kept`]: the pastures
    /// with room and livestock). Nothing found answers −1.
    pub(super) fn closest_object(
        &mut self,
        me: ObjectId,
        player: PlayerId,
        filter: Filter,
        reach: &mut Reach<'_>,
        checks: ClosestChecks<'_>,
    ) -> GenieResult<(Option<ObjectId>, f32)> {
        let (roles, spatial_tuning) = (checks.roles, checks.spatial_tuning);
        let at = self.objects.get(me);
        let searcher = Searcher {
            class_10: at
                .is_some_and(|placed| roles.of_class(placed.class).contains(ClassRoles::PREDATOR)),
            farmer: at.is_some_and(|placed| gathers_farms(placed.class, roles)),
            trapper: at.is_some_and(|placed| gathers_fish_traps(placed.master, roles)),
        };
        let spared = filter.spared.map(QueryRelation::list);
        let mut best: Option<(ObjectId, f32)> = None;
        for relation in UnitRelation::ALL {
            // no filter (−1) keeps every list
            let related = filter
                .relation
                .is_none_or(|query_relation| query_relation.keeps(relation));
            // each record is read where it is tested: the reachability check is handed the
            // spatial index, which it writes
            let mut next = 0;
            while let Some(record) = self.last_found()?[relation].get(next).copied() {
                next += 1;
                if record.id == me {
                    continue;
                }
                let sight = self.sight.as_ref().ok_or(NO_SIGHT)?;
                let (Some(seer), Some(placed)) =
                    (sight.seer(record.id), self.objects.get(record.id))
                else {
                    continue;
                };
                if ObjectState::Removed <= seer.state || !related || spared == Some(relation) {
                    continue;
                }
                if !closest_kept(
                    record.id,
                    (seer, placed),
                    filter,
                    (player, searcher),
                    checks,
                )? {
                    continue;
                }
                if filter.reach
                    && !reach(self, record.id, Distance::new(spatial_tuning.closest_range))?
                {
                    continue;
                }
                best = nearer(best, &record);
            }
        }
        // the distance starts at `DAT_14330ec20` (−1) without a find
        Ok(best.map_or((None, -1.0), |(id, distance)| (Some(id), distance)))
    }
}

/// `FUN_141195750`: an object of class `class` gathers from farms, a villager (class 4).
pub(super) fn gathers_farms(class: ClassId, roles: &Roles) -> bool {
    roles.of_class(class).contains(ClassRoles::VILLAGER)
}

/// `FUN_1411945a0`: an object of master `master` gathers from fish traps, a fishing ship
/// (masters 13, 2356 and 2171).
pub(super) fn gathers_fish_traps(master: MasterId, roles: &Roles) -> bool {
    roles.of_master(master).contains(MasterRoles::TRAPPER)
}

/// What `ClosestObject` reads of the world beside its objects: the attribute of a pasture's
/// workers (290) and whether the loaded world's content mode (`+0x6b4`, [`World::mode_680`])
/// is 1, which lets a searcher of class 10 take what [`shuns`] names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClosestWorld {
    /// The owner's attribute of a pasture's workers ([`AttributeSlots::pasture_workers`]).
    ///
    /// [`AttributeSlots::pasture_workers`]: crate::world::layer::AttributeSlots::pasture_workers
    pub(crate) pasture_workers: AttributeId,
    /// The content mode `+0x6b4` is 1.
    pub(crate) mode_one: bool,
}

impl World {
    /// What `ClosestObject` reads of the world beside its objects ([`ClosestWorld`]).
    #[must_use]
    pub(crate) const fn closest_world(&self) -> ClosestWorld {
        ClosestWorld {
            pasture_workers: self.model.slots.attribute.pasture_workers,
            mode_one: self.mode_680.is_one(),
        }
    }
}

/// What `ClosestObject` reads of its searcher (`FUN_140c488b0`): of class 10 it passes the
/// objects `shuns` names; a farmer takes farms, a trapper fish traps ([`farm_kept`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Searcher {
    pub(super) class_10: bool,
    pub(super) farmer: bool,
    pub(super) trapper: bool,
}

/// `ClosestObject`'s tests of the object `id` it found (its seer and its place `placed`) past the
/// relation: the [`Filter`]'s class, master and state; its workers (`+0x644`) below the cap
/// (argument 6, "Too many workers"); for a search of state 0, a farm (masters 50 and 1187)
/// without workers ("Unbuilt farm that already has workers"); a farm ([`farm_kept`]); livestock
/// (class 58) the searcher's player owns or not in state 2, and not [`herded`]; for a searcher of
/// class 10, not what [`shuns`] names unless the content mode is 1.
pub(super) fn closest_kept(
    id: ObjectId,
    (seer, placed): (&Seer, &Placed),
    filter: Filter,
    (player, searcher): (PlayerId, Searcher),
    ClosestChecks {
        objects,
        roles,
        world,
        ..
    }: ClosestChecks<'_>,
) -> GenieResult<bool> {
    if filter
        .group
        .is_some_and(|class_id| placed.class != class_id)
    {
        return Ok(false);
    }
    if filter
        .master
        .is_some_and(|master_id| placed.master != master_id)
    {
        return Ok(false);
    }
    if filter.state.is_some_and(|state| seer.state != state) {
        return Ok(false);
    }
    let workers = || objects.get(id).map(|unit| unit.workers).ok_or(NOT_HELD);
    if filter.workers != WorkerCount::NO_CAP && filter.workers <= workers()? {
        return Ok(false);
    }
    if filter.state == Some(ObjectState::Foundation)
        && roles
            .of_master(placed.master)
            .intersects(MasterRoles::FARM | MasterRoles::RICE_FARM)
        && WorkerCount::ZERO < workers()?
    {
        return Ok(false);
    }
    if roles
        .of_some_class(filter.group)
        .contains(ClassRoles::CLOSEST_PASTURE)
        && !farm_kept(id, seer, placed, filter, searcher, objects, world)?
    {
        return Ok(false);
    }
    if roles
        .of_some_class(filter.group)
        .contains(ClassRoles::CLOSEST_LIVESTOCK)
        && ((placed.player != player && seer.state == ObjectState::Alive)
            || herded(objects, id, placed)?)
    {
        return Ok(false);
    }
    if searcher.class_10 && !world.mode_one && shuns(objects, roles, id, placed)? {
        return Ok(false);
    }
    Ok(true)
}

/// `ClosestObject`'s "Sheep is moving" (`FUN_140c488b0`) of the object `id` at `placed`: its slot
/// 0x288 is not −1 and its slot 0x530 is 107. Below master type 70 slot 0x288 is −1
/// (`FUN_14106c3e0`). From 70 on (`FUN_141082dc0`) it is the top action's point (`+0x38`), and
/// slot 0x530 (`RGE_Action_Object::actionType`) the top action's type: the Artifact with a point.
/// `by_id` reads its master and its action layer.
///
/// # Errors
///
/// A found object without its master.
pub(crate) fn herded(by_id: ObjectsRef<'_>, id: ObjectId, placed: &Placed) -> GenieResult<bool> {
    if by_id.masters().of_placed(id, placed)?.kind < MasterKind::COMBAT {
        return Ok(false);
    }
    Ok(by_id.actions(id).is_some_and(|action| {
        action.top_action_kind() == Some(ARTIFACT_KIND)
            && action
                .list
                .artifact()
                .is_some_and(|artifact| artifact.goal.is_some_and(|position| position.x() != -1.0))
    }))
}

/// What a searcher of class 10 passes (`FUN_140c488b0`): the found object `id`'s master (its own
/// first, [`crate::objects::masters::MasterTables::of_placed`]) is the petard (master 440) or
/// holds object flag 0x10.
///
/// # Errors
///
/// A found object without its master.
pub(super) fn shuns(
    objects: ObjectsRef<'_>,
    roles: &Roles,
    id: ObjectId,
    placed: &Placed,
) -> GenieResult<bool> {
    let master = objects.masters().of_placed(id, placed)?;
    Ok(roles.of_master(placed.master).contains(MasterRoles::PETARD)
        || master.object_flags.contains(ObjectFlags::SCOUT))
}

/// A farm (class 49) `ClosestObject` found, its workers (`+0x644`) and pasture (`+0x790`,
/// `parent_pasture_id`) read from `objects`: not a pasture's livestock (`+0x790`); a pasture
/// (masters 1893, 1897) in state 2 as a building (type 80) that is full or without livestock
/// is passed ([`pasture_open`]); then, without a master asked for, a pasture or an object
/// without workers that the searcher gathers from: a fish trap for a trapper
/// ([`gathers_fish_traps`]), any other for a farmer ([`gathers_farms`]).
///
/// # Errors
///
/// A found object the world does not hold, its master, and its owner's attribute 290.
pub(super) fn farm_kept(
    id: ObjectId,
    seer: &Seer,
    placed: &Placed,
    filter: Filter,
    searcher: Searcher,
    objects: ObjectsRef<'_>,
    world: ClosestWorld,
) -> GenieResult<bool> {
    let roles = objects.roles();
    let (workers, parent) = objects
        .get(id)
        .map(|unit| (unit.workers, unit.pasture))
        .ok_or(NOT_HELD)?;
    if parent.is_some() {
        return Ok(false);
    }
    let pasture = roles
        .of_master(placed.master)
        .contains(MasterRoles::COUNTED_PASTURE);
    if pasture
        && seer.state == ObjectState::Alive
        && objects.masters().of_placed(id, placed)?.kind == MasterKind::BUILDING
        && !pasture_open(id, placed.player, objects, world)?
    {
        return Ok(false);
    }
    if filter.master.is_some() {
        return Ok(true);
    }
    if !pasture && WorkerCount::ONE <= workers {
        return Ok(false);
    }
    let fish_trap = roles
        .of_master(placed.master)
        .contains(MasterRoles::FISH_TRAP);
    if fish_trap {
        return Ok(searcher.trapper);
    }
    Ok(searcher.farmer)
}

/// `ClosestObject`'s test of a pasture `id` of player `owner` (`FUN_140c488b0`): its livestock's
/// workers (slot 0x780, [`ObjectsRef::livestock_workers`]; `CVTDQ2PS`) below the owner's
/// attribute 290 (`COMISS`, `JC`: a NaN is below), and it holds livestock (slot 0x788,
/// `FUN_1411c5db0`: its list's length).
///
/// # Errors
///
/// An owner without attribute 290.
fn pasture_open(
    id: ObjectId,
    owner: PlayerId,
    objects: ObjectsRef<'_>,
    world: ClosestWorld,
) -> GenieResult<bool> {
    let room = objects
        .attribute(owner, world.pasture_workers)
        .ok_or(NO_ATTRIBUTE)?;
    let workers = objects.livestock_workers(id);
    let livestock = objects
        .building(id)
        .map_or(0, |building| building.livestock.len());
    Ok(below_or_unordered(workers.as_f32(), room.get()) && livestock != 0)
}

/// The nearer of the nearest so far, `best`, and `record`, by the search's distance; a tie
/// goes to the lower id (a NaN distance so far gives way).
pub(super) fn nearer(best: Option<(ObjectId, f32)>, record: &Record) -> Option<(ObjectId, f32)> {
    match best {
        Some((id, distance)) if distance <= record.distance.get() => {
            if record.distance == Distance::new(distance) && record.id < id {
                Some((record.id, distance))
            } else {
                best
            }
        }
        _ => Some((record.id, record.distance.get())),
    }
}
