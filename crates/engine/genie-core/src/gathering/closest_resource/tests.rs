//! `ClosestResourceObject` (`FUN_140c49760`): where it searches from and what it finds.

use super::*;
use crate::map::Distance;
use crate::map::Restriction;
use crate::map::TerrainId;
use crate::map::spatial::Placed;
use crate::objects::ListIndex;
use crate::objects::masters::AutoGatherMode;
use crate::objects::masters::MasterId;
use crate::objects::masters::{Master, Masters};
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::ids;
use crate::unit_ai::{Gatherable, NO_POINT, ResourceSearch};
use crate::world::World;
use crate::world::tests::{forage_world, in_update};
use std::sync::Arc;

/// [`forage_world`] whose bushes (master 59) gather by themselves as group 1, its forager and
/// its town centre 700 seeing 4 tiles: the forager's search reaches 9 tiles about where it
/// starts (its sight plus 5), a search from the centre's footprint 14 (the centre's sight plus
/// 10).
fn bush_world() -> World {
    let mut world = forage_world();
    for masters in &mut world.model.masters {
        let masters = Arc::make_mut(masters);
        let bush = masters.get_mut(master_id(59)).unwrap();
        (bush.auto_gather, bush.gather_group) = (AutoGatherMode::ON, GatherGroup::FORAGE);
        masters.get_mut(master_id(120)).unwrap().line_of_sight = Distance::new(4.0);
        masters.get_mut(ids::TOWN_CENTRE).unwrap().line_of_sight = Distance::new(4.0);
    }
    world
}

/// The forager's `ClosestResourceObject` from its update ([`Unit::resource_search_from`]) for
/// group 1, any workers, from the drop site `site`.
fn searched(world: &mut World, site: Option<DropSiteId>) -> GenieResult<Option<ObjectId>> {
    in_update(
        world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            unit.resource_search_from(
                actions,
                step,
                GatherGroup::FORAGE,
                WorkerCount::NO_CAP,
                site,
            )
        },
    )
}

/// The town centre 700's tile (20, 89) is of terrain 1, which the forager's restriction 7 does
/// not let it stand on: the tile lies in another zone than the forager's.
fn centre_apart(world: &mut World) {
    let terrain = world.spatial.terrain.as_mut().unwrap();
    terrain.restrictions[7] = Restriction::new(vec![1.0, 0.0]);
    terrain.kinds[89 * 120 + 20] = TerrainId::named(1);
}

/// The town centre 700 is of master `master`, a copy of the town centre's.
fn centre_of(world: &mut World, master: MasterId) {
    for masters in &mut world.model.masters {
        let centre = Master {
            id: master,
            ..masters.get(ids::TOWN_CENTRE).unwrap().clone()
        };
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([centre])));
    }
    world
        .live_object_mut(PlayerId::new(1), object_id(700))
        .unwrap()
        .life
        .master_id = master;
    let placed = *world.spatial.get(object_id(700)).unwrap();
    world
        .spatial
        .insert(object_id(700), Placed { master, ..placed });
}

/// `FUN_140c49760` from a drop site (`0x140c49d17`): the search starts from the site's
/// footprint, reaching the site's sight plus 10, where the site's tile lies in the forager's
/// zone (`FUN_14118eea0` against `FUN_1411958c0`) or the site is a dock or a port (masters 45 and
/// 2172); else from the site's tile alone, reaching the forager's sight plus 5, without the
/// second pass from the forager. Bush 600 lies 12.5 tiles from the centre.
#[test]
fn search_from_drop_site_starts_at_its_footprint_in_unit_zone_or_at_dock() {
    let centre = Some(DropSiteId::new(object_id(700)));
    let mut world = bush_world();
    assert_eq!(searched(&mut world, centre), Ok(Some(object_id(600))));
    let mut apart = bush_world();
    centre_apart(&mut apart);
    assert_eq!(searched(&mut apart, centre), Ok(None));
    // from the forager without a site: bush 600 lies 3 tiles away
    assert_eq!(searched(&mut apart, None), Ok(Some(object_id(600))));
    let mut dock = bush_world();
    centre_apart(&mut dock);
    centre_of(&mut dock, master_id(45));
    assert_eq!(searched(&mut dock, centre), Ok(Some(object_id(600))));
}

/// The unit AI's `ClosestResourceObject` from its update ([`Unit::resource_search`]) passes over
/// the unit AI's current target (`+0x48`): with bush 600 its target the nearer is 601.
#[test]
fn resource_search_passes_over_unit_ai_current_target() {
    let mut world = bush_world();
    let search = |world: &mut World| {
        in_update(
            world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, actions, step| {
                unit.resource_search(actions, step, GatherGroup::FORAGE, WorkerCount::NO_CAP)
            },
        )
    };
    assert_eq!(search(&mut world), Ok(Some(object_id(600))));
    let id = world.players[1].objects[ListIndex::new(0)].id;
    world.parts.unit_ai.get_mut(id).unwrap().current_target = Some(object_id(600));
    assert_eq!(search(&mut world), Ok(Some(object_id(601))));
}

/// The unit AI's `ClosestResourceObject` while it runs ([`UnitView::closest_gatherable`]): what
/// it found with its place and its master's class; none where nothing is found (the forager
/// below the state asked for).
#[test]
fn closest_gatherable_answers_found_object_place_and_class() {
    let mut world = bush_world();
    let gatherable = |world: &mut World, min_state: ObjectState| {
        let mut found = None;
        world
            .run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
                found = Some(side.closest_gatherable(ResourceSearch {
                    kind: GatherGroup::FORAGE,
                    cap: WorkerCount::NO_CAP,
                    site: None,
                    current: None,
                    point: NO_POINT,
                    min_state,
                })?);
                Ok(())
            })
            .unwrap();
        found.unwrap()
    };
    assert_eq!(
        gatherable(&mut world, ObjectState::Alive),
        Some(Gatherable {
            id: object_id(600),
            position: Position::new(32.5, 89.5, 0.0),
            class: class::CLASS_7,
        })
    );
    assert_eq!(gatherable(&mut world, ObjectState::Dying), None);
}
