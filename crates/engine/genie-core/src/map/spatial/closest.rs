//! `ClosestObject` (`FUN_140c488b0`): the search's pick of the closest object it found, and its tests
//! of each one.

use crate::map::Distance;
use crate::map::spatial::{ClosestChecks, Filter, Placed, Reach, Spatial};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::player_master;
use crate::ownership::artifact::ARTIFACT_KIND;
use crate::players::PlayerId;
use crate::players::{QueryRelation, UnitRelation};
use crate::vision::registry::Record;
use crate::vision::visibility::Seer;
use crate::world::index::ObjectsRef;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::{GenieError, GenieResult};
use std::fmt::Debug;

impl Spatial {
    /// `ClosestObject` (`FUN_140c488b0`) for object `me` over what its search `found`, its
    /// reachability check on (argument 8, every caller's): each record by relation and order:
    /// itself, an object that is gone and one dying (state 7 and up) are passed, so is one the
    /// [`Filter`] leaves out; then one the unit reaches within `SpatialTuning::closest_range` (slot 0x3d0,
    /// `reach`) and nearer than the nearest so far (by the search's distance; a tie goes to the
    /// lower id) is the nearest. Livestock (class 58) counts when the searcher's player owns it or
    /// it is not in state 2, and not while its top action is the Artifact with a point
    /// ([`herded`]: slot 0x288 not −1 and slot 0x530 107, "Sheep is moving"). A searcher of class
    /// 10 passes the objects [`shuns`] names (of master 440 or a master with object flag 0x10; the
    /// game's `+0x6b4` of 1, not met, would let it take them). Not modelled: a workers cap
    /// (argument 6; the objects' `+0x644`), the unbuilt farm test of a search for state 0, the
    /// pastures and farms (class 49). Nothing found answers −1.
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
            // The villagers' class, which gathers from farms (`FUN_141195750`).
            farmer: at
                .is_some_and(|placed| roles.of_class(placed.class).contains(ClassRoles::VILLAGER)),
            trapper: at.is_some_and(|placed| {
                roles
                    .of_master(placed.master)
                    .contains(MasterRoles::TRAPPER)
            }),
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
                let sight = self.sight.as_ref().ok_or(GenieError::unsupported(
                    "a search without the players' sight",
                ))?;
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

/// What `ClosestObject` reads of its searcher (`FUN_140c488b0`): of class 10 it passes the
/// objects `shuns` names; a farmer takes farms, a trapper fish traps ([`farm_kept`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Searcher {
    pub(super) class_10: bool,
    pub(super) farmer: bool,
    pub(super) trapper: bool,
}

/// `ClosestObject`'s tests of the object `id` it found (its seer and its place `placed`) past the
/// relation: the [`Filter`]'s class, master and state; a farm ([`farm_kept`]); livestock
/// (class 58) the searcher's player owns or not in state 2, and not [`herded`]; for a searcher of
/// class 10, not what [`shuns`] names. A workers cap and the unbuilt farm test (a state of 0)
/// are not modelled.
pub(super) fn closest_kept(
    id: ObjectId,
    (seer, placed): (&Seer, &Placed),
    filter: Filter,
    (player, searcher): (PlayerId, Searcher),
    ClosestChecks { objects, roles, .. }: ClosestChecks<'_>,
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
    if filter.workers != WorkerCount::NO_CAP {
        return Err(GenieError::unsupported(
            "ClosestObject's workers cap (+0x644)",
        ));
    }
    if filter.state == Some(ObjectState::Foundation) {
        return Err(GenieError::unsupported("ClosestObject's unbuilt farm test"));
    }
    if roles
        .of_some_class(filter.group)
        .contains(ClassRoles::CLOSEST_PASTURE)
        && !farm_kept(id, placed, filter, searcher, objects, roles)?
    {
        return Ok(false);
    }
    if roles
        .of_some_class(filter.group)
        .contains(ClassRoles::CLOSEST_LIVESTOCK)
        && ((placed.player != player && seer.state == ObjectState::Alive) || herded(objects, id))
    {
        return Ok(false);
    }
    if searcher.class_10 && shuns(objects, roles, placed)? {
        return Ok(false);
    }
    Ok(true)
}

/// `ClosestObject`'s "Sheep is moving" (`FUN_140c488b0`): the object's top action is the Artifact
/// (slot 0x530 107) with a point (slot 0x288, the action's `+0x38`, not −1); `by_id` reads its
/// action layer.
pub(crate) fn herded(by_id: ObjectsRef<'_>, id: ObjectId) -> bool {
    by_id.actions(id).is_some_and(|action| {
        action.top_action_kind() == Some(ARTIFACT_KIND)
            && action
                .list
                .artifact()
                .is_some_and(|artifact| artifact.goal.is_some_and(|position| position.x() != -1.0))
    })
}

/// What a searcher of class 10 passes (`FUN_140c488b0`): the found object's master, read from its
/// player's table, is the petard (master 440) or holds object flag 0x10.
///
/// # Errors
///
/// A found object of a master its player's table does not hold.
pub(super) fn shuns(objects: ObjectsRef<'_>, roles: &Roles, placed: &Placed) -> GenieResult<bool> {
    let master = player_master(objects.masters().players, placed.player, placed.master).ok_or(
        GenieError::unsupported("a found object of a master the slice does not have"),
    )?;
    Ok(roles.of_master(placed.master).contains(MasterRoles::PETARD)
        || master.object_flags.contains(ObjectFlags::SCOUT))
}

/// A farm (class 49) `ClosestObject` found, its workers (`+0x644`) and pasture (`+0x790`,
/// `parent_pasture_id`) read from `objects`: not a pasture's livestock
/// (`+0x790`); a pasture is not modelled; without a master asked for, one without workers that
/// the searcher gathers from (a fish trap for a trapper, any other for a farmer).
pub(super) fn farm_kept(
    id: ObjectId,
    placed: &Placed,
    filter: Filter,
    searcher: Searcher,
    objects: ObjectsRef<'_>,
    roles: &Roles,
) -> GenieResult<bool> {
    let (workers, parent) = objects
        .get(id)
        .map(|unit| (unit.workers, unit.pasture))
        .ok_or(GenieError::unsupported(
            "a found farm the slice does not hold",
        ))?;
    if parent.is_some() {
        return Ok(false);
    }
    if roles
        .of_master(placed.master)
        .contains(MasterRoles::COUNTED_PASTURE)
    {
        return Err(GenieError::unsupported(
            "ClosestObject over pastures (their workers and livestock)",
        ));
    }
    if filter.master.is_none() {
        if WorkerCount::ONE <= workers {
            return Ok(false);
        }
        // The fish trap (raw `0x6163`), which only the fishing ships gather from (`FUN_1411945a0`).
        if !searcher.trapper
            && roles
                .of_master(placed.master)
                .contains(MasterRoles::FISH_TRAP)
        {
            return Ok(false);
        }
        if !searcher.farmer
            && !roles
                .of_master(placed.master)
                .contains(MasterRoles::FISH_TRAP)
        {
            return Ok(false);
        }
    }
    Ok(true)
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
