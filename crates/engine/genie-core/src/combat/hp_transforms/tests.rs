//! The hit point transforms: their entries from the tasks of type 158 and their test.

use crate::combat::attack::AttackState;
use crate::combat::attack::tests::own_attack;
use crate::combat::damage::{Victim, lose};
use crate::combat::hp_transforms::{HP_TRANSFORM, HitTransforms, HpTransform, hp_transforms};
use crate::combat::victims::change_masters_of_hit;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Position;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::Regeneration;
use crate::objects::actions::ActionList;
use crate::objects::layers::Action;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ActionPart;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Masters;
use crate::objects::masters::Task;
use crate::objects::masters::TaskFlags;
use crate::objects::masters::WorkValue;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::testing::player_holding;
use crate::testing::scene::{PLAYER, Scene, SceneObject, moving_master, walking};
use crate::world::Player;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::World;
use crate::world::index::Place;
use crate::world::slots::PlayerSlotsExt;
use crate::world::tests::in_update;

/// A hit point transform task into master `into` at `threshold`, without attributes, not
/// scaled.
fn transform_task(into: f32, threshold: f32) -> Task {
    Task {
        action_kind: HP_TRANSFORM,
        work_val1: WorkValue::new(into),
        work_val2: WorkValue::new(threshold),
        ..Task::default()
    }
}

/// The transform into master 1976 at `threshold`, without attributes, not scaled.
const fn into_1976(threshold: f32) -> HpTransform {
    HpTransform {
        into: 1976,
        threshold,
        enablement: None,
        addend: None,
        scaled: false,
    }
}

/// A walking combat master `id` of 70 hit points with `tasks`.
fn jian(id: i16, tasks: Vec<Task>) -> Master {
    Master {
        id: MasterId::named(id),
        kind: MasterKind::COMBAT,
        hit_points: MasterHitPoints::of_data(70),
        action: Some(ActionPart {
            tasks,
            ..ActionPart::default()
        }),
        ..moving_master(walking(Speed::new(1.0)))
    }
}

/// Slot 277 takes the master's tasks of type 158 in their order: the master the work value
/// names (`CVTTSS2SI`, toward 0; a NaN the integer indefinite), the threshold, the enabling
/// and the added attribute, and a share of the master's hit points when the task's flag byte
/// is 1 (not bit 0: 3 is not scaled).
#[test]
fn transforms_are_the_hit_point_tasks_in_their_order() {
    let scaled = Task {
        attribute_kind2: Some(AttributeId::named(7)),
        attribute_kind3: Some(AttributeId::named(8)),
        combat_level_flag: TaskFlags::from_bits_retain(1),
        ..transform_task(-3.5, 0.5)
    };
    let flagged = Task {
        combat_level_flag: TaskFlags::from_bits_retain(3),
        ..transform_task(f32::NAN, 10.0)
    };
    let aura = Task {
        action_kind: ActionKind::named(155),
        ..transform_task(1.0, 1.0)
    };
    let master = jian(
        1974,
        vec![transform_task(1976.9, -45.0), aura, scaled, flagged],
    );
    assert_eq!(
        hp_transforms(&master),
        [
            into_1976(-45.0),
            HpTransform {
                into: -3,
                threshold: 0.5,
                enablement: Some(AttributeId::named(7)),
                addend: Some(AttributeId::named(8)),
                scaled: true,
            },
            HpTransform {
                into: i32::MIN,
                threshold: 10.0,
                enablement: None,
                addend: None,
                scaled: false,
            },
        ]
    );
    assert_eq!(hp_transforms(&jian(1974, Vec::new())), []);
}

/// The masters 1974 and 1976, both of 70 hit points.
fn jians() -> Masters {
    Masters::new([jian(1974, Vec::new()), jian(1976, Vec::new())])
}

/// A player holding attributes 0 to 8, attribute 7 at `enabled` and attribute 8 at `added`.
fn holder(enabled: f32, added: f32) -> Player {
    let mut attributes = vec![Amount::ZERO; 9];
    attributes[7] = Amount::new(enabled);
    attributes[8] = Amount::new(added);
    player_holding(attributes)
}

/// The master `transform` gives an object of master 1974 at `hp` hit points of `player`.
fn found(transform: HpTransform, hp: f32, player: &Player) -> Option<MasterId> {
    transform
        .master_of(&jian(1974, Vec::new()), hp, &player.attributes, &jians())
        .expect("the player's attributes")
        .map(|master| master.id)
}

/// A threshold above 0 holds for hit points above it, one below 0 for hit points below its
/// size; 0 and a NaN, threshold or hit points, hold for none.
#[test]
fn transform_holds_above_its_threshold_or_below_its_size() {
    let player = holder(0.0, 0.0);
    let jian_1976 = Some(MasterId::named(1976));
    let at = |threshold, hp| found(into_1976(threshold), hp, &player);
    assert_eq!(
        [at(30.0, 31.0), at(30.0, 30.0), at(30.0, 29.0)],
        [jian_1976, None, None]
    );
    assert_eq!(
        [
            at(-45.0, 44.0),
            at(-45.0, 45.0),
            at(-45.0, 46.0),
            at(-45.0, 0.0)
        ],
        [jian_1976, None, None, jian_1976]
    );
    assert_eq!(
        [at(0.0, 10.0), at(0.0, -1.0), at(-0.0, -1.0)],
        [None, None, None]
    );
    assert_eq!(
        [at(f32::NAN, 10.0), at(30.0, f32::NAN), at(-45.0, f32::NAN)],
        [None, None, None]
    );
}

/// The enabling attribute must not be at most 0 (`JNC`: a NaN enables); the added attribute
/// adds to the threshold before the scaling by the master's hit points; an attribute the player
/// does not hold fails.
#[test]
fn transform_reads_its_player_attributes_and_scales_by_the_master() {
    let enabled = HpTransform {
        enablement: Some(AttributeId::named(7)),
        ..into_1976(30.0)
    };
    let jian_1976 = Some(MasterId::named(1976));
    assert_eq!(
        [1.0, 0.0, -1.0, f32::NAN].map(|amount| found(enabled, 31.0, &holder(amount, 0.0))),
        [jian_1976, None, None, jian_1976]
    );
    let added = HpTransform {
        addend: Some(AttributeId::named(8)),
        ..into_1976(10.0)
    };
    let player = holder(0.0, 20.0);
    assert_eq!(
        [found(added, 31.0, &player), found(added, 29.0, &player)],
        [jian_1976, None]
    );
    let scaled = HpTransform {
        scaled: true,
        ..into_1976(0.5)
    };
    assert_eq!(
        [found(scaled, 36.0, &player), found(scaled, 34.0, &player)],
        [jian_1976, None],
        "a half of 70"
    );
    let both = HpTransform {
        addend: Some(AttributeId::named(8)),
        scaled: true,
        ..into_1976(0.25)
    };
    let quarter = holder(0.0, 0.25);
    assert_eq!(
        [found(both, 36.0, &quarter), found(both, 34.0, &quarter)],
        [jian_1976, None],
        "the attribute added before the scaling"
    );
    let unheld = HpTransform {
        addend: Some(AttributeId::named(9)),
        ..into_1976(10.0)
    };
    let master = jian(1974, Vec::new());
    assert!(
        unheld
            .master_of(&master, 31.0, &player.attributes, &jians())
            .is_err()
    );
    let unheld_enablement = HpTransform {
        enablement: Some(AttributeId::named(9)),
        ..into_1976(10.0)
    };
    assert!(
        unheld_enablement
            .master_of(&master, 31.0, &player.attributes, &jians())
            .is_err()
    );
}

/// A transform whose test holds gives no master its player's table lacks, none beyond the
/// table's ids and none below 0.
#[test]
fn transform_into_master_the_table_lacks_gives_none() {
    let player = holder(0.0, 0.0);
    let into = |id| HpTransform {
        into: id,
        ..into_1976(30.0)
    };
    assert_eq!(
        [1977, 70_000, -1, i32::MIN].map(|id| found(into(id), 31.0, &player)),
        [None, None, None, None]
    );
}

/// Object 7 of `transforming` at `hp` hit points, with an Attack on object 8, a foundation when
/// `desolid`; and object 8 of `other` (so that the players' tables hold it).
fn transforming_world(hp: f32, transforming: Master, other: Master, desolid: bool) -> World {
    let attacking = Action {
        list: ActionList::default().with_attack(Some(own_attack(
            8,
            AttackState::New,
            Distance::new(1.0),
        ))),
        ..Action::default()
    };
    Scene::open(GridSize::new(8, 8))
        .object(SceneObject {
            actions: Some(attacking),
            hp: HitPoints::new(hp),
            desolid,
            ..SceneObject::new(7, PLAYER, transforming, Position::new(2.5, 2.5, 0.0))
        })
        .object(SceneObject::new(
            8,
            PLAYER,
            other,
            Position::new(5.5, 5.5, 0.0),
        ))
        .build()
}

/// Object 7, a 1974 at `hp` hit points that becomes a 1976 below 45 hit points, with an Attack
/// on object 8, a 1976.
fn jian_world(hp: f32) -> World {
    let transforming = jian(1974, vec![transform_task(1976.0, -45.0)]);
    transforming_world(hp, transforming, jian(1976, Vec::new()), false)
}

/// Object 7's master and hit points, and whether it still attacks.
fn transformed(world: &World) -> (MasterId, HitPoints, bool) {
    let id = ObjectId::try_from(7).unwrap();
    let Some(Place::Live(player, idx)) = world.place(id) else {
        panic!("object 7 lives");
    };
    let unit = world.players.live(player, idx).unwrap();
    let attacks = world.parts.actions_of(id).list.attack().is_some();
    (unit.life.master_id, unit.life.hp, attacks)
}

/// Runs the hit point transforms of object 7.
fn transform(world: &mut World) {
    let Some(Place::Live(player, idx)) = world.place(ObjectId::try_from(7).unwrap()) else {
        panic!("object 7 lives");
    };
    world.transform_by_hit_points(player, idx).unwrap();
}

/// Below its threshold the unit takes its transform's master and keeps its actions (slot 0x90
/// with 1) and its share of the hit points; above it nothing changes.
#[test]
fn unit_below_its_threshold_takes_the_transform_master_and_keeps_its_actions() {
    let mut wounded = jian_world(40.0);
    transform(&mut wounded);
    assert_eq!(
        transformed(&wounded),
        (MasterId::named(1976), HitPoints::new(40.0), true)
    );
    let mut healthy = jian_world(50.0);
    transform(&mut healthy);
    assert_eq!(
        transformed(&healthy),
        (MasterId::named(1974), HitPoints::new(50.0), true)
    );
}

/// A building `id` of 70 hit points with `tasks`, its build time `id` seconds less 1900.
fn tower(id: i16, tasks: Vec<Task>) -> Master {
    Master {
        id: MasterId::named(id),
        kind: MasterKind::BUILDING,
        hit_points: MasterHitPoints::of_data(70),
        action: Some(ActionPart {
            tasks,
            ..ActionPart::default()
        }),
        creatable: Some(CreatablePart {
            build_time: WholeSeconds::of_data(id - 1900),
            ..CreatablePart::default()
        }),
        ..Master::default()
    }
}

/// A building's transform is its slot 0x90 (`FUN_1411e6d90`): it takes the new master and, a
/// foundation, the new master's build time.
#[test]
fn building_below_its_threshold_takes_the_transform_master_and_build_time() {
    let transforming = tower(1974, vec![transform_task(1976.0, -45.0)]);
    let mut world = transforming_world(40.0, transforming, tower(1976, Vec::new()), true);
    transform(&mut world);
    assert_eq!(transformed(&world).0, MasterId::named(1976));
    let progress = world
        .parts
        .building_of(ObjectId::try_from(7).unwrap())
        .construction
        .as_ref()
        .map(|construction| construction.progress);
    assert_eq!(progress, Some(Seconds::new(76.0)));
}

/// Object 7 of master `id` at `hp` hit points, regenerating `rate` points a minute, that becomes
/// master `into` past `threshold`, and object 8 of master `into`.
fn regenerating_world(hp: f32, id: i16, into: i16, threshold: f32, rate: f32) -> World {
    let regenerating = |id, tasks| Master {
        creatable: Some(CreatablePart {
            regeneration: Regeneration::new(rate),
            ..CreatablePart::default()
        }),
        ..jian(id, tasks)
    };
    let transforming = regenerating(id, vec![transform_task(f32::from(into), threshold)]);
    transforming_world(hp, transforming, regenerating(into, Vec::new()), false)
}

/// The regeneration's gain below the most and its loss run the transforms' test
/// (`TRIBE_Combat_Object::update`'s slots 0xb8 and 0xb0): a 1976 at 44 hit points that gains 2
/// becomes a 1974 above 45, a 1974 at 46 that loses 2 becomes a 1976 below 45.
#[test]
fn regeneration_past_threshold_transforms_the_unit() {
    let regenerate = |world: &mut World| {
        let id = ObjectId::try_from(7).unwrap();
        let index = world.live_index(PLAYER, id).unwrap();
        world
            .regenerate_object((PLAYER, index, id), Seconds::new(2.0))
            .unwrap();
    };
    let mut healed = regenerating_world(44.0, 1976, 1974, 45.0, 60.0);
    regenerate(&mut healed);
    assert_eq!(
        transformed(&healed),
        (MasterId::named(1974), HitPoints::new(46.0), true)
    );
    let mut hurt = regenerating_world(46.0, 1974, 1976, -45.0, -60.0);
    regenerate(&mut hurt);
    assert_eq!(
        transformed(&hurt),
        (MasterId::named(1976), HitPoints::new(44.0), true)
    );
}

/// Object 7 as a hit of player 2 reads it.
fn hit_victim(world: &World) -> Victim {
    world
        .victim(ObjectId::try_from(7).unwrap(), PlayerId::new(2))
        .unwrap()
        .expect("object 7 lives")
}

/// A hit's read of a transforming unit resolves its transforms: from its master on, each master
/// a transform gives with what the hit reads of it; a master without transforms has none.
#[test]
fn hit_victim_reads_the_masters_its_transforms_give() {
    let victim = hit_victim(&jian_world(46.0));
    let into = |current, hp| {
        victim
            .transforms
            .first_holding(MasterId::named(current), HitPoints::new(hp))
            .map(|master| (master.id, master.reads.master_id))
    };
    let jian_1976 = MasterId::named(1976);
    assert_eq!(into(1974, 44.0), Some((jian_1976, jian_1976)));
    assert_eq!(into(1974, 46.0), None);
    assert_eq!(into(1976, 46.0), None, "1976 has no transform");
    let plain = Scene::open(GridSize::new(8, 8))
        .object(SceneObject::new(
            7,
            PLAYER,
            jian(1974, Vec::new()),
            Position::new(2.5, 2.5, 0.0),
        ))
        .build();
    assert_eq!(hit_victim(&plain).transforms, HitTransforms::default());
}

/// Slot 22 in a hit that leaves the victim past its threshold gives it the new master at once
/// (the rest of the hit reads it) and records it; the write-back gives the object the new master
/// with its actions kept. A loss below 1 hit point credits the kill and transforms nothing.
#[test]
fn hit_loss_past_threshold_changes_the_master_at_once_and_at_write_back() {
    let mut world = jian_world(46.0);
    let mut victim = hit_victim(&world);
    lose(&mut victim, HitPoints::new(2.0));
    let jian_1976 = MasterId::named(1976);
    assert_eq!(
        (victim.master_id, victim.hp, victim.master_changes.clone()),
        (jian_1976, HitPoints::new(44.0), vec![jian_1976])
    );
    change_masters_of_hit(&mut world, &mut victim).unwrap();
    assert_eq!(victim.master_changes, []);
    assert_eq!(transformed(&world).0, jian_1976);
    assert!(transformed(&world).2, "its actions kept");
    let mut dying = hit_victim(&jian_world(1.5));
    lose(&mut dying, HitPoints::new(1.0));
    assert_eq!(
        (dying.master_id, dying.master_changes.len()),
        (MasterId::named(1974), 0)
    );
}

/// The updating unit's own master changes (a retaliation's transforms) go to it lent to the
/// world.
#[test]
fn own_master_changes_go_to_the_updating_unit() {
    let mut world = jian_world(40.0);
    let Some(Place::Live(player, idx)) = world.place(ObjectId::try_from(7).unwrap()) else {
        panic!("object 7 lives");
    };
    let master = in_update(&mut world, (player, idx), 0, |unit, actions, step| {
        let mut own = Victim {
            master_changes: vec![MasterId::named(1976)],
            ..Victim::default()
        };
        unit.take_own_master_changes(step, actions, &mut own)?;
        Ok(unit.life.master_id)
    })
    .unwrap();
    assert_eq!(master, MasterId::named(1976));
}
