//! The bound of a unit's speed and reach: the largest sprite factor and speed-up, each aura
//! source once, a stinger that raises the speed, a siege unit's infantry, a charger's run, the
//! range auras, and a speed that is not a number.

use crate::combat::Charge;
use crate::combat::stingers::STINGER;
use crate::gathering::hunt::tests::walker as hunt_walker;
use crate::map::Distance;
use crate::movement::MotionBound;
use crate::movement::Speed;
use crate::objects::FacetCount;
use crate::objects::Frame;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::layers::{Speedup, SpeedupValue};
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ActionPart;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatPart;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::GarrisonCapacity;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Masters;
use crate::objects::masters::SpecialAttack;
use crate::objects::masters::Task;
use crate::objects::masters::TaskFlags;
use crate::objects::masters::WorkValue;
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::model;
use crate::testing::scene::{moving_master, walking};
use crate::world::Model;
use crate::world::Seconds;
use crate::world::parts::Parts;
use crate::world::parts::no_parts;
use crate::world::player::Player;
use std::sync::Arc;

/// The class the test auras and stingers name (one the game never uses).
const POWERED: ClassId = ClassId::of_master(99);

/// The graphic `id` with the speed factor `factor`.
const fn graphic(id: i16, factor: f32) -> Graphic {
    Graphic {
        id: SpriteId::named(id),
        frames: Frame::of_data(1),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::of_data(factor),
        frame_duration: Seconds::ZERO,
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    }
}

/// A master `id` of class [`POWERED`] (type 70) moving at `speed` with `tasks`.
fn walker(id: i16, speed: f32, tasks: Vec<Task>) -> Master {
    Master {
        id: MasterId::of_data(id).unwrap(),
        kind: MasterKind::COMBAT,
        class: POWERED,
        action: Some(ActionPart {
            tasks,
            ..ActionPart::default()
        }),
        ..moving_master(walking(Speed::new(speed)))
    }
}

/// An aura task (type 155) of `attribute` (5 the speed, 12 the range) naming [`POWERED`], of
/// `amount`, a share of the object's value with `share`.
fn aura(attribute: f32, amount: f32, share: bool) -> Task {
    Task {
        action_kind: ActionKind::AURA,
        object_group: Some(POWERED),
        wait_time: Seconds::new(attribute),
        work_val1: WorkValue::new(amount),
        combat_level_flag: if share {
            TaskFlags::AURA_SHARE
        } else {
            TaskFlags::empty()
        },
        ..Task::default()
    }
}

/// The test build's model with player 1's masters `masters` and the graphics `graphics`.
fn model_of(masters: Vec<Master>, graphics: Vec<Graphic>) -> Model {
    let mut built = model().clone();
    built.masters = vec![Arc::new(Masters::new([])), Arc::new(Masters::new(masters))];
    built.graphics = Arc::new(
        graphics
            .into_iter()
            .map(|graphic| (graphic.id, graphic))
            .collect(),
    );
    built
}

/// Players holding one object of each master of `model`'s tables (player 0 Gaia's, 1 the
/// test masters'), the sources of the bound's auras and stingers.
fn holding_all(model: &Model) -> Vec<Player> {
    let mut next = 0;
    model
        .masters
        .iter()
        .map(|masters| Player {
            objects: masters
                .iter()
                .map(|master| {
                    next += 1;
                    Unit {
                        id: object_id(next),
                        life: Lifecycle {
                            master_id: master.id,
                            state: ObjectState::Alive,
                            ..Lifecycle::default()
                        },
                        ..Unit::default()
                    }
                })
                .collect(),
            ..Player::default()
        })
        .collect()
}

/// The bound of `model` without movers, every master held by an object: its speed of
/// `master` and its reach.
fn bound_of(model: &Model, master: &Master) -> (Speed, Distance) {
    let bound = MotionBound::of(model, no_parts(), &holding_all(model)).unwrap();
    (bound.speed(master), bound.reach(master))
}

/// The speed takes the largest factor of the sprites the master shows (at least 1: a factor not
/// above 0 applies none; a sprite it does not show has no say) and a loaded speed-up's share of
/// the master's speed (only while it holds a building).
#[test]
fn bound_speed_takes_the_largest_sprite_factor_and_speedup() {
    let unit = Master {
        standing_sprite: Some(SpriteId::named(1)),
        ..walker(4, 0.8, vec![])
    };
    let running = Master {
        standing_sprite_2: Some(SpriteId::named(2)),
        ..unit.clone()
    };
    let slow = model_of(vec![], vec![graphic(1, 0.5), graphic(3, 3.0)]);
    let fast = model_of(
        vec![],
        vec![graphic(1, 0.5), graphic(2, 1.5), graphic(3, 3.0)],
    );
    assert_eq!(bound_of(&slow, &unit).0, Speed::new(0.8));
    assert_eq!(bound_of(&fast, &running).0, Speed::new(0.8 * 1.5));
    let unit = running;
    let mut parts = Parts::default();
    let mut mover = hunt_walker();
    mover.speedup = Speedup {
        building: Some(object_id(9)),
        value: SpeedupValue::of_data(0.25),
    };
    parts.put_mover(object_id(1), Some(mover.clone()));
    let sped = MotionBound::of(&fast, &parts, &[]).unwrap().speed(&unit);
    assert_eq!(sped, Speed::new(0.8 * (1.5 + 0.25)));
    mover.speedup.building = None;
    parts.put_mover(object_id(1), Some(mover));
    assert_eq!(
        MotionBound::of(&fast, &parts, &[]).unwrap().speed(&unit),
        Speed::new(0.8 * 1.5)
    );
}

/// The speed powerups of the auras that name the master: per source its largest (an object
/// holds one powerup of a source and attribute at a time), summed; a share adds the amount less
/// 1 of the master's speed, a plain amount itself; an aura of another class adds nothing, and
/// neither does one whose master no object holds.
#[test]
fn bound_speed_adds_each_aura_source_once() {
    let unit = walker(4, 1.0, vec![]);
    let first = walker(5, 0.0, vec![aura(5.0, 1.1, true), aura(5.0, 1.25, true)]);
    let second = walker(6, 0.0, vec![aura(5.0, 0.5, false)]);
    let other = walker(
        7,
        0.0,
        vec![Task {
            object_group: Some(class::VILLAGER),
            ..aura(5.0, 3.0, false)
        }],
    );
    let model = model_of(vec![unit.clone(), first, second, other], vec![]);
    assert_eq!(bound_of(&model, &unit).0, Speed::new(1.0 + (0.25 + 0.5)));
    // no object holds a source: no aura is near
    let unheld = MotionBound::of(&model, no_parts(), &[]).unwrap();
    assert_eq!(unheld.speed(&unit), Speed::new(1.0));
}

/// A stinger task (type 157) that raises the speed of the units it names leaves their speed
/// without bound (each hit adds it again); one that slows them adds nothing.
#[test]
fn raising_stinger_leaves_the_speed_without_bound() {
    let stinger = |value| Task {
        action_kind: STINGER,
        object_group: Some(POWERED),
        wait_time: Seconds::new(5.0),
        work_val1: WorkValue::new(value),
        ..Task::default()
    };
    let unit = walker(4, 1.0, vec![]);
    let slowing = model_of(
        vec![unit.clone(), walker(5, 0.0, vec![stinger(-0.15)])],
        vec![],
    );
    let raising = model_of(
        vec![unit.clone(), walker(5, 0.0, vec![stinger(0.15)])],
        vec![],
    );
    assert_eq!(
        (bound_of(&slowing, &unit).0, bound_of(&raising, &unit).0),
        (Speed::new(1.0), Speed::new(f32::INFINITY))
    );
}

/// A siege unit (class 13) adds the speed of the infantry its garrison capacity holds
/// (`powerup::SPEED_PER_INFANTRY` each); a unit of another class adds none.
#[test]
fn siege_unit_bound_adds_its_garrison_of_infantry() {
    let ram = Master {
        class: class::SIEGE,
        garrison: GarrisonCapacity::of_data(4),
        ..walker(35, 0.5, vec![])
    };
    let cart = Master {
        class: class::VILLAGER,
        ..ram.clone()
    };
    let model = model_of(vec![ram.clone()], vec![]);
    let per_infantry = model.tuning.powerup.speed_per_infantry;
    assert_eq!(
        (bound_of(&model, &ram).0, bound_of(&model, &cart).0),
        (Speed::new(0.5 + 4.0 * per_infantry), Speed::new(0.5))
    );
}

/// A type-70 master with a charge (`max_charge` above 0) of special attack 3 runs by its run
/// task's factor (0x85, any, 0x7d1: its `work_range`), else by the tuning's; one without a
/// charge, or with a charge of another special attack, does not run.
#[test]
fn charger_bound_runs_by_its_run_factor() {
    let charger = |tasks| Master {
        creatable: Some(CreatablePart {
            max_charge: Charge::new(10.0),
            special_attack: SpecialAttack::CHARGE,
            ..CreatablePart::default()
        }),
        ..walker(4, 1.0, tasks)
    };
    let other_charge = Master {
        creatable: Some(CreatablePart {
            max_charge: Charge::new(10.0),
            ..CreatablePart::default()
        }),
        ..walker(4, 1.0, vec![])
    };
    let run = Task {
        action_kind: ActionKind::named(0x85),
        work_flag2: 0x7d1,
        work_range: Distance::new(2.0),
        ..Task::default()
    };
    let model = model_of(vec![], vec![]);
    let factor = model.tuning.movement.charge_run_factor;
    assert_eq!(
        (
            bound_of(&model, &charger(vec![run])).0,
            bound_of(&model, &charger(vec![])).0,
            bound_of(&model, &walker(4, 1.0, vec![])).0,
            bound_of(&model, &other_charge).0
        ),
        (
            Speed::new(2.0),
            Speed::new(factor),
            Speed::new(1.0),
            Speed::new(1.0)
        )
    );
}

/// The reach is the largest work range, or the weapon range plus the range auras that name the
/// master.
#[test]
fn bound_reach_adds_the_range_auras_to_the_weapon_range() {
    let archer = |work_range| Master {
        combat: Some(CombatPart {
            weapon_range: Distance::new(5.0),
            ..CombatPart::default()
        }),
        ..walker(
            4,
            1.0,
            vec![Task {
                work_range: Distance::new(work_range),
                ..Task::default()
            }],
        )
    };
    let model = model_of(vec![walker(5, 0.0, vec![aura(12.0, 1.0, false)])], vec![]);
    assert_eq!(
        (
            bound_of(&model, &archer(3.0)).1,
            bound_of(&model, &archer(7.0)).1
        ),
        (Distance::new(6.0), Distance::new(7.0))
    );
}

/// A speed that is not a number has no bound.
#[test]
fn nan_speed_has_no_bound() {
    let unit = walker(4, f32::NAN, vec![]);
    let model = model_of(vec![], vec![]);
    assert_eq!(bound_of(&model, &unit).0, Speed::new(f32::INFINITY));
}
