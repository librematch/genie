//! Slot 0x208's terms and its readers: the own speed with a speed-up, a combat object's lock,
//! powerup and stinger, the run of a ready charge, and the moving object's speed-up drop.

use crate::combat::Charge;
use crate::combat::LockdownCount;
use crate::combat::damage::SpecialCharge;
use crate::gathering::hunt::tests::walker;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::movement::Speed;
use crate::movement::top_speed::{ChargeRun, CombatSpeed, RunFactor, TopSpeed, task_by_flags};
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::layers::{Speedup, SpeedupValue};
use crate::objects::masters::ActionKind;
use crate::objects::masters::ActionPart;
use crate::objects::masters::ChargeTarget;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::SpecialAttack;
use crate::objects::masters::Task;
use crate::objects::masters::WorkValue;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::scene::{Scene, SceneObject, walking};
use crate::unit_ai::UnitAi;
use crate::world::World;
use crate::world::tests::in_update;
use std::sync::Arc;

/// The walker's own speed: 0.8 tiles a second.
const OWN: f32 = 0.8;

/// A combat object's terms of a lock `locked_down`, a powerup `powerup`, a stinger `stinger`
/// and a run `run`.
fn combat(locked_down: i32, (powerup, stinger): (f32, f32), run: Option<ChargeRun>) -> TopSpeed {
    TopSpeed::Combat(CombatSpeed {
        locked_down: LockdownCount::of_data(locked_down),
        powerup: Speed::new(powerup),
        stinger: Speed::new(stinger),
        run,
    })
}

/// A run to a target at (10, 0), both radii 0.5, reach 3 and factor 2.
fn run() -> ChargeRun {
    ChargeRun {
        target: Point::new(10.0, 0.0),
        radii: (Distance::new(0.5), Distance::new(0.5)),
        reach: Distance::new(3.0),
        factor: RunFactor::new(2.0),
    }
}

/// `FUN_14114d1d0`: the top speed with its sprite, plus the master's speed times the speed-up's
/// value while a building holds it (`+0x808` not −1).
#[test]
fn own_speed_adds_its_speedup_share_while_its_building_holds() {
    let mut mover = walker();
    mover.stats = walking(Speed::new(OWN));
    mover.stats.base = Speed::new(1.0);
    assert_eq!(mover.own_speed(), Speed::new(OWN));
    mover.speedup = Speedup {
        building: Some(object_id(600)),
        value: SpeedupValue::of_data(0.5),
    };
    assert_eq!(mover.own_speed(), Speed::new(OWN + 1.0 * 0.5));
    mover.speedup.building = None;
    assert_eq!(mover.own_speed(), Speed::new(OWN));
}

/// `FUN_141202c70`: 0 while a lock holds it (`+0xa08` above 0, signed); else the own speed plus
/// the powerup plus the stinger, left to right; RGE's slot is the own speed alone.
#[test]
fn combat_object_reads_its_lock_then_its_powerup_and_stinger() {
    let at = Point::ORIGIN;
    let own = Speed::new(OWN);
    assert_eq!(TopSpeed::Own.of(own, at), own);
    assert_eq!(
        combat(0, (0.25, 0.125), None).of(own, at),
        Speed::new(OWN + 0.25 + 0.125)
    );
    assert_eq!(combat(1, (0.25, 0.125), None).of(own, at), Speed::ZERO);
    assert_eq!(
        combat(-1, (0.25, 0.0), None).of(own, at),
        Speed::new(OWN + 0.25)
    );
    // a stinger that takes all the speed leaves 0
    assert_eq!(combat(0, (0.0, -OWN), None).of(own, at), Speed::ZERO);
}

/// The run of a ready charge: the gap less both radii below the reach multiplies the sum by
/// the factor; a sum of 0 stays (`UCOMISS`, `JP` then `JZ`), a NaN sum runs, a NaN gap does
/// not (`COMISS`, `JBE`).
#[test]
fn ready_charge_runs_within_its_reach() {
    let own = Speed::new(OWN);
    let near = Point::new(7.5, 0.0);
    let far = Point::new(6.0, 0.0);
    assert!(run().reaches(near));
    assert!(!run().reaches(far));
    assert!(!run().reaches(Point::new(f32::NAN, 0.0)));
    assert_eq!(
        combat(0, (0.0, 0.0), Some(run())).of(own, near),
        Speed::new(OWN * 2.0)
    );
    assert_eq!(combat(0, (0.0, 0.0), Some(run())).of(own, far), own);
    assert_eq!(
        combat(0, (0.0, -OWN), Some(run())).of(own, near),
        Speed::ZERO
    );
    assert!(
        combat(0, (f32::NAN, 0.0), Some(run()))
            .of(own, near)
            .get()
            .is_nan()
    );
}

/// `FUN_14109e1a0`: the first task of the kind whose flags match, −1 matching any.
#[test]
fn task_by_flags_takes_the_first_task_of_its_kind_and_flags() {
    let task = |kind, work_flag, work_flag2, range| Task {
        action_kind: kind,
        work_flag,
        work_flag2,
        work_range: Distance::new(range),
        ..Task::default()
    };
    let tasks = [
        task(ActionKind::ACTION_133, 1, 0x7d0, 1.0),
        task(ActionKind::named(7), 1, 0x7d1, 2.0),
        task(ActionKind::ACTION_133, 4, 0x7d1, 3.0),
        task(ActionKind::ACTION_133, 1, 0x7d1, 4.0),
    ];
    let found = |flag, flag2| {
        task_by_flags(&tasks, (ActionKind::ACTION_133, flag, flag2)).map(|task| task.work_range)
    };
    assert_eq!(found(None, Some(0x7d1)), Some(Distance::new(3.0)));
    assert_eq!(found(Some(1), Some(0x7d1)), Some(Distance::new(4.0)));
    assert_eq!(found(None, None), Some(Distance::new(1.0)));
    assert_eq!(found(Some(9), None), None);
}

/// A charger: type 70, walking at [`OWN`], special attack 3 with a charge of 10 on every
/// target; its run task (0x85, 0x7d1) of reach 3 and factor 2.
fn charger() -> Master {
    Master {
        id: MasterId::named(1755),
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        hit_points: MasterHitPoints::of_data(100),
        radius: Footprint::new(0.5, 0.5),
        move_stats: Some(walking(Speed::new(OWN))),
        creatable: Some(CreatablePart {
            special_attack: SpecialAttack::CHARGE,
            max_charge: Charge::new(10.0),
            charge_target: ChargeTarget::EVERY,
            ..CreatablePart::default()
        }),
        action: Some(ActionPart {
            tasks: vec![Task {
                action_kind: ActionKind::ACTION_133,
                work_flag2: 0x7d1,
                work_val2: WorkValue::new(3.0),
                work_range: Distance::new(2.0),
                ..Task::default()
            }],
            ..ActionPart::default()
        }),
        ..Master::default()
    }
}

/// The charger 5 at (`x`, 4) with its charge at `charge` and its unit AI's target the charger 6
/// at (10, 4), both of player 1.
fn charge_world(x: f32, charge: f32) -> World {
    let mut me = SceneObject::new(5, PlayerId::new(1), charger(), Position::new(x, 4.0, 0.0));
    me.ai = Some(UnitAi {
        current_target: Some(object_id(6)),
        ..UnitAi::default()
    });
    let target = SceneObject::new(
        6,
        PlayerId::new(1),
        charger(),
        Position::new(10.0, 4.0, 0.0),
    );
    let mut world = Scene::open(GridSize::new(16, 16))
        .object(me)
        .object(target)
        .build();
    world.parts.combat_mut(object_id(5)).unwrap().charge = Charge::new(charge);
    world
}

/// The charger's slot 0x208 in `world`.
fn top_of(world: &World) -> Speed {
    let unit = &world.players[1].objects[ListIndex::new(0)];
    world.top_speed(PlayerId::new(1), unit).unwrap()
}

/// `FUN_141202c70`'s tail: a charger of type 70 with a charge, its charge ready
/// (`FUN_1412161b0`: at least the most less 0.01, both widened) and its unit AI's target
/// within the run task's reach runs at twice its speed; farther, or not ready, at its own.
#[test]
fn ready_charger_runs_near_its_unit_ai_target() {
    // the gap 2.5 less both radii 0.5 is 1.5, below the reach 3
    assert_eq!(top_of(&charge_world(7.5, 10.0)), Speed::new(OWN * 2.0));
    assert_eq!(top_of(&charge_world(7.5, 9.995)), Speed::new(OWN * 2.0));
    assert_eq!(top_of(&charge_world(7.5, 9.9)), Speed::new(OWN));
    // the gap 6 less both radii is 5, beyond the reach
    assert_eq!(top_of(&charge_world(4.0, 10.0)), Speed::new(OWN));
    // its lock, powerup and stinger read its combat layer
    let mut world = charge_world(4.0, 0.0);
    let combat = world.parts.combat_mut(object_id(5)).unwrap();
    (combat.speed_powerup, combat.speed_stinger) = (Speed::new(0.5), Speed::new(-0.25));
    assert_eq!(top_of(&world), Speed::new(OWN + 0.5 - 0.25));
    world.parts.combat_mut(object_id(5)).unwrap().locked_down = LockdownCount::of_data(1);
    assert_eq!(top_of(&world), Speed::ZERO);
}

/// `FUN_1412161b0`'s answer as a hit's attacker holds it: [`World::special_charge`] reads the
/// target of the unit AI the world holds, [`World::special_charge_with`] the one its caller
/// passes (a lent unit AI's): ready with the target, unready without one or not ready.
#[test]
fn special_charge_reads_the_unit_ai_target_it_is_given() {
    let answers = |world: &World| {
        let unit = &world.players[1].objects[ListIndex::new(0)];
        let master = unit
            .master(world.master_tables(), PlayerId::new(1))
            .unwrap();
        (
            world.special_charge(unit, master).unwrap(),
            world
                .special_charge_with(unit, Some(object_id(6)), master)
                .unwrap(),
            world.special_charge_with(unit, None, master).unwrap(),
        )
    };
    assert_eq!(
        answers(&charge_world(7.5, 10.0)),
        (
            SpecialCharge::Ready,
            SpecialCharge::Ready,
            SpecialCharge::Unready
        )
    );
    assert_eq!(
        answers(&charge_world(7.5, 9.9)),
        (
            SpecialCharge::Unready,
            SpecialCharge::Unready,
            SpecialCharge::Unready
        )
    );
}

/// A master below type 70 reads RGE's slot: its own speed, whatever its combat layer holds;
/// an object without movement has none.
#[test]
fn moving_object_below_kind_70_reads_its_own_speed() {
    let mut animal = charge_world(4.0, 10.0);
    for masters in &mut animal.model.masters {
        if let Some(master) = Arc::make_mut(masters).get_mut(MasterId::named(1755)) {
            master.kind = MasterKind::MOVING;
        }
    }
    animal.parts.combat_mut(object_id(5)).unwrap().speed_powerup = Speed::new(0.5);
    assert_eq!(top_of(&animal), Speed::new(OWN));
    let id = animal.players[1].objects[ListIndex::new(0)].id;
    animal.set_motion(id, None);
    assert_eq!(top_of(&animal), Speed::ZERO);
}

/// `RGE_Moving_Object::update`'s speed-up check (`0x14114de0a`): the building stays while its
/// squared footprint gap is at most the square of the owner's attribute 256 (`JBE`: equal
/// keeps, a NaN range keeps); beyond it, or gone, the pair is dropped.
#[test]
fn moving_speedup_drops_out_of_range_or_gone() {
    let mut world = charge_world(4.0, 0.0);
    world.players[1].attributes.resize(300, Amount::ZERO);
    let me = (PlayerId::new(1), ListIndex::new(0));
    let held = Speedup {
        building: Some(object_id(6)),
        value: SpeedupValue::of_data(1.5),
    };
    let roles = world.model.roles.clone();
    let check = |world: &mut World, speedup: Speedup, range: f32| {
        world.players[1].attributes[256] = Amount::new(range);
        world.parts.mover_mut(object_id(5)).unwrap().speedup = speedup;
        in_update(world, me, 50, |unit, actions, step| {
            unit.motion_step(actions, step, &roles)
        })
        .unwrap();
        world.parts.mover_of(object_id(5)).unwrap().speedup
    };
    // the footprints' gap on x: 6 less both radii 0.5 is 5, squared 25
    assert_eq!(check(&mut world, held, 5.0), held);
    assert_eq!(check(&mut world, held, f32::NAN), held);
    assert_eq!(check(&mut world, held, 4.9), Speedup::NONE);
    let gone = Speedup {
        building: Some(object_id(9999)),
        ..held
    };
    assert_eq!(check(&mut world, gone, 1000.0), Speedup::NONE);
}
