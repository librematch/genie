//! An object's stingers: its master's stinger tasks, a hit's hand-off, the save's entries and
//! the stinger update.

use crate::combat::AttackBonus;
use crate::combat::ClassOrMaster;
use crate::combat::KillState;
use crate::combat::damage::Allied;
use crate::combat::damage::ArmorBonuses;
use crate::combat::damage::Armored;
use crate::combat::damage::Attacker;
use crate::combat::damage::KillCount;
use crate::combat::damage::Victim;
use crate::combat::damage::lose;
use crate::combat::powerup::PowerupAttribute;
use crate::combat::stingers::update::{StingerReads, update_stingers};
use crate::combat::stingers::{
    ActiveStinger, STINGER, StingerAttackers, StingerReceiver, StingerSeconds, StingerTask,
    StingerValue, credit_combat, hand_off, stinger_tasks,
};
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Position;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
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
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::test_ids::costs;
use crate::test_ids::object_id;
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::scene::{PLAYER, Scene, SceneObject};
use crate::world::Seconds;
use crate::world::World;
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

/// A combat unit of player 2 at `hp` hit points, its stingers' list held with the burns of
/// `burning` ([`StingerAttackers`]), for slot 22's tests (`FUN_141205c80`).
fn burning(hp: f32, burning: Vec<Option<ObjectId>>) -> Victim {
    Victim {
        id: object_id(70),
        kind: MasterKind::COMBAT,
        player: PlayerId::new(2),
        hp: HitPoints::new(hp),
        armored: Some(Armored::default()),
        stinger_attackers: StingerAttackers {
            held: true,
            burning,
        },
        ..Victim::default()
    }
}

/// Slot 22's loss that leaves its object below 1 hit point (`FUN_141205c80`) records the
/// attackers of its burns, those of its list in their order (an ended one and a −1 among them),
/// then those the hit handed it (a speed stinger's attacker is not one), and counts its kill
/// (`+0x954`).
#[test]
fn loss_below_one_credits_the_burns_and_counts_the_kill() {
    let [burn, speed] = [0, 1].map(|index| stinger_tasks(&archer())[index]);
    let mut victim = burning(3.0, vec![Some(object_id(5)), None, Some(object_id(5))]);
    victim.stung = vec![(object_id(7), burn), (object_id(8), speed)];
    lose(&mut victim, HitPoints::new(2.5));
    assert_eq!(victim.hp, HitPoints::new(0.5));
    assert_eq!(
        victim.kill_counts,
        vec![KillCount::Stingers(vec![
            Some(object_id(5)),
            None,
            Some(object_id(5)),
            Some(object_id(7)),
        ])]
    );
    assert_eq!(victim.kill_state, KillState::Counted);
}

/// Slot 22 counts the kill of an object with stingers but no burns too, crediting nobody, and
/// a counted kill credits nothing again; a survivor, an object without stingers and one below
/// type 70 (`FUN_141189160`) credit nothing; a NaN loss takes the credits' branch
/// (`COMISS hp, 1` / `JC`).
#[test]
fn slot_22_credits_only_the_first_death_of_an_object_with_stingers() {
    let mut no_burns = burning(2.0, Vec::new());
    lose(&mut no_burns, HitPoints::new(1.5));
    assert_eq!(no_burns.kill_counts, vec![KillCount::Stingers(Vec::new())]);
    assert_eq!(no_burns.kill_state, KillState::Counted);
    lose(&mut no_burns, HitPoints::new(0.25));
    assert_eq!(no_burns.kill_counts.len(), 1);
    let mut survivor = burning(5.0, vec![Some(object_id(5))]);
    lose(&mut survivor, HitPoints::new(1.0));
    let mut bare = Victim {
        stinger_attackers: StingerAttackers::default(),
        ..burning(2.0, Vec::new())
    };
    lose(&mut bare, HitPoints::new(1.5));
    let mut below_tribe = Victim {
        kind: MasterKind::RGE_COMBAT,
        ..burning(2.0, vec![Some(object_id(5))])
    };
    lose(&mut below_tribe, HitPoints::new(1.5));
    for victim in [&survivor, &bare, &below_tribe] {
        assert!(victim.kill_counts.is_empty() && !victim.kill_state.is_counted());
    }
    let mut unordered = burning(2.0, vec![Some(object_id(5))]);
    lose(&mut unordered, HitPoints::new(f32::NAN));
    assert_eq!(
        unordered.kill_counts,
        vec![KillCount::Stingers(vec![Some(object_id(5))])]
    );
}

/// The regeneration's slot 22 (`Unit::lose_hp`) credits the burns its object holds (kinds 109
/// and 120, not the speed stinger) once: at a loss below 1 hit point, its kill not counted yet.
#[test]
fn the_regenerations_loss_credits_the_held_burns_once() {
    let held = |kind, attacker| ActiveStinger {
        attacker: Some(object_id(attacker)),
        ..stinger(kind, -1.0, TaskFlags::NONE)
    };
    let mut combat = Combat {
        active_stingers: vec![
            held(PowerupAttribute::REGENERATION_RATE, 5),
            held(PowerupAttribute::MOVEMENT_SPEED, 6),
            held(PowerupAttribute::REGENERATION_HP_PERCENT, 7),
        ],
        ..Combat::default()
    };
    assert_eq!(credit_combat(&mut combat, HitPoints::new(1.0)), None);
    assert_eq!(combat.kill_state, KillState::Open);
    assert_eq!(
        credit_combat(&mut combat, HitPoints::new(0.5)),
        Some(KillCount::Stingers(vec![
            Some(object_id(5)),
            Some(object_id(7))
        ]))
    );
    assert_eq!(combat.kill_state, KillState::Counted);
    assert_eq!(credit_combat(&mut combat, HitPoints::new(0.25)), None);
}

/// Slot 22's credits in the world (`World::credit_burns`, `FUN_1412005b0` per player): each player
/// of an attacker the world holds counts once, in the order they first appear; an attacker it no
/// longer holds and a −1 credit nobody. The game walks the players in an address order no run
/// repeats: the counts come out the same in either order, bit for bit (`CODING_STYLE.md` section
/// 12).
#[test]
fn burns_credit_each_player_once_in_any_order() {
    let at = |x| Position::new(x, 1.5, 0.0);
    let mut world = Scene::open(GridSize::new(8, 8))
        .players(3)
        .object(SceneObject::new(5, PlayerId::new(1), archer(), at(1.5)))
        .object(SceneObject::new(6, PlayerId::new(1), archer(), at(2.5)))
        .object(SceneObject::new(7, PlayerId::new(3), archer(), at(3.5)))
        .build();
    for player in &mut world.players {
        player.attributes = vec![Amount::ZERO; 500];
    }
    let attackers = [
        Some(object_id(6)),
        None,
        Some(object_id(99)),
        Some(object_id(7)),
        Some(object_id(5)),
    ];
    assert_eq!(
        world.burning_players(&attackers),
        [PlayerId::new(1), PlayerId::new(3)]
    );
    let victim = Master {
        creatable: Some(CreatablePart {
            cost: costs(&[(attribute::WOOD, 25, true), (attribute::GOLD, 45, true)]),
            ..CreatablePart::default()
        }),
        ..archer()
    };
    let credited = |order: &[Option<ObjectId>]| {
        let mut credited = world.clone();
        credited
            .credit_burns(PlayerId::new(2), &victim, order)
            .unwrap();
        credited
            .players
            .iter()
            .map(|player| {
                player
                    .attributes
                    .iter()
                    .map(|amount| amount.get().to_bits())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    let reversed: Vec<_> = attackers.iter().rev().copied().collect();
    assert_eq!(credited(&attackers), credited(&reversed));
    let mut forward = world.clone();
    forward
        .credit_burns(PlayerId::new(2), &victim, &attackers)
        .unwrap();
    let (one, two) = (
        &forward.players[1].attributes,
        &forward.players[2].attributes,
    );
    assert_eq!(
        [one[20], one[302], one[170], one[402]],
        [1.0, 1.0, 70.0, 70.0].map(Amount::new)
    );
    assert_eq!(
        [two[154], two[326], two[328], two[152]],
        [2.0, 1.0, 1.0, 140.0].map(Amount::new)
    );
}

/// A burn's kill through the regeneration (`World::regenerate_object`, slot 22 in
/// `TRIBE_Combat_Object::update`'s loss): a unit of player 1 at 1.5 hit points that burns at −60
/// a minute (a point a second) for 2 seconds dies, its kill counted (`+0x954`), and the burn's
/// attacker's player 3 counts it once (attribute 20, 300 + 1; its owner 325 + 3); a later loss
/// credits nothing again.
#[test]
fn burning_units_death_credits_its_burner_once() {
    let at = |x| Position::new(x, 1.5, 0.0);
    let (burner, burnt) = (object_id(7), object_id(9));
    let mut world = Scene::open(GridSize::new(8, 8))
        .players(3)
        .object(SceneObject::new(7, PlayerId::new(3), archer(), at(1.5)))
        .object(SceneObject::new(9, PlayerId::new(1), archer(), at(3.5)))
        .build();
    for player in &mut world.players {
        player.attributes = vec![Amount::ZERO; 500];
    }
    world.object_mut(burnt).unwrap().1.life.hp = HitPoints::new(1.5);
    let combat = world.parts.combat_mut(burnt).unwrap();
    combat.regen_stinger = Regeneration::new(-60.0);
    combat.active_stingers = vec![ActiveStinger {
        attacker: Some(burner),
        ..stinger(PowerupAttribute::REGENERATION_RATE, -60.0, TaskFlags::NONE)
    }];
    let index = world.live_index(PlayerId::new(1), burnt).unwrap();
    world
        .regenerate_object((PlayerId::new(1), index, burnt), Seconds::new(2.0))
        .unwrap();
    assert!(world.object(burnt).unwrap().1.life.hp < HitPoints::new(1.0));
    assert_eq!(world.parts.combat_of(burnt).kill_state, KillState::Counted);
    let counted = |world: &World| {
        let (killer, owner) = (&world.players[3].attributes, &world.players[1].attributes);
        [killer[20], killer[301], owner[154], owner[328]]
    };
    assert_eq!(counted(&world), [1.0; 4].map(Amount::new));
    world.object_mut(burnt).unwrap().1.life.hp = HitPoints::new(1.5);
    world
        .regenerate_object((PlayerId::new(1), index, burnt), Seconds::new(2.0))
        .unwrap();
    assert_eq!(counted(&world), [1.0; 4].map(Amount::new));
}
