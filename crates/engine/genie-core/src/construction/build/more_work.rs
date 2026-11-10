//! A builder's search for more work once its Build ended without queued orders (the Build's
//! case 0xdf): a resource around the building it finished, as the unit AI's
//! `ClosestResourceObject` names it.

use crate::construction::build::BUILDER_UNMOVING;
use crate::construction::build::BUILDING_UNPLACED;
use crate::map::Distance;
use crate::map::spatial::Spatial;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::at_most_or_unordered;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::masters::MasterId;
use crate::objects::masters::OwnerMasters;
use crate::objects::unit::Unit;
use crate::orders::Work;
use crate::unit_ai::task_finder::drop_site;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::GatherSearchTuning;
use crate::world::objects_ref::ObjectsRef;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

/// The reach a line of sight below the floor takes (the Build's immediate 4.0, `0x140ab0b5a`).
const REACH_LOW: Distance = Distance::new(4.0);

/// The Build's case 0xdf (`0x140ab0b52..0x140ab0bb8`): the reach of its search for more work
/// about a building of line of sight `line_of_sight` ([`Unit::more_work`]). Below the tuning's
/// floor the reach is the Build's 4.0, and it is at most the tuning's cap. A NaN line of sight
/// passes both compares: the `CMOVBE` after `COMISS floor, los` takes it, and the `JA` after
/// `COMISS reach, cap` is not taken.
fn more_work_reach(line_of_sight: Distance, gather_search_tuning: &GatherSearchTuning) -> Distance {
    let held = if at_most_or_unordered(gather_search_tuning.reach_floor, line_of_sight.get()) {
        line_of_sight
    } else {
        REACH_LOW
    };
    if at_most_or_unordered(held.get(), gather_search_tuning.reach_cap) {
        held
    } else {
        Distance::new(gather_search_tuning.reach_cap)
    }
}

impl Unit {
    /// The Build's case 0xdf once its work ended without queued orders (`0x140ab0b43`): the
    /// builder looks for more work around building `building` of master `built`
    /// ([`crate::gathering::gather_search::more_work`] with the drop site's entry for the building, the
    /// builder's class and civ, the reach the building's line of sight held to 4 to 20, and the
    /// planner of the builder's restriction). Nothing found: None, the caller lets the foundation
    /// go. A farm found is not modelled; else the unit AI's `ClosestResourceObject` for the found
    /// object's gather group and workers cap ([`crate::gathering::gather_search::gather_group`],
    /// [`crate::gathering::gather_search::closest_resource`]) names the target, the found object when it
    /// names none, and slot 0x7e8 works it at its place (the work flag 1 only logged, any task,
    /// the retargeting byte 0, the unit's `+0xa14` as it stands; the caller's).
    pub(crate) fn more_work(
        &self,
        actions: &Action,
        step: &mut Step<'_>,
        building: ObjectId,
        built: MasterId,
        roles: &Roles,
    ) -> GenieResult<Option<Work>> {
        use crate::gathering::gather_search::{self, Candidate, Candidates};
        use crate::gathering::gather_search::{gather_group, more_work as gather_search_more_work};
        let masters =
            step.world
                .model
                .masters
                .get(step.player.index())
                .ok_or(GenieError::unsupported(
                    "a builder without its player's masters",
                ))?;
        let master = self.master_in(OwnerMasters::new(
            &step.world.parts.own_master,
            Some(masters),
        ))?;
        let site = masters.get(built).ok_or(GenieError::unsupported(
            "a building of a master the slice does not have",
        ))?;
        let at = *step.world.spatial.get(building).ok_or(BUILDING_UNPLACED)?;
        let entry = drop_site(
            &step.world.model.drop_sites,
            built,
            master.class,
            step.world.players.player(step.player)?.civ,
        );
        let reach = more_work_reach(site.line_of_sight, &step.world.model.tuning.gather_search);
        let builder = gather_search::Builder {
            position: self.position(),
            radius: Distance::new(master.radius.x()),
            building: at.position,
            range: cvttss2si(reach.get()) as i16,
        };
        let mover = step.world.parts.mover_of(self.id).ok_or(BUILDER_UNMOVING)?;
        let restriction = mover.stats.restriction;
        let objects = ObjectsRef::new(
            &step.world.players,
            &step.world.index,
            &step.world.model.masters,
            &step.world.parts,
            &step.world.model.roles,
            &step.world.model.tuning,
            step.world.treaty.as_ref(),
        );
        // the builder out of its slot while it updates, as it stands
        let me = Candidate::of(
            self.id,
            step.player,
            self,
            self.master(objects.masters(), step.player)?,
            self.position(),
        );
        let spatial: &Spatial = &step.world.spatial;
        let found = {
            let planner = step
                .world
                .pathing
                .planner(restriction, spatial.tiles())?
                .ok_or(GenieError::unsupported(
                    "a builder's restriction without a planner",
                ))?;
            gather_search_more_work(
                entry,
                &builder,
                (spatial, planner),
                &Candidates {
                    objects,
                    me: Some(me),
                },
                roles,
            )?
        };
        let Some(found) = found else {
            return Ok(None);
        };
        if roles.of_class(found.class).contains(ClassRoles::FARM) {
            return Err(GenieError::unsupported(
                "a builder's search for more work finding a farm",
            ));
        }
        let (kind, cap) = gather_group(found.attribute, roles);
        let target = self.resource_search(actions, step, kind, cap)?;
        let spatial: &Spatial = &step.world.spatial;
        let target = target
            .filter(|&object_id| spatial.get(object_id).is_some())
            .unwrap_or(found.id);
        let point = spatial
            .get(target)
            .ok_or(GenieError::unsupported(
                "a builder's new work without a place",
            ))?
            .position;
        Ok(Some(Work::on(Some(target), point)))
    }
}

#[cfg(test)]
mod tests;
