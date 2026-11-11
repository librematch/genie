//! A target's release by the action that held it, an object's leave of the world, the update's
//! regeneration and hit point check, and a walk's pause.

use super::*;
use crate::combat::damage::ArmorPair;
use crate::combat::missile::Missile;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Offset;
use crate::map::Position;
use crate::movement::PathData;
use crate::movement::Speed;
use crate::objects::HpPowerup;
use crate::objects::MasterHitPoints;
use crate::objects::Regeneration;
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::{CreatablePart, Master, MasterId};
use crate::players::Amount;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::scene::{Scene, SceneObject, moving_master, walking};
use crate::unit_ai::Attackers;
use crate::vision::VisionBits;
use crate::vision::ghost::{Cell, Watched};
use crate::world::parts::Part;

/// A stand-in static object (type 10) of the villager's class: master 379, which build 158041's
/// `.dat` leaves empty in every civilisation.
const STATIC_POST: MasterId = MasterId::named(379);
/// A stand-in missile (type 60) of the villager's class: master 382, empty in the `.dat` too.
const KEPT_MISSILE: MasterId = MasterId::named(382);
/// A stand-in walker (type 30): master 383, empty in the `.dat` too.
const WALKER: MasterId = MasterId::named(383);

/// The villager 40 of player 1 that lets its target go.
const VILLAGER_RELEASER: Releaser = Releaser {
    id: object_id(40),
    class: Some(class::VILLAGER),
    player: PlayerId::new(1),
};

/// Object 9 in state `state` with `hp` hit points and `workers` workers.
fn target(state: ObjectState, hp: f32, workers: i32) -> Unit {
    Unit {
        id: object_id(9),
        workers: WorkerCount::of_data(workers),
        life: Lifecycle {
            state,
            hp: HitPoints::new(hp),
            ..Lifecycle::default()
        },
        ..Unit::default()
    }
}

/// [`released_fields`] on a static object (type 10) `unit` let go by [`VILLAGER_RELEASER`]: its
/// workers after and whether it is flagged to sleep.
fn static_let_go(mut unit: Unit) -> (WorkerCount, bool) {
    let notify = released_fields(
        &mut unit,
        None,
        (MasterKind::STATIC, false),
        VILLAGER_RELEASER,
    );
    assert_eq!(notify, None);
    (unit.workers, unit.life.goto_sleep)
}

/// `FUN_141197d00`: without workers left an undead object (state 6) goes to sleep whatever its
/// hit points; a dying one (3) and a foundation (0) stay awake.
#[test]
fn static_object_let_go_sleeps_undead_but_not_dying_or_unbuilt() {
    assert_eq!(
        static_let_go(target(ObjectState::Undead, -5.0, 1)),
        (WorkerCount::ZERO, true)
    );
    for state in [
        ObjectState::Dying,
        ObjectState::Foundation,
        ObjectState::Dead,
    ] {
        assert_eq!(
            static_let_go(target(state, 5.0, 1)),
            (WorkerCount::ZERO, false),
            "{state:?}"
        );
    }
}

/// `FUN_141197d00` counts a worker less only above 0 and tests the count after: a static object
/// with a worker left stays awake, one without workers before or after goes to sleep.
#[test]
fn static_object_sleeps_only_without_workers_left() {
    assert_eq!(
        static_let_go(target(ObjectState::Alive, 5.0, 2)),
        (WorkerCount::of_data(1), false)
    );
    assert_eq!(
        static_let_go(target(ObjectState::Alive, 5.0, 0)),
        (WorkerCount::ZERO, true)
    );
}

/// `FUN_141197d00`'s `COMISS hp, 0` and `JNC` (`0x141197d6c`): an object alive (state 2) at 0
/// hit points goes to sleep, one below 0 stays awake, and so does one of NaN hit points, whose
/// unordered compare falls through to the test of state 6. An undead object of NaN hit points
/// goes to sleep.
#[test]
fn static_object_sleeps_at_zero_hit_points_but_not_below_or_at_nan() {
    assert_eq!(
        static_let_go(target(ObjectState::Alive, 0.0, 1)),
        (WorkerCount::ZERO, true)
    );
    assert_eq!(
        static_let_go(target(ObjectState::Alive, -0.5, 1)),
        (WorkerCount::ZERO, false)
    );
    assert_eq!(
        static_let_go(target(ObjectState::Alive, f32::NAN, 1)),
        (WorkerCount::ZERO, false)
    );
    assert_eq!(
        static_let_go(target(ObjectState::Undead, f32::NAN, 1)),
        (WorkerCount::ZERO, true)
    );
}

/// The combat object's slot 0x180 (`FUN_141086e60`, slot 0x328 with 699): the releaser's id is
/// the caller, the object the recipient and the releaser's master class the first parameter;
/// a building's (`FUN_1411d6020`) ends in it too. The objects below the combat type tell nobody.
#[test]
fn combat_object_and_building_tell_releaser_class_in_notify_699() {
    let told = |kind| {
        let mut unit = target(ObjectState::Alive, 5.0, 1);
        released_fields(&mut unit, None, (kind, false), VILLAGER_RELEASER)
    };
    let notify = Notify {
        caller: Some(object_id(40)),
        recipient: object_id(9),
        kind: NotifyKind::NO_LONGER_ON_ME,
        params: [4, 0, 0],
    };
    assert_eq!(told(MasterKind::COMBAT), Some(notify));
    assert_eq!(told(MasterKind::BUILDING), Some(notify));
    for kind in [MasterKind::ANIMATED, MasterKind::MOVING, MasterKind::ACTION] {
        assert_eq!(told(kind), None, "{kind:?}");
    }
}

/// `FUN_141086e60` tells nothing for a releaser of a negative id (`-1 < id`), and passes −1 for
/// a releaser without a master class; it counts the worker less either way.
#[test]
fn combat_object_tells_nothing_for_recyclable_releaser() {
    let mut unit = target(ObjectState::Alive, 5.0, 1);
    let missile = Releaser {
        id: object_id(-5),
        ..VILLAGER_RELEASER
    };
    assert_eq!(
        released_fields(&mut unit, None, (MasterKind::COMBAT, false), missile),
        None
    );
    assert_eq!(unit.workers, WorkerCount::ZERO);
    let classless = Releaser {
        class: None,
        ..VILLAGER_RELEASER
    };
    assert_eq!(
        released_fields(&mut unit, None, (MasterKind::COMBAT, false), classless)
            .map(|notify| notify.params),
        Some([-1, 0, 0])
    );
}

/// `FUN_1411d6020`: a building that lists its releaser among its attacking units (`listed`)
/// drops the releaser's first entry, the others keeping their order; one that does not list it
/// keeps them all, as does a list without the releaser.
#[test]
fn building_drops_listed_releaser_first_attacking_entry() {
    let attacking = |listed, units: &[i32]| {
        let mut unit = target(ObjectState::Foundation, 5.0, 3);
        let mut layer = Building {
            attacking_units: units.iter().map(|&id| object_id(id)).collect(),
            ..Building::default()
        };
        let _ = released_fields(
            &mut unit,
            Some(&mut layer),
            (MasterKind::BUILDING, listed),
            VILLAGER_RELEASER,
        );
        layer.attacking_units
    };
    let ids = |units: &[i32]| units.iter().map(|&id| object_id(id)).collect::<Vec<_>>();
    assert_eq!(attacking(true, &[41, 40, 42, 40]), ids(&[41, 42, 40]));
    assert_eq!(attacking(false, &[41, 40, 42, 40]), ids(&[41, 40, 42, 40]));
    assert_eq!(attacking(true, &[41, 42]), ids(&[41, 42]));
}

/// [`released`] posts the notify 699 to the object's unit AI, which takes it from one of its
/// attackers; an object without a unit AI only counts the worker less.
#[test]
fn released_posts_notify_699_to_unit_ai() {
    let mut unit = target(ObjectState::Alive, 5.0, 1);
    let mut ai = UnitAi {
        attackers: Attackers::of_saved(&[40]),
        ..UnitAi::default()
    };
    released(
        &mut unit,
        (Some(&mut ai), None),
        (MasterKind::COMBAT, false),
        VILLAGER_RELEASER,
    )
    .unwrap();
    assert_eq!(
        ai.notifies,
        [Notify {
            caller: Some(object_id(40)),
            recipient: object_id(9),
            kind: NotifyKind::NO_LONGER_ON_ME,
            params: [4, 0, 0],
        }]
    );
    let mut unit = target(ObjectState::Alive, 5.0, 1);
    released(
        &mut unit,
        (None, None),
        (MasterKind::COMBAT, false),
        VILLAGER_RELEASER,
    )
    .unwrap();
    assert_eq!(unit.workers, WorkerCount::ZERO);
}

/// Player 1's villager 9 (type 70, class 4), its static post 8 of the same class (type 10) and
/// its missile 7 of a master kept, not recycled; player 1 counts 2 civilians (attribute 37) and
/// watches each.
fn leaving_world() -> World {
    let master = |id, kind| Master {
        id,
        kind,
        class: class::VILLAGER,
        ..Master::default()
    };
    let at = |x| Position::new(x, 2.5, 0.0);
    let mut world = Scene::open(GridSize::new(8, 8))
        .object(SceneObject::new(
            9,
            PlayerId::new(1),
            master(ids::VILLAGER_MALE, MasterKind::COMBAT),
            at(2.5),
        ))
        .object(SceneObject::new(
            8,
            PlayerId::new(1),
            master(STATIC_POST, MasterKind::STATIC),
            at(4.5),
        ))
        .object(SceneObject::new(
            7,
            PlayerId::new(1),
            master(KEPT_MISSILE, MasterKind::MISSILE),
            at(6.5),
        ))
        .build();
    world
        .set_part(
            object_id(7),
            Part::Missile(Some(Missile {
                max_range: Distance::ZERO,
                fired_from: None,
                charge: false,
                flying: true,
                step: Offset::ZERO,
            })),
        )
        .unwrap();
    let player = &mut world.players[1];
    player.attributes = vec![Amount::ZERO; 64];
    player.attributes[37] = Amount::new(2.0);
    player.watched = [9, 8, 7]
        .map(|id| Watched {
            id: object_id(id),
            cell: Cell::Tile(0),
            bits: VisionBits::NONE,
        })
        .to_vec();
    world
}

/// [`World::leave_world`] of player 1's object `id`, out of its live list first.
fn leave(world: &mut World, id: i32) {
    let list_index = world.live_index(PlayerId::new(1), object_id(id)).unwrap();
    let gone = world
        .swap_remove_live(PlayerId::new(1), list_index)
        .unwrap();
    world.leave_world(PlayerId::new(1), &gone).unwrap();
}

/// The leave of a combat object (type 70) lowers its player's class counters (its destructor's
/// `FUN_141226060` with −1: a villager in attribute 37); a static object's of the same class
/// does not. Each leaves the map and its owner's watched list.
#[test]
fn leaving_combat_object_lowers_class_counters_and_static_one_does_not() {
    let mut world = leaving_world();
    leave(&mut world, 9);
    assert_eq!(world.players[1].attributes[37], Amount::new(1.0));
    leave(&mut world, 8);
    assert_eq!(world.players[1].attributes[37], Amount::new(1.0));
    assert_eq!(
        (
            world.spatial.get(object_id(9)),
            world.spatial.get(object_id(8))
        ),
        (None, None)
    );
    assert_eq!(
        world.players[1]
            .watched
            .iter()
            .map(|watched| watched.id)
            .collect::<Vec<_>>(),
        [object_id(7)]
    );
}

/// A missile of a master that keeps its objects (`recyclable` 0) does not go to the pool of type
/// 60 when it leaves.
#[test]
fn leaving_missile_of_kept_master_stays_out_of_pool() {
    let mut world = leaving_world();
    leave(&mut world, 7);
    assert_eq!(world.recycling.missiles, Vec::new());
}

/// Player 1's archer 9 (type 70) and its walker 8 (type 30), both at 10 of 20 hit points, each
/// of a master that regains 60 hit points a minute.
fn regenerating_world() -> World {
    let master = |id, kind| Master {
        id,
        kind,
        hit_points: MasterHitPoints::of_data(20),
        creatable: Some(CreatablePart {
            regeneration: Regeneration::new(60.0),
            ..CreatablePart::default()
        }),
        ..moving_master(walking(Speed::new(1.0)))
    };
    let at = |x| Position::new(x, 2.5, 0.0);
    let mut world = Scene::open(GridSize::new(8, 8))
        .object(SceneObject::new(
            9,
            PlayerId::new(1),
            master(master_id(4), MasterKind::COMBAT),
            at(2.5),
        ))
        .object(SceneObject::new(
            8,
            PlayerId::new(1),
            master(WALKER, MasterKind::MOVING),
            at(4.5),
        ))
        .build();
    for id in [9, 8] {
        world.object_mut(object_id(id)).unwrap().1.life.hp = HitPoints::new(10.0);
    }
    world
}

/// Object `id`'s slot in player 1's list.
fn slot(world: &World, id: i32) -> (PlayerId, ListIndex, ObjectId) {
    let list_index = world.live_index(PlayerId::new(1), object_id(id)).unwrap();
    (PlayerId::new(1), list_index, object_id(id))
}

/// Object `id`'s hit points.
fn hp(world: &World, id: i32) -> HitPoints {
    world.object(object_id(id)).unwrap().1.life.hp
}

/// `TRIBE_Combat_Object::update`'s regeneration runs on a combat object (type 70): 60 a minute
/// for 2 seconds gives 2 hit points. An object of a type below it does not regenerate.
#[test]
fn regeneration_runs_on_combat_objects_only() {
    let mut world = regenerating_world();
    for id in [9, 8] {
        let slot = slot(&world, id);
        world.regenerate_object(slot, Seconds::new(2.0)).unwrap();
    }
    assert_eq!(
        (hp(&world, 9), hp(&world, 8)),
        (HitPoints::new(12.0), HitPoints::new(10.0))
    );
}

/// A slot whose object the world no longer holds is passed over by the regeneration and the
/// powerup check; one whose list moved is followed to the object's place ([`World::live_slot`]).
#[test]
fn regeneration_follows_moved_object_and_passes_over_gone_one() {
    let mut world = regenerating_world();
    let (player_id, _, id) = slot(&world, 9);
    world
        .regenerate_object((player_id, ListIndex::new(5), id), Seconds::new(2.0))
        .unwrap();
    assert_eq!(hp(&world, 9), HitPoints::new(12.0));
    world
        .regenerate_object(
            (player_id, ListIndex::new(5), object_id(77)),
            Seconds::new(2.0),
        )
        .unwrap();
    world
        .clamp_object_powerup((player_id, ListIndex::new(5), object_id(77)))
        .unwrap();
    assert_eq!(hp(&world, 9), HitPoints::new(12.0));
}

/// The regeneration's end (`0x141208440`): at its most hit points the armor its charge's hits
/// took comes back; below them it stays.
#[test]
fn regeneration_to_most_hit_points_gives_charged_armor_back() {
    let removed = ArmorPair::new(2, 1);
    let regenerated = |hp: f32| {
        let mut world = regenerating_world();
        world.object_mut(object_id(9)).unwrap().1.life.hp = HitPoints::new(hp);
        world.parts.combat_mut(object_id(9)).unwrap().armor_removed = removed;
        let slot = slot(&world, 9);
        world.regenerate_object(slot, Seconds::new(2.0)).unwrap();
        world.parts.combat_of(object_id(9)).armor_removed
    };
    assert_eq!(regenerated(19.0), ArmorPair::ZERO);
    assert_eq!(regenerated(10.0), removed);
}

/// `TRIBE_Combat_Object::update`'s powerup check: a combat object's hit point powerup that took
/// more than its most comes off its hit points; an object of a type below it keeps its own.
#[test]
fn powerup_check_clamps_combat_objects_only() {
    let mut world = regenerating_world();
    for id in [9, 8] {
        world.parts.combat_mut(object_id(id)).unwrap().hp_powerup = HpPowerup {
            taken: HitPoints::new(5.0),
            max: HitPoints::new(3.0),
        };
        let slot = slot(&world, id);
        world.clamp_object_powerup(slot).unwrap();
    }
    assert_eq!(
        (hp(&world, 9), hp(&world, 8)),
        (HitPoints::new(8.0), HitPoints::new(10.0))
    );
    assert_eq!(
        world.parts.combat_of(object_id(9)).hp_powerup.taken,
        HitPoints::new(3.0)
    );
}

/// Slot 0x5f0 (`FUN_141158b90`): the unit's path data (`+0x858`), its first level, pauses; a unit
/// without path data, and an id the world does not hold, pause nothing.
#[test]
fn pause_sets_first_path_level_pause_time() {
    let mut world = regenerating_world();
    world
        .parts
        .mover_mut(object_id(9))
        .unwrap()
        .paths
        .extend([PathData::without_path(), PathData::without_path()]);
    world.pause(object_id(9), Seconds::new(1.5));
    world.pause(object_id(8), Seconds::new(1.5));
    world.pause(object_id(77), Seconds::new(1.5));
    let pauses = |world: &World, id| {
        world
            .parts
            .motion
            .mover(object_id(id))
            .map(|mover| {
                mover
                    .paths
                    .iter()
                    .map(|level| level.pause_time)
                    .collect::<Vec<_>>()
            })
            .unwrap()
    };
    assert_eq!(pauses(&world, 9), [Seconds::new(1.5), Seconds::ZERO]);
    assert_eq!(pauses(&world, 8), Vec::new());
}
