//! The aura groups and the powerup update.

use crate::combat::AttackBonus;
use crate::combat::ClassOrMaster;
use crate::combat::aura::{
    AuraArea, AuraEntry, AuraPowerup, AuraRole, MostObjects, aura_groups, aura_relations,
};
use crate::combat::powerup::{PowerupAmount, PowerupAttribute, PowerupShare, powerup_until};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::math::msvc::{HashKey, MapHash};
use crate::math::rng::Stream;
use crate::objects::Graphics;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ActionPart;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::CombatPart;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::TargetOwner;
use crate::objects::masters::Task;
use crate::objects::masters::TaskFlags;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::WorkValue;
use crate::objects::sprite::Graphic;
use crate::objects::{FacetCount, Frame};
use crate::players::RelationMask;
use crate::players::UnitRelation;
use crate::testing::class;
use crate::testing::scene::{PLAYER, Scene, SceneObject};
use crate::vision::VisionBits;
use crate::vision::registry::QueryShape;
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;

/// Each owner type keeps its relations; an unnamed one keeps every relation.
#[test]
fn aura_relations_follow_the_owner_kind() {
    let bit = UnitRelation::bit;
    assert_eq!(aura_relations(TargetOwner::OWN).get(), 2);
    assert_eq!(aura_relations(TargetOwner::ENEMY).get(), 0x18);
    assert_eq!(aura_relations(TargetOwner::GAIA).get(), 1);
    assert_eq!(aura_relations(TargetOwner::FRIENDLY).get(), 7);
    assert_eq!(aura_relations(TargetOwner::ENEMY_OR_GAIA).get(), 0x19);
    assert_eq!(aura_relations(TargetOwner::OTHER).get(), 0x1d);
    assert_eq!(aura_relations(TargetOwner::ANY).get(), 0x1f);
    assert_eq!(aura_relations(TargetOwner::of_data(9)).get(), 0x1f);
    assert_eq!(
        aura_relations(TargetOwner::OTHER).get(),
        0x1f & !bit(UnitRelation::Own)
    );
}

/// The keys hash as the game's `FUN_1411e86d0` and `FUN_141200200` (the values computed from
/// the decompiled formulas on their own).
#[test]
fn aura_keys_hash_as_the_game() {
    let area = AuraArea {
        radius: Distance::new(10.0),
        shape: QueryShape::RoundedRectangle,
        relations: RelationMask::of(&[UnitRelation::Own]),
    };
    assert_eq!(area.map_hash(), MapHash::Data(0x50ff_fac3_9b13_d616));
    let powerup = AuraPowerup {
        share: PowerupShare::MasterShare,
        attribute: PowerupAttribute::MOVEMENT_SPEED,
        amount: PowerupAmount::new(0.2),
        most: MostObjects::new(1.0),
        sprite: None,
    };
    assert_eq!(powerup.map_hash(), MapHash::Data(0x22f5_db9e_18b0_ebb5));
}

/// An aura task of class `target`, attribute `attribute` (its wait time), amount `amount` at most
/// `most` objects within `radius`, for `owner`'s objects, with `flags`.
fn aura_task(
    target: ClassId,
    attribute: f32,
    (amount, most): (f32, f32),
    radius: f32,
    (owner, flags): (TargetOwner, TaskFlags),
) -> Task {
    Task {
        action_kind: ActionKind::AURA,
        object_group: Some(target),
        wait_time: Seconds::new(attribute),
        work_val1: WorkValue::new(amount),
        work_val2: WorkValue::new(most),
        work_range: Distance::new(radius),
        owner_kind: owner,
        combat_level_flag: flags,
        ..Task::default()
    }
}

/// A master of id 901 with `tasks` and the combat ability `ability`, of radii 0.2.
fn aura_master(tasks: Vec<Task>, ability: CombatAbility) -> Master {
    Master {
        id: MasterId::named(901),
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        radius: Footprint::new(0.2, 0.2),
        action: Some(ActionPart {
            tasks,
            ..ActionPart::default()
        }),
        combat: Some(CombatPart {
            combat_ability: ability,
            ..CombatPart::default()
        }),
        ..Master::default()
    }
}

/// The Elite Hoplite's (2111) groups: a 10-tile attack aura for the enemies, a 3-tile armor aura
/// of four parts and a 2-tile one for its own. The game's map lists the 3-tile group first (its
/// bucket, 4, the 10-tile one's, goes in front of it), then the 10-tile and the 2-tile ones; each
/// group's class is listed once; a task of no most objects joins none.
#[test]
fn aura_groups_list_areas_in_the_maps_order() {
    let rounded = TaskFlags::AURA_ROUNDED;
    let own = (TargetOwner::OWN, rounded);
    let enemy = (TargetOwner::ENEMY, rounded);
    let infantry = class::VILLAGER;
    let tasks = vec![
        aura_task(infantry, 9.0, (3.0, 15.0), 10.0, enemy),
        aura_task(infantry, 116.0, (3.0, 6.0), 3.0, own),
        aura_task(infantry, 117.0, (1.0, 2.0), 3.0, own),
        aura_task(infantry, 116.0, (4.0, 8.0), 3.0, own),
        aura_task(infantry, 117.0, (2.0, 4.0), 3.0, own),
        aura_task(infantry, 116.0, (3.0, 6.0), 2.0, own),
        aura_task(infantry, 116.0, (3.0, 0.0), 2.0, own),
    ];
    let master = aura_master(tasks, CombatAbility::AURA);
    let groups = aura_groups(&master, None, &Graphics::new());
    let radii: Vec<f32> = groups.iter().map(|group| group.area.radius.get()).collect();
    assert_eq!(radii, [3.0, 10.0, 2.0]);
    let three = &groups[0];
    assert_eq!(three.area.relations.get(), 2);
    assert_eq!(three.area.shape, QueryShape::RoundedRectangle);
    assert_eq!(three.classes, [infantry]);
    let parts: Vec<(i32, f32)> = three
        .parts
        .iter()
        .map(|part| (part.powerup.attribute.get(), part.powerup.amount.get()))
        .collect();
    assert_eq!(parts, [(116, 3.0), (117, 1.0), (116, 4.0), (117, 2.0)]);
    assert_eq!(groups[1].area.relations.get(), 0x18);
    assert_eq!(groups[2].parts.len(), 1);
    assert_eq!(
        groups[2].parts[0].entries,
        [AuraEntry {
            target: ClassOrMaster::of_class(infantry),
            role: AuraRole::Powered,
            flags: rounded,
            enablement: None,
        }]
    );
}

/// A graphic of id `id`, one frame.
fn graphic(id: i16) -> Graphic {
    Graphic {
        id: SpriteId::named(id),
        frames: Frame::of_data(1),
        facets: FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    }
}

/// HYUJI's (2047) two parts equal but for their sprites (13023 and 13024): the game hashes each
/// sprite's address, so genie lists them in their tasks' order, either way round, as the game
/// does unless the two share a bucket; a part without a sprite still goes by its hash.
#[test]
fn parts_differing_by_sprite_keep_task_order() {
    let own = (TargetOwner::OWN, TaskFlags::NONE);
    let with_sprite = |sprite: i16| Task {
        sprites: TaskSprites {
            proceeding: Some(SpriteId::named(sprite)),
            ..TaskSprites::default()
        },
        ..aura_task(class::VILLAGER, 109.0, (150.0, 1.0), 6.0, own)
    };
    let graphics = Graphics::from([
        (SpriteId::named(13023), graphic(13023)),
        (SpriteId::named(13024), graphic(13024)),
    ]);
    for sprites in [[13023, 13024], [13024, 13023]] {
        let tasks = sprites.iter().map(|&sprite| with_sprite(sprite)).collect();
        let groups = aura_groups(&aura_master(tasks, CombatAbility::AURA), None, &graphics);
        let listed: Vec<Option<SpriteId>> = groups[0]
            .parts
            .iter()
            .map(|part| part.powerup.sprite)
            .collect();
        assert_eq!(listed, sprites.map(|sprite| Some(SpriteId::named(sprite))));
        assert_eq!(groups[0].parts[0].powerup.map_hash(), MapHash::Address);
    }
}

/// A task without the rounded flag searches a box (mode 1); a NaN radius is never found again,
/// so each such task makes a group of its own.
#[test]
fn nan_radius_makes_group_each_time() {
    let box_own = (TargetOwner::OWN, TaskFlags::NONE);
    let tasks = vec![
        aura_task(class::VILLAGER, 9.0, (1.0, 1.0), f32::NAN, box_own),
        aura_task(class::VILLAGER, 9.0, (1.0, 1.0), f32::NAN, box_own),
    ];
    let groups = aura_groups(
        &aura_master(tasks, CombatAbility::AURA),
        None,
        &Graphics::new(),
    );
    assert_eq!(groups.len(), 2);
    assert!(
        groups
            .iter()
            .all(|group| group.area.shape == QueryShape::Rectangle)
    );
}

/// Object 7 of `master` at (2.5, 2.5) and object 8, a villager of master 902 of player 1 at
/// (3.5, 2.5), both in player 1's sight registry; the world time 10 s.
fn aura_world(master: Master) -> World {
    let villager = Master {
        id: MasterId::named(902),
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        radius: Footprint::new(0.2, 0.2),
        ..Master::default()
    };
    let seen = Some(VisionBits::visible(PLAYER));
    let mut world = Scene::open(GridSize::new(8, 8))
        .object(SceneObject {
            seen_by: seen,
            ..SceneObject::new(7, PLAYER, master, Position::new(2.5, 2.5, 0.0))
        })
        .object(SceneObject {
            seen_by: seen,
            ..SceneObject::new(8, PLAYER, villager, Position::new(3.5, 2.5, 0.0))
        })
        .build();
    world.world_time = WorldTime::from_millis(10_000);
    world
}

/// Object `id`.
fn object(id: i32) -> ObjectId {
    ObjectId::try_from(id).unwrap()
}

/// Object 7's powerup update.
fn update(world: &mut World) {
    let index = world.live_index(PLAYER, object(7)).unwrap();
    world.powerup_update((PLAYER, index, object(7))).unwrap();
}

/// The attack aura finds the villager nearby and gives it the amount at one object of one, the
/// Object stream's draw ending it; the aura's own object takes nothing.
#[test]
fn attack_aura_powers_up_the_object_it_finds() {
    let task = aura_task(
        class::VILLAGER,
        9.0,
        (2.0, 1.0),
        3.0,
        (TargetOwner::OWN, TaskFlags::AURA_ROUNDED),
    );
    let mut world = aura_world(aura_master(vec![task], CombatAbility::AURA));
    let mut streams = world.streams.clone();
    let draw = streams.get_mut(Stream::Object).rand15();
    update(&mut world);
    let found = world.parts.combat_of(object(8));
    assert_eq!(found.attack_powerup, AttackBonus::new(2.0));
    assert_eq!(found.powerup_update, powerup_until(world.world_time, draw));
    assert_eq!(
        world.parts.combat_of(object(7)).attack_powerup,
        AttackBonus::ZERO
    );
    assert_eq!(world.streams, streams);
}

/// A counted entry counts the villager toward the amount; on its own object the aura gives it
/// to itself. Without the ability, or out of reach, nothing happens.
#[test]
fn counted_aura_on_self_gives_its_amount_to_itself() {
    let mut task = aura_task(
        class::VILLAGER,
        9.0,
        (4.0, 2.0),
        3.0,
        (TargetOwner::OWN, TaskFlags::AURA_ROUNDED),
    );
    task.search_flag = 1;
    let ability = CombatAbility::AURA | CombatAbility::AURA_ON_SELF;
    let mut world = aura_world(aura_master(vec![task], ability));
    update(&mut world);
    // one object of most 2: 1 × 4 / 2
    assert_eq!(
        world.parts.combat_of(object(7)).attack_powerup,
        AttackBonus::new(2.0)
    );
    assert_eq!(
        world.parts.combat_of(object(8)).attack_powerup,
        AttackBonus::ZERO
    );
    let mut idle = aura_world(aura_master(vec![task], CombatAbility::NONE));
    let streams = idle.streams.clone();
    update(&mut idle);
    assert_eq!(
        idle.parts.combat_of(object(7)).attack_powerup,
        AttackBonus::ZERO
    );
    assert_eq!(idle.streams, streams);
    let far = Task {
        work_range: Distance::new(0.1),
        ..task
    };
    let mut away = aura_world(aura_master(vec![far], ability));
    update(&mut away);
    assert_eq!(
        away.parts.combat_of(object(7)).attack_powerup,
        AttackBonus::ZERO
    );
}

/// An entry that is off, or that works only while a timed ability lasts when none began, names
/// no target; a dead object's aura does not run.
#[test]
fn aura_entries_off_or_waiting_for_ability_do_not_work() {
    let rounded = TaskFlags::AURA_ROUNDED;
    for flags in [
        rounded | TaskFlags::AURA_OFF,
        rounded | TaskFlags::AURA_WHILE_ABILITY,
    ] {
        let task = aura_task(
            class::VILLAGER,
            9.0,
            (2.0, 1.0),
            3.0,
            (TargetOwner::OWN, flags),
        );
        let mut world = aura_world(aura_master(vec![task], CombatAbility::AURA));
        update(&mut world);
        assert_eq!(
            world.parts.combat_of(object(8)).attack_powerup,
            AttackBonus::ZERO
        );
    }
    let task = aura_task(
        class::VILLAGER,
        9.0,
        (2.0, 1.0),
        3.0,
        (TargetOwner::OWN, rounded),
    );
    let mut world = aura_world(aura_master(vec![task], CombatAbility::AURA));
    let index = world.live_index(PLAYER, object(7)).unwrap();
    world.players[1].objects[index].life.state = ObjectState::Dying;
    update(&mut world);
    assert_eq!(
        world.parts.combat_of(object(8)).attack_powerup,
        AttackBonus::ZERO
    );
}

/// Once the world time is past an object's powerups' end, the update resets them.
#[test]
fn powerups_past_their_end_reset() {
    let mut world = aura_world(aura_master(Vec::new(), CombatAbility::NONE));
    world.parts.combat_mut(object(7)).unwrap().attack_powerup = AttackBonus::new(3.0);
    world.parts.combat_mut(object(7)).unwrap().powerup_update = WorldTime::from_millis(10_000);
    update(&mut world);
    assert_eq!(
        world.parts.combat_of(object(7)).attack_powerup,
        AttackBonus::new(3.0)
    );
    world.world_time = WorldTime::from_millis(10_001);
    update(&mut world);
    assert_eq!(
        world.parts.combat_of(object(7)).attack_powerup,
        AttackBonus::ZERO
    );
}
