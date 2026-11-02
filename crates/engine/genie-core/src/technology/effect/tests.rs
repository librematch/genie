use super::*;
use crate::combat::Charge;
use crate::construction::MainBuildingId;
use crate::garrison::ContainerId;
use crate::gathering::make::MakeState;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::layers::Building;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::ClassId;
use crate::objects::masters::CostAmount;
use crate::objects::masters::GarrisonCapacity;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::master_type;
use crate::objects::masters::{CostSlot, Master, Masters};
use crate::objects::unit::Unit;
use crate::ownership::CaptureKind;
use crate::players::Amount;
use crate::players::CivId;
use crate::players::PlayerId;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::production::LocationButton;
use crate::technology::CostPaid;
use crate::technology::EffectId;
use crate::technology::ResearchState;
use crate::technology::TechId;
use crate::technology::TechMode;
use crate::test_ids::costs;
use crate::test_ids::group_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::tech_id;
use crate::vision::SightTiles;
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::world::Millis;
use crate::world::Player;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::WorldTime;

/// A combat unit's charge after its master's maximum charge changes (`FUN_1412163c0`'s
/// `;`): a set and an add keep the charge's share of the maximum, without a maximum a set
/// takes the value and an add adds it, a multiply scales the charge.
#[test]
fn a_charge_keeps_its_share_of_a_changed_maximum() {
    assert_eq!(
        charge_after(Charge::new(10.0), Charge::new(20.0), Change::Set, 40.0),
        Charge::new(20.0)
    );
    assert_eq!(
        charge_after(Charge::new(10.0), Charge::ZERO, Change::Set, 40.0),
        Charge::new(40.0)
    );
    assert_eq!(
        charge_after(Charge::new(10.0), Charge::new(20.0), Change::Add, 20.0),
        Charge::new(20.0)
    );
    assert_eq!(
        charge_after(Charge::new(10.0), Charge::ZERO, Change::Add, 5.0),
        Charge::new(15.0)
    );
    assert_eq!(
        charge_after(Charge::new(10.0), Charge::new(20.0), Change::Multiply, 1.5),
        Charge::new(15.0)
    );
}

/// An object has actions while it holds a top action, its production list holds one (a
/// building's Make) or it is a missile; an empty queue or none at all is no action.
#[test]
fn an_object_has_actions_while_it_holds_one() {
    let u = Unit::default();
    assert!(!has_actions(&u, None, false));
    assert!(has_actions(&u, None, true));
    let mut production = crate::production::Production::default();
    assert!(!has_actions(&u, Some(&production), false));
    production.action = Some(crate::production::Making::Research(
        crate::production::MakeTech {
            state: crate::production::MakeTechState::Researching,
            paid: false,
            tech: tech_id(22),
            master: -1,
            progress: Seconds::ZERO,
        },
    ));
    assert!(has_actions(&u, Some(&production), false));
}

/// An Attack's slot 0x50 (`FUN_140a7f800`): its sprites take the master's unless they are
/// its task's (the walking sprite the task's moving one, the attack and standing sprites the
/// task's proceeding or working one), a sprite it lacks stays lacking, and only a combat
/// master's idle attack and second attack sprites replace its own.
#[test]
fn an_attack_takes_its_master_s_sprites_but_its_task_s() {
    let g = |id| crate::objects::sprite::Graphic {
        id,
        frames: crate::objects::Frame::of_data(10),
        facets: crate::objects::FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    };
    let arms = crate::combat::attack::Arms {
        move_sprite: Some(g(SpriteId::named(1))),
        sprite: Some(g(SpriteId::named(2))),
        second_sprite: Some(g(SpriteId::named(3))),
        idle_sprite: Some(g(SpriteId::named(4))),
        idle_attack_sprite: None,
        missile: None,
        fire_frame: crate::objects::Frame::of_data(0),
        interrupt_frame: crate::objects::Frame::of_data(-1),
    };
    let sprites = MasterSprites {
        moving: Some(g(SpriteId::named(11))),
        attack: Some(g(SpriteId::named(12))),
        standing: Some(g(SpriteId::named(14))),
        idle_attack: Some(g(SpriteId::named(15))),
        second: Some(g(SpriteId::named(13))),
        combat: false,
    };
    let mut a = arms;
    follow_master(&mut a, None, &sprites);
    assert_eq!(
        [
            a.move_sprite,
            a.sprite,
            a.idle_sprite,
            a.second_sprite,
            a.idle_attack_sprite
        ],
        [
            Some(g(SpriteId::named(11))),
            Some(g(SpriteId::named(12))),
            Some(g(SpriteId::named(14))),
            Some(g(SpriteId::named(3))),
            None
        ]
    );
    // the task's moving (1) and working (4, the standing sprite's) sprites stay
    let mut a = arms;
    follow_master(&mut a, Some(TaskSprites::of_data([1, 9, 4, 9])), &sprites);
    assert_eq!(
        [a.move_sprite, a.sprite, a.idle_sprite],
        [
            Some(g(SpriteId::named(1))),
            Some(g(SpriteId::named(12))),
            Some(g(SpriteId::named(4)))
        ]
    );
    // a sprite it lacks stays lacking; a combat master's last two replace its own
    let mut a = crate::combat::attack::Arms {
        move_sprite: None,
        ..arms
    };
    follow_master(
        &mut a,
        None,
        &MasterSprites {
            combat: true,
            ..sprites
        },
    );
    assert_eq!(
        [a.move_sprite, a.second_sprite, a.idle_attack_sprite],
        [
            None,
            Some(g(SpriteId::named(13))),
            Some(g(SpriteId::named(15)))
        ]
    );
}

/// `FUN_1412163c0`'s population set (110) over the master's first three storage entries: the
/// headroom (4) takes back the old amount's magnitude and adds the value, the population (11)
/// and resource 19 take back the old amount and add the value's magnitude; other kinds and a
/// fourth entry change nothing.
#[test]
fn a_population_set_moves_each_storage_entry_to_the_new_value() {
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            attributes: vec![Amount::ZERO; 40],
            ..Player::default()
        }],
    );
    let master = Master {
        id: ids::HOUSE,
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(-5.0), 0),
            (Some(POPULATION), Amount::new(3.0), 0),
            (Some(attribute::RELICS_HELD), Amount::new(1.0), 0),
            (Some(RESOURCE_19), Amount::new(2.0), 0),
        ],
        ..Master::default()
    };
    t.population_storage(PlayerId::new(0), &master, -8.0)
        .unwrap();
    let at = |a: AttributeId| t.players[0].attributes[a.index()];
    assert_eq!(
        [
            at(attribute::HEADROOM),
            at(POPULATION),
            at(attribute::RELICS_HELD),
            at(RESOURCE_19)
        ],
        crate::test_ids::amounts([-3.0, 5.0, 0.0, 0.0])
    );
}

/// `FUN_14118c540`: an object below state 7, not inside another, of a master with a height
/// takes its master's record again (kind and class not 0); one dying, inside or of a flat
/// master keeps none.
#[test]
fn an_object_takes_its_record_again_unless_dying_inside_or_flat() {
    let unit = |id, state: ObjectState, inside: Option<ObjectId>| {
        let mut u = Unit {
            id,
            body: crate::objects::layers::Body::at(Position::new(5.5, 5.5, 0.0)),
            ..Unit::default()
        };
        u.life.master_id = ids::HOUSE;
        u.life.state = state;
        u.life.inside = inside.map(ContainerId::new);
        u
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: vec![
                unit(object_id(1), ObjectState::Alive, None),
                unit(object_id(2), ObjectState::Removed, None),
                unit(object_id(3), ObjectState::Alive, Some(object_id(9))),
            ]
            .into(),
            ..Player::default()
        }],
    );
    t.spatial = crate::map::spatial::Spatial::new(GridSize::new(10, 10));
    for id in 1..=3 {
        t.spatial.insert(
            object_id(id),
            crate::map::spatial::Placed {
                position: Position::new(5.5, 5.5, 0.0),
                radius: Footprint::new(0.5, 0.5),
                class: class::CLASS_0,
                player: PlayerId::new(0),
                master: ids::HOUSE,
                obstruction_type: ObstructionType::TYPE_2,
                buildable_on: BuildableOn::NO,
                tall: true,
                moves: false,
                unknown: None,
                obstruction: None,
            },
        );
    }
    let master = Master {
        id: ids::HOUSE,
        height: Distance::new(2.0),
        obstruction_type: ObstructionType::TYPE_2,
        obstruction_class: ObstructionClass::UNIT,
        ..Master::default()
    };
    for id in 1..=3 {
        t.give_record(object_id(id), &master);
    }
    let record = |t: &World, id: ObjectId| t.spatial.get(id).and_then(|p| p.obstruction);
    let wall = Some(crate::map::spatial::Obstruction {
        kind: ObstructionType::TYPE_2,
        class: ObstructionClass::UNIT,
    });
    assert_eq!(
        [
            record(&t, object_id(1)),
            record(&t, object_id(2)),
            record(&t, object_id(3))
        ],
        [wall, None, None]
    );
    // a flat master gives none
    t.spatial.drop_record(object_id(1));
    t.give_record(
        object_id(1),
        &Master {
            height: Distance::new(0.0),
            ..master
        },
    );
    assert_eq!(record(&t, object_id(1)), None);
}

/// `FUN_1416136f0` (slot 0x1a8) of villagers (0x53) at player 1's town centre: each draws
/// its sex from the Player stream (above 49 a female, 0x125), the owner's slot 0x1c0 makes it
/// at the centre's place, it goes inside (the centre has room) and, attribute 283 at 0, comes
/// out at once toward no point (no gather point: it stays beside the centre). A master the
/// player lacks, Gaia lacking it too, spawns nothing.
#[test]
fn an_effect_spawns_its_villagers_at_each_built_town_centre() {
    use crate::garrison::tests::{TC, VILLAGER, let_out_world};
    let mut t = let_out_world();
    let standing = crate::objects::sprite::Graphic {
        id: SpriteId::named(300),
        frames: crate::objects::Frame::of_data(10),
        facets: crate::objects::FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let _ = t.graphics.insert(SpriteId::named(300), standing);
    for ms in &mut t.masters {
        ms.get_mut(VILLAGER).unwrap().standing_sprite = Some(SpriteId::named(300));
        let female = Master {
            id: master_id(0x125),
            ..ms.get(VILLAGER).unwrap().clone()
        };
        *ms = Masters::new(ms.iter().cloned().chain([female]));
        ms.get_mut(TC).unwrap().garrison = GarrisonCapacity::of_data(15);
    }
    t.players[1].attributes = vec![Amount::ZERO; 600];
    let before = t.players[1].objects.len();
    let mut stream = t.streams.get(Stream::Player).clone();
    t.spawn_at(PlayerId::new(1), (Some(VILLAGER), Some(TC)), 2)
        .unwrap();
    let made: Vec<(MasterId, Option<ContainerId>)> = t.players[1].objects.as_slice()[before..]
        .iter()
        .map(|u| (u.life.master_id, u.life.inside))
        .collect();
    let sex = |r: u32| {
        if 49 < r as i32 % 100 {
            master_id(0x125)
        } else {
            VILLAGER
        }
    };
    let expected = [sex(stream.rand15()), sex(stream.rand15())];
    assert_eq!(made, vec![(expected[0], None), (expected[1], None)]);
    assert_eq!(t.object(object_id(100)).unwrap().1.garrison, Vec::new());
    // a master neither the player nor Gaia has
    let mut t = let_out_world();
    let before = t.players[1].objects.len();
    t.spawn_at(PlayerId::new(1), (Some(master_id(999)), Some(TC)), 2)
        .unwrap();
    assert_eq!(t.players[1].objects.len(), before);
}

/// Loom's effect (22) on player 2 of MarvinBlue/426048731 in update 4588: villagers (class 4)
/// of 25 hit points take 15 more, one melee and two pierce armor.
#[test]
fn loom_adds_hit_points_to_its_villagers_and_their_master_and_armor_to_the_master() {
    let villager = |id, hp| {
        let mut u = Unit {
            id,
            ..Unit::default()
        };
        u.life.master_id = ids::VILLAGER_MALE;
        u.life.hp = hp;
        u
    };
    let player = Player {
        objects: vec![
            villager(object_id(9175), HitPoints::new(25.0)),
            villager(object_id(9177), HitPoints::new(10.0)),
        ]
        .into(),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    t.masters = vec![Masters::new([Master {
        id: ids::VILLAGER_MALE,
        kind: master_type::COMBAT,
        class: class::VILLAGER,
        hit_points: MasterHitPoints::of_data(25),
        armors: vec![ClassAmount::new(4, 0), ClassAmount::new(3, 0)],
        ..Master::default()
    }])];
    let loom = [
        EffectCommand::of_data(4, -1, 4, 0, 15.0).unwrap(),
        EffectCommand::of_data(4, -1, 4, 8, 1025.0).unwrap(),
        EffectCommand::of_data(4, -1, 4, 8, 770.0).unwrap(),
    ];
    // the load files the objects in the players' trackers
    t.load_trackers().unwrap();
    t.do_effect(PlayerId::new(0), &loom, None).unwrap();
    let hp: Vec<f32> = t.players[0]
        .objects
        .iter()
        .map(|u| u.life.hp.get())
        .collect();
    // (25 + 15) × 10 / 25 = 16
    assert_eq!(hp, [40.0, 16.0]);
    let m = t.masters[0].get(ids::VILLAGER_MALE).unwrap();
    assert_eq!(
        (m.hit_points, m.armors.clone()),
        (
            MasterHitPoints::of_data(40),
            vec![ClassAmount::new(4, 1), ClassAmount::new(3, 2)]
        )
    );
}

/// A unit of `master` with `hp` hit points.
/// A building's construction whose build points reached its build time: built, so the load
/// completes it in its player's tracker ([`World::load_trackers`]).
fn built() -> crate::construction::build::Construction {
    crate::construction::build::Construction {
        progress: Seconds::new(1.0),
        desolid: false,
        builders: crate::construction::BuilderCount::of_data(0),
        hit_points: MasterHitPoints::of_data(1),
        build_time: WholeSeconds::of_data(1),
        record: crate::map::spatial::Obstruction {
            kind: ObstructionType::NONE,
            class: ObstructionClass::NONE,
        },
        snow: false,
        sprite_override: false,
    }
}

/// A building layer holding a built construction ([`built`]).
fn built_layer() -> Building {
    Building {
        construction: Some(built()),
        ..Building::default()
    }
}

fn unit_of(id: i32, master: MasterId, hp: HitPoints) -> Unit {
    let mut u = Unit {
        id: object_id(id),
        ..Unit::default()
    };
    u.life.master_id = master;
    u.life.hp = hp;
    u.life.state = ObjectState::Alive;
    u
}

fn combat(id: MasterId, class: ClassId, hit_points: i16) -> Master {
    Master {
        id,
        kind: master_type::COMBAT,
        class,
        hit_points: MasterHitPoints::of_data(hit_points),
        ..Master::default()
    }
}

fn attribute(kind: u8, a: i16, b: i16, c: i16, d: f32) -> EffectCommand {
    EffectCommand::of_data(kind, a, b, c, d).unwrap()
}

/// `FUN_14165a400` (a master within the table, kind 0 with its id): only that master's
/// objects and the master change; a master id the table holds no master at (a null slot)
/// changes nothing, the class besides.
#[test]
fn an_effect_on_one_master_changes_its_objects_and_it_alone() {
    let player = Player {
        objects: vec![
            unit_of(1, master_id(4), HitPoints::new(30.0)),
            unit_of(2, master_id(5), HitPoints::new(30.0)),
        ]
        .into(),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    t.masters = vec![Masters::new([
        combat(master_id(4), class::CLASS_6, 60),
        combat(master_id(5), class::CLASS_6, 60),
        combat(ids::ID_7, class::CLASS_6, 60),
    ])];
    // set master 4's hit points to 90: its unit keeps its share, 30 × 90 / 60 = 45
    // the load files the objects in the players' trackers
    t.load_trackers().unwrap();
    t.do_effect(PlayerId::new(0), &[attribute(0, 4, 6, 0, 90.0)], None)
        .unwrap();
    let hp: Vec<f32> = t.players[0]
        .objects
        .iter()
        .map(|u| u.life.hp.get())
        .collect();
    assert_eq!(hp, [45.0, 30.0]);
    let hps: Vec<i16> = t.masters[0].iter().map(|m| m.hit_points.get()).collect();
    assert_eq!(hps, [90, 60, 60]);
    // master 6 lies within the table (up to 7) but has no master: nothing, not the class
    t.do_effect(PlayerId::new(0), &[attribute(4, 6, 6, 0, 10.0)], None)
        .unwrap();
    let hps: Vec<i16> = t.masters[0].iter().map(|m| m.hit_points.get()).collect();
    assert_eq!(hps, [90, 60, 60]);
    // past the table the class changes: (60 + 10) × 30 / 60 = 35
    t.do_effect(PlayerId::new(0), &[attribute(4, 100, 6, 0, 10.0)], None)
        .unwrap();
    let hps: Vec<i16> = t.masters[0].iter().map(|m| m.hit_points.get()).collect();
    assert_eq!(hps, [100, 70, 70]);
    assert_eq!(
        t.players[0].objects[ListIndex::new(1)].life.hp,
        HitPoints::new(35.0)
    );
}

/// `FUN_141614f40`'s hooks: a building's volleys lose the shares of the combat units inside
/// before the change and take the new shares after (`FUN_141217ac0`, `FUN_14120b910`); the
/// change of the archers' attack moves an archer's share.
#[test]
fn a_change_moves_the_shares_of_the_units_inside_a_building() {
    let mut tower = unit_of(10, master_id(79), HitPoints::new(1000.0));
    tower.garrison = vec![object_id(11)];
    tower.combat.firepower = crate::combat::Firepower::new(2.0);
    let mut archer = unit_of(11, master_id(4), HitPoints::new(30.0));
    archer.life.inside = Some(ContainerId::new(object_id(10)));
    let player = Player {
        objects: vec![tower, archer].into(),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    let mut tower_master = combat(master_id(79), class::TOWER, 1020);
    tower_master.kind = master_type::BUILDING;
    tower_master.reload = Seconds::new(2.0);
    tower_master.weapon_range = Distance::new(8.0);
    tower_master.weapons = vec![ClassAmount::new(3, 5)];
    let mut archer_master = combat(master_id(4), class::CLASS_0, 30);
    archer_master.reload = Seconds::new(2.0);
    archer_master.weapons = vec![ClassAmount::new(3, 4)];
    archer_master.garrison_firepower = crate::combat::Firepower::new(1.0);
    t.masters = vec![Masters::new([archer_master, tower_master])];
    // the tower's rate: 5 / 2 = 2.5; the archer's share 4 / 2 / 2.5 = 0.8, then 6 / 2 / 2.5
    t.do_effect(
        PlayerId::new(0),
        &[attribute(4, -1, 0, 9, f32::from(0x302_i16))],
        None,
    )
    .unwrap();
    assert_eq!(
        t.masters[0].get(master_id(4)).unwrap().weapons,
        [ClassAmount::new(3, 6)]
    );
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)].combat.firepower,
        crate::combat::Firepower::new(2.0 - 0.8 + 1.2)
    );
}

/// The class path (`FUN_14165a000`): a building of the class takes the change, then its
/// annexes of the class, which the tracker files under no class; a foundation of the class
/// (pending) takes it too.
#[test]
fn a_class_effect_reaches_buildings_their_annexes_and_foundations() {
    let mut main = unit_of(20, master_id(30), HitPoints::new(100.0));
    main.life.master_id = master_id(30);
    let main_layer = Building {
        construction: Some(built()),
        annexes: crate::construction::Annexes::new([Some(object_id(21)), None, None, None]),
        ..Building::default()
    };
    let annex = unit_of(21, master_id(31), HitPoints::new(50.0));
    let annex_layer = Building {
        annex_of: Some(MainBuildingId::new(object_id(20))),
        construction: Some(built()),
        ..Building::default()
    };
    let mut foundation = unit_of(22, master_id(30), HitPoints::new(10.0));
    foundation.life.state = ObjectState::Foundation;
    let player = Player {
        objects: vec![main, annex, foundation].into(),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    )
    .with_buildings([(object_id(20), main_layer), (object_id(21), annex_layer)]);
    let building = |id: i16, hit_points, head_unit: i16, annexes| Master {
        id: master_id(id),
        kind: master_type::BUILDING,
        class: class::BUILDING,
        hit_points,
        building: Some(crate::objects::masters::BuildingMaster {
            stack_unit: None,
            head_unit: crate::test_ids::opt_master(head_unit),
            annexes,
            ..crate::objects::masters::BuildingMaster::default()
        }),
        ..Master::default()
    };
    t.masters = vec![Masters::new([
        building(
            30,
            MasterHitPoints::of_data(200),
            -1,
            [Some(master_id(31)), None, None, None],
        ),
        building(31, MasterHitPoints::of_data(100), 30, [None; 4]),
    ])];
    // multiply the class's hit points by 1.5: every object's hit points × 1.5 + 0.5
    // the load files the objects in the players' trackers
    t.load_trackers().unwrap();
    t.do_effect(PlayerId::new(0), &[attribute(5, -1, 3, 0, 1.5)], None)
        .unwrap();
    let hp: Vec<f32> = t.players[0]
        .objects
        .iter()
        .map(|u| u.life.hp.get())
        .collect();
    assert_eq!(hp, [150.0, 75.0, 15.0]);
    let hps: Vec<i16> = t.masters[0].iter().map(|m| m.hit_points.get()).collect();
    assert_eq!(hps, [300, 150]);
}

/// The allies' block (14: add) reaches the player's mutual allies, itself among them, and
/// Gaia's (44) Gaia alone.
#[test]
fn a_team_effect_reaches_the_mutual_allies() {
    let player = |relations: Vec<u8>| Player {
        relations: crate::players::relation_row(relations),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO; 4],
        vec![
            player(vec![0, 3, 3, 3]),
            player(vec![3, 0, 0, 3]),
            player(vec![3, 0, 0, 3]),
            player(vec![3, 3, 3, 0]),
        ],
    );
    t.masters = vec![Masters::new([combat(master_id(4), class::CLASS_6, 60)]); 4];
    t.do_effect(PlayerId::new(1), &[attribute(14, -1, 6, 0, 5.0)], None)
        .unwrap();
    let hps: Vec<i16> = t
        .masters
        .iter()
        .map(|m| m.get(master_id(4)).unwrap().hit_points.get())
        .collect();
    assert_eq!(hps, [60, 65, 65, 60]);
    t.do_effect(PlayerId::new(1), &[attribute(44, -1, 6, 0, 5.0)], None)
        .unwrap();
    let hps: Vec<i16> = t
        .masters
        .iter()
        .map(|m| m.get(master_id(4)).unwrap().hit_points.get())
        .collect();
    assert_eq!(hps, [65, 65, 65, 60]);
    // the enemies' block (24): player 3 counts player 1 an enemy and is counted one
    t.do_effect(PlayerId::new(1), &[attribute(24, -1, 6, 0, 5.0)], None)
        .unwrap();
    let hps: Vec<i16> = t
        .masters
        .iter()
        .map(|m| m.get(master_id(4)).unwrap().hit_points.get())
        .collect();
    assert_eq!(hps, [65, 65, 65, 65]);
}

/// A set of a resource storage (`FUN_141195a50`'s 21): each object's player loses the old
/// amount and gains the new (slot 0xf8 twice), unless the master keeps it (`'@'`).
#[test]
fn a_storage_set_moves_the_player_s_attribute_by_each_object() {
    let player = Player {
        objects: vec![
            unit_of(1, ids::HOUSE, HitPoints::new(550.0)),
            unit_of(2, ids::HOUSE, HitPoints::new(550.0)),
        ]
        .into(),
        attributes: vec![Amount::ZERO; 40],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    let mut house = combat(ids::HOUSE, class::BUILDING, 550);
    house.storage = vec![
        (Some(attribute::HEADROOM), Amount::new(5.0), 4),
        (None, Amount::ZERO, 0),
        (None, Amount::ZERO, 0),
    ];
    t.masters = vec![Masters::new([house])];
    t.players[0].attributes[4] = Amount::new(10.0);
    // the load files the objects in the players' trackers
    t.load_trackers().unwrap();
    t.do_effect(PlayerId::new(0), &[attribute(0, 70, -1, 21, 15.0)], None)
        .unwrap();
    assert_eq!(t.players[0].attributes[4], Amount::new(30.0));
    assert_eq!(
        t.masters[0].get(ids::HOUSE).unwrap().storage[0].1,
        Amount::new(15.0)
    );
}

/// `FUN_141195a50`'s line of sight on a static object (kind 10) on the map: its old circle
/// (the seer's own, 2) goes and the new one is explored (a set: the value; an add: the
/// master's plus it; a multiply: the master's times it, truncated); off the map (state 0)
/// nothing. A set of the second storage (26) moves its attribute as the first does.
#[test]
fn a_static_object_s_line_of_sight_and_second_storage_take_the_change() {
    let (w, h) = (20, 20);
    let n = (w * h) as usize;
    let sight = |t: &World| {
        let map = &t.spatial.sight.as_ref().unwrap().maps[0];
        map.counts.iter().filter(|&&c| c > 0).count()
    };
    let world = || {
        let player = Player {
            objects: vec![unit_of(1, ids::FARM, HitPoints::new(10.0))].into(),
            attributes: vec![Amount::ZERO; 40],
            ..Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(0),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![player],
        );
        let stone = Master {
            id: ids::FARM,
            kind: master_type::STATIC,
            class: class::CLASS_11,
            hit_points: MasterHitPoints::of_data(10),
            line_of_sight: Distance::new(2.0),
            storage: vec![
                (None, Amount::ZERO, 0),
                (Some(attribute::HEADROOM), Amount::new(5.0), 0),
                (None, Amount::ZERO, 0),
            ],
            ..Master::default()
        };
        t.masters = vec![Masters::new([stone])];
        t.load_trackers().unwrap();
        let mut v = crate::vision::visibility::Visibility::new(
            GridSize::new(w, h),
            vec![crate::vision::visibility::VisibleMap {
                counts: vec![0; n],
                explored: TileCount::ZERO,
            }],
            vec![VisionBits::NONE; n],
            Vec::new(),
            0,
            vec![false],
        );
        let seer = crate::vision::visibility::Seer {
            player: PlayerId::new(0),
            master: ids::FARM,
            los: Distance::new(2.0),
            state: ObjectState::Alive,
            explored: true,
            building: None,
            umv: 0,
            capture: CaptureKind::NONE,
            kind: master_type::STATIC,
        };
        v.add_seer(object_id(1), seer);
        let position = Position::new(10.5, 10.5, 0.0);
        let radius = Footprint::new(0.5, 0.5);
        let _ = v.sight_on(object_id(1), position, radius, false, false);
        t.spatial.sight = Some(v);
        t.spatial.insert(
            object_id(1),
            crate::map::spatial::Placed {
                position,
                radius,
                class: class::CLASS_11,
                player: PlayerId::new(0),
                master: ids::FARM,
                obstruction_type: ObstructionType::NONE,
                buildable_on: BuildableOn::NO,
                tall: false,
                moves: false,
                unknown: None,
                obstruction: None,
            },
        );
        t
    };
    // the tiles a circle of `los` explores, on a map of its own
    let circle = |los| {
        let mut t = world();
        let v = t.spatial.sight.as_mut().unwrap();
        v.sight_off(
            object_id(1),
            Position::new(10.5, 10.5, 0.0),
            Footprint::new(0.5, 0.5),
            false,
            false,
        )
        .unwrap();
        let _ = v.sight_on_los(
            object_id(1),
            Position::new(10.5, 10.5, 0.0),
            Footprint::new(0.5, 0.5),
            false,
            SightTiles::new(los),
            false,
        );
        sight(&t)
    };
    let (two, four, five, six) = (circle(2), circle(4), circle(5), circle(6));
    assert!(two < four && four < five && five < six);
    let mut t = world();
    assert_eq!(sight(&t), two);
    // a set to 4.9: truncated to 4
    t.do_effect(PlayerId::new(0), &[attribute(0, -1, 11, 1, 4.9)], None)
        .unwrap();
    assert_eq!(sight(&t), four);
    // an add of 3: the master's 2 (the clone before its own change) plus 3
    let mut t = world();
    t.do_effect(PlayerId::new(0), &[attribute(4, -1, 11, 1, 3.0)], None)
        .unwrap();
    assert_eq!(sight(&t), five);
    // a multiply by 3: the master's 2 times 3
    let mut t = world();
    t.do_effect(PlayerId::new(0), &[attribute(5, -1, 11, 1, 3.0)], None)
        .unwrap();
    assert_eq!(sight(&t), six);
    // off the map: the circle stays
    let mut t = world();
    t.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Foundation;
    t.do_effect(PlayerId::new(0), &[attribute(0, -1, 11, 1, 6.0)], None)
        .unwrap();
    assert_eq!(sight(&t), two);
    // the second storage: its attribute 4 loses the old 5 and takes the new 8
    let mut t = world();
    t.players[0].attributes[4] = Amount::new(10.0);
    t.do_effect(PlayerId::new(0), &[attribute(0, -1, 11, 26, 8.0)], None)
        .unwrap();
    assert_eq!(t.players[0].attributes[4], Amount::new(13.0));
}

/// A change of the master's speed reaches its moving objects' base (`+0x740`), and their top
/// speed is the new base times the factor of the sprite each shows (slot 65): a unit showing a
/// sprite of factor 2 moves at twice the new speed, one whose sprite the slice does not hold
/// at the master's running speed.
#[test]
fn a_master_s_new_speed_reaches_its_units_top_speed_by_their_sprites() {
    use crate::objects::masters::attribute as a;
    let fast = crate::objects::sprite::Graphic {
        id: SpriteId::named(7),
        frames: crate::objects::Frame::of_data(1),
        facets: crate::objects::FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::of_data(2.0),
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    };
    let moving = |id| {
        let mut u = unit_of(id, master_id(4), HitPoints::new(30.0));
        u.body = crate::objects::layers::Body::new(
            Position::new(5.5, 5.5, 0.0),
            None,
            Some(crate::gathering::hunt::tests::walker()),
        );
        u
    };
    let mut shows = moving(1);
    shows.sprite = Some(crate::objects::sprite::ActiveSprite::start(
        fast,
        &mut crate::math::rng::Well512::default(),
    ));
    let player = Player {
        objects: vec![shows, moving(2)].into(),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    let mut m = combat(master_id(4), class::CLASS_6, 60);
    (m.max_speed, m.move_sprite, m.run_sprite, m.standing_sprite) =
        (Speed::new(1.5), None, None, None);
    t.masters = vec![Masters::new([m])];
    t.master_followers(PlayerId::new(0), master_id(4), a::MOVEMENT_SPEED)
        .unwrap();
    let speeds = |u: &Unit| u.body.mover().map(|m| (m.stats.base, m.stats.speed));
    assert_eq!(
        speeds(&t.players[0].objects[ListIndex::new(0)]),
        Some((Speed::new(1.5), Speed::new(3.0)))
    );
    assert_eq!(
        speeds(&t.players[0].objects[ListIndex::new(1)]),
        Some((Speed::new(1.5), Speed::new(1.5)))
    );
}

/// What the slice keeps beside a master follows its change: its objects' (live and asleep)
/// dying sprite (none below 0), death spawn (a first spawn of 0 or more) and remains (the
/// second), a foundation's build time and record, a placed object's height test and a
/// seer's line of sight; another master's objects and a master the player lacks keep theirs.
#[test]
fn an_object_s_copies_of_its_master_follow_its_change() {
    use crate::objects::masters::attribute as a;
    let site = unit_of(1, master_id(4), HitPoints::new(30.0));
    let player = Player {
        objects: vec![site, unit_of(3, master_id(5), HitPoints::new(30.0))].into(),
        sleeping: vec![unit_of(2, master_id(4), HitPoints::new(30.0))].into(),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    )
    .with_buildings([(object_id(1), built_layer())]);
    let mut m = combat(master_id(4), class::CLASS_6, 60);
    (m.death_sprite, m.death_spawns) = (
        Some(SpriteId::named(7)),
        [Some(master_id(0)), Some(master_id(9))],
    );
    (m.build_time, m.obstruction_type, m.obstruction_class) = (
        WholeSeconds::of_data(33),
        ObstructionType::CIRCLE,
        ObstructionClass::UNIT,
    );
    (m.line_of_sight, m.tall) = (Distance::new(6.0), true);
    t.masters = vec![Masters::new([m, combat(master_id(5), class::CLASS_6, 60)])];
    for id in [1, 3] {
        t.spatial.insert(
            object_id(id),
            crate::map::spatial::Placed {
                position: Position::new(5.5, 5.5, 0.0),
                radius: Footprint::new(0.5, 0.5),
                class: class::CLASS_6,
                player: PlayerId::new(0),
                master: master_id(4),
                obstruction_type: ObstructionType::NONE,
                buildable_on: BuildableOn::NO,
                tall: false,
                moves: false,
                unknown: None,
                obstruction: None,
            },
        );
    }
    let mut v = crate::vision::visibility::Visibility::new(
        GridSize::new(10, 10),
        Vec::new(),
        vec![VisionBits::NONE; 100],
        Vec::new(),
        0,
        Vec::new(),
    );
    let seer = crate::vision::visibility::Seer {
        player: PlayerId::new(0),
        master: master_id(4),
        los: Distance::new(2.0),
        state: ObjectState::Alive,
        explored: true,
        building: None,
        umv: 0,
        capture: CaptureKind::NONE,
        kind: master_type::COMBAT,
    };
    v.add_seer(object_id(1), seer);
    v.add_seer(object_id(3), seer);
    t.spatial.sight = Some(v);
    let all = [
        a::DYING_GRAPHIC,
        a::DEAD_UNIT_ID,
        a::BLOOD_UNIT,
        a::TRAIN_TIME,
        a::OBSTRUCTION_TYPE,
        a::SIZE_Z,
        a::LINE_OF_SIGHT,
    ];
    for attribute in all {
        t.master_followers(PlayerId::new(0), master_id(4), attribute)
            .unwrap();
        t.master_followers(PlayerId::new(0), master_id(99), attribute)
            .unwrap();
    }
    let follows = |u: &Unit| (u.life.dying_sprite, u.life.death_spawn);
    let p = &t.players[0];
    assert_eq!(
        follows(&p.objects[ListIndex::new(0)]),
        (Some(SpriteId::named(7)), true)
    );
    assert_eq!(
        follows(&p.sleeping[ListIndex::new(0)]),
        (Some(SpriteId::named(7)), true)
    );
    assert_eq!(follows(&p.objects[ListIndex::new(1)]), (None, false));
    let c = t
        .parts
        .building_of(p.objects[ListIndex::new(0)].id)
        .construction
        .unwrap();
    assert_eq!(c.build_time, WholeSeconds::of_data(33));
    assert_eq!(
        c.record,
        crate::map::spatial::Obstruction {
            kind: ObstructionType::CIRCLE,
            class: ObstructionClass::UNIT
        }
    );
    let tall = |id: ObjectId| t.spatial.get(id).unwrap().tall;
    assert_eq!((tall(object_id(1)), tall(object_id(3))), (true, false));
    let los = |id: ObjectId| t.spatial.sight.as_ref().unwrap().seer(id).unwrap().los;
    assert_eq!(
        (los(object_id(1)), los(object_id(3))),
        (Distance::new(6.0), Distance::new(2.0))
    );
    // a dying sprite below 0 is none
    t.masters[0].get_mut(master_id(4)).unwrap().death_sprite = None;
    t.master_followers(PlayerId::new(0), master_id(4), a::DYING_GRAPHIC)
        .unwrap();
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)].life.dying_sprite,
        None
    );
}

/// A speed's change reaches what a unit of the master moves with (the game reads its master).
#[test]
fn a_speed_change_reaches_the_units_movement() {
    let mut walker = unit_of(1, master_id(4), HitPoints::new(25.0));
    let mover = crate::gathering::hunt::tests::walker();
    let stats = mover.stats;
    walker.body.put_mover(Some(mover));
    let player = Player {
        objects: vec![walker].into(),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    let mut villager = combat(master_id(4), class::VILLAGER, 25);
    villager.max_speed = Speed::new(0.8);
    villager.moving = Some(stats);
    t.masters = vec![Masters::new([villager])];
    t.do_effect(PlayerId::new(0), &[attribute(5, -1, 4, 5, 1.1)], None)
        .unwrap();
    let s = t.players[0].objects[ListIndex::new(0)]
        .body
        .mover()
        .unwrap()
        .stats;
    // no graphics: the sprites' factors are 0, the speeds the master's
    let speed = 0.8_f32 * 1.1;
    assert_eq!(s.speed, Speed::new(speed));
    // a speed the loader left at 0 under the moving values stops the change first
    t.masters[0].get_mut(master_id(4)).unwrap().max_speed = Speed::ZERO;
    assert!(
        t.do_effect(PlayerId::new(0), &[attribute(5, -1, 4, 5, 1.1)], None)
            .is_err()
    );
    let stats = t.players[0].objects[ListIndex::new(0)]
        .body
        .mover()
        .unwrap()
        .stats;
    assert_eq!(stats.speed, Speed::new(speed));
}

/// The resource commands (kind 1, `FUN_141620c70`): mode 0 sets the attribute, any other adds
/// to it, scaled by the attribute the third argument names; kind 6 multiplies it
/// (`FUN_1416614c0`).
#[test]
fn the_resource_commands_set_add_and_multiply_an_attribute() {
    let player = Player {
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    t.players[0].attributes[69] = Amount::new(2.0);
    t.do_effect(PlayerId::new(0), &[attribute(1, 78, 0, -1, 0.15)], None)
        .unwrap();
    // Horse Collar: 75 × attribute 69 added to 36
    t.do_effect(PlayerId::new(0), &[attribute(1, 36, 1, 69, 75.0)], None)
        .unwrap();
    t.do_effect(PlayerId::new(0), &[attribute(6, 78, -1, -1, 2.0)], None)
        .unwrap();
    // gold, a resource, lives beside the attributes
    t.do_effect(PlayerId::new(0), &[attribute(1, 3, 1, -1, 50.0)], None)
        .unwrap();
    let a = &t.players[0].attributes;
    assert_eq!(
        (a[78], a[36], t.players[0].attributes[3]),
        (Amount::new(0.3), Amount::new(150.0), Amount::new(50.0))
    );
}

/// A change of the farms' food (resource 0x24) grows the food each farm still makes:
/// `(f / before)^exponent × amount + f − held` with `f = held + still to make`
/// (`FUN_141615270`), the exponent attribute 31.
#[test]
fn a_farm_food_change_grows_the_farms_food() {
    let mut farm = unit_of(5, ids::FARM, HitPoints::new(480.0));
    farm.life.amount = Amount::new(75.0);
    farm.actions
        .list
        .set_make(Some(crate::gathering::make::MakeAction {
            state: MakeState::Done,
            remaining: Amount::new(100.0),
            total: Amount::new(175.0),
            task: crate::objects::masters::Task::default(),
        }));
    let player = Player {
        objects: vec![farm].into(),
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    )
    .with_buildings([(object_id(5), built_layer())]);
    let mut master = combat(ids::FARM, class::FARM, 480);
    master.kind = master_type::BUILDING;
    t.masters = vec![Masters::new([master])];
    t.players[0].attributes[31] = Amount::new(1.0);
    t.players[0].attributes[36] = Amount::new(175.0);
    // the load files the objects in the players' trackers
    t.load_trackers().unwrap();
    t.do_effect(PlayerId::new(0), &[attribute(1, 36, 1, -1, 75.0)], None)
        .unwrap();
    assert_eq!(t.players[0].attributes[36], Amount::new(250.0));
    // (175 / 175)^1 × 75 + 175 − 75
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)]
            .actions
            .list
            .make()
            .copied()
            .unwrap()
            .remaining,
        Amount::new(175.0)
    );
}

/// Cartography (resource 50, added): the allied vision turns on and the masks are rebuilt
/// before the attribute takes its sum (`FUN_141616820`); Treason's set (0xb7) rebuilds them
/// after; resource 0xc4 spreads to the players at most 0 unless the player's is above 0; the
/// scripts' effect function (0x21 above 0) is not modelled.
#[test]
fn the_resource_commands_side_effects() {
    let player = |relations: Vec<u8>| Player {
        relations: crate::players::relation_row(relations),
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO; 3],
        vec![
            player(vec![0, 3, 3]),
            player(vec![3, 0, 0]),
            player(vec![3, 0, 0]),
        ],
    );
    t.spatial.vision =
        crate::players::ByPlayer::new(vec![crate::vision::VisionMasks::default(); 3]);
    t.do_effect(PlayerId::new(1), &[attribute(1, 50, 1, -1, 1.0)], None)
        .unwrap();
    assert!(t.players[1].allied_vision);
    assert_eq!(t.players[1].attributes[50], Amount::new(1.0));
    assert!(
        t.spatial
            .vision
            .get(PlayerId::new(1))
            .is_some_and(|m| m.visible.bits() & 0b110 == 0b110)
    );
    t.do_effect(PlayerId::new(2), &[attribute(1, 0xc4, 0, -1, 7.0)], None)
        .unwrap();
    let spread: Vec<f32> = t.players.iter().map(|p| p.attributes[0xc4].get()).collect();
    assert_eq!(spread, [0.0, 7.0, 7.0]);
    t.do_effect(PlayerId::new(1), &[attribute(1, 0xc4, 0, -1, 9.0)], None)
        .unwrap();
    assert_eq!(t.players[1].attributes[0xc4], Amount::new(7.0));
    t.do_effect(PlayerId::new(1), &[attribute(1, 0xb7, 0, -1, 1.0)], None)
        .unwrap();
    assert!(
        t.spatial
            .vision
            .get(PlayerId::new(1))
            .is_some_and(|m| m.visible.bits() & 0b110 == 0b110)
    );
    assert!(
        t.do_effect(PlayerId::new(1), &[attribute(1, 0x21, 0, -1, 7.0)], None)
            .is_err()
    );
}

/// A technology at the barracks (master 12, 30 seconds in the `.dat`) in state 1 with 20
/// seconds of work done.
fn barracks_tech() -> World {
    use crate::technology::{Location, TechDef, TechState};
    let state = TechState {
        state: ResearchState::Available,
        research_done: Seconds::new(20.0),
        locations: vec![Location {
            building: Some(master_id(12)),
            time: WholeSeconds::of_data(30),
            button: LocationButton::of_data(3),
            adjustment: Seconds::ZERO,
        }],
        building_master: None,
        ..TechState::default()
    };
    let player = Player {
        techs: vec![state],
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    t.techs = vec![TechDef {
        required: [TechId::of_data(-1); 6],
        locations: vec![(MasterId::of_data(12), WholeSeconds::of_data(30))],
        ..TechDef::default()
    }];
    t
}

/// `FUN_1413d1210`: mode 0 sets the time (the adjustment is the value less the `.dat`'s
/// time), mode 1 adds the truncated value, mode 2 adds `(v − 1) × time` rounded half away
/// from 1; once the work done reaches the new time a technology in state 1 completes.
#[test]
fn the_research_time_modifier_changes_the_adjustment_and_may_complete() {
    let mut t = barracks_tech();
    t.do_effect(PlayerId::new(0), &[attribute(b'g', 0, -1, 1, 4.9)], None)
        .unwrap();
    assert_eq!(
        t.players[0].techs[0].locations[0].adjustment,
        Seconds::new(4.0)
    );
    t.do_effect(PlayerId::new(0), &[attribute(b'g', 0, -1, 2, 0.8)], None)
        .unwrap();
    // (0.8 − 1) × 30 − 0.5 = −6.5, truncated −6, plus 4
    assert_eq!(
        t.players[0].techs[0].locations[0].adjustment,
        Seconds::new(-2.0)
    );
    assert_eq!(t.players[0].techs[0].state, ResearchState::Available);
    // set to 20: the adjustment −10, and 20 seconds are done
    t.do_effect(PlayerId::new(0), &[attribute(b'g', 0, -1, 0, 20.0)], None)
        .unwrap();
    assert_eq!(
        t.players[0].techs[0].locations[0].adjustment,
        Seconds::new(-10.0)
    );
    assert_eq!(t.players[0].techs[0].state, ResearchState::Researched);
}

/// Kind 8 (`FUN_1415c3f20`'s technology change): a cost mode changes a cost (0x4002: add to
/// stone), 4 a location's building to a master the player has, 5 its button, 10 the research
/// cap, 12 the state, −1 the research time (with the dependents' update).
#[test]
fn a_technology_change_reaches_its_costs_locations_and_state() {
    use crate::technology::Cost;
    let mut t = barracks_tech();
    t.techs[0].costs[0] = Cost {
        attribute: Some(attribute::STONE),
        amount: CostAmount::of_data(100),
        paid: CostPaid::PLAYER,
    };
    t.masters = vec![Masters::new([combat(master_id(87), class::BUILDING, 1000)])];
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 0x4002, -1, 25.0)], None)
        .unwrap();
    assert_eq!(
        t.players[0].techs[0].cost_adj.map(CostAmount::get),
        [25, 0, 0]
    );
    // a building the player lacks leaves the location
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 4, -1, 88.0)], None)
        .unwrap();
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 4, -1, 87.0)], None)
        .unwrap();
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 5, -1, 7.0)], None)
        .unwrap();
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 10, -1, 2.0)], None)
        .unwrap();
    let s = &t.players[0].techs[0];
    assert_eq!(
        (s.locations[0].building, s.locations[0].button),
        (Some(master_id(87)), LocationButton::of_data(7))
    );
    assert_eq!(s.research_cap, crate::technology::ResearchCap::of_data(2));
    // a location past the list adds a copy of the last, without a building
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 5, 3, 9.0)], None)
        .unwrap();
    let s = &t.players[0].techs[0];
    assert_eq!(s.locations.len(), 2);
    assert_eq!(
        (s.locations[1].building, s.locations[1].button),
        (None, LocationButton::of_data(9))
    );
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 12, -1, 0.0)], None)
        .unwrap();
    assert_eq!(t.players[0].techs[0].state, ResearchState::Disabled);
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 12, -1, 2.0)], None)
        .unwrap();
    assert_eq!(
        (t.players[0].techs[0].state, t.players[0].techs[0].enabled),
        (ResearchState::Available, true)
    );
    // state 3 completes it (its first location has a building and a button: it counts);
    // once researched, again gives its effect and counts whatever its location
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 12, -1, 3.0)], None)
        .unwrap();
    assert_eq!(t.players[0].techs[0].state, ResearchState::Researched);
    assert_eq!(t.players[0].attributes[0x15], Amount::new(1.0));
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 12, -1, 3.0)], None)
        .unwrap();
    assert_eq!(t.players[0].attributes[0x15], Amount::new(2.0));
}

/// Kind 8's other modes: a technology past the player's tree changes nothing; a location
/// within the list changes it alone (−2, a research time's add: at its building); one past
/// the list of a technology without locations changes nothing; 9 the stacking, 12 with 1
/// makes it unavailable; 18 the effect override (a researched technology's new effect would
/// run from the world's effect table, not modelled).
#[test]
fn a_technology_change_s_location_stacking_and_effect_override() {
    use crate::technology::Location;
    let mut t = barracks_tech();
    t.players[0].techs[0].locations.push(Location {
        building: Some(master_id(13)),
        time: WholeSeconds::of_data(40),
        button: LocationButton::of_data(4),
        adjustment: Seconds::ZERO,
    });
    t.do_effect(PlayerId::new(0), &[attribute(8, 5, 9, -1, 1.0)], None)
        .unwrap();
    assert!(!t.players[0].techs[0].stack);
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 9, -1, 1.0)], None)
        .unwrap();
    assert!(t.players[0].techs[0].stack);
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 5, 1, 9.0)], None)
        .unwrap();
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, -2, 1, 5.9)], None)
        .unwrap();
    let s = &t.players[0].techs[0];
    let at = |i: usize| (s.locations[i].button.get(), s.locations[i].adjustment);
    assert_eq!([at(0), at(1)], [(3, Seconds::ZERO), (9, Seconds::new(5.0))]);
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 12, -1, 1.0)], None)
        .unwrap();
    assert_eq!(t.players[0].techs[0].state, ResearchState::Unavailable);
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 18, -1, 7.0)], None)
        .unwrap();
    assert_eq!(t.players[0].techs[0].effect_override, EffectId::of_data(7));
    t.players[0].techs[0].state = ResearchState::Researched;
    assert!(
        t.do_effect(PlayerId::new(0), &[attribute(8, 0, 18, -1, 8.0)], None)
            .is_err()
    );
    // no locations: a location past the list changes nothing
    let mut t = barracks_tech();
    t.players[0].techs[0].locations.clear();
    t.do_effect(PlayerId::new(0), &[attribute(8, 0, 5, 2, 9.0)], None)
        .unwrap();
    assert_eq!(t.players[0].techs[0].locations.len(), 0);
}

/// Kind 3 (`FUN_14160e410`, mode −1): the old master takes the new one's values but keeps its
/// id, class, availability and disabling (`Master::copy_from`); its objects first take the
/// new one through their slot 0x98 ([`World::upgrade_object`]).
#[test]
fn an_upgrade_copies_the_new_master_into_the_old() {
    let player = Player::default();
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    let mut militia = combat(master_id(74), class::CLASS_6, 40);
    militia.available = true;
    let mut man_at_arms = combat(master_id(75), class::CLASS_6, 45);
    man_at_arms.weapons = vec![ClassAmount::new(4, 6)];
    man_at_arms.available = false;
    t.masters = vec![Masters::new([militia, man_at_arms])];
    t.do_effect(PlayerId::new(0), &[attribute(3, 74, 75, -1, 0.0)], None)
        .unwrap();
    let m = t.masters[0].get(master_id(74)).unwrap();
    assert_eq!(
        (m.id, m.hit_points, m.weapons.clone()),
        (
            master_id(74),
            MasterHitPoints::of_data(45),
            vec![ClassAmount::new(4, 6)]
        )
    );
    assert!(m.available);
    // mode 1 leaves the master
    t.masters[0].get_mut(master_id(75)).unwrap().hit_points = MasterHitPoints::of_data(50);
    t.do_effect(PlayerId::new(0), &[attribute(3, 74, 75, 1, 0.0)], None)
        .unwrap();
    assert_eq!(
        t.masters[0].get(master_id(74)).unwrap().hit_points,
        MasterHitPoints::of_data(45)
    );
    // with an object: its slot 0x98 keeps its share of the hit points (45 of 45: 50 of 50)
    let _ = t
        .push_live(
            PlayerId::new(0),
            unit_of(1, master_id(74), HitPoints::new(45.0)),
        )
        .unwrap();
    t.do_effect(PlayerId::new(0), &[attribute(3, 74, 75, -1, 0.0)], None)
        .unwrap();
    let u = &t.players[0].objects[ListIndex::new(0)];
    assert_eq!(
        (u.life.master_id, u.life.hp),
        (master_id(74), HitPoints::new(50.0))
    );
    assert_eq!(
        t.masters[0].get(master_id(74)).unwrap().hit_points,
        MasterHitPoints::of_data(50)
    );
}

/// Kind 7 (`FUN_1416136f0`) makes objects at the built buildings of a master, which this file
/// leaves to the spawning; with none built it does nothing. Kind 5 on a cost by a negative
/// amount empties (−2) a cost slot in its place or fills (−1) the first empty one
/// (`FUN_1415c1f70`): the slots after an emptied one keep their places.
#[test]
fn spawns_and_cost_entries() {
    let mut centre = unit_of(1, ids::TOWN_CENTRE, HitPoints::new(2400.0));
    centre.life.state = ObjectState::Foundation;
    let player = Player {
        objects: vec![centre].into(),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    let mut tc = combat(ids::TOWN_CENTRE, ClassId::of_master(80), 2400);
    tc.kind = master_type::BUILDING;
    let mut serjeant = combat(master_id(1658), class::CLASS_6, 60);
    serjeant.cost = costs(&[(attribute::FOOD, 60, true), (attribute::STONE, 35, true)]);
    t.masters = vec![Masters::new([tc, serjeant])];
    t.do_effect(PlayerId::new(0), &[attribute(7, 1658, 109, 5, 0.0)], None)
        .unwrap();
    t.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Alive;
    assert!(
        t.do_effect(PlayerId::new(0), &[attribute(7, 1658, 109, 5, 0.0)], None)
            .is_err()
    );
    // the food's slot (103) empties; the stone stays second
    t.do_effect(PlayerId::new(0), &[attribute(5, 1658, -1, 103, -2.0)], None)
        .unwrap();
    assert_eq!(
        t.masters[0].get(master_id(1658)).unwrap().cost,
        [
            CostSlot::default(),
            CostSlot::new(attribute::STONE, CostAmount::of_data(35), true),
            CostSlot::default()
        ]
    );
    // the gold (105) fills the first empty slot, before the stone
    t.do_effect(PlayerId::new(0), &[attribute(5, 1658, -1, 105, -1.0)], None)
        .unwrap();
    assert_eq!(
        t.masters[0].get(master_id(1658)).unwrap().cost,
        costs(&[(attribute::GOLD, -1, true), (attribute::STONE, 35, true)])
    );
}

/// The objects' kinds: 200 to 202 change the objects with masters of their own (none held);
/// 203 to 206 need the effect's object, which a research has not.
#[test]
fn the_object_kinds_without_an_object_do_nothing() {
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player::default()],
    );
    t.masters = vec![Masters::new([combat(ids::CASTLE, class::TOWER, 4800)])];
    for kind in 200..=206 {
        t.do_effect(PlayerId::new(0), &[attribute(kind, 82, -1, 8, 770.0)], None)
            .unwrap();
    }
    assert!(
        t.do_effect(
            PlayerId::new(0),
            &[attribute(204, 82, -1, 0, 1500.0)],
            Some(object_id(5))
        )
        .is_err()
    );
    assert_eq!(
        t.masters[0].get(ids::CASTLE).unwrap().hit_points,
        MasterHitPoints::of_data(4800)
    );
}

/// The enable command (`FUN_1416665b0`, kind 2): `available` takes the byte unless the master
/// is disabled; master 0x26d is never enabled in game mode 11.
#[test]
fn the_enable_command_sets_available_unless_disabled() {
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player::default()],
    );
    let mut locked = combat(master_id(5), class::CLASS_6, 10);
    locked.disabled = true;
    t.masters = vec![Masters::new([
        combat(master_id(4), class::CLASS_6, 10),
        locked,
        combat(master_id(0x26d), class::BUILDING, 10),
    ])];
    t.do_effect(
        PlayerId::new(0),
        &[attribute(2, 4, 1, -1, 0.0), attribute(2, 5, 1, -1, 0.0)],
        None,
    )
    .unwrap();
    let available: Vec<bool> = t.masters[0].iter().map(|m| m.available).collect();
    assert_eq!(available, [true, false, false]);
    t.settings.game_mode = crate::world::GameMode::of_data(11);
    t.do_effect(PlayerId::new(0), &[attribute(2, 0x26d, 1, -1, 0.0)], None)
        .unwrap();
    assert!(!t.masters[0].get(master_id(0x26d)).unwrap().available);
    t.settings.game_mode = crate::world::GameMode::of_data(0);
    assert!(
        t.do_effect(PlayerId::new(0), &[attribute(2, 0x26d, 1, -1, 0.0)], None)
            .is_err()
    );
    t.do_effect(PlayerId::new(0), &[attribute(2, 4, 0, -1, 0.0)], None)
        .unwrap();
    assert!(!t.masters[0].get(master_id(4)).unwrap().available);
}

/// A research whose effect reaches its building (the object `FUN_1413cdcd0` passes, which its
/// callers do not yet) stops before it changes anything; one with an effect override too.
#[test]
fn a_research_whose_effect_needs_its_building_stops() {
    use crate::technology::{TechDef, TechState};
    let state = TechState {
        state: ResearchState::Researching,
        building_master: None,
        ..TechState::default()
    };
    let player = Player {
        techs: vec![state.clone(), state],
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    let def = |effect| TechDef {
        required: [TechId::of_data(-1); 6],
        effect_id: EffectId::of_data(7),
        effect,
        ..TechDef::default()
    };
    t.techs = vec![
        def(vec![attribute(204, 82, -1, 0, 1500.0)]),
        def(vec![attribute(4, -1, 4, 0, 15.0)]),
    ];
    assert!(
        t.complete_research(PlayerId::new(0), tech_id(0), None)
            .is_err()
    );
    assert_eq!(t.players[0].techs[0].state, ResearchState::Researching);
    t.players[0].techs[1].effect_override = EffectId::of_data(9);
    assert!(
        t.complete_research(PlayerId::new(0), tech_id(1), None)
            .is_err()
    );
    t.players[0].techs[1].effect_override = EffectId::of_data(7);
    assert_eq!(
        t.complete_research(PlayerId::new(0), tech_id(1), None),
        Ok(true)
    );
}

/// An age (101 to 103) completes as any research and adds its event (0 to 2) to the
/// player's history at the world's time (`FUN_1413cdcd0`'s end); another research adds none.
#[test]
fn an_age_adds_its_event_to_the_history() {
    use crate::technology::{TechDef, TechState};
    let state = TechState {
        state: ResearchState::Researching,
        building_master: None,
        ..TechState::default()
    };
    let player = Player {
        techs: vec![state; 104],
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    t.techs = vec![
        TechDef {
            required: [TechId::of_data(-1); 6],
            ..TechDef::default()
        };
        104
    ];
    t.world_time = WorldTime::from_millis(612_345);
    t.players[0].history.entries = vec![
        crate::players::history::HistoryEntry {
            civilian: crate::players::history::Population::of_data(3),
            military: crate::players::history::Population::ZERO,
        };
        20
    ];
    assert_eq!(
        t.complete_research(PlayerId::new(0), tech_id(100), None),
        Ok(true)
    );
    assert_eq!(t.players[0].history.events, []);
    assert_eq!(
        t.complete_research(PlayerId::new(0), tech_id(102), None),
        Ok(true)
    );
    assert_eq!(t.players[0].techs[102].state, ResearchState::Researched);
    assert_eq!(
        t.players[0].history.events,
        [crate::players::history::Event {
            kind: crate::players::history::EventKind::CASTLE_AGE,
            time_slice: crate::players::history::TimeSlice::of_data(20),
            world_time: WorldTime::from_millis(612_345),
            ..crate::players::history::Event::default()
        }]
    );
}

/// A technology whose requirement is researched and whose research takes no time at a
/// location (`FUN_1413cce70`'s call of `FUN_1413cdcd0`) is done at once, and so are those
/// that depend on it in turn; one still missing a requirement stays unavailable.
#[test]
fn a_dependent_technology_done_already_completes_with_its_requirement() {
    use crate::technology::{Location, TechDef, TechState};
    let def = |required: i16, count| TechDef {
        required: [TechId::of_data(required), None, None, None, None, None],
        required_count: count,
        ..TechDef::default()
    };
    let free = TechState {
        locations: vec![Location {
            building: Some(ids::TOWN_CENTRE),
            ..Location::default()
        }],
        building_master: None,
        ..TechState::default()
    };
    let player = Player {
        techs: vec![TechState::default(), free.clone(), free.clone(), free],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    t.techs = vec![def(-1, 0), def(0, 1), def(1, 1), def(0, 2)];
    assert_eq!(
        t.complete_research(PlayerId::new(0), tech_id(0), None),
        Ok(true)
    );
    let states: Vec<i16> = t.players[0].techs.iter().map(|s| s.state.get()).collect();
    assert_eq!(states, [3, 3, 3, 0]);
}

/// `FUN_1413cce70`'s civilisation test: a dependent of another civilisation turns −1; one of
/// its own of mode 0 does too under the full tech tree (`+0x63`), not one of mode 1; one for
/// every civilisation (civ 0) is available either way.
#[test]
fn the_full_tech_tree_disables_a_civ_s_own_technologies_but_those_of_mode_1() {
    use crate::technology::{TechDef, TechState};
    let def = |required: i16, civ: i16, mode: i16| TechDef {
        required: [TechId::of_data(required), None, None, None, None, None],
        required_count: i16::from(required != -1),
        civ: CivId::of_tech(civ).unwrap(),
        mode: TechMode::of_data(mode),
        ..TechDef::default()
    };
    let states = |full_tech_tree: bool| {
        let player = Player {
            civ: CivId::new(5),
            techs: vec![TechState::default(); 5],
            ..Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(0),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![player],
        );
        t.settings.full_tech_tree = full_tech_tree;
        t.techs = vec![
            def(-1, 0, 0),
            def(0, 5, 0),
            def(0, 5, 1),
            def(0, 7, 0),
            def(0, 0, 0),
        ];
        assert_eq!(
            t.complete_research(PlayerId::new(0), tech_id(0), None),
            Ok(true)
        );
        t.players[0]
            .techs
            .iter()
            .map(|s| s.state.get())
            .collect::<Vec<i16>>()
    };
    assert_eq!(states(false), [3, 1, 1, -1, 1]);
    assert_eq!(states(true), [3, -1, 1, -1, 1]);
}

/// The technology commands of an effect: the disable command (`'f'`, MarvinBlue/426048737's
/// civ effect disabling technology 238 in update 1) sets a technology to −1 but a researched
/// one, one of type 0x21 to 1, and leaves one of the shared kind (a building's own); the cost
/// modifier (`'e'`) adds to, sets or scales a paid cost entry's adjustment.
#[test]
fn the_technology_commands_disable_and_adjust_costs() {
    use crate::technology::{Cost, TechDef, TechState};
    let state = |s: ResearchState| TechState {
        state: s,
        ..TechState::default()
    };
    let player = Player {
        techs: vec![
            state(ResearchState::Available),
            state(ResearchState::Researched),
            state(ResearchState::Unavailable),
            state(ResearchState::Available),
        ],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    let kind = |k: crate::technology::TechType| TechDef {
        kind: k,
        costs: [
            Cost {
                attribute: Some(attribute::FOOD),
                amount: CostAmount::of_data(150),
                paid: CostPaid::PLAYER,
            },
            Cost {
                attribute: Some(attribute::GOLD),
                amount: CostAmount::of_data(50),
                paid: CostPaid::PLAYER,
            },
            Cost::default(),
        ],
        ..TechDef::default()
    };
    t.techs = vec![
        kind(crate::technology::TechType::default()),
        kind(crate::technology::TechType::default()),
        kind(crate::technology::TechType::REENABLED),
        kind(crate::technology::TechType::SHARED),
    ];
    let command = |kind: u8, a: i16, b: i16, c: i16, d: f32| {
        EffectCommand::of_data(kind, a, b, c, d).unwrap()
    };
    for tech in 0..4 {
        t.do_effect(
            PlayerId::new(0),
            &[command(b'f', -1, -1, -1, tech as f32)],
            None,
        )
        .unwrap();
    }
    let states: Vec<i16> = t.players[0].techs.iter().map(|s| s.state.get()).collect();
    assert_eq!(states, [-1, 3, 1, 1]);
    // gold of technology 0 costs 10 more; food set to 100; every paid cost scaled by 0.5
    t.do_effect(PlayerId::new(0), &[command(b'e', 0, 3, 1, 10.0)], None)
        .unwrap();
    assert_eq!(
        t.players[0].techs[0].cost_adj.map(CostAmount::get),
        [0, 10, 0]
    );
    t.do_effect(PlayerId::new(0), &[command(b'e', 0, 0, 0, 100.0)], None)
        .unwrap();
    assert_eq!(
        t.players[0].techs[0].cost_adj.map(CostAmount::get),
        [-50, 10, 0]
    );
    t.do_effect(PlayerId::new(0), &[command(b'e', 1, -1, 2, 0.5)], None)
        .unwrap();
    assert_eq!(
        t.players[0].techs[1].cost_adj.map(CostAmount::get),
        [-75, -25, 0]
    );
    // the research time modifier changes a location's adjustment; a technology without
    // locations has none to change
    t.do_effect(PlayerId::new(0), &[command(b'g', 0, -1, 1, 5.0)], None)
        .unwrap();
}

/// `FUN_1413cdb00` reads the id as a short (`MOVZX EDX, BX` in `FUN_14163a500`): 65538 is
/// technology 2, 65535 is −1, every technology.
#[test]
fn the_disable_command_reads_its_id_s_low_16_bits() {
    use crate::technology::{TechDef, TechState};
    let state = |s: ResearchState| TechState {
        state: s,
        ..TechState::default()
    };
    let player = Player {
        techs: vec![
            state(ResearchState::Unavailable),
            state(ResearchState::Unavailable),
            state(ResearchState::Unavailable),
        ],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    t.techs = vec![TechDef::default(); 3];
    let disable = |d: f32| EffectCommand::of_data(b'f', -1, -1, -1, d).unwrap();
    t.do_effect(PlayerId::new(0), &[disable(65538.0)], None)
        .unwrap();
    let states: Vec<i16> = t.players[0].techs.iter().map(|s| s.state.get()).collect();
    assert_eq!(states, [0, 0, -1]);
    t.do_effect(PlayerId::new(0), &[disable(65535.0)], None)
        .unwrap();
    let states: Vec<i16> = t.players[0].techs.iter().map(|s| s.state.get()).collect();
    assert_eq!(states, [-1, -1, -1]);
}

/// `FUN_1413d10f0` converts with `CVTTSS2SI` and adds the low word: an amount of 2^31 or
/// more adds 0 (0x80000000's low word), where a saturating cast would add −1.
#[test]
fn a_cost_change_past_the_i32_range_adds_nothing() {
    use crate::technology::{Cost, TechDef, TechState};
    let player = Player {
        techs: vec![TechState::default()],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    t.techs = vec![TechDef {
        costs: [
            Cost {
                attribute: Some(attribute::FOOD),
                amount: CostAmount::of_data(150),
                paid: CostPaid::PLAYER,
            },
            Cost::default(),
            Cost::default(),
        ],
        ..TechDef::default()
    }];
    let change = EffectCommand::of_data(b'e', 0, 0, 1, 3.0e9).unwrap();
    t.do_effect(PlayerId::new(0), &[change], None).unwrap();
    assert_eq!(
        t.players[0].techs[0].cost_adj.map(CostAmount::get),
        [0, 0, 0]
    );
    // mode 2 at 1.5: (1.5 − 1) × 150 + 0.5 = 75.5, truncated to 75
    let scale = EffectCommand::of_data(b'e', 0, 0, 2, 1.5).unwrap();
    t.do_effect(PlayerId::new(0), &[scale], None).unwrap();
    assert_eq!(
        t.players[0].techs[0].cost_adj.map(CostAmount::get),
        [75, 0, 0]
    );
}

/// A dead building's master technology goes back to 0 (`FUN_1413d4ec0`) when no other
/// building of the master stands, and only from a state above 0.
#[test]
fn the_last_building_of_a_master_withdraws_its_technology() {
    use crate::technology::{TechDef, TechState};
    let building = |id, state| {
        let mut u = Unit {
            id,
            ..Unit::default()
        };
        u.life.master_id = master_id(1640);
        u.life.state = state;
        u
    };
    let player = Player {
        objects: vec![
            building(object_id(1), ObjectState::Dying),
            building(object_id(2), ObjectState::Alive),
        ]
        .into(),
        techs: vec![TechState {
            state: ResearchState::Researched,
            ..TechState::default()
        }],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    t.techs = vec![TechDef {
        required: [TechId::of_data(-1); 6],
        ..TechDef::default()
    }];
    t.withdraw_tech(PlayerId::new(0), master_id(1640), tech_id(0))
        .unwrap();
    assert_eq!(t.players[0].techs[0].state, ResearchState::Researched);
    t.players[0].objects.truncate(1);
    t.reindex();
    t.withdraw_tech(PlayerId::new(0), master_id(1640), tech_id(0))
        .unwrap();
    assert_eq!(t.players[0].techs[0].state, ResearchState::Unavailable);
}

/// The sprite part of `FUN_141614f40` after a change of the standing sprite (attribute 71) of
/// master 4: its idle unit without actions shows the new standing sprite (slot 0x68); one whose
/// idle variant is 1 keeps its sprite (it shows the second standing sprite, 72); one with
/// actions leaves its sprite to them (an Explore's slot 0x50 does nothing).
#[test]
fn a_standing_sprite_change_shows_on_the_master_s_idle_units() {
    use crate::objects::sprite::{ActiveSprite, Graphic};
    let graphic = |id| Graphic {
        id,
        frames: crate::objects::Frame::of_data(10),
        facets: crate::objects::FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let mut rng = crate::math::rng::Well512::default();
    let mut shown = |id, variant| {
        let mut u = unit_of(id, master_id(4), HitPoints::new(30.0));
        u.sprite = Some(ActiveSprite::start(graphic(SpriteId::named(300)), &mut rng));
        u.idle_variant = variant;
        u
    };
    let mut exploring = shown(3, 0);
    exploring.actions.list.set_explore(Some(
        crate::movement::explore::ExploreAction::new(
            Position::new(5.0, 5.0, 0.0),
            &crate::movement::explore::bare_walk(),
        )
        .unwrap()
        .0,
    ));
    let player = Player {
        objects: vec![shown(1, 0), shown(2, 1), exploring].into(),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    t.masters = vec![Masters::new([Master {
        standing_sprite: Some(SpriteId::named(300)),
        ..combat(master_id(4), class::CLASS_6, 60)
    }])];
    t.graphics = [
        (SpriteId::named(300), graphic(SpriteId::named(300))),
        (SpriteId::named(301), graphic(SpriteId::named(301))),
    ]
    .into_iter()
    .collect();
    t.load_trackers().unwrap();
    t.do_effect(PlayerId::new(0), &[attribute(0, 4, -1, 71, 301.0)], None)
        .unwrap();
    let sprite = |i: ListIndex| {
        t.players[0].objects[i]
            .sprite
            .as_ref()
            .map(|s| s.graphic.id)
    };
    assert_eq!(
        t.masters[0].get(master_id(4)).map(|m| m.standing_sprite),
        Some(Some(SpriteId::named(301)))
    );
    // the unit with an action leaves its sprite to the action (an Explore holds none)
    assert_eq!(
        (
            sprite(ListIndex::new(0)),
            sprite(ListIndex::new(1)),
            sprite(ListIndex::new(2))
        ),
        (
            Some(SpriteId::named(301)),
            Some(SpriteId::named(300)),
            Some(SpriteId::named(300))
        )
    );
}

/// `FUN_14165a400`'s records around a master's change: the height (32) of a house master
/// takes each built house's record off first (`FUN_14165b510`) and gives it again after when
/// the master stands (`FUN_1411c3f70`): from 0 to 2 the house gains its wall record, back to 0
/// it has none; an obstruction type's change (78) to another gives the record again with the
/// new kind. A hero status (40) of a unit in a group is not modelled; one outside a group
/// changes.
#[test]
fn a_height_or_obstruction_change_takes_the_records_off_and_gives_them_again() {
    let mut house = unit_of(1, ids::HOUSE, HitPoints::new(550.0));
    house.place_for_test(Position::new(5.5, 5.5, 0.0));
    let mut soldier = unit_of(2, master_id(4), HitPoints::new(30.0));
    soldier.group.id = Some(group_id(7));
    let player = Player {
        objects: vec![house, soldier].into(),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    )
    .with_buildings([(object_id(1), built_layer())]);
    t.masters = vec![Masters::new([
        combat(master_id(4), class::CLASS_6, 60),
        Master {
            id: ids::HOUSE,
            kind: master_type::BUILDING,
            class: class::BUILDING,
            hit_points: MasterHitPoints::of_data(550),
            radius: Footprint::new(0.5, 0.5),
            obstruction_type: ObstructionType::TYPE_2,
            obstruction_class: ObstructionClass::UNIT,
            ..Master::default()
        },
    ])];
    t.spatial = crate::map::spatial::Spatial::new(GridSize::new(10, 10));
    t.spatial.insert(
        object_id(1),
        crate::map::spatial::Placed {
            position: Position::new(5.5, 5.5, 0.0),
            radius: Footprint::new(0.5, 0.5),
            class: class::BUILDING,
            player: PlayerId::new(0),
            master: ids::HOUSE,
            obstruction_type: ObstructionType::TYPE_2,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    t.load_trackers().unwrap();
    let record = |t: &World| t.spatial.get(object_id(1)).and_then(|p| p.obstruction);
    t.do_effect(PlayerId::new(0), &[attribute(0, 70, -1, 32, 2.0)], None)
        .unwrap();
    assert_eq!(
        record(&t),
        Some(crate::map::spatial::Obstruction {
            kind: ObstructionType::TYPE_2,
            class: ObstructionClass::UNIT
        })
    );
    t.do_effect(PlayerId::new(0), &[attribute(0, 70, -1, 78, 3.0)], None)
        .unwrap();
    assert_eq!(
        record(&t),
        Some(crate::map::spatial::Obstruction {
            kind: ObstructionType::TYPE_3,
            class: ObstructionClass::UNIT
        })
    );
    t.do_effect(PlayerId::new(0), &[attribute(0, 70, -1, 32, 0.0)], None)
        .unwrap();
    assert_eq!(record(&t), None);
    assert!(
        t.do_effect(PlayerId::new(0), &[attribute(0, 4, -1, 40, 1.0)], None)
            .is_err()
    );
    t.players[0].objects[ListIndex::new(1)].group.id = None;
    t.do_effect(PlayerId::new(0), &[attribute(0, 4, -1, 40, 1.0)], None)
        .unwrap();
}
