//! A siege unit's count of the infantry inside it, and a ranged unit's reload as it leaves one.

use crate::garrison::ContainerId;
use crate::garrison::LetOut;
use crate::garrison::NO_POINT;
use crate::garrison::tests::standing;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatPart;
use crate::objects::masters::GarrisonCapacity;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::testing::class;
use crate::testing::scene::{PLAYER, Scene, SceneObject};
use crate::vision::VisionBits;
use crate::world::Seconds;
use crate::world::World;

/// A class whose units speed up a siege unit they are inside (6, the infantry's).
const INFANTRY: ClassId = ClassId::named(6);

/// A combat master `id` of class `class`, range `range` and reload 2.
fn combat_master(id: i16, class: ClassId, range: f32) -> Master {
    Master {
        id: MasterId::named(id),
        kind: MasterKind::COMBAT,
        class,
        radius: Footprint::new(0.2, 0.2),
        combat: Some(CombatPart {
            weapon_range: Distance::new(range),
            reload: Seconds::new(2.0),
            ..CombatPart::default()
        }),
        ..Master::default()
    }
}

/// Object 7 of the siege master `siege` and object 8 of `unit` beside it, both player 1's.
fn siege_world(siege: Master, unit: Master) -> World {
    let seen = Some(VisionBits::visible(PLAYER));
    Scene::open(GridSize::new(8, 8))
        .object(SceneObject {
            seen_by: seen,
            ..SceneObject::new(7, PLAYER, siege, Position::new(2.5, 2.5, 0.0))
        })
        .object(SceneObject {
            seen_by: seen,
            ..SceneObject::new(8, PLAYER, unit, Position::new(3.5, 2.5, 0.0))
        })
        .build()
}

/// Object `id`.
fn object(id: i32) -> ObjectId {
    ObjectId::try_from(id).unwrap()
}

/// Object 8 goes into object 7 at once (slot 0x1a0).
fn enter(world: &mut World) {
    let index = world.live_index(PLAYER, object(8)).unwrap();
    world
        .enter_forced(PLAYER, index, ContainerId::new(object(7)))
        .unwrap();
}

/// Infantry entering a siege unit counts in its `+0xad8` and adds the tuning's speed per unit to
/// its speed powerup `+0xa2c`, and leaving takes both off; a ranged unit leaving one starts its
/// attack timer at its full reload (the stinger plus the master's reload less the powerup).
#[test]
fn siege_unit_counts_infantry_inside_and_ranged_unit_leaves_reloaded() {
    let ram = combat_master(1258, class::SIEGE, 0.0);
    let infantry = combat_master(1259, INFANTRY, 4.0);
    let mut world = siege_world(ram.clone(), infantry.clone());
    enter(&mut world);
    let siege = world.parts.combat_of(object(7));
    assert_eq!(siege.infantry_inside.get(), 1);
    assert_eq!(siege.speed_powerup, Speed::new(0.05));
    let combat = world.parts.combat_mut(object(8)).unwrap();
    combat.reload_stinger = Seconds::new(0.5);
    combat.reload_powerup = Seconds::new(0.25);
    world
        .leave_siege(object(7), object(8), &infantry, &ram)
        .unwrap();
    let siege = world.parts.combat_of(object(7));
    assert_eq!(siege.infantry_inside.get(), 0);
    assert_eq!(siege.speed_powerup, Speed::new(0.0));
    assert_eq!(
        world.parts.combat_of(object(8)).attack_timer,
        Seconds::new(0.5 + (2.0 - 0.25))
    );
}

/// Masters 1704 and 1706 count no infantry and reload no one; a class that does not speed a
/// siege unit up, or a container that is no siege unit, counts none.
#[test]
fn unsped_siege_masters_and_other_classes_count_none() {
    let infantry = combat_master(1259, INFANTRY, 0.5);
    for siege in [
        combat_master(1704, class::SIEGE, 0.0),
        combat_master(1706, class::SIEGE, 0.0),
        combat_master(1258, class::BUILDING, 0.0),
    ] {
        let mut world = siege_world(siege, infantry.clone());
        enter(&mut world);
        assert_eq!(world.parts.combat_of(object(7)).infantry_inside.get(), 0);
    }
    let mut world = siege_world(
        combat_master(1258, class::SIEGE, 0.0),
        combat_master(1260, class::MONK, 0.5),
    );
    enter(&mut world);
    assert_eq!(world.parts.combat_of(object(7)).infantry_inside.get(), 0);
    // a melee unit (range 0.5) keeps its attack timer
    world
        .leave_siege(
            object(7),
            object(8),
            &combat_master(1260, class::MONK, 0.5),
            &combat_master(1258, class::SIEGE, 0.0),
        )
        .unwrap();
    assert_eq!(world.parts.combat_of(object(8)).attack_timer, Seconds::ZERO);
}

/// Slot 0x1a0's room test keeps infantry out of a full siege unit, and its tail counts it all
/// the same (the game's count then runs ahead of the units inside).
#[test]
fn full_siege_unit_counts_infantry_its_room_test_kept_out() {
    let ram = combat_master(1258, class::SIEGE, 0.0);
    let infantry = combat_master(1259, INFANTRY, 0.0);
    let mut world = siege_world(ram, infantry);
    let index = world.live_index(PLAYER, object(8)).unwrap();
    // the ram has no room (its master's garrison capacity 0): the enter does not get in
    assert!(
        world
            .enter_now(PLAYER, index, ContainerId::new(object(7)))
            .is_err()
    );
    assert_eq!(world.parts.combat_of(object(7)).infantry_inside.get(), 1);
    let unit = world.object(object(8)).unwrap().1;
    assert_eq!(unit.life.inside, None);
}

/// `FUN_1411e7220` tests no class: a building of the siege class (13) lets an infantry unit of
/// range 4 out as any building does, and its slot 0x1a8 (`FUN_141213120`) ends as a siege
/// unit's: the building's count of the infantry inside goes back to 0 (slot 0x1b0 with −1), and
/// the unit leaves with its attack timer at its full reload.
#[test]
fn siege_class_building_lets_infantry_out_reloaded() {
    let workshop = Master {
        kind: MasterKind::BUILDING,
        garrison: GarrisonCapacity::of_data(5),
        ..combat_master(1258, class::SIEGE, 0.0)
    };
    let infantry = Master {
        move_stats: Some(standing()),
        ..combat_master(1259, INFANTRY, 4.0)
    };
    let mut world = siege_world(workshop, infantry);
    enter(&mut world);
    assert_eq!(world.parts.combat_of(object(7)).infantry_inside.get(), 1);
    let building = world.live_index(PLAYER, object(7)).unwrap();
    assert_eq!(
        world.let_out(
            PLAYER,
            building,
            object(8),
            Point::new(NO_POINT, NO_POINT),
            (None, false)
        ),
        Ok(LetOut::Out)
    );
    assert_eq!(world.object(object(8)).unwrap().1.life.inside, None);
    assert_eq!(world.parts.combat_of(object(7)).infantry_inside.get(), 0);
    assert_eq!(
        world.parts.combat_of(object(8)).attack_timer,
        Seconds::new(2.0)
    );
}
