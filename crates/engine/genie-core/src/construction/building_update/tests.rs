//! A building's own update: its links to removed objects, a gate's update, a farm's stage, the
//! completion and the whole seconds; the idle swap and the blast at death.

use crate::GenieResult;
use crate::combat::hp_transforms::HP_TRANSFORM;
use crate::construction::MainBuildingId;
use crate::construction::building_update::{FARM_TERRAIN_LACKING, blasts_at_death};
use crate::garrison::HOUSED_NOT_LIVE;
use crate::gathering::hunt::tests::walker;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::TerrainId;
use crate::map::Tile;
use crate::map::spatial::Placed;
use crate::math::rng::Well512;
use crate::movement::MoveAlgorithm;
use crate::objects::Graphics;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::layers::{Motion, MotionMut};
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::ActionPart;
use crate::objects::masters::BuildingMaster;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatPart;
use crate::objects::masters::DamageShare;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::RestrictionField;
use crate::objects::masters::Task;
use crate::objects::masters::WorkValue;
use crate::objects::sprite::{ActiveSprite, Graphic};
use crate::objects::unit::Unit;
use crate::objects::{DamagedTimer, FacetCount, Frame};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::test_ids::{master_id, object_id};
use crate::testing::class;
use crate::testing::ids;
use crate::testing::roles;
use crate::testing::scene::{Scene, SceneObject};
use crate::testing::tuning;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::tests::{
    annexed_house, centre_seconds, garrisoned_centre, gate_update_of, house_builder, in_update,
    site_mut,
};
use crate::world::{GATE_CLOSE_DELAY, Gate, Seconds, WholeSeconds};
use std::sync::Arc;

/// Player `index`.
const fn player(index: u8) -> PlayerId {
    PlayerId::new(index)
}

/// The reason of `result`'s error, or its value.
fn reason_of<T>(result: GenieResult<T>) -> Result<T, &'static str> {
    result.map_err(|error| error.reason())
}

/// A master of `kind` and `class`, its id `id`.
fn master(id: MasterId, kind: MasterKind, class: ClassId) -> Master {
    Master {
        id,
        kind,
        class,
        hit_points: MasterHitPoints::of_data(40),
        radius: Footprint::new(0.5, 0.5),
        ..Master::default()
    }
}

/// `TRIBE_Combat_Object::update`'s test for the blast at death: a unit of a combat object or a
/// building (types 70 and 80) below 1 hit point in state 2, of a master that blasts (the
/// demolition ships and rafts, 527, 528, 1104, the incendiary ones, 2133 to 2135) or of a class
/// that does (the petard's 35, the land mine's 64).
#[test]
fn unit_blasts_at_death_below_one_hit_point_by_its_master_or_class() {
    let life = |hp: f32, state| Lifecycle {
        state,
        hp: HitPoints::new(hp),
        ..Lifecycle::default()
    };
    let blasts =
        |master: &Master, life: &Lifecycle| blasts_at_death(master, life, &roles(), &tuning().turn);
    let alive = life(0.5, ObjectState::Alive);
    let ship = master(ids::ID_527, MasterKind::COMBAT, class::CLASS_0);
    let petard = master(master_id(440), MasterKind::COMBAT, class::CLASS_35);
    let mine = master(master_id(99), MasterKind::BUILDING, class::CLASS_64);
    for blaster in [
        &ship,
        &master(ids::ID_528, MasterKind::COMBAT, class::CLASS_0),
        &master(ids::ID_1104, MasterKind::COMBAT, class::CLASS_0),
        &master(ids::ID_2133, MasterKind::COMBAT, class::CLASS_0),
        &master(ids::ID_2135, MasterKind::COMBAT, class::CLASS_0),
        &petard,
        &mine,
    ] {
        assert!(blasts(blaster, &alive), "{:?}", blaster.id);
    }
    // at 1 hit point, dying, a soldier, or an object below the combat objects
    assert!(!blasts(&ship, &life(1.0, ObjectState::Alive)));
    assert!(!blasts(&ship, &life(0.5, ObjectState::Dying)));
    assert!(!blasts(
        &master(master_id(4), MasterKind::COMBAT, class::CLASS_0),
        &alive
    ));
    assert!(!blasts(
        &master(ids::ID_527, MasterKind::RGE_COMBAT, class::CLASS_64),
        &alive
    ));
}

/// A demolition ship's master (527, class 0, type 70): 60 hit points, a blast of reach 1 of its
/// whole damage (`area_damage` 1) whose weapon does 10 to armor class 4.
fn demolition_ship() -> Master {
    Master {
        hit_points: MasterHitPoints::of_data(60),
        combat: Some(CombatPart {
            area_effect: Distance::new(1.0),
            area_damage: DamageShare::new(1.0),
            weapons: vec![ClassAmount::new(4, 10)],
            terrain_bonus: RestrictionField::NONE,
            ..CombatPart::default()
        }),
        ..master(ids::ID_527, MasterKind::COMBAT, class::CLASS_0)
    }
}

/// A guard's master (5, class 0, type 70): 40 hit points, armor class 4 of 0.
fn guard() -> Master {
    Master {
        combat: Some(CombatPart {
            armors: vec![ClassAmount::new(4, 0)],
            terrain_bonus: RestrictionField::NONE,
            ..CombatPart::default()
        }),
        ..master(master_id(5), MasterKind::COMBAT, class::CLASS_0)
    }
}

/// A 32-tile map of two players: player 1's demolition ship 7 at (5.5, 5.5) with `hp` hit points,
/// player 2's guards 8 beside it at (6.5, 5.5) and 9 far off at (20.5, 20.5); every player's 500
/// attributes 0.
fn blast_world(hp: f32) -> World {
    let mut world = Scene::open(GridSize::new(32, 32))
        .players(2)
        .object(SceneObject {
            hp: HitPoints::new(hp),
            ..SceneObject::new(
                7,
                player(1),
                demolition_ship(),
                Position::new(5.5, 5.5, 0.0),
            )
        })
        .object(SceneObject::new(
            8,
            player(2),
            guard(),
            Position::new(6.5, 5.5, 0.0),
        ))
        .object(SceneObject::new(
            9,
            player(2),
            guard(),
            Position::new(20.5, 20.5, 0.0),
        ))
        .build();
    for owner in &mut world.players {
        owner.attributes = vec![Amount::ZERO; 500];
    }
    world
}

/// The hit points of [`blast_world`]'s guards after the ship's update ran its blast at death
/// ([`Unit::death_blast`]).
fn guards_after_blast(world: &mut World) -> GenieResult<[Option<f32>; 2]> {
    in_update(
        world,
        (player(1), ListIndex::new(0)),
        50,
        |unit, actions, step| unit.death_blast(actions, step, &roles()),
    )?;
    let hp = |id| {
        world
            .object(object_id(id))
            .map(|(_, unit)| unit.life.hp.get())
    };
    Ok([hp(8), hp(9)])
}

/// `TRIBE_Combat_Object::update`'s blast at death (slot 0x848, `FUN_14107f690`, factor 1, the
/// ship itself left out): a demolition ship below 1 hit point blasts the objects its update
/// gathered about it, written back at once: the guard beside it loses the weapon's 10, the guard
/// out of its reach none. At 1 hit point it blasts nothing.
#[test]
fn demolition_ship_below_one_hit_point_blasts_about_it() {
    assert_eq!(
        guards_after_blast(&mut blast_world(0.5)),
        Ok([Some(30.0), Some(40.0)])
    );
    assert_eq!(
        guards_after_blast(&mut blast_world(1.0)),
        Ok([Some(40.0), Some(40.0)])
    );
}

/// The update of `world`'s object `id` of player `owner` as far as
/// [`Unit::let_removed_links_go`]: its main building and its annexes after it.
fn links_of(
    world: &mut World,
    owner: PlayerId,
    id: ObjectId,
) -> GenieResult<(Option<MainBuildingId>, Vec<ObjectId>)> {
    let list_index = world.players[owner.index()]
        .objects
        .position_of(id)
        .unwrap();
    in_update(world, (owner, list_index), 50, |unit, _, step| {
        unit.let_removed_links_go(step, &roles())?;
        let layer = step.world.parts.building_of(unit.id);
        Ok((layer.annex_of, layer.annexes.ids().collect()))
    })
}

/// The links let go are `TRIBE_Building_Object::update`'s (`0x1411c9000`): a building lets its
/// link to a removed main building go; a unit that is no building, the same link in its layer,
/// keeps it.
#[test]
fn only_building_lets_its_links_to_removed_objects_go() {
    let main = MainBuildingId::new(object_id(20));
    let linked = |id, master| SceneObject {
        annex_of: Some(main),
        ..SceneObject::new(id, player(1), master, Position::new(8.5, 8.5, 0.0))
    };
    let mut world = Scene::open(GridSize::new(16, 16))
        .object(SceneObject {
            state: ObjectState::Removed,
            ..SceneObject::new(
                20,
                player(1),
                master(master_id(20), MasterKind::BUILDING, class::BUILDING),
                Position::new(4.5, 4.5, 0.0),
            )
        })
        .object(linked(
            21,
            master(master_id(21), MasterKind::BUILDING, class::BUILDING),
        ))
        .object(linked(
            22,
            master(master_id(22), MasterKind::COMBAT, class::CLASS_6),
        ))
        .build();
    assert_eq!(
        links_of(&mut world, player(1), object_id(21)),
        Ok((None, vec![]))
    );
    assert_eq!(
        links_of(&mut world, player(1), object_id(22)),
        Ok((Some(main), vec![]))
    );
}

/// A gate state of [`gate_world`]'s gate before its update: `locked`, its close timer at
/// `close_timer` seconds, its first update to come.
fn gate_state(locked: bool, close_timer: f32) -> Gate {
    Gate {
        locked,
        close_timer: Seconds::new(close_timer),
        first_update: true,
        ..Gate::default()
    }
}

/// [`gate_state`] after an update that ran, its close timer at `close_timer` seconds.
fn gate_ran(locked: bool, close_timer: Seconds) -> Gate {
    Gate {
        locked,
        close_timer,
        first_update: false,
        ..Gate::default()
    }
}

/// [`gate_update_of`] of [`gate_world`]'s gate as master `master` with its gate state `gate`,
/// `setup` run first: its master after the update and its gate state.
fn gate_updated(
    master: MasterId,
    gate: Gate,
    setup: impl FnOnce(&mut World),
) -> GenieResult<(MasterId, Gate)> {
    let (master, gate) = gate_update_of(master, Some(gate), setup)?;
    Ok((master, gate.expect("the gate keeps its gate state")))
}

/// [`gate_world`]'s soldier stands aside, away from the gate.
fn soldier_aside(world: &mut World) {
    let placed = *world.spatial.get(object_id(1)).unwrap();
    world.spatial.insert(
        object_id(1),
        Placed {
            position: Position::new(20.0, 30.0, 0.0),
            ..placed
        },
    );
}

/// The master `id` of [`gate_world`]'s gate tables changed by `change`.
fn change_gate_master(world: &mut World, id: i16, change: impl Fn(&mut Master)) {
    for masters in &mut world.model.masters {
        if let Some(master) = Arc::make_mut(masters).get_mut(master_id(id)) {
            change(master);
        }
    }
}

/// The gate's update (`FUN_1411e7730`) runs for a gate in state 2 only, of the gate class
/// (`TRIBE_Building_Object::update` calls it for class 39): a foundation, a dying gate and a
/// building of another class whose master transforms pass it over, their first update kept.
#[test]
fn gate_update_passes_over_gate_not_standing_or_no_gate() {
    let set_state = |state| {
        move |world: &mut World| world.players[1].objects[ListIndex::new(0)].life.state = state
    };
    for state in [ObjectState::Foundation, ObjectState::Dying] {
        assert_eq!(
            gate_updated(master_id(91), gate_state(false, 0.3), set_state(state)),
            Ok((master_id(91), gate_state(false, 0.3)))
        );
    }
    let no_gate = |world: &mut World| {
        change_gate_master(world, 91, |master| master.class = class::BUILDING);
    };
    assert_eq!(
        gate_updated(master_id(91), gate_state(false, 0.3), no_gate),
        Ok((master_id(91), gate_state(false, 0.3)))
    );
}

/// The lock keeps a gate shut (`+0xc7c` with a height above the epsilon: the update ends), but an
/// open gate counts and closes as an unlocked one: with nothing in it, a locked open gate starts
/// its close timer at 0.5 s and at its end shuts (slot 0x338 does not ask the lock).
#[test]
fn locked_open_gate_shuts_after_its_close_timer() {
    assert_eq!(
        gate_updated(master_id(88), gate_state(true, 0.0), soldier_aside),
        Ok((master_id(88), gate_ran(true, GATE_CLOSE_DELAY)))
    );
    assert_eq!(
        gate_updated(master_id(88), gate_state(true, 0.05), soldier_aside),
        Ok((master_id(91), gate_ran(true, Seconds::ZERO)))
    );
}

/// A shut gate opens for the units of an ally in it alone (`FUN_141663d90`): a moving Gaia unit
/// between its posts counts, but leaves it shut and its close timer as it is.
#[test]
fn shut_gate_with_no_ally_inside_stays_shut() {
    let gaia_soldier = |world: &mut World| {
        let placed = *world.spatial.get(object_id(1)).unwrap();
        world.spatial.insert(
            object_id(1),
            Placed {
                player: PlayerId::GAIA,
                ..placed
            },
        );
    };
    assert_eq!(
        gate_updated(master_id(91), gate_state(false, 0.3), gaia_soldier),
        Ok((master_id(91), gate_ran(false, Seconds::new(0.3))))
    );
}

/// The player's toggles test the gate's height: slot 0x340 (`FUN_14162f7e0`) opens a gate of a
/// height of at least 1, slot 0x338 (`FUN_1416193e0`) shuts one of at most 0. A shut gate half a
/// tile high with an ally inside takes its close timer to 0 and stays; an open gate higher than 0
/// but below the epsilon runs its close timer out and stays open.
#[test]
fn gate_toggles_by_its_height_alone() {
    let half_high = |world: &mut World| {
        change_gate_master(world, 91, |master| master.height = Distance::new(0.5));
    };
    assert_eq!(
        gate_updated(master_id(91), gate_state(false, 0.3), half_high),
        Ok((master_id(91), gate_ran(false, Seconds::ZERO)))
    );
    let barely_high = |world: &mut World| {
        soldier_aside(world);
        change_gate_master(world, 88, |master| master.height = Distance::new(5e-8));
    };
    assert_eq!(
        gate_updated(master_id(88), gate_state(false, 0.05), barely_high),
        Ok((master_id(88), gate_ran(false, Seconds::ZERO)))
    );
}

/// A close timer that the world's dt (0.05 s) runs past 0 ends at 0, and the gate shuts.
#[test]
fn close_timer_run_past_its_end_stops_at_zero_and_shuts() {
    assert_eq!(
        gate_updated(master_id(88), gate_state(false, 0.03), soldier_aside),
        Ok((master_id(91), gate_ran(false, Seconds::ZERO)))
    );
}

/// [`house_builder`]'s object `list_index` of player 1 after its own completion test
/// ([`Unit::complete_on_update`]) within its update: its state and its owner's attributes.
fn completed(world: &mut World, list_index: ListIndex) -> GenieResult<(ObjectState, Vec<Amount>)> {
    in_update(world, (player(1), list_index), 50, |unit, actions, step| {
        unit.complete_on_update(actions, step, &roles())?;
        Ok((unit.life.state, step.world.players[1].attributes.clone()))
    })
}

/// The walls' branch of `TRIBE_Building_Object::update` (a house, master 70) completes an annex
/// in its own update as well: [`annexed_house`]'s annex 10030 at its build time goes to state 2
/// though its main building still is a foundation.
#[test]
fn house_annex_completes_in_its_own_update() {
    let mut world = annexed_house();
    let construction = site_mut(&mut world, object_id(10030));
    construction.progress = Seconds::of_master(construction.build_time);
    let annex = world.players[1]
        .objects
        .position_of(object_id(10030))
        .unwrap();
    let (state, _) = completed(&mut world, annex).unwrap();
    assert_eq!(state, ObjectState::Alive);
    assert_eq!(
        world
            .object(object_id(10028))
            .map(|(_, unit)| unit.life.state),
        Some(ObjectState::Foundation)
    );
}

/// The completion is a foundation's (state 0): a standing house whose build points reached its
/// build time completes not again, and its owner gains nothing.
#[test]
fn standing_building_completes_not_again() {
    let mut world = house_builder();
    let construction = site_mut(&mut world, object_id(10028));
    construction.progress = Seconds::of_master(construction.build_time);
    world.players[1].objects[ListIndex::new(1)].life.state = ObjectState::Alive;
    let before = world.players[1].attributes.clone();
    assert_eq!(
        completed(&mut world, ListIndex::new(1)),
        Ok((ObjectState::Alive, before))
    );
}

/// A foundation without a build time (its master's first train time 0, as a death spawn's)
/// completes in its first update: its build points, 0, reached it.
#[test]
fn foundation_without_build_time_completes_at_once() {
    let mut world = house_builder();
    let construction = site_mut(&mut world, object_id(10028));
    construction.build_time = WholeSeconds::of_data(0);
    construction.progress = Seconds::ZERO;
    let before = world.players[1].attributes.clone();
    let (state, after) = completed(&mut world, ListIndex::new(1)).unwrap();
    assert_eq!(state, ObjectState::Alive);
    assert_ne!(after, before);
}

/// A farm's master (`id`, class 49, type 80) of foundation terrain `foundation`: radii and
/// clearance 1.5.
fn farm_master(id: MasterId, foundation: u8) -> Master {
    Master {
        clearance: Footprint::new(1.5, 1.5),
        radius: Footprint::new(1.5, 1.5),
        building: Some(BuildingMaster {
            foundation_terrain: Some(TerrainId::named(foundation)),
            ..BuildingMaster::default()
        }),
        ..master(id, MasterKind::BUILDING, class::FARM)
    }
}

/// A 20-tile map of land with player 1's foundation 40 of `master` at (10.5, 10.5) in state
/// `state`, of a build time of `build_time` seconds, 7.5 of them done.
fn farm_world(master: Master, state: ObjectState, build_time: i16) -> World {
    let mut world = Scene::open(GridSize::new(20, 20))
        .object(SceneObject {
            state,
            desolid: true,
            ..SceneObject::new(40, player(1), master, Position::new(10.5, 10.5, 0.0))
        })
        .build();
    let construction = site_mut(&mut world, object_id(40));
    construction.build_time = WholeSeconds::of_data(build_time);
    construction.progress = Seconds::new(7.5);
    world
}

/// The terrains of tiles (8, 10) to (12, 10) of `world`'s map.
fn row_terrains(world: &World) -> Vec<u8> {
    let terrain = world.spatial.terrain.as_ref().unwrap();
    (8..=12)
        .map(|x| terrain.kind(Tile::new(x, 10)).unwrap().get())
        .collect()
}

/// [`Unit::farm_stage`] on [`farm_world`]'s foundation within its update.
fn farm_staged(world: &mut World) -> Result<(), &'static str> {
    reason_of(in_update(
        world,
        (player(1), ListIndex::new(0)),
        50,
        |unit, _, step| unit.farm_stage(step, &roles()),
    ))
}

/// `TRIBE_Building_Object::update`'s farm part (`FUN_1411ce250` -> `FUN_1411ddd90`): a farm
/// foundation half built (7.5 of 15 s: stage 1) lays terrain 30 over its clearance, the three
/// tiles of each row about it; the tiles beside keep the land.
#[test]
fn farm_foundation_lays_its_stage_terrain_under_it() {
    let mut world = farm_world(farm_master(ids::FARM, 7), ObjectState::Foundation, 15);
    assert_eq!(farm_staged(&mut world), Ok(()));
    assert_eq!(row_terrains(&world), [0, 30, 30, 30, 0]);
}

/// The farm part runs for a foundation (state 0) with a build time, of a farm's master: a built
/// farm, a farm foundation without a build time and a house's foundation lay nothing.
#[test]
fn farm_stage_needs_farm_foundation_with_build_time() {
    let house = Master {
        class: class::BUILDING,
        ..farm_master(ids::HOUSE, 7)
    };
    for (master, state, build_time) in [
        (farm_master(ids::FARM, 7), ObjectState::Alive, 15),
        (farm_master(ids::FARM, 7), ObjectState::Foundation, 0),
        (house, ObjectState::Foundation, 15),
    ] {
        let mut world = farm_world(master, state, build_time);
        assert_eq!(farm_staged(&mut world), Ok(()));
        assert_eq!(row_terrains(&world), [0; 5]);
    }
}

/// Not modelled: a stage terrain that `FUN_1411ce250` maps by the local player's view (a
/// pasture of foundation terrain 62 at stage 0 lays 64), and a map without its terrain.
#[test]
fn farm_stage_of_view_mapped_terrain_or_without_map_terrain_is_not_modelled() {
    let mut world = farm_world(
        farm_master(ids::PASTURE_1889, 62),
        ObjectState::Foundation,
        150,
    );
    assert_eq!(
        farm_staged(&mut world),
        Err("a farm's stage terrain the local player's view maps (8, 27, 64)")
    );
    let mut world = farm_world(farm_master(ids::FARM, 7), ObjectState::Foundation, 15);
    world.spatial.terrain = None;
    assert_eq!(farm_staged(&mut world), Err(FARM_TERRAIN_LACKING.reason()));
}

/// The town centre's master (109) in every player's table changed by `change`.
fn change_centre(world: &mut World, change: impl Fn(&mut Master)) {
    for masters in &mut world.model.masters {
        change(Arc::make_mut(masters).get_mut(ids::TOWN_CENTRE).unwrap());
    }
}

/// The whole seconds read the master's building part (which every building's master holds): a
/// town centre whose master lacks it counts none, its garrison time kept and the forager inside
/// not healed.
#[test]
fn master_without_building_part_counts_no_whole_seconds() {
    assert_eq!(
        centre_seconds(|world| change_centre(world, |master| master.building = None)),
        Ok((10.0, 0))
    );
}

/// The garrison's ejection at a fifth of the hit points (`FUN_1411e7220`) needs a garrison: an
/// empty town centre at 480 of its 2400 counts its whole seconds.
#[test]
fn empty_building_at_fifth_of_its_hit_points_counts_its_seconds() {
    assert_eq!(
        centre_seconds(|world| {
            let centre = &mut world.players[1].objects[ListIndex::new(2)];
            centre.life.hp = HitPoints::new(480.0);
            centre.garrison.clear();
        }),
        Ok((10.0, 3000))
    );
}

/// The raider countdowns pass over a garrison entry its owner does not hold live, and the heal
/// refuses an entry outside the live lists ([`World::live_inside`]; the game's garrison holds
/// the objects themselves).
///
/// [`World::live_inside`]: crate::world::World::live_inside
#[test]
fn garrison_entry_outside_live_lists_is_not_modelled() {
    assert_eq!(
        reason_of(centre_seconds(|world| {
            world.players[1].objects[ListIndex::new(2)].garrison =
                vec![object_id(55_555), object_id(9175)];
        })),
        Err(HOUSED_NOT_LIVE.reason())
    );
}

/// A building heals its garrison only by a heal rate above 0 (the master's
/// `garrison_heal_rate`, `+0x9c0`).
#[test]
fn building_without_heal_rate_heals_none() {
    assert_eq!(
        centre_seconds(|world| change_centre(world, |master| {
            if let Some(building) = master.building.as_mut() {
                building.garrison_heal_rate = HitPoints::ZERO;
            }
        })),
        Ok((10.0, 3000))
    );
}

/// [`garrisoned_centre`]'s forager after the centre's whole seconds, its master (the builder's,
/// as it builds) holding a hit point transform into the gatherer's master 120 above `threshold`
/// hit points: its master and its hit points.
fn healed_forager(threshold: f32) -> (MasterId, HitPoints) {
    let mut world = garrisoned_centre();
    let forager = world.players[1].objects[ListIndex::new(0)].life.master_id;
    for masters in &mut world.model.masters {
        let master = Arc::make_mut(masters).get_mut(forager).unwrap();
        master
            .action
            .get_or_insert_with(ActionPart::default)
            .tasks
            .push(Task {
                action_kind: HP_TRANSFORM,
                work_val1: WorkValue::new(120.0),
                work_val2: WorkValue::new(threshold),
                ..Task::default()
            });
    }
    in_update(
        &mut world,
        (player(1), ListIndex::new(2)),
        50,
        |unit, actions, step| unit.building_seconds(actions, step, &roles()),
    )
    .unwrap();
    let forager = &world.players[1].objects[ListIndex::new(0)];
    (forager.life.master_id, forager.life.hp)
}

/// A unit inside heals by its type's slot 0x110 (`FUN_141213b10`), whose heal (`FUN_141193b30`)
/// runs its slot 0xb8 first (`FUN_141203be0`): a gain below its most runs its hit point
/// transforms' test (`FUN_141204450`, [`World::transform_by_hit_points`]). The forager healed
/// from 10 to 12 takes its transform's master above 11, and keeps its own below 13.
///
/// [`World::transform_by_hit_points`]: crate::world::World::transform_by_hit_points
#[test]
fn healed_unit_inside_takes_its_hit_point_transform() {
    assert_eq!(healed_forager(11.0), (master_id(120), HitPoints::new(12.0)));
    assert_eq!(healed_forager(13.0), (ids::BUILDER, HitPoints::new(12.0)));
}

/// [`garrisoned_centre`] at world time `time` with its `damaged_lately_timer` at 5, after its
/// whole seconds: the timer.
fn damaged_lately_after(time: u32) -> DamagedTimer {
    let mut world = garrisoned_centre();
    world.world_time = WorldTime::from_millis(time);
    world.players[1].objects[ListIndex::new(2)].damaged_lately = DamagedTimer::of_data(5);
    in_update(
        &mut world,
        (player(1), ListIndex::new(2)),
        50,
        |unit, actions, step| {
            unit.building_seconds(actions, step, &roles())?;
            Ok(unit.damaged_lately)
        },
    )
    .unwrap()
}

/// The `damaged_lately_timer` (`+0x30d`) loses one in an update that passes whole seconds, however
/// many (here 3), and none in an update that passes none.
#[test]
fn damaged_lately_timer_loses_one_per_update_with_whole_seconds() {
    assert_eq!(damaged_lately_after(3000), DamagedTimer::of_data(4));
    assert_eq!(damaged_lately_after(999), DamagedTimer::of_data(5));
}

/// An idle sprite (`id`) of ten frames of 0.1 s and eight facets.
fn idle_graphic(id: i16) -> Graphic {
    Graphic {
        id: SpriteId::named(id),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    }
}

/// The sprite stream seeded so that its first draw is above 0x3fff.
fn high_draw() -> Well512 {
    let seeded = |seed| {
        let mut rng = Well512::default();
        rng.set_seed(seed);
        rng
    };
    let seed = (0..100u32)
        .find(|&seed| 0x3fff < seeded(seed).rand15())
        .expect("a high draw");
    seeded(seed)
}

/// A unit showing sprite `id` at its fifth frame, looped in this update, in idle variant
/// `variant`, after the idle swap between the standing sprites 100 and 101 with its motion
/// layer `motion`: the sprite it shows, its variant, and how many draws the swap took.
fn after_idle_swap(
    id: Option<i16>,
    variant: i32,
    motion: Option<&mut Motion>,
) -> (Option<SpriteId>, i32, u32) {
    let graphics: Graphics = [idle_graphic(100), idle_graphic(101)]
        .into_iter()
        .map(|graphic| (graphic.id, graphic))
        .collect();
    let mut unit = Unit {
        sprite: id.map(|id| ActiveSprite {
            frame: Frame::of_data(4),
            looped: true,
            ..ActiveSprite::at_rest(idle_graphic(id), Frame::of_data(4))
        }),
        idle_variant: variant,
        ..Unit::default()
    };
    let mut rng = high_draw();
    let before = rng.draws;
    unit.idle_swap(
        MotionMut::new(motion),
        Some(SpriteId::named(100)),
        Some(SpriteId::named(101)),
        &graphics,
        &mut rng,
        tuning().sprite,
    );
    (
        unit.sprite.map(|sprite| sprite.graphic.id),
        unit.idle_variant,
        rng.draws - before,
    )
}

/// `0x141071880`'s idle swap from the second standing sprite (variant 1) goes back to the first
/// (`FUN_141189760` with index 0).
#[test]
fn idle_swap_from_the_second_standing_sprite_goes_back_to_the_first() {
    assert_eq!(
        after_idle_swap(Some(101), 1, None),
        (Some(SpriteId::named(100)), 0, 1)
    );
}

/// Slot 0x5d8 of a moving object answers its move algorithm's bit 0: an object that alternates
/// its sprites as it moves swaps none when idle, and draws nothing; neither does an object that
/// shows no sprite. A walker without the bit swaps as an animated object does.
#[test]
fn idle_swap_skips_an_alternating_mover_and_an_object_without_sprite() {
    let mut mover = walker();
    mover.stats.move_algorithm = MoveAlgorithm::ALTERNATES_SPRITES;
    let mut motion = Motion::Moving(mover);
    assert_eq!(
        after_idle_swap(Some(100), 0, Some(&mut motion)),
        (Some(SpriteId::named(100)), 0, 0)
    );
    assert_eq!(after_idle_swap(None, 0, None), (None, 0, 0));
    let mut walking = Motion::Moving(walker());
    assert_eq!(
        after_idle_swap(Some(100), 0, Some(&mut walking)),
        (Some(SpriteId::named(101)), 1, 1)
    );
}
