use super::*;
use crate::combat::attack::AttackState;
use crate::combat::damage::Victim;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::TerrainFlags;
use crate::map::Tile;
use crate::map::terrain::TileTerrain;
use crate::math::fmath::BVector;
use crate::objects::ActionState;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::Voice;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::CarryCapacity;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::SoundEvents;
use crate::objects::masters::TaskFlags;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::TaskTerrain;
use crate::ownership::CaptureKind;
use crate::players::Amount;
use crate::players::PlayerKind;
use crate::players::attribute::AttributeId;
use crate::test_ids::class_id;
use crate::test_ids::costs;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::ids;
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::world::Millis;
use crate::world::WorldTime;
use std::sync::Arc;

fn task(action_kind: ActionKind) -> Task {
    Task {
        action_kind,
        ..Task::default()
    }
}

/// The Feitoria's first task (1021, task 0): food from attribute 205 at 1.6 a second.
fn feitoria_food() -> Task {
    Task {
        object_group: None,
        attribute_kind2: Some(AttributeId::named(205)),
        attribute_kind3: Some(attribute::FOOD),
        work_val1: WorkValue::new(1.6),
        combat_level_flag: TaskFlags::from_bits_retain(0x1),
        ..task(GENERATE)
    }
}

/// The Keshik's task 5 (1228): gold from attribute 213 at 0.01 while it fights class 0.
fn keshik_gold() -> Task {
    Task {
        object_group: Some(class_id(0)),
        attribute_kind2: Some(AttributeId::named(213)),
        attribute_kind3: Some(attribute::GOLD),
        work_val1: WorkValue::new(0.01),
        combat_level_flag: TaskFlags::from_bits_retain(0x1),
        ..task(GENERATE)
    }
}

#[test]
fn the_living_path_takes_at_least_1_hit_point_or_nan() {
    assert!(alive(HitPoints::new(1.0)));
    assert!(alive(HitPoints::new(25.0)));
    assert!(alive(HitPoints::new(f32::NAN)));
    assert!(!alive(HitPoints::new(0.999_999_94)));
    assert!(!alive(HitPoints::ZERO));
}

#[test]
fn task_with_its_kind_flag_bit_generates_always() {
    // bit 1 for an object of type 70, bit 2 for one of type 80
    let mut task = feitoria_food();
    task.combat_level_flag = TaskFlags::UNITS_ALWAYS;
    assert!(generates(
        &task,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        None,
        None
    ));
    assert!(!generates(
        &task,
        MasterKind::BUILDING,
        MasterKind::BUILDING,
        None,
        None
    ));
    task.combat_level_flag = TaskFlags::BUILDINGS_ALWAYS;
    assert!(generates(
        &task,
        MasterKind::BUILDING,
        MasterKind::BUILDING,
        None,
        None
    ));
    assert!(!generates(
        &task,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        None,
        None
    ));
}

#[test]
fn building_generates_once_built() {
    let task = feitoria_food();
    assert!(!generates(
        &task,
        MasterKind::BUILDING,
        MasterKind::BUILDING,
        None,
        None
    ));
    assert!(generates(
        &task,
        MasterKind::BUILDING,
        MasterKind::BUILDING,
        None,
        Some(BuildingSight::Built)
    ));
    // a combat unit's master is not a building's
    assert!(!generates(
        &task,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        None,
        Some(BuildingSight::Built)
    ));
}

#[test]
fn unit_generates_while_its_top_action_works_on_target_of_the_task_class() {
    let task = keshik_gold();
    let target = WorkTarget {
        hp: HitPoints::new(45.0),
        storage: Amount::ZERO,
        class: class::CLASS_0,
    };
    let top = |kind, state, target| {
        Some(TopWork {
            kind,
            state,
            target,
        })
    };
    // the Attack (9), type 7, the Gather (5) and the Hunt (0x6e), in state 7
    for kind in [ATTACK, ActionKind::COMBAT, GATHER, HUNT] {
        assert!(generates(
            &task,
            MasterKind::COMBAT,
            MasterKind::COMBAT,
            top(kind, ActionState::new(7), Some(target)),
            None
        ));
    }
    assert!(!generates(
        &task,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        top(MOVE_TO, ActionState::new(7), Some(target)),
        None
    ));
    assert!(!generates(
        &task,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        top(ATTACK, ActionState::new(6), Some(target)),
        None
    ));
    assert!(!generates(
        &task,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        top(ATTACK, ActionState::new(7), None),
        None
    ));
    assert!(!generates(
        &task,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        None,
        None
    ));
    // another class
    let other = WorkTarget {
        class: class::CLASS_6,
        ..target
    };
    assert!(!generates(
        &task,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        top(ATTACK, ActionState::new(7), Some(other)),
        None
    ));
    // a dead target counts while its master stores some, never with a NaN
    let dead = WorkTarget {
        hp: HitPoints::ZERO,
        storage: Amount::new(1.0),
        ..target
    };
    assert!(generates(
        &task,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        top(ATTACK, ActionState::new(7), Some(dead)),
        None
    ));
    let empty = WorkTarget {
        hp: HitPoints::ZERO,
        storage: Amount::ZERO,
        ..target
    };
    assert!(!generates(
        &task,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        top(ATTACK, ActionState::new(7), Some(empty)),
        None
    ));
    let nan = WorkTarget {
        hp: HitPoints::new(f32::NAN),
        storage: Amount::new(f32::NAN),
        ..target
    };
    assert!(!generates(
        &task,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        top(ATTACK, ActionState::new(7), Some(nan)),
        None
    ));
}

#[test]
fn the_amount_is_dt_times_the_rate_times_the_attribute_in_that_order() {
    let task = feitoria_food();
    let dt = 0.044_f32;
    assert_eq!(
        generated(Seconds::new(dt), &task, Amount::new(1.0))
            .get()
            .to_bits(),
        (dt * 1.6 * 1.0).to_bits()
    );
    let amount = 0.3_f32;
    assert_eq!(
        generated(Seconds::new(dt), &task, Amount::new(amount))
            .get()
            .to_bits(),
        ((dt * 1.6) * amount).to_bits()
    );
    assert_eq!(
        generated(Seconds::new(dt), &task, Amount::ZERO),
        Amount::ZERO
    );
}

#[test]
fn the_store_gives_what_reached_1_and_starts_again() {
    let amount = Amount::new;
    let mut stores = vec![Amount::ZERO; 4];
    assert_eq!(store(amount(0.6), attribute::FOOD, &mut stores), None);
    assert_eq!(stores[0], amount(0.6));
    // ADDSS: the amount plus the store
    let sum = 0.5_f32 + 0.6;
    assert_eq!(
        store(amount(0.5), attribute::FOOD, &mut stores),
        Some(amount(sum))
    );
    assert_eq!(stores[0], Amount::ZERO);
    // nothing above 0 joins: the store stays, and nothing is given
    stores[1] = amount(0.9);
    assert_eq!(store(Amount::ZERO, attribute::WOOD, &mut stores), None);
    assert_eq!(store(amount(f32::NAN), attribute::WOOD, &mut stores), None);
    assert_eq!(store(amount(-2.0), attribute::WOOD, &mut stores), None);
    assert_eq!(stores[1], amount(0.9));
    // a NaN store gives nothing (JC)
    stores[2] = amount(f32::NAN);
    assert_eq!(store(amount(0.5), attribute::STONE, &mut stores), None);
    // a store the player does not have: no store and no give, however large
    assert_eq!(store(amount(5.0), AttributeId::named(9), &mut stores), None);
}

#[test]
fn the_movement_damage_timer_runs_down_to_0_and_then_hits() {
    let mut seconds = Seconds::new(0.5);
    assert!(!movement_damage_due(&mut seconds, Seconds::new(0.2)));
    assert_eq!(seconds, Seconds::new(0.5) - Seconds::new(0.2));
    assert!(movement_damage_due(&mut seconds, Seconds::new(0.4)));
    assert_eq!(seconds, Seconds::ZERO);
    // at 0 it does not run down and hits
    assert!(movement_damage_due(&mut seconds, Seconds::new(0.4)));
    assert_eq!(seconds, Seconds::ZERO);
    // to exactly 0
    let mut seconds = Seconds::new(0.25);
    assert!(movement_damage_due(&mut seconds, Seconds::new(0.25)));
    assert_eq!(seconds, Seconds::ZERO);
    // below 0 (a timer the save holds so) hits as it is
    let mut timer = Seconds::new(-1.0);
    assert!(movement_damage_due(&mut timer, Seconds::new(0.1)));
    assert_eq!(timer, Seconds::new(-1.0));
    // a NaN never hits and stays
    let mut seconds = Seconds::new(f32::NAN);
    assert!(!movement_damage_due(&mut seconds, Seconds::new(0.1)));
    assert!(seconds.get().is_nan());
}

#[test]
fn the_reach_is_the_truncated_range_about_the_truncated_place_within_the_map() {
    assert_eq!(
        reach(
            Position::new(10.7, 20.2, 0.0),
            Distance::new(2.9),
            GridSize::new(120, 120)
        ),
        (Tile::new(8, 18), Tile::new(12, 22))
    );
    // from 0, and up to the map's size itself
    assert_eq!(
        reach(
            Position::new(1.5, 118.9, 0.0),
            Distance::new(3.0),
            GridSize::new(120, 120)
        ),
        (Tile::new(0, 115), Tile::new(4, 120))
    );
    // a NaN range is the integer indefinite
    // (5 less it wraps to a negative value, 0; 5 plus it stays below the map's size): no
    // tile
    let (low, high) = reach(
        Position::new(5.0, 5.0, 0.0),
        Distance::new(f32::NAN),
        GridSize::new(120, 120),
    );
    assert_eq!(low, Tile::new(0, 0));
    assert_eq!(high, Tile::new(i32::MIN + 5, i32::MIN + 5));
}

fn hitter() -> Hitter {
    Hitter {
        id: object_id(1),
        blast_level: BlastLevel::from_bits_retain(1),
        position: Position::new(10.0, 10.0, 0.0),
        radius: Footprint::new(0.4, 0.4),
    }
}

fn bystander() -> Bystander {
    Bystander {
        id: object_id(2),
        player: PlayerId::new(2),
        allied: false,
        kind: MasterKind::COMBAT,
        class: class::CLASS_6,
        state: ObjectState::Alive,
        blast_defense: BlastDefense::new(1),
        position: Position::new(11.5, 10.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
    }
}

#[test]
fn movement_damage_strikes_enemies_in_reach() {
    let hitter = hitter();
    let bystander = bystander();
    // the gap is 1.5 - 0.6 = 0.9: within 1 (0.81 at most 1), not within 0.5
    assert!(struck(&hitter, Distance::new(1.0), &bystander, &roles()));
    assert!(!struck(&hitter, Distance::new(0.5), &bystander, &roles()));
    assert!(struck(
        &hitter,
        Distance::new(f32::NAN),
        &bystander,
        &roles()
    ));
    assert!(!struck(
        &hitter,
        Distance::new(1.0),
        &Bystander {
            id: object_id(1),
            ..bystander
        },
        &roles()
    ));
    assert!(!struck(
        &hitter,
        Distance::new(1.0),
        &Bystander {
            allied: true,
            ..bystander
        },
        &roles()
    ));
    assert!(!struck(
        &hitter,
        Distance::new(1.0),
        &Bystander {
            player: PlayerId::new(0),
            ..bystander
        },
        &roles()
    ));
    assert!(!struck(
        &hitter,
        Distance::new(1.0),
        &Bystander {
            state: ObjectState::Dying,
            ..bystander
        },
        &roles()
    ));
    // buildings only of classes 51 and 54
    let building = Bystander {
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        ..bystander
    };
    assert!(!struck(&hitter, Distance::new(1.0), &building, &roles()));
    assert!(struck(
        &hitter,
        Distance::new(1.0),
        &Bystander {
            class: class::CLASS_51,
            ..building
        },
        &roles()
    ));
    assert!(struck(
        &hitter,
        Distance::new(1.0),
        &Bystander {
            class: class::CLASS_54,
            ..building
        },
        &roles()
    ));
    // the blast levels
    let high = Hitter {
        blast_level: BlastLevel::from_bits_retain(3),
        ..hitter
    };
    assert!(!struck(&high, Distance::new(1.0), &bystander, &roles()));
    assert!(struck(
        &high,
        Distance::new(1.0),
        &Bystander {
            blast_defense: BlastDefense::new(2),
            ..bystander
        },
        &roles()
    ));
    assert!(struck(
        &Hitter {
            blast_level: BlastLevel::from_bits_retain(1),
            ..hitter
        },
        Distance::new(1.0),
        &Bystander {
            blast_defense: BlastDefense::new(1),
            ..bystander
        },
        &roles()
    ));
}

#[test]
fn hit_below_1_hit_point_leaves_0_and_holds_nothing() {
    let mut victim = Victim {
        hp: HitPoints::new(0.5),
        amount: Amount::new(30.0),
        capacity: CarryCapacity::of_data(100),
        ..Victim::default()
    };
    assert_eq!(finish(&mut victim), HitEnd::Killed);
    assert_eq!((victim.hp, victim.amount), (HitPoints::ZERO, Amount::ZERO));
    // a capacity below 0 is taken as it is
    let mut victim = Victim {
        hp: HitPoints::new(-3.0),
        amount: Amount::new(30.0),
        capacity: CarryCapacity::of_data(-1),
        ..Victim::default()
    };
    assert_eq!(finish(&mut victim), HitEnd::Killed);
    assert_eq!(
        (victim.hp, victim.amount),
        (HitPoints::ZERO, Amount::new(-1.0))
    );
    let mut victim = Victim {
        hp: HitPoints::new(1.0),
        amount: Amount::new(30.0),
        ..Victim::default()
    };
    assert_eq!(finish(&mut victim), HitEnd::Survived);
    assert_eq!(victim.amount, Amount::new(30.0));
    let mut victim = Victim {
        hp: HitPoints::new(f32::NAN),
        ..Victim::default()
    };
    assert_eq!(finish(&mut victim), HitEnd::Survived);
}

#[test]
fn the_terrain_mask_is_bitset_of_8() {
    // the longboats' -32: the mask 32
    assert_eq!(
        terrain_mask(TaskTerrain::of_data(-32)),
        Some(TerrainFlags::LAND)
    );
    let water = TerrainFlags::WATER | TerrainFlags::DEEP_WATER;
    assert_eq!(terrain_mask(TaskTerrain::of_data(-3)), Some(water));
    // the negation's low byte
    assert_eq!(terrain_mask(TaskTerrain::of_data(-0x103)), Some(water));
    assert_eq!(
        terrain_mask(TaskTerrain::of_data(i16::MIN)),
        Some(TerrainFlags::NONE)
    );
    assert_eq!(
        terrain_mask(TaskTerrain::of_data(1)),
        Some(TerrainFlags::WATER)
    );
    assert_eq!(
        terrain_mask(TaskTerrain::of_data(8)),
        Some(TerrainFlags::SNOW)
    );
    assert_eq!(
        terrain_mask(TaskTerrain::of_data(9)),
        Some(TerrainFlags::NONE)
    );
    assert_eq!(terrain_mask(TaskTerrain::of_data(0)), None);
}

/// The longboat's task 3 (250 in the current build): terrains of flag 32, sprites 19601,
/// −1, 19599, 19599, the dying and undead sprites 19598 and 19600.
fn longboat_look() -> Task {
    Task {
        terrain: TaskTerrain::of_data(-32),
        work_val1: WorkValue::new(0.35),
        work_val2: WorkValue::new(1.0),
        sprites: TaskSprites::of_data([19601, -1, 19599, 19599]),
        sound_events: SoundEvents::new(19598, 19600),
        ..task(TERRAIN_LOOK)
    }
}

#[test]
fn terrain_look_gives_the_task_sprites_on_its_terrains() {
    let longboat = longboat_look();
    let all = |_| true;
    assert_eq!(
        look(
            &longboat,
            TerrainFlags::LAND,
            TerrainFlags::WATER | TerrainFlags::LAND,
            all
        ),
        Some(Look {
            standing: Some(SpriteId::named(19599)),
            walking: Some(SpriteId::named(19601)),
            attacking: Some(SpriteId::named(19599)),
            running: None,
            dying: Some(SpriteId::named(19598)),
            undead: Some(SpriteId::named(19600)),
        })
    );
    assert_eq!(
        look(&longboat, TerrainFlags::LAND, TerrainFlags::WATER, all),
        None
    );
    // a sprite the table lacks is a null one
    let some = |id: i32| id != 19600 && id != 19599;
    let looked = look(&longboat, TerrainFlags::LAND, TerrainFlags::LAND, some).unwrap();
    assert_eq!(
        (looked.standing, looked.undead, looked.walking),
        (None, None, Some(SpriteId::named(19601)))
    );
    // an event id past the table's ids is none
    let task = Task {
        sound_events: SoundEvents::new(0x62d4_b3ec, -5),
        ..longboat_look()
    };
    let look = look(&task, TerrainFlags::LAND, TerrainFlags::LAND, all).unwrap();
    assert_eq!((look.dying, look.undead), (None, None));
}

/// A removal reward task: 10 gold (3) a removal, by attribute 7's rate.
fn bounty() -> Task {
    Task {
        attribute_kind2: Some(attribute::RELICS_HELD),
        attribute_kind3: Some(attribute::GOLD),
        work_val1: WorkValue::new(10.0),
        ..task(REMOVAL_REWARD)
    }
}

#[test]
fn removal_reward_pays_the_rate_times_the_attribute() {
    let task = bounty();
    assert_eq!(
        reward(&task, PlayerId::new(2), Some(Amount::new(1.5)), &[]),
        Reward {
            player: PlayerId::new(2),
            attribute: Some(attribute::GOLD),
            amount: 15
        }
    );
    // truncated
    assert_eq!(
        reward(&task, PlayerId::new(2), Some(Amount::new(0.25)), &[]).amount,
        2
    );
    // the player lacks the attribute, or the task names none
    assert_eq!(reward(&task, PlayerId::new(2), None, &[]).amount, 10);
    let none = Task {
        attribute_kind2: None,
        ..bounty()
    };
    assert_eq!(
        reward(&none, PlayerId::new(2), Some(Amount::new(3.0)), &[]).amount,
        10
    );
    // a NaN rate is the integer indefinite
    assert_eq!(
        reward(&task, PlayerId::new(2), Some(Amount::new(f32::NAN)), &[]).amount,
        i32::MIN
    );
}

#[test]
fn removal_reward_with_its_flag_pays_the_last_paid_cost_of_its_resource_times_the_rate() {
    let task = Task {
        combat_level_flag: TaskFlags::from_bits_retain(0x1),
        ..bounty()
    };
    let cost = costs(&[
        (attribute::GOLD, 20, true),
        (attribute::FOOD, 50, true),
        (attribute::GOLD, 30, false),
    ]);
    assert_eq!(
        reward(&task, PlayerId::new(1), Some(Amount::new(0.5)), &cost).amount,
        100
    );
    let cost = costs(&[(attribute::GOLD, 20, true), (attribute::GOLD, 30, true)]);
    assert_eq!(
        reward(&task, PlayerId::new(1), Some(Amount::new(1.0)), &cost).amount,
        300
    );
    assert_eq!(
        reward(
            &task,
            PlayerId::new(1),
            Some(Amount::new(1.0)),
            &costs(&[(attribute::FOOD, 50, true)])
        )
        .amount,
        0
    );
}

use crate::combat::AttackTargetId;
use crate::combat::attack::{Arms, Attack};
use crate::gathering::hunt::tests::walker;
use crate::map::spatial::{Placed, Spatial};
use crate::map::terrain::{Terrain, TerrainKind};
use crate::movement::Velocity;
use crate::movement::move_to::{MOVE_TO, TargetMoved};
use crate::objects::layers::Static;
use crate::objects::masters::ActionPart;
use crate::objects::masters::WorkValue;
use crate::objects::masters::{Master, Masters};
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::objects::{FacetCount, Frame};
use crate::players::relation_row;
use crate::test_ids::{amounts, attributes};
use crate::testing::roles;
use crate::testing::slots;
use crate::vision::visibility::{BuildingSight, Seer, Visibility, VisibleMap};
use crate::world::FrameSeed;
use crate::world::Player;

/// Gaia and two enemies, the world's dt half a second, a 20 by 20 map.
fn world() -> World {
    let player = |relations: Vec<u8>| Player {
        attributes: vec![Amount::ZERO; 600],
        generated: vec![Amount::ZERO; 600],
        relations: relation_row(relations),
        ..Player::default()
    };
    let players = vec![
        player(vec![0, 3, 3]),
        player(vec![3, 0, 3]),
        player(vec![3, 3, 0]),
    ];
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO; 3],
        players,
    );
    world.dt_seconds = Seconds::new(0.5);
    world.spatial = Spatial::new(GridSize::new(20, 20));
    world.spatial.relations = world
        .players
        .iter()
        .map(|player| player.relations.clone())
        .collect();
    world.model.masters = vec![
        Arc::new(Masters::default()),
        Arc::new(Masters::default()),
        Arc::new(Masters::default()),
    ];
    world
}

/// Master `id` of `kind` and `class` with `tasks`, given to every player.
fn give_master(
    world: &mut World,
    id: MasterId,
    kind: MasterKind,
    class: ClassId,
    tasks: Vec<Task>,
) {
    let master = Master {
        id,
        kind,
        class,
        radius: Footprint::new(0.5, 0.5),
        hit_points: MasterHitPoints::of_data(100),
        action: Some(ActionPart {
            tasks,
            ..ActionPart::default()
        }),
        ..Master::default()
    };
    for ms in &mut world.model.masters {
        let mut all: Vec<Master> = ms.iter().cloned().collect();
        all.push(master.clone());
        *ms = Arc::new(Masters::new(all));
    }
}

/// Object `id` of master `master_id` with `hp` hit points at (`x`, `y`) in player `player_id`'s
/// list, placed on its tile.
fn put(
    world: &mut World,
    player_id: PlayerId,
    id: i32,
    master_id: MasterId,
    hp: HitPoints,
    at: Point,
) {
    let mut unit = Unit {
        id: object_id(id),
        place: Static::at(at.with_height(0.0)),
        ..Unit::default()
    };
    unit.life.master_id = master_id;
    unit.life.hp = hp;
    unit.life.state = ObjectState::Alive;
    let _ = world.push_live(player_id, unit).unwrap();
    let radius = world.model.masters[player_id.index()]
        .get(master_id)
        .map_or(Footprint::new(0.5, 0.5), |master| master.radius);
    world.spatial.insert(
        object_id(id),
        Placed {
            position: at.with_height(0.0),
            radius,
            class: class::CLASS_0,
            player: player_id,
            master: master_id,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: false,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
}

/// The abilities of object `id` of player `player_id` as `player_id`'s, which issue no command.
fn run(world: &mut World, player_id: PlayerId, id: i32) {
    assert_eq!(rewards(world, player_id, id, false), Vec::new());
}

/// The abilities of object `id` of player `player_id` as `player_id`'s: the commands they issue.
fn rewards(world: &mut World, player_id: PlayerId, id: i32, leaves: bool) -> Vec<Reward> {
    let list_index = world.players[player_id.index()]
        .objects
        .position_of(object_id(id))
        .unwrap();
    world
        .abilities((object_id(id), player_id, list_index), player_id, leaves)
        .unwrap()
}

/// The players' sight with `id` a building of player `player_id` whose sight is `sight`.
fn building_sight(world: &mut World, player_id: PlayerId, id: i32, sight: BuildingSight) {
    let map = || VisibleMap {
        counts: vec![0; 400],
        explored: TileCount::ZERO,
    };
    let mut visibility = Visibility::new(
        GridSize::new(20, 20),
        vec![map(), map(), map()],
        vec![VisionBits::NONE; 400],
        Vec::new(),
        0,
        vec![PlayerKind::Human; 3],
    );
    visibility.add_seer(
        object_id(id),
        Seer {
            player: player_id,
            master: master_id(1021),
            los: Distance::new(6.0),
            los_powerup: Distance::ZERO,
            state: ObjectState::Alive,
            explored: true,
            building: Some(sight),
            umv: 0,
            capture: CaptureKind::None,
            kind: MasterKind::BUILDING,
        },
    );
    world.spatial.sight = Some(visibility);
}

#[test]
fn built_feitoria_trickles_food_through_its_player_store() {
    let mut world = world();
    // its explore task (6) does nothing here
    give_master(
        &mut world,
        master_id(1021),
        MasterKind::BUILDING,
        class::BUILDING,
        vec![task(ActionKind::named(6)), feitoria_food()],
    );
    put(
        &mut world,
        PlayerId::new(1),
        50,
        master_id(1021),
        HitPoints::new(2000.0),
        Point::new(5.5, 5.5),
    );
    building_sight(&mut world, PlayerId::new(1), 50, BuildingSight::Built);
    world.players[1].attributes[205] = Amount::new(1.0);
    run(&mut world, PlayerId::new(1), 50);
    let made = 0.5_f32 * 1.6 * 1.0;
    assert_eq!(world.players[1].generated[0], Amount::new(made));
    assert_eq!(world.players[1].attributes[0], Amount::ZERO);
    run(&mut world, PlayerId::new(1), 50);
    let given = made + made;
    assert_eq!(world.players[1].attributes[0], Amount::new(given));
    assert_eq!(world.players[1].attributes[166], Amount::new(given));
    assert_eq!(world.players[1].generated[0], Amount::ZERO);
    // unbuilt, nothing
    building_sight(&mut world, PlayerId::new(1), 50, BuildingSight::Started);
    run(&mut world, PlayerId::new(1), 50);
    assert_eq!(world.players[1].generated[0], Amount::ZERO);
    // the player's attribute 205 at 0: nothing joins the store
    building_sight(&mut world, PlayerId::new(1), 50, BuildingSight::Built);
    world.players[1].attributes[205] = Amount::ZERO;
    run(&mut world, PlayerId::new(1), 50);
    assert_eq!(world.players[1].generated[0], Amount::ZERO);
}

/// A Keshik of player 1 attacking object 77 in state 7.
fn keshik(world: &mut World) {
    give_master(
        world,
        master_id(1228),
        MasterKind::COMBAT,
        class::CLASS_12,
        vec![keshik_gold()],
    );
    put(
        world,
        PlayerId::new(1),
        60,
        master_id(1228),
        HitPoints::new(100.0),
        Point::new(5.5, 5.5),
    );
    let attack = Attack {
        target: Some(AttackTargetId::new(object_id(77))),
        target2: None,
        state: AttackState::Swing,
        timer: Seconds::ZERO,
        range: Distance::ZERO,
        min_range: Distance::ZERO,
        point: Position::new(-1.0, -1.0, -1.0),
        seen: Position::new(-1.0, -1.0, -1.0),
        pending: false,
        swing: Seconds::ZERO,
        rest: Seconds::ZERO,
        arms: Arms {
            move_sprite: None,
            sprite: None,
            second_sprite: None,
            idle_sprite: None,
            idle_attack_sprite: None,
            missile: None,
            fire_frame: Frame::of_data(0),
            interrupt_frame: Frame::of_data(-1),
        },
        voice: Voice::Loud,
        on_target_moved: TargetMoved::Follows,
        same_owner: false,
    };
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_attack(Some(attack));
    world.players[1].attributes[213] = Amount::new(0.5);
    world.players[1].generated[3] = Amount::new(0.999);
}

#[test]
fn keshik_makes_gold_while_it_fights_target_of_its_task_class() {
    let mut world = world();
    keshik(&mut world);
    give_master(
        &mut world,
        master_id(4),
        MasterKind::COMBAT,
        class::CLASS_0,
        Vec::new(),
    );
    put(
        &mut world,
        PlayerId::new(2),
        77,
        master_id(4),
        HitPoints::new(30.0),
        Point::new(6.5, 5.5),
    );
    run(&mut world, PlayerId::new(1), 60);
    let given = 0.5_f32 * 0.01 * 0.5 + 0.999;
    assert_eq!(world.players[1].attributes[3], Amount::new(given));
    assert_eq!(world.players[1].attributes[49], Amount::new(given));
    assert_eq!(world.players[1].attributes[169], Amount::new(given));
    assert_eq!(world.players[1].generated[3], Amount::ZERO);
    // in another state, nothing
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .attack_mut()
        .unwrap()
        .state = AttackState::Ready;
    run(&mut world, PlayerId::new(1), 60);
    assert_eq!(world.players[1].generated[3], Amount::ZERO);
}

/// A combat unit of master 9 at (5.5, 5.5) moving east, with a movement damage of range 2,
/// 5 hit points and a timer of 3 s.
fn trampler(world: &mut World) {
    let trample = Task {
        work_range: Distance::new(2.0),
        work_val1: WorkValue::new(5.0),
        work_val2: WorkValue::new(3.0),
        ..task(MOVEMENT_DAMAGE)
    };
    give_master(
        world,
        master_id(9),
        MasterKind::COMBAT,
        class::CLASS_12,
        vec![trample],
    );
    put(
        world,
        PlayerId::new(1),
        10,
        master_id(9),
        HitPoints::new(100.0),
        Point::new(5.5, 5.5),
    );
    let mut mover = walker();
    mover.velocity = Some(Velocity {
        velocity: BVector::new(1.0, 0.0, 0.0),
        acceleration: BVector::ZERO,
    });
    world
        .parts
        .put_mover(world.players[1].objects[ListIndex::new(0)].id, Some(mover));
}

#[test]
fn moving_unit_movement_damage_hits_the_enemies_about_it() {
    let mut world = world();
    trampler(&mut world);
    give_master(
        &mut world,
        master_id(4),
        MasterKind::COMBAT,
        class::CLASS_6,
        Vec::new(),
    );
    // an enemy in reach, one out of it, one of its own player, one of Gaia
    put(
        &mut world,
        PlayerId::new(2),
        20,
        master_id(4),
        HitPoints::new(30.0),
        Point::new(6.5, 5.5),
    );
    put(
        &mut world,
        PlayerId::new(2),
        21,
        master_id(4),
        HitPoints::new(30.0),
        Point::new(9.5, 5.5),
    );
    put(
        &mut world,
        PlayerId::new(1),
        30,
        master_id(4),
        HitPoints::new(30.0),
        Point::new(5.5, 6.5),
    );
    put(
        &mut world,
        PlayerId::new(0),
        40,
        master_id(4),
        HitPoints::new(30.0),
        Point::new(4.5, 5.5),
    );
    run(&mut world, PlayerId::new(1), 10);
    let hp = |world: &World, id: ObjectId| world.object(id).unwrap().1.life.hp;
    assert_eq!(hp(&world, object_id(20)), HitPoints::new(25.0));
    assert_eq!(hp(&world, object_id(21)), HitPoints::new(30.0));
    assert_eq!(hp(&world, object_id(30)), HitPoints::new(30.0));
    assert_eq!(hp(&world, object_id(40)), HitPoints::new(30.0));
    assert_eq!(
        world
            .parts
            .combat_of(world.players[1].objects[ListIndex::new(0)].id)
            .abilities
            .movement_damage_timer,
        Seconds::new(3.0)
    );
    // the timer runs down first: no hit for 3 s
    run(&mut world, PlayerId::new(1), 10);
    assert_eq!(hp(&world, object_id(20)), HitPoints::new(25.0));
    assert_eq!(
        world
            .parts
            .combat_of(world.players[1].objects[ListIndex::new(0)].id)
            .abilities
            .movement_damage_timer,
        Seconds::new(2.5)
    );
    // standing, no hit even when due
    world
        .parts
        .combat_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .abilities
        .movement_damage_timer = Seconds::ZERO;
    world
        .parts
        .mover_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .velocity = None;
    run(&mut world, PlayerId::new(1), 10);
    assert_eq!(hp(&world, object_id(20)), HitPoints::new(25.0));
}

#[test]
fn movement_damage_kill_leaves_0_hit_points_and_counts_the_kill() {
    let mut world = world();
    trampler(&mut world);
    give_master(
        &mut world,
        master_id(4),
        MasterKind::COMBAT,
        class::CLASS_6,
        Vec::new(),
    );
    put(
        &mut world,
        PlayerId::new(2),
        20,
        master_id(4),
        HitPoints::new(3.0),
        Point::new(6.5, 5.5),
    );
    world.players[2].objects[ListIndex::new(0)].life.amount = Amount::new(10.0);
    run(&mut world, PlayerId::new(1), 10);
    let (_, unit) = world.object(object_id(20)).unwrap();
    assert_eq!(
        (unit.life.hp, unit.life.amount),
        (HitPoints::ZERO, Amount::ZERO)
    );
    // FUN_1412005b0: player 1 a kill (20) of player 2 (302), player 2 a loss (154)
    assert_eq!(world.players[1].attributes[20], Amount::new(1.0));
    assert_eq!(world.players[1].attributes[302], Amount::new(1.0));
    assert_eq!(world.players[2].attributes[154], Amount::new(1.0));
    assert_eq!(world.players[2].attributes[326], Amount::new(1.0));
}

/// The map's terrain: every tile of terrain 1 but (5, 6) of terrain 2, whose flag byte has
/// 32 (deep water).
fn waters(world: &mut World) {
    let mut tiles = vec![(1u8, 255u8, -1i16); 400];
    tiles[6 * 20 + 5] = (2, 255, -1);
    let kind = |flags| TerrainKind {
        shown: true,
        flags,
        forest: false,
    };
    world.spatial.terrain = Some(Terrain::new(
        GridSize::new(20, 20),
        tiles
            .into_iter()
            .map(|(kind, restore, layer)| TileTerrain::of_save(kind, restore, layer))
            .collect(),
        vec![
            kind(TerrainFlags::NONE),
            kind(TerrainFlags::WATER),
            kind(TerrainFlags::WATER | TerrainFlags::LAND),
        ],
        Vec::new(),
    ));
    let graphic = |id| Graphic {
        id,
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    for id in [19598, 19599, 19600, 19601] {
        let id = SpriteId::named(id);
        let _ = Arc::make_mut(&mut world.model.graphics).insert(id, graphic(id));
    }
}

#[test]
fn longboat_takes_its_task_look_on_its_terrains_and_its_master_off_them() {
    let mut world = world();
    waters(&mut world);
    give_master(
        &mut world,
        master_id(250),
        MasterKind::COMBAT,
        class::WARSHIP,
        vec![longboat_look()],
    );
    put(
        &mut world,
        PlayerId::new(1),
        70,
        master_id(250),
        HitPoints::new(100.0),
        Point::new(5.5, 6.5),
    );
    let mut mover = walker();
    mover.moved_at = Position::new(5.5, 6.5, 0.0);
    world
        .parts
        .put_mover(world.players[1].objects[ListIndex::new(0)].id, Some(mover));
    run(&mut world, PlayerId::new(1), 70);
    let players = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(world.parts.mover_of(players.id).unwrap().factor, 0.35);
    let abilities = world.parts.combat_of(players.id).abilities;
    assert_eq!(abilities.attack_rate, 1.0);
    assert_eq!(
        abilities.look,
        Some(Look {
            standing: Some(SpriteId::named(19599)),
            walking: Some(SpriteId::named(19601)),
            attacking: Some(SpriteId::named(19599)),
            running: None,
            dying: Some(SpriteId::named(19598)),
            undead: Some(SpriteId::named(19600)),
        })
    );
    // where it last moved to decides, not its place
    world
        .parts
        .mover_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .moved_at = Position::new(5.5, 7.5, 0.0);
    run(&mut world, PlayerId::new(1), 70);
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(world.parts.mover_of(unit.id).unwrap().factor, 1.0);
    let abilities = world.parts.combat_of(unit.id).abilities;
    assert_eq!((abilities.attack_rate, abilities.look), (1.0, None));
}

#[test]
fn dead_object_that_leaves_issues_its_removal_rewards() {
    let mut world = world();
    let far = Task {
        work_range: Distance::new(1.0),
        ..bounty()
    };
    give_master(
        &mut world,
        master_id(5),
        MasterKind::COMBAT,
        class::CLASS_6,
        vec![bounty(), far, bounty()],
    );
    put(
        &mut world,
        PlayerId::new(1),
        80,
        master_id(5),
        HitPoints::ZERO,
        Point::new(5.5, 5.5),
    );
    world.players[1].attributes[7] = Amount::new(2.0);
    assert_eq!(rewards(&mut world, PlayerId::new(1), 80, false), Vec::new());
    let reward = Reward {
        player: PlayerId::new(1),
        attribute: Some(attribute::GOLD),
        amount: 20,
    };
    assert_eq!(
        rewards(&mut world, PlayerId::new(1), 80, true),
        vec![reward, reward]
    );
    // alive, it issues none
    world.players[1].objects[ListIndex::new(0)].life.hp = HitPoints::new(1.0);
    assert_eq!(rewards(&mut world, PlayerId::new(1), 80, true), Vec::new());
}

#[test]
fn an_object_of_another_kind_has_no_abilities() {
    let mut world = world();
    give_master(
        &mut world,
        ids::ID_7,
        MasterKind::MOVING,
        class::CLASS_6,
        vec![bounty()],
    );
    put(
        &mut world,
        PlayerId::new(1),
        90,
        ids::ID_7,
        HitPoints::ZERO,
        Point::new(5.5, 5.5),
    );
    assert_eq!(rewards(&mut world, PlayerId::new(1), 90, true), Vec::new());
}

/// `FUN_1416625b0`: an attribute below the player's count, none past it; the resources are the
/// first four attributes, and the construction rate attribute 195 (0 past the count).
#[test]
fn player_attribute_reads_its_place_and_none_past_the_count() {
    let mut player = Player {
        attributes: attributes(6, [1.0, 2.0, 3.0, 4.0]),
        ..Player::default()
    };
    player.attributes[5] = Amount::new(9.0);
    assert_eq!(player.attribute(attribute::STONE), Some(Amount::new(3.0)));
    assert_eq!(
        player.attribute(AttributeId::named(5)),
        Some(Amount::new(9.0))
    );
    assert_eq!(player.attribute(attribute::CURRENT_AGE), None);
    assert_eq!(player.resources(), amounts([1.0, 2.0, 3.0, 4.0]));
    assert_eq!(player.construction_rate(slots()), Amount::ZERO);
    player.attributes.resize(196, Amount::ZERO);
    player.attributes[attribute::CONSTRUCTION_RATE.index()] = Amount::new(1.5);
    assert_eq!(player.construction_rate(slots()), Amount::new(1.5));
    assert_eq!(Player::default().resources(), amounts([0.0; 4]));
}
