//! A builder's search for more work once its Build ended without queued orders (the Build's
//! case 0xdf): a resource around the building it finished, as the unit AI's
//! `ClosestResourceObject` names it.

use crate::map::Distance;
use crate::map::spatial::Spatial;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::masters::MasterId;
use crate::objects::masters::OwnerMasters;
use crate::objects::unit::Unit;
use crate::orders::OrderSource;
use crate::orders::Work;
use crate::unit_ai::task_finder::drop_site;
use crate::world::index::ObjectsRef;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

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
    /// no carried-resource test, `+0xa14` as it stands outside slot 0x128: 1; the caller's).
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
        /// The reach a line of sight below the floor takes (the Build's immediate 4.0).
        const REACH_LOW: f32 = 4.0;
        let masters =
            step.w
                .model
                .masters
                .get(step.player.index())
                .ok_or(GenieError::unsupported(
                    "a builder without its player's masters",
                ))?;
        let master = self.master_in(OwnerMasters::new(&step.w.parts.own_master, Some(masters)))?;
        let site = masters.get(built).ok_or(GenieError::unsupported(
            "a building of a master the slice does not have",
        ))?;
        let at = *step.w.spatial.get(building).ok_or(GenieError::unsupported(
            "a builder's building without a place",
        ))?;
        let entry = drop_site(
            &step.w.model.drop_sites,
            built,
            master.class,
            step.w.players.player(step.player)?.civ,
        );
        let los = site.line_of_sight.get();
        let held = if step.w.model.tuning.gather_search.reach_floor <= los {
            los
        } else {
            REACH_LOW
        };
        let reach = if held <= step.w.model.tuning.gather_search.reach_cap {
            held
        } else {
            step.w.model.tuning.gather_search.reach_cap
        };
        let builder = gather_search::Builder {
            position: self.position(),
            radius: Distance::new(master.radius.x()),
            building: at.position,
            range: reach as i32 as i16,
        };
        let mover = step
            .w
            .parts
            .mover_of(self.id)
            .ok_or(GenieError::unsupported("a builder without movement state"))?;
        let restriction = mover.stats.restriction;
        let objects = ObjectsRef::new(
            &step.w.players,
            &step.w.index,
            &step.w.model.masters,
            &step.w.parts,
            &step.w.model.roles,
            &step.w.model.tuning,
        );
        // the builder out of its slot while it updates, as it stands
        let me = Candidate::of(
            self.id,
            step.player,
            self,
            self.master(objects.masters(), step.player)?,
            self.position(),
        );
        let spatial: &Spatial = &step.w.spatial;
        let found = {
            let planner = step.w.pathing.planner(restriction, spatial.tiles()).ok_or(
                GenieError::unsupported("a builder's restriction without a planner"),
            )?;
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
        let spatial: &Spatial = &step.w.spatial;
        let target = target
            .filter(|&object_id| spatial.get(object_id).is_some())
            .unwrap_or(found.id);
        let point = spatial
            .get(target)
            .ok_or(GenieError::unsupported(
                "a builder's new work without a place",
            ))?
            .position;
        Ok(Some(Work {
            target: Some(target),
            point,
            carry_test: false,
            filter: None,
            source: OrderSource::Direct,
        }))
    }
}
