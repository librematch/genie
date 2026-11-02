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
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::ownership::CaptureKind;
use crate::players::Amount;
use crate::players::PlayerType;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::test_ids::class_id;
use crate::test_ids::costs;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::world::Millis;
use crate::world::WorldTime;
use std::sync::Arc;

fn task(action_type: ActionKind) -> Task {
    Task {
        action_kind: action_type,
        ..Task::default()
    }
}

/// The Feitoria's first task (1021, task 0): food from attribute 205 at 1.6 a second.
fn feitoria_food() -> Task {
    Task {
        object_group: None,
        attribute_type2: Some(AttributeId::numbered(205)),
        attribute_type3: Some(attribute::FOOD),
        work_val1: WorkValue::new(1.6),
        combat_level_flag: TaskFlags::from_bits_retain(0x1),
        ..task(GENERATE)
    }
}

/// The Keshik's task 5 (1228): gold from attribute 213 at 0.01 while it fights class 0.
fn keshik_gold() -> Task {
    Task {
        object_group: Some(class_id(0)),
        attribute_type2: Some(AttributeId::numbered(213)),
        attribute_type3: Some(attribute::GOLD),
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
fn task_with_its_type_flag_bit_generates_always() {
    // bit 1 for an object of type 70, bit 2 for one of type 80
    let mut t = feitoria_food();
    t.combat_level_flag = TaskFlags::UNITS_ALWAYS;
    assert!(generates(
        &t,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        None,
        false
    ));
    assert!(!generates(
        &t,
        MasterKind::BUILDING,
        MasterKind::BUILDING,
        None,
        false
    ));
    t.combat_level_flag = TaskFlags::BUILDINGS_ALWAYS;
    assert!(generates(
        &t,
        MasterKind::BUILDING,
        MasterKind::BUILDING,
        None,
        false
    ));
    assert!(!generates(
        &t,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        None,
        false
    ));
}

#[test]
fn building_generates_once_built() {
    let t = feitoria_food();
    assert!(!generates(
        &t,
        MasterKind::BUILDING,
        MasterKind::BUILDING,
        None,
        false
    ));
    assert!(generates(
        &t,
        MasterKind::BUILDING,
        MasterKind::BUILDING,
        None,
        true
    ));
    // a combat unit's master is not a building's
    assert!(!generates(
        &t,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        None,
        true
    ));
}

#[test]
fn unit_generates_while_its_top_action_works_on_target_of_the_task_class() {
    let t = keshik_gold();
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
            &t,
            MasterKind::COMBAT,
            MasterKind::COMBAT,
            top(kind, ActionState::new(7), Some(target)),
            false
        ));
    }
    assert!(!generates(
        &t,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        top(MOVE_TO, ActionState::new(7), Some(target)),
        false
    ));
    assert!(!generates(
        &t,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        top(ATTACK, ActionState::new(6), Some(target)),
        false
    ));
    assert!(!generates(
        &t,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        top(ATTACK, ActionState::new(7), None),
        false
    ));
    assert!(!generates(
        &t,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        None,
        false
    ));
    // another class
    let other = WorkTarget {
        class: class::CLASS_6,
        ..target
    };
    assert!(!generates(
        &t,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        top(ATTACK, ActionState::new(7), Some(other)),
        false
    ));
    // a dead target counts while its master stores some, never with a NaN
    let dead = WorkTarget {
        hp: HitPoints::ZERO,
        storage: Amount::new(1.0),
        ..target
    };
    assert!(generates(
        &t,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        top(ATTACK, ActionState::new(7), Some(dead)),
        false
    ));
    let empty = WorkTarget {
        hp: HitPoints::ZERO,
        storage: Amount::ZERO,
        ..target
    };
    assert!(!generates(
        &t,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        top(ATTACK, ActionState::new(7), Some(empty)),
        false
    ));
    let nan = WorkTarget {
        hp: HitPoints::new(f32::NAN),
        storage: Amount::new(f32::NAN),
        ..target
    };
    assert!(!generates(
        &t,
        MasterKind::COMBAT,
        MasterKind::COMBAT,
        top(ATTACK, ActionState::new(7), Some(nan)),
        false
    ));
}

#[test]
fn the_amount_is_dt_times_the_rate_times_the_attribute_in_that_order() {
    let t = feitoria_food();
    let dt = 0.044_f32;
    assert_eq!(
        generated(Seconds::new(dt), &t, Amount::new(1.0))
            .get()
            .to_bits(),
        (dt * 1.6 * 1.0).to_bits()
    );
    let m = 0.3_f32;
    assert_eq!(
        generated(Seconds::new(dt), &t, Amount::new(m))
            .get()
            .to_bits(),
        ((dt * 1.6) * m).to_bits()
    );
    assert_eq!(generated(Seconds::new(dt), &t, Amount::ZERO), Amount::ZERO);
}

#[test]
fn the_store_gives_what_reached_1_and_starts_again() {
    let a = Amount::new;
    let mut stores = vec![Amount::ZERO; 4];
    assert_eq!(store(a(0.6), attribute::FOOD, &mut stores), None);
    assert_eq!(stores[0], a(0.6));
    // ADDSS: the amount plus the store
    let sum = 0.5_f32 + 0.6;
    assert_eq!(store(a(0.5), attribute::FOOD, &mut stores), Some(a(sum)));
    assert_eq!(stores[0], Amount::ZERO);
    // nothing above 0 joins: the store stays, and nothing is given
    stores[1] = a(0.9);
    assert_eq!(store(Amount::ZERO, attribute::WOOD, &mut stores), None);
    assert_eq!(store(a(f32::NAN), attribute::WOOD, &mut stores), None);
    assert_eq!(store(a(-2.0), attribute::WOOD, &mut stores), None);
    assert_eq!(stores[1], a(0.9));
    // a NaN store gives nothing (JC)
    stores[2] = a(f32::NAN);
    assert_eq!(store(a(0.5), attribute::STONE, &mut stores), None);
    // a store the player does not have: no store and no give, however large
    assert_eq!(store(a(5.0), AttributeId::numbered(9), &mut stores), None);
}

#[test]
fn the_movement_damage_timer_runs_down_to_0_and_then_hits() {
    let mut t = Seconds::new(0.5);
    assert!(!movement_damage_due(&mut t, Seconds::new(0.2)));
    assert_eq!(t, Seconds::new(0.5) - Seconds::new(0.2));
    assert!(movement_damage_due(&mut t, Seconds::new(0.4)));
    assert_eq!(t, Seconds::ZERO);
    // at 0 it does not run down and hits
    assert!(movement_damage_due(&mut t, Seconds::new(0.4)));
    assert_eq!(t, Seconds::ZERO);
    // to exactly 0
    let mut t = Seconds::new(0.25);
    assert!(movement_damage_due(&mut t, Seconds::new(0.25)));
    assert_eq!(t, Seconds::ZERO);
    // below 0 (a timer the save holds so) hits as it is
    let mut t = Seconds::new(-1.0);
    assert!(movement_damage_due(&mut t, Seconds::new(0.1)));
    assert_eq!(t, Seconds::new(-1.0));
    // a NaN never hits and stays
    let mut t = Seconds::new(f32::NAN);
    assert!(!movement_damage_due(&mut t, Seconds::new(0.1)));
    assert!(t.get().is_nan());
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
    let h = hitter();
    let b = bystander();
    // the gap is 1.5 - 0.6 = 0.9: within 1 (0.81 at most 1), not within 0.5
    assert!(struck(&h, Distance::new(1.0), &b));
    assert!(!struck(&h, Distance::new(0.5), &b));
    assert!(struck(&h, Distance::new(f32::NAN), &b));
    assert!(!struck(
        &h,
        Distance::new(1.0),
        &Bystander {
            id: object_id(1),
            ..b
        }
    ));
    assert!(!struck(
        &h,
        Distance::new(1.0),
        &Bystander { allied: true, ..b }
    ));
    assert!(!struck(
        &h,
        Distance::new(1.0),
        &Bystander {
            player: PlayerId::new(0),
            ..b
        }
    ));
    assert!(!struck(
        &h,
        Distance::new(1.0),
        &Bystander {
            state: ObjectState::Dying,
            ..b
        }
    ));
    // buildings only of classes 51 and 54
    let building = Bystander {
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        ..b
    };
    assert!(!struck(&h, Distance::new(1.0), &building));
    assert!(struck(
        &h,
        Distance::new(1.0),
        &Bystander {
            class: class::CLASS_51,
            ..building
        }
    ));
    assert!(struck(
        &h,
        Distance::new(1.0),
        &Bystander {
            class: class::CLASS_54,
            ..building
        }
    ));
    // the blast levels
    let high = Hitter {
        blast_level: BlastLevel::from_bits_retain(3),
        ..h
    };
    assert!(!struck(&high, Distance::new(1.0), &b));
    assert!(struck(
        &high,
        Distance::new(1.0),
        &Bystander {
            blast_defense: BlastDefense::new(2),
            ..b
        }
    ));
    assert!(struck(
        &Hitter {
            blast_level: BlastLevel::from_bits_retain(1),
            ..h
        },
        Distance::new(1.0),
        &Bystander {
            blast_defense: BlastDefense::new(1),
            ..b
        }
    ));
}

#[test]
fn hit_below_1_hit_point_leaves_0_and_holds_nothing() {
    let mut v = Victim {
        hp: HitPoints::new(0.5),
        amount: Amount::new(30.0),
        capacity: CarryCapacity::of_data(100),
        ..Victim::default()
    };
    assert!(finish(&mut v));
    assert_eq!((v.hp, v.amount), (HitPoints::ZERO, Amount::ZERO));
    // a capacity below 0 is taken as it is
    let mut v = Victim {
        hp: HitPoints::new(-3.0),
        amount: Amount::new(30.0),
        capacity: CarryCapacity::of_data(-1),
        ..Victim::default()
    };
    assert!(finish(&mut v));
    assert_eq!((v.hp, v.amount), (HitPoints::ZERO, Amount::new(-1.0)));
    let mut v = Victim {
        hp: HitPoints::new(1.0),
        amount: Amount::new(30.0),
        ..Victim::default()
    };
    assert!(!finish(&mut v));
    assert_eq!(v.amount, Amount::new(30.0));
    let mut v = Victim {
        hp: HitPoints::new(f32::NAN),
        ..Victim::default()
    };
    assert!(!finish(&mut v));
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
    let t = longboat_look();
    let all = |_| true;
    assert_eq!(
        look(
            &t,
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
    assert_eq!(look(&t, TerrainFlags::LAND, TerrainFlags::WATER, all), None);
    // a sprite the table lacks is a null one
    let some = |id: i32| id != 19600 && id != 19599;
    let l = look(&t, TerrainFlags::LAND, TerrainFlags::LAND, some).unwrap();
    assert_eq!(
        (l.standing, l.undead, l.walking),
        (None, None, Some(SpriteId::named(19601)))
    );
    // an event id past the table's ids is none
    let t = Task {
        sound_events: SoundEvents::new(0x62d4_b3ec, -5),
        ..longboat_look()
    };
    let l = look(&t, TerrainFlags::LAND, TerrainFlags::LAND, all).unwrap();
    assert_eq!((l.dying, l.undead), (None, None));
}

/// A removal reward task: 10 gold (3) a removal, by attribute 7's rate.
fn bounty() -> Task {
    Task {
        attribute_type2: Some(attribute::RELICS_HELD),
        attribute_type3: Some(attribute::GOLD),
        work_val1: WorkValue::new(10.0),
        ..task(REMOVAL_REWARD)
    }
}

#[test]
fn removal_reward_pays_the_rate_times_the_attribute() {
    let t = bounty();
    assert_eq!(
        reward(&t, PlayerId::new(2), Some(Amount::new(1.5)), &[]),
        Reward {
            player: PlayerId::new(2),
            attribute: Some(attribute::GOLD),
            amount: 15
        }
    );
    // truncated
    assert_eq!(
        reward(&t, PlayerId::new(2), Some(Amount::new(0.25)), &[]).amount,
        2
    );
    // the player lacks the attribute, or the task names none
    assert_eq!(reward(&t, PlayerId::new(2), None, &[]).amount, 10);
    let none = Task {
        attribute_type2: None,
        ..bounty()
    };
    assert_eq!(
        reward(&none, PlayerId::new(2), Some(Amount::new(3.0)), &[]).amount,
        10
    );
    // a NaN rate is the integer indefinite
    assert_eq!(
        reward(&t, PlayerId::new(2), Some(Amount::new(f32::NAN)), &[]).amount,
        i32::MIN
    );
}

#[test]
fn removal_reward_with_its_flag_pays_the_last_paid_cost_of_its_resource_times_the_rate() {
    let t = Task {
        combat_level_flag: TaskFlags::from_bits_retain(0x1),
        ..bounty()
    };
    let cost = costs(&[
        (attribute::GOLD, 20, true),
        (attribute::FOOD, 50, true),
        (attribute::GOLD, 30, false),
    ]);
    assert_eq!(
        reward(&t, PlayerId::new(1), Some(Amount::new(0.5)), &cost).amount,
        100
    );
    let cost = costs(&[(attribute::GOLD, 20, true), (attribute::GOLD, 30, true)]);
    assert_eq!(
        reward(&t, PlayerId::new(1), Some(Amount::new(1.0)), &cost).amount,
        300
    );
    assert_eq!(
        reward(
            &t,
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
use crate::objects::masters::WorkValue;
use crate::objects::masters::{Master, Masters};
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::objects::{FacetCount, Frame};
use crate::players::relation_row;
use crate::test_ids::{amounts, attributes};
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
    let mut t = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO; 3],
        players,
    );
    t.dt_seconds = Seconds::new(0.5);
    t.spatial = Spatial::new(GridSize::new(20, 20));
    t.spatial.relations = t.players.iter().map(|p| p.relations.clone()).collect();
    t.model.masters = vec![
        Arc::new(Masters::default()),
        Arc::new(Masters::default()),
        Arc::new(Masters::default()),
    ];
    t
}

/// Master `id` of `kind` and `class` with `tasks`, given to every player.
fn give_master(t: &mut World, id: MasterId, kind: MasterKind, class: ClassId, tasks: Vec<Task>) {
    let m = Master {
        id,
        kind,
        class,
        tasks,
        radius: Footprint::new(0.5, 0.5),
        hit_points: MasterHitPoints::of_data(100),
        ..Master::default()
    };
    for ms in &mut t.model.masters {
        let mut all: Vec<Master> = ms.iter().cloned().collect();
        all.push(m.clone());
        *ms = Arc::new(Masters::new(all));
    }
}

/// Object `id` of master `m` with `hp` hit points at (`x`, `y`) in player `p`'s list, placed
/// on its tile.
fn put(t: &mut World, p: PlayerId, id: i32, m: MasterId, hp: HitPoints, at: Point) {
    let mut u = Unit {
        id: object_id(id),
        place: Static::at(at.with_height(0.0)),
        ..Unit::default()
    };
    u.life.master_id = m;
    u.life.hp = hp;
    u.life.state = ObjectState::Alive;
    let _ = t.push_live(p, u).unwrap();
    let radius = t.model.masters[p.index()]
        .get(m)
        .map_or(Footprint::new(0.5, 0.5), |m| m.radius);
    t.spatial.insert(
        object_id(id),
        Placed {
            position: at.with_height(0.0),
            radius,
            class: class::CLASS_0,
            player: p,
            master: m,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: false,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
}

/// The abilities of object `id` of player `p` as `p`'s, which issue no command.
fn run(t: &mut World, p: PlayerId, id: i32) {
    assert_eq!(rewards(t, p, id, false), Vec::new());
}

/// The abilities of object `id` of player `p` as `p`'s: the commands they issue.
fn rewards(t: &mut World, p: PlayerId, id: i32, leaves: bool) -> Vec<Reward> {
    let i = t.players[p.index()]
        .objects
        .position_of(object_id(id))
        .unwrap();
    t.abilities((object_id(id), p, i), p, leaves).unwrap()
}

/// The players' sight with `id` a building of player `p` that is `built`.
fn building_sight(t: &mut World, p: PlayerId, id: i32, built: bool) {
    let map = || VisibleMap {
        counts: vec![0; 400],
        explored: TileCount::ZERO,
    };
    let mut v = Visibility::new(
        GridSize::new(20, 20),
        vec![map(), map(), map()],
        vec![VisionBits::NONE; 400],
        Vec::new(),
        0,
        vec![PlayerType::HUMAN; 3],
    );
    v.add_seer(
        object_id(id),
        Seer {
            player: p,
            master: master_id(1021),
            los: Distance::new(6.0),
            state: ObjectState::Alive,
            explored: true,
            building: Some(BuildingSight {
                built,
                started: true,
            }),
            umv: 0,
            capture: CaptureKind::NONE,
            kind: MasterKind::BUILDING,
        },
    );
    t.spatial.sight = Some(v);
}

#[test]
fn built_feitoria_trickles_food_through_its_player_store() {
    let mut t = world();
    // its explore task (6) does nothing here
    give_master(
        &mut t,
        master_id(1021),
        MasterKind::BUILDING,
        class::BUILDING,
        vec![task(ActionKind::named(6)), feitoria_food()],
    );
    put(
        &mut t,
        PlayerId::new(1),
        50,
        master_id(1021),
        HitPoints::new(2000.0),
        Point::new(5.5, 5.5),
    );
    building_sight(&mut t, PlayerId::new(1), 50, true);
    t.players[1].attributes[205] = Amount::new(1.0);
    run(&mut t, PlayerId::new(1), 50);
    let made = 0.5_f32 * 1.6 * 1.0;
    assert_eq!(t.players[1].generated[0], Amount::new(made));
    assert_eq!(t.players[1].attributes[0], Amount::ZERO);
    run(&mut t, PlayerId::new(1), 50);
    let given = made + made;
    assert_eq!(t.players[1].attributes[0], Amount::new(given));
    assert_eq!(t.players[1].attributes[166], Amount::new(given));
    assert_eq!(t.players[1].generated[0], Amount::ZERO);
    // unbuilt, nothing
    building_sight(&mut t, PlayerId::new(1), 50, false);
    run(&mut t, PlayerId::new(1), 50);
    assert_eq!(t.players[1].generated[0], Amount::ZERO);
    // the player's attribute 205 at 0: nothing joins the store
    building_sight(&mut t, PlayerId::new(1), 50, true);
    t.players[1].attributes[205] = Amount::ZERO;
    run(&mut t, PlayerId::new(1), 50);
    assert_eq!(t.players[1].generated[0], Amount::ZERO);
}

/// A Keshik of player 1 attacking object 77 in state 7.
fn keshik(t: &mut World) {
    give_master(
        t,
        master_id(1228),
        MasterKind::COMBAT,
        class::CLASS_12,
        vec![keshik_gold()],
    );
    put(
        t,
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
    t.parts
        .actions
        .of_mut(t.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_attack(Some(attack));
    t.players[1].attributes[213] = Amount::new(0.5);
    t.players[1].generated[3] = Amount::new(0.999);
}

#[test]
fn keshik_makes_gold_while_it_fights_target_of_its_task_class() {
    let mut t = world();
    keshik(&mut t);
    give_master(
        &mut t,
        master_id(4),
        MasterKind::COMBAT,
        class::CLASS_0,
        Vec::new(),
    );
    put(
        &mut t,
        PlayerId::new(2),
        77,
        master_id(4),
        HitPoints::new(30.0),
        Point::new(6.5, 5.5),
    );
    run(&mut t, PlayerId::new(1), 60);
    let given = 0.5_f32 * 0.01 * 0.5 + 0.999;
    assert_eq!(t.players[1].attributes[3], Amount::new(given));
    assert_eq!(t.players[1].attributes[49], Amount::new(given));
    assert_eq!(t.players[1].attributes[169], Amount::new(given));
    assert_eq!(t.players[1].generated[3], Amount::ZERO);
    // in another state, nothing
    t.parts
        .actions
        .of_mut(t.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .attack_mut()
        .unwrap()
        .state = AttackState::Ready;
    run(&mut t, PlayerId::new(1), 60);
    assert_eq!(t.players[1].generated[3], Amount::ZERO);
}

/// A combat unit of master 9 at (5.5, 5.5) moving east, with a movement damage of range 2,
/// 5 hit points and a timer of 3 s.
fn trampler(t: &mut World) {
    let trample = Task {
        work_range: Distance::new(2.0),
        work_val1: WorkValue::new(5.0),
        work_val2: WorkValue::new(3.0),
        ..task(MOVEMENT_DAMAGE)
    };
    give_master(
        t,
        master_id(9),
        MasterKind::COMBAT,
        class::CLASS_12,
        vec![trample],
    );
    put(
        t,
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
    t.parts
        .put_mover(t.players[1].objects[ListIndex::new(0)].id, Some(mover));
}

#[test]
fn moving_unit_movement_damage_hits_the_enemies_about_it() {
    let mut t = world();
    trampler(&mut t);
    give_master(
        &mut t,
        master_id(4),
        MasterKind::COMBAT,
        class::CLASS_6,
        Vec::new(),
    );
    // an enemy in reach, one out of it, one of its own player, one of Gaia
    put(
        &mut t,
        PlayerId::new(2),
        20,
        master_id(4),
        HitPoints::new(30.0),
        Point::new(6.5, 5.5),
    );
    put(
        &mut t,
        PlayerId::new(2),
        21,
        master_id(4),
        HitPoints::new(30.0),
        Point::new(9.5, 5.5),
    );
    put(
        &mut t,
        PlayerId::new(1),
        30,
        master_id(4),
        HitPoints::new(30.0),
        Point::new(5.5, 6.5),
    );
    put(
        &mut t,
        PlayerId::new(0),
        40,
        master_id(4),
        HitPoints::new(30.0),
        Point::new(4.5, 5.5),
    );
    run(&mut t, PlayerId::new(1), 10);
    let hp = |t: &World, id: ObjectId| t.object(id).unwrap().1.life.hp;
    assert_eq!(hp(&t, object_id(20)), HitPoints::new(25.0));
    assert_eq!(hp(&t, object_id(21)), HitPoints::new(30.0));
    assert_eq!(hp(&t, object_id(30)), HitPoints::new(30.0));
    assert_eq!(hp(&t, object_id(40)), HitPoints::new(30.0));
    assert_eq!(
        t.parts
            .combat_of(t.players[1].objects[ListIndex::new(0)].id)
            .abilities
            .movement_damage_timer,
        Seconds::new(3.0)
    );
    // the timer runs down first: no hit for 3 s
    run(&mut t, PlayerId::new(1), 10);
    assert_eq!(hp(&t, object_id(20)), HitPoints::new(25.0));
    assert_eq!(
        t.parts
            .combat_of(t.players[1].objects[ListIndex::new(0)].id)
            .abilities
            .movement_damage_timer,
        Seconds::new(2.5)
    );
    // standing, no hit even when due
    t.parts
        .combat_mut(t.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .abilities
        .movement_damage_timer = Seconds::ZERO;
    t.parts
        .mover_mut(t.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .velocity = None;
    run(&mut t, PlayerId::new(1), 10);
    assert_eq!(hp(&t, object_id(20)), HitPoints::new(25.0));
}

#[test]
fn movement_damage_kill_leaves_0_hit_points_and_counts_the_kill() {
    let mut t = world();
    trampler(&mut t);
    give_master(
        &mut t,
        master_id(4),
        MasterKind::COMBAT,
        class::CLASS_6,
        Vec::new(),
    );
    put(
        &mut t,
        PlayerId::new(2),
        20,
        master_id(4),
        HitPoints::new(3.0),
        Point::new(6.5, 5.5),
    );
    t.players[2].objects[ListIndex::new(0)].life.amount = Amount::new(10.0);
    run(&mut t, PlayerId::new(1), 10);
    let (_, u) = t.object(object_id(20)).unwrap();
    assert_eq!((u.life.hp, u.life.amount), (HitPoints::ZERO, Amount::ZERO));
    // FUN_1412005b0: player 1 a kill (20) of player 2 (302), player 2 a loss (154)
    assert_eq!(t.players[1].attributes[20], Amount::new(1.0));
    assert_eq!(t.players[1].attributes[302], Amount::new(1.0));
    assert_eq!(t.players[2].attributes[154], Amount::new(1.0));
    assert_eq!(t.players[2].attributes[326], Amount::new(1.0));
}

/// The map's terrain: every tile of terrain 1 but (5, 6) of terrain 2, whose flag byte has
/// 32 (deep water).
fn waters(t: &mut World) {
    let mut tiles = vec![(1u8, 255u8, -1i16); 400];
    tiles[6 * 20 + 5] = (2, 255, -1);
    let kind = |flags| TerrainKind {
        shown: true,
        flags,
        forest: false,
    };
    t.spatial.terrain = Some(Terrain::new(
        GridSize::new(20, 20),
        tiles
            .into_iter()
            .map(|(k, r, l)| TileTerrain::of_save(k, r, l))
            .collect(),
        vec![
            kind(TerrainFlags::NONE),
            kind(TerrainFlags::WATER),
            kind(TerrainFlags::WATER | TerrainFlags::LAND),
        ],
        Vec::new(),
    ));
    let g = |id| Graphic {
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
        let _ = Arc::make_mut(&mut t.model.graphics).insert(id, g(id));
    }
}

#[test]
fn longboat_takes_its_task_look_on_its_terrains_and_its_master_off_them() {
    let mut t = world();
    waters(&mut t);
    give_master(
        &mut t,
        master_id(250),
        MasterKind::COMBAT,
        class::WARSHIP,
        vec![longboat_look()],
    );
    put(
        &mut t,
        PlayerId::new(1),
        70,
        master_id(250),
        HitPoints::new(100.0),
        Point::new(5.5, 6.5),
    );
    let mut mover = walker();
    mover.moved_at = Position::new(5.5, 6.5, 0.0);
    t.parts
        .put_mover(t.players[1].objects[ListIndex::new(0)].id, Some(mover));
    run(&mut t, PlayerId::new(1), 70);
    let u = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(t.parts.mover_of(u.id).unwrap().factor, 0.35);
    let abilities = t.parts.combat_of(u.id).abilities;
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
    t.parts
        .mover_mut(t.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .moved_at = Position::new(5.5, 7.5, 0.0);
    run(&mut t, PlayerId::new(1), 70);
    let u = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(t.parts.mover_of(u.id).unwrap().factor, 1.0);
    let abilities = t.parts.combat_of(u.id).abilities;
    assert_eq!((abilities.attack_rate, abilities.look), (1.0, None));
}

#[test]
fn dead_object_that_leaves_issues_its_removal_rewards() {
    let mut t = world();
    let far = Task {
        work_range: Distance::new(1.0),
        ..bounty()
    };
    give_master(
        &mut t,
        master_id(5),
        MasterKind::COMBAT,
        class::CLASS_6,
        vec![bounty(), far, bounty()],
    );
    put(
        &mut t,
        PlayerId::new(1),
        80,
        master_id(5),
        HitPoints::ZERO,
        Point::new(5.5, 5.5),
    );
    t.players[1].attributes[7] = Amount::new(2.0);
    assert_eq!(rewards(&mut t, PlayerId::new(1), 80, false), Vec::new());
    let r = Reward {
        player: PlayerId::new(1),
        attribute: Some(attribute::GOLD),
        amount: 20,
    };
    assert_eq!(rewards(&mut t, PlayerId::new(1), 80, true), vec![r, r]);
    // alive, it issues none
    t.players[1].objects[ListIndex::new(0)].life.hp = HitPoints::new(1.0);
    assert_eq!(rewards(&mut t, PlayerId::new(1), 80, true), Vec::new());
}

#[test]
fn an_object_of_another_type_has_no_abilities() {
    let mut t = world();
    give_master(
        &mut t,
        ids::ID_7,
        MasterKind::MOVING,
        class::CLASS_6,
        vec![bounty()],
    );
    put(
        &mut t,
        PlayerId::new(1),
        90,
        ids::ID_7,
        HitPoints::ZERO,
        Point::new(5.5, 5.5),
    );
    assert_eq!(rewards(&mut t, PlayerId::new(1), 90, true), Vec::new());
}

/// `FUN_1416625b0`: an attribute below the player's count, none past it; the resources are the
/// first four attributes, and the construction rate attribute 195 (0 past the count).
#[test]
fn player_attribute_reads_its_place_and_none_past_the_count() {
    let mut p = Player {
        attributes: attributes(6, [1.0, 2.0, 3.0, 4.0]),
        ..Player::default()
    };
    p.attributes[5] = Amount::new(9.0);
    assert_eq!(p.attribute(attribute::STONE), Some(Amount::new(3.0)));
    assert_eq!(
        p.attribute(AttributeId::numbered(5)),
        Some(Amount::new(9.0))
    );
    assert_eq!(p.attribute(attribute::CURRENT_AGE), None);
    assert_eq!(p.resources(), amounts([1.0, 2.0, 3.0, 4.0]));
    assert_eq!(p.construction_rate(), Amount::ZERO);
    p.attributes.resize(196, Amount::ZERO);
    p.attributes[attribute::CONSTRUCTION_RATE.index()] = Amount::new(1.5);
    assert_eq!(p.construction_rate(), Amount::new(1.5));
    assert_eq!(Player::default().resources(), amounts([0.0; 4]));
}
