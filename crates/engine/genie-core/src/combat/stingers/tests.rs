//! An object's stingers: its master's stinger tasks, a hit's hand-off, the save's entries and
//! the stinger update.

use crate::combat::AttackBonus;
use crate::combat::ClassOrMaster;
use crate::combat::damage::Allied;
use crate::combat::damage::ArmorBonuses;
use crate::combat::damage::Attacker;
use crate::combat::damage::Victim;
use crate::combat::powerup::PowerupAttribute;
use crate::combat::stingers::update::{StingerReads, update_stingers};
use crate::combat::stingers::{
    ActiveStinger, STINGER, StingerReceiver, StingerSeconds, StingerTask, StingerValue, hand_off,
    stinger_tasks,
};
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Position;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::Regeneration;
use crate::objects::layers::Combat;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ActionPart;
use crate::objects::masters::AnimatedPart;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::CombatPart;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::DamageAmount;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Task;
use crate::objects::masters::TaskFlags;
use crate::objects::masters::WorkValue;
use crate::players::attribute::AttributeId;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::scene::{PLAYER, Scene, SceneObject};
use crate::world::Seconds;
use crate::world::WorldTime;

/// A master with stinger tasks among its tasks: the Fire Archer's burn of a class (attribute
/// 277, kind 109, −60 for 5 seconds, on its target), a lasting scaled and unique speed stinger
/// on itself for master 83, and a task of another type.
fn archer() -> Master {
    Master {
        id: MasterId::named(1968),
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        hit_points: MasterHitPoints::of_data(50),
        animated: Some(AnimatedPart {
            max_speed: Speed::new(1.25),
        }),
        action: Some(ActionPart {
            tasks: vec![
                Task {
                    action_kind: STINGER,
                    object_group: Some(ClassId::named(3)),
                    attribute_kind2: Some(AttributeId::named(277)),
                    wait_time: Seconds::new(109.0),
                    work_val1: WorkValue::new(-60.0),
                    work_val2: WorkValue::new(5.0),
                    work_range: Distance::new(1.0),
                    ..Task::default()
                },
                Task {
                    action_kind: ActionKind::AURA,
                    ..Task::default()
                },
                Task {
                    action_kind: STINGER,
                    object_id: Some(MasterId::named(83)),
                    wait_time: Seconds::new(5.9),
                    work_val1: WorkValue::new(-0.15),
                    work_val2: WorkValue::new(-1.0),
                    work_range: Distance::ZERO,
                    combat_level_flag: TaskFlags::STINGER_SCALED | TaskFlags::STINGER_UNIQUE,
                    ..Task::default()
                },
            ],
            ..ActionPart::default()
        }),
        combat: Some(CombatPart {
            reload: Seconds::new(2.0),
            base_armor: DamageAmount::new(1),
            armors: vec![
                ClassAmount::new(4, 2),
                ClassAmount::new(3, 5),
                ClassAmount::new(3, 7),
            ],
            ..CombatPart::default()
        }),
        creatable: Some(CreatablePart {
            regeneration: Regeneration::new(3.0),
            ..CreatablePart::default()
        }),
        ..Master::default()
    }
}

/// Slot 277's stinger tasks are the master's tasks of type 157 in their order: the target by
/// class (900 up) or master, the enabling attribute, the kind and seconds as whole numbers
/// (`CVTTSS2SI`), the value, the attacker for a work range of 0, and the flags.
#[test]
fn stinger_tasks_keep_the_masters_tasks_of_kind_157_in_order() {
    let tasks = stinger_tasks(&archer());
    assert_eq!(
        tasks,
        vec![
            StingerTask {
                target: ClassOrMaster::of_class(ClassId::named(3)),
                enablement: Some(AttributeId::named(277)),
                kind: PowerupAttribute::REGENERATION_RATE,
                value: StingerValue::new(-60.0),
                seconds: StingerSeconds::new(5),
                receiver: StingerReceiver::Target,
                flags: TaskFlags::NONE,
            },
            StingerTask {
                target: ClassOrMaster::of_master(MasterId::named(83)),
                enablement: None,
                kind: PowerupAttribute::MOVEMENT_SPEED,
                value: StingerValue::new(-0.15),
                seconds: StingerSeconds::new(-1),
                receiver: StingerReceiver::Attacker,
                flags: TaskFlags::STINGER_SCALED | TaskFlags::STINGER_UNIQUE,
            },
        ]
    );
}

/// A handed stinger ends its seconds after the world time (32-bit wrapping), or for good below
/// 0; it keeps the task's value, kind, target and flags and names its attacker.
#[test]
fn handed_stinger_ends_its_seconds_after_now_or_never() {
    let now = WorldTime::from_millis(1000);
    assert_eq!(
        StingerSeconds::new(5).end(now),
        WorldTime::from_millis(6000)
    );
    assert_eq!(
        StingerSeconds::new(-1).end(now),
        WorldTime::from_millis(u32::MAX)
    );
    assert_eq!(
        StingerSeconds::new(1).end(WorldTime::from_millis(u32::MAX - 500)),
        WorldTime::from_millis(499)
    );
    let task = stinger_tasks(&archer())[0];
    let stinger = ActiveStinger::handed(&task, object_id(7), now);
    assert_eq!(
        stinger,
        ActiveStinger {
            end: WorldTime::from_millis(6000),
            value: StingerValue::new(-60.0),
            kind: PowerupAttribute::REGENERATION_RATE,
            key: ClassOrMaster::of_class(ClassId::named(3)),
            attacker: Some(object_id(7)),
            flags: TaskFlags::NONE,
        }
    );
    assert_eq!(stinger.value.get(), -60.0);
}

/// The load reads a saved stinger's 32 bytes as they are: the end, the value, the kind, the key,
/// the attacker (−1 none) and the flags' byte; the sprite pointer (`+0x18`) is the renderer's.
#[test]
fn saved_stinger_reads_its_32_bytes() {
    let mut raw = [0xaa_u8; 32];
    raw[0..4].copy_from_slice(&9000_u32.to_le_bytes());
    raw[4..8].copy_from_slice(&(-0.15_f32).to_le_bytes());
    raw[8..12].copy_from_slice(&5_i32.to_le_bytes());
    raw[12..16].copy_from_slice(&904_i32.to_le_bytes());
    raw[16..20].copy_from_slice(&(-1_i32).to_le_bytes());
    raw[20..24].copy_from_slice(&3_u32.to_le_bytes());
    assert_eq!(
        ActiveStinger::of_save(&raw),
        ActiveStinger {
            end: WorldTime::from_millis(9000),
            value: StingerValue::new(-0.15),
            kind: PowerupAttribute::MOVEMENT_SPEED,
            key: ClassOrMaster::of_data(904),
            attacker: None,
            flags: TaskFlags::STINGER_SCALED | TaskFlags::STINGER_UNIQUE,
        }
    );
}

/// A stinger of `kind` with `value` and `flags` until world time 10000, for the update's tests.
fn stinger(kind: PowerupAttribute, value: f32, flags: TaskFlags) -> ActiveStinger {
    ActiveStinger {
        end: WorldTime::from_millis(10_000),
        value: StingerValue::new(value),
        kind,
        key: ClassOrMaster::of_data(-1),
        attacker: Some(object_id(7)),
        flags,
    }
}

/// The update starts every sum at 0, then each kind adds its value, scaled by the master's (hit
/// points, speed, the shown attack, reload, regeneration, armor of class 4 or 3); the attack
/// unscaled is its floor, the armors are whole numbers, and kind 120 adds the hit points' share
/// only unscaled. A kind of no case adds nothing.
#[test]
fn update_rebuilds_each_kinds_sum_from_zero() {
    let scaled = TaskFlags::STINGER_SCALED;
    let plain = TaskFlags::NONE;
    let mut combat = Combat {
        speed_stinger: Speed::new(9.0),
        regen_stinger: Regeneration::new(9.0),
        reload_stinger: Seconds::new(9.0),
        hp_stinger: HitPoints::new(9.0),
        attack_stinger: AttackBonus::new(9.0),
        armor_stingers: ArmorBonuses::new(9, 9),
        active_stingers: vec![
            stinger(PowerupAttribute::HIT_POINTS, 0.5, scaled),
            stinger(PowerupAttribute::HIT_POINTS, 2.0, plain),
            stinger(PowerupAttribute::MOVEMENT_SPEED, -0.2, scaled),
            stinger(PowerupAttribute::ATTACK, 0.5, scaled),
            stinger(PowerupAttribute::ATTACK, 2.7, plain),
            stinger(PowerupAttribute::ATTACK_RELOAD_TIME, 0.25, scaled),
            stinger(PowerupAttribute::REGENERATION_RATE, -60.0, plain),
            stinger(PowerupAttribute::REGENERATION_RATE, 2.0, scaled),
            stinger(PowerupAttribute::REGENERATION_HP_PERCENT, 0.1, plain),
            stinger(PowerupAttribute::REGENERATION_HP_PERCENT, 0.1, scaled),
            stinger(PowerupAttribute::MELEE_ARMOR, 1.5, scaled),
            stinger(PowerupAttribute::MELEE_ARMOR, -2.9, plain),
            stinger(PowerupAttribute::PIERCE_ARMOR, 0.5, scaled),
            stinger(PowerupAttribute::WORK_RATE, 4.0, plain),
        ],
        ..Combat::default()
    };
    let master = archer();
    let reads = StingerReads {
        master: &master,
        shown_attack: Some(DamageAmount::new(12)),
    };
    update_stingers(&mut combat, reads, WorldTime::from_millis(500)).unwrap();
    assert_eq!(combat.hp_stinger, HitPoints::new(2.0 + 50.0 * 0.5));
    assert_eq!(combat.speed_stinger, Speed::new(1.25 * -0.2 + 0.0));
    assert_eq!(combat.attack_stinger, AttackBonus::new(2.0 + 12.0 * 0.5));
    assert_eq!(combat.reload_stinger, Seconds::new(0.25 * 2.0 + 0.0));
    assert_eq!(
        combat.regen_stinger,
        Regeneration::new(50.0 * 0.1 + (2.0 * 3.0 + (-60.0 + 0.0)))
    );
    // melee: 2 × 1.5 = 3, then −2 (CVTTSS2SI truncates); pierce: the first class-3 entry, 5
    assert_eq!(combat.armor_stingers, ArmorBonuses::new(1, 2));
    assert_eq!(combat.active_stingers.len(), 14);
}

/// An ended stinger goes (the world time at or past its end, unsigned); the loop's index moves
/// on past the erase, so the stinger that took its place waits for the next pass.
#[test]
fn ended_stinger_goes_and_the_next_one_waits_one_pass() {
    let plain = TaskFlags::NONE;
    let ended = ActiveStinger {
        end: WorldTime::from_millis(500),
        ..stinger(PowerupAttribute::REGENERATION_RATE, -100.0, plain)
    };
    let mut combat = Combat {
        active_stingers: vec![
            ended,
            stinger(PowerupAttribute::REGENERATION_RATE, -1.0, plain),
            stinger(PowerupAttribute::REGENERATION_RATE, -2.0, plain),
        ],
        ..Combat::default()
    };
    let master = archer();
    let reads = StingerReads {
        master: &master,
        shown_attack: None,
    };
    update_stingers(&mut combat, reads, WorldTime::from_millis(500)).unwrap();
    assert_eq!(combat.regen_stinger, Regeneration::new(-2.0 + 0.0));
    assert_eq!(combat.active_stingers.len(), 2);
    update_stingers(&mut combat, reads, WorldTime::from_millis(600)).unwrap();
    assert_eq!(combat.regen_stinger, Regeneration::new(-2.0 + (-1.0 + 0.0)));
}

/// A unique stinger whose kind and key an earlier active one of the pass had goes; every active
/// one counts toward the pairs, a unique one or not. The erase skips the next one too.
#[test]
fn unique_stinger_counts_once_per_kind_and_key() {
    let unique = TaskFlags::STINGER_UNIQUE;
    let keyed = |key: i32, value: f32, flags: TaskFlags| ActiveStinger {
        key: ClassOrMaster::of_data(key),
        ..stinger(PowerupAttribute::REGENERATION_RATE, value, flags)
    };
    let mut combat = Combat {
        active_stingers: vec![
            keyed(904, -1.0, TaskFlags::NONE),
            keyed(904, -2.0, unique),
            keyed(905, -4.0, unique),
            keyed(906, -8.0, unique),
        ],
        ..Combat::default()
    };
    let master = archer();
    let reads = StingerReads {
        master: &master,
        shown_attack: None,
    };
    update_stingers(&mut combat, reads, WorldTime::from_millis(500)).unwrap();
    assert_eq!(combat.regen_stinger, Regeneration::new(-8.0 + (-1.0 + 0.0)));
    assert_eq!(
        combat
            .active_stingers
            .iter()
            .map(|active| active.key.get())
            .collect::<Vec<_>>(),
        vec![904, 905, 906]
    );
}

/// A scaled attack stinger reads the shown attack; without one read the update fails.
#[test]
fn scaled_attack_stinger_needs_the_shown_attack() {
    let mut combat = Combat {
        active_stingers: vec![stinger(
            PowerupAttribute::ATTACK,
            0.5,
            TaskFlags::STINGER_SCALED,
        )],
        ..Combat::default()
    };
    let master = archer();
    let reads = StingerReads {
        master: &master,
        shown_attack: None,
    };
    assert!(update_stingers(&mut combat, reads, WorldTime::ZERO).is_err());
}

/// A combat unit with the hit effects ability against a target whose player and its own are
/// not allied both ways hands each stinger task that names the target, in order: the target's
/// to the target, the attacker's to a TRIBE attacker only.
#[test]
fn hand_off_gives_the_named_target_its_stingers_in_task_order() {
    let tasks = vec![
        StingerTask {
            target: ClassOrMaster::of_class(class::VILLAGER),
            ..stinger_tasks(&archer())[0]
        },
        StingerTask {
            target: ClassOrMaster::of_data(-1),
            ..stinger_tasks(&archer())[1]
        },
        StingerTask {
            target: ClassOrMaster::of_master(MasterId::named(84)),
            ..stinger_tasks(&archer())[0]
        },
    ];
    let attacker = || Victim {
        id: object_id(7),
        kind: MasterKind::COMBAT,
        source: Attacker {
            ability: CombatAbility::HIT_EFFECTS,
            ..Attacker::default()
        },
        stinger_tasks: tasks.clone(),
        ..Victim::default()
    };
    let target = || Victim {
        id: object_id(8),
        class: class::VILLAGER,
        master_id: MasterId::named(83),
        ..Victim::default()
    };
    let (mut src, mut struck) = (attacker(), target());
    hand_off(&mut src, &mut struck);
    assert_eq!(struck.stung, vec![(object_id(7), tasks[0])]);
    assert_eq!(src.stung, vec![(object_id(7), tasks[1])]);
    // a missile (type 60) cannot take its own
    let (mut src, mut struck) = (
        Victim {
            kind: MasterKind::MISSILE,
            ..attacker()
        },
        target(),
    );
    hand_off(&mut src, &mut struck);
    assert_eq!((src.stung.len(), struck.stung.len()), (0, 1));
    // allied both ways, or without the ability: none
    let (mut src, mut struck) = (
        attacker(),
        Victim {
            allied: Allied::BOTH,
            ..target()
        },
    );
    hand_off(&mut src, &mut struck);
    assert!(src.stung.is_empty() && struck.stung.is_empty());
    let mut src = attacker();
    src.source.ability = CombatAbility::empty();
    let mut struck = target();
    hand_off(&mut src, &mut struck);
    assert!(src.stung.is_empty() && struck.stung.is_empty());
}

/// A lasting task's stinger (seconds below 0) never ends.
#[test]
fn lasting_tasks_stinger_never_ends() {
    let task = stinger_tasks(&archer())[1];
    let stinger = ActiveStinger::handed(&task, object_id(12), WorldTime::from_millis(70_000));
    assert_eq!(stinger.end, WorldTime::from_millis(u32::MAX));
    assert_eq!(stinger.attacker, Some(object_id(12)));
}

/// The world's stinger update of a combat unit rebuilds its sums from its active stingers at the
/// world time; an object below type 70 keeps its own.
#[test]
fn world_updates_the_stingers_of_combat_units_only() {
    let animal = Master {
        id: MasterId::named(594),
        kind: MasterKind::ANIMATED,
        ..Master::default()
    };
    let mut world = Scene::open(GridSize::new(8, 8))
        .object(SceneObject::new(
            7,
            PLAYER,
            archer(),
            Position::new(2.5, 2.5, 0.0),
        ))
        .object(SceneObject::new(
            8,
            PLAYER,
            animal,
            Position::new(4.5, 2.5, 0.0),
        ))
        .build();
    world.world_time = WorldTime::from_millis(9000);
    for id in [object_id(7), object_id(8)] {
        let combat = world.parts.combat_mut(id).unwrap();
        combat.active_stingers = vec![
            stinger(PowerupAttribute::REGENERATION_RATE, -60.0, TaskFlags::NONE),
            ActiveStinger {
                end: WorldTime::from_millis(9000),
                ..stinger(PowerupAttribute::REGENERATION_RATE, -1.0, TaskFlags::NONE)
            },
        ];
        combat.regen_stinger = Regeneration::new(5.0);
    }
    for id in [object_id(7), object_id(8)] {
        let index = world.live_index(PLAYER, id).unwrap();
        world.object_stingers((PLAYER, index, id)).unwrap();
    }
    let unit = world.parts.combat_of(object_id(7));
    assert_eq!(unit.regen_stinger, Regeneration::new(-60.0 + 0.0));
    assert_eq!(unit.active_stingers.len(), 1);
    let animal = world.parts.combat_of(object_id(8));
    assert_eq!(animal.regen_stinger, Regeneration::new(5.0));
    assert_eq!(animal.active_stingers.len(), 2);
}
