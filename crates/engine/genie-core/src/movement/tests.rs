use super::*;
use crate::groups::MoveReason;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::RestrictionId;
use crate::map::TerrainMask;
use crate::map::Tile;
use crate::map::spatial::RecordId;
use crate::map::spatial::{Gait, Obstruction, Placed};
use crate::movement::LevelFlags;
use crate::movement::WaypointLevel;
use crate::movement::move_to::MoveWaitState;
use crate::movement::move_to::Walk;
use crate::movement::request_plans::{NewPath, Way};
use crate::objects::ObjectState;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::masters::ClassId;
use crate::objects::masters::FlyMode;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::ids::ID_45;
use crate::pathing::ShortFound;
use crate::players::ByPlayer;
use crate::players::relation_row;
use crate::test_ids::group_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_id_list;
use crate::unit_ai::UnitStance;
use crate::world::Millis;

const DT: f32 = 0.02;

/// MarvinBlue/426048731's boar (master 48): a walking sprite of 16 facets at a turn speed of
/// 0.05 turns 2π / 0.8 a second, 11.25° in a tick of 0.025 s; a villager turns at once.
#[test]
fn a_master_s_turn_rate_comes_from_its_turn_speed_and_walking_facets() {
    let rate = turn_rate(
        Seconds::new(0.05),
        Some(crate::objects::FacetCount::of_data(16)),
    );
    assert_eq!(rate, TurnRate::new(TWO_PI / (16.0f32 * 0.05)));
    assert_eq!(rate.get().to_bits(), 7.853_982f32.to_bits());
    assert_eq!(
        turn_rate(Seconds::ZERO, Some(crate::objects::FacetCount::of_data(16))),
        TurnRate::MAX
    );
    assert_eq!(turn_rate(Seconds::new(0.05), None), TurnRate::MAX);
}

/// `FUN_1410e3a70` writes all five turning values: a zero turn speed resets them, a walking
/// sprite sets the revolution and both rates, no sprite changes nothing.
#[test]
fn a_master_s_turning_values_follow_its_turn_speed_and_walking_sprite() {
    let before = Turning {
        radius: Distance::new(1.0),
        radius_speed: Speed::new(2.0),
        rate: TurnRate::new(3.0),
        revolution: Seconds::new(4.0),
        revolution_rate: TurnRate::new(5.0),
    };
    assert_eq!(
        turning(
            Seconds::ZERO,
            Some(crate::objects::FacetCount::of_data(16)),
            before
        ),
        (Turning::PRESET, false)
    );
    assert_eq!(turning(Seconds::new(0.05), None, before), (before, false));
    let (after, set) = turning(
        Seconds::new(0.05),
        Some(crate::objects::FacetCount::of_data(16)),
        before,
    );
    assert!(set);
    let rate = TWO_PI / (16.0f32 * 0.05);
    assert_eq!(
        after,
        Turning {
            revolution: Seconds::new(16.0f32 * 0.05),
            revolution_rate: TurnRate::new(rate),
            rate: TurnRate::new(rate),
            ..before
        }
    );
    // a sprite of no facets: a revolution of 0, the largest float
    assert_eq!(
        turning(
            Seconds::new(0.05),
            Some(crate::objects::FacetCount::of_data(0)),
            before
        )
        .0
        .rate,
        TurnRate::MAX
    );
}

fn stats() -> MoveStats {
    MoveStats {
        fly_mode: FlyMode::GROUND,
        speed: Speed::new(0.8),
        base: Speed::new(0.8),
        turn_rate: TurnRate::MAX,
        turn_radius: Distance::ZERO,
        turn_radius_speed: Speed::MAX,
        radius: Radii::new(0.2, 0.2, 0.5),
        restriction: RestrictionId::named(7),
        restriction_mask: TerrainMask::of_bits(6),
        turn_time: Seconds::ZERO,
        move_algorithm: MoveAlgorithm::NONE,
        min_collision: 1.0,
        fast_turn: false,
        facets: Some(crate::objects::facing::SpriteFacets {
            count: crate::objects::FacetCount::of_data(16),
            turns: true,
        }),
        weapon_range: Distance::ZERO,
    }
}

fn level(level: u8, points: &[Point], state: PathState) -> PathData {
    PathData {
        level: WaypointLevel::of_byte(level),
        linked_type: i32::from(level == 0),
        points: points
            .iter()
            .map(|p| BVector::new(p.x(), 0.0, p.y()))
            .collect(),
        has_path: true,
        path_flags: if level == 0 {
            PathFlags::BACK_AND_FORTH | PathFlags::FORWARD
        } else {
            PathFlags::FORWARD
        },
        waypoint: WaypointIndex::of_data(1),
        state,
        range: Distance::new(0.1),
        target: Some(object_id(10028)),
        pause_time: Seconds::ZERO,
        flags: LevelFlags::NONE,
        length: Distance::ZERO,
        created: WorldTime::from_millis(16360),
        continue_counter: 10,
        repath_time: 0.0,
        ignore: Vec::new(),
        ignored: Vec::new(),
    }
}

/// MarvinBlue/426048731's boar 9729 in update 4193: its Attack's `MoveTo` (`+0x61` clear)
/// walks to its path's end when its target, the hunter 10049, walks off; the walked level's
/// waypoint and the top path's end follow the hunter, in the paths' layout (x, z, y).
#[test]
fn a_path_s_end_follows_a_moved_target_in_the_paths_layout() {
    let mut m = builder_9177();
    m.paths[0].flags = LevelFlags::REACHED | LevelFlags::AT_WAYPOINT;
    assert!(m.walks_to_path_end().unwrap());
    m.follow_path_end(Position::new(99.515_114, 41.147_972, 0.0));
    assert_eq!(
        m.paths[0].points[1],
        BVector::new(99.515_114, 0.0, 41.147_972)
    );
    assert_eq!(
        m.paths[1].points[1],
        BVector::new(99.515_114, 0.0, 41.147_972)
    );
    assert_eq!(m.paths[0].flags, LevelFlags::NONE);
    assert!(m.walks_to_path_end().unwrap());
}

/// MarvinBlue/426048731's builder 9177 as the game saved it after tick 679 (update 680): its
/// first path toward the house foundation 10028, facing along it, not yet moving.
pub(crate) fn builder_9177() -> Mover {
    Mover {
        forward: BVector::new(0.894_427_2, 0.0, -0.447_213_62),
        right: BVector::new(-0.447_213_62, 0.0, -0.894_427_2),
        velocity: Some(Velocity {
            velocity: BVector::ZERO,
            acceleration: BVector::new(0.0, 0.0, f32::MAX),
        }),
        paths: vec![
            level(
                0,
                &[Point::new(28.0, 89.0), Point::new(30.0, 88.0)],
                PathState::Walking,
            ),
            level(
                2,
                &[Point::new(28.0, 89.0), Point::new(30.0, 88.0)],
                PathState::Idle,
            ),
        ],
        factor: 1.0,
        stats: stats(),
        waiting: 0,
        wait_delays: 0,
        attack_moving: false,
        last_move: WorldTime::ZERO,
        moved_at: Position::new(28.0, 89.0, 0.0),
        substitute_count: 0,
        ignore_units: Vec::new(),
        user_waypoints: Vec::new(),
        last_turn: Seconds::ZERO,
        angle: Angle::ZERO,
        animated: Animated::default(),
        pauses: Vec::new(),
        in_the_way: Vec::new(),
        sprite_facets: None,
    }
}

/// `FUN_14114ece0`: the unit, its target, then its own list but itself when the walked
/// level's top path holds a path made this update.
#[test]
fn the_ignore_list_takes_the_own_list_when_the_top_path_is_this_update_s() {
    let world = world();
    let mut pathing = Pathing::default();
    let mut object = Well512::default();
    let groups = crate::groups::UnitGroups::default();
    let mut plan = Planning {
        pathing: &mut pathing,
        now: WorldTime::from_millis(16360),
        computer: false,
        object: &mut object,
        groups: &groups,
    };
    let me = Me {
        id: object_id(9177),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    let mut m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    m.ignore_units = vec![object_id(5), object_id(9177), object_id(6)];
    let house = *world.get(object_id(10028)).unwrap();
    let line = (Point::new(28.0, 0.0), Point::new(30.0, 0.0));
    let list = |m: &Mover, plan: &mut Planning<'_>| {
        m.ignore_list(&me, &world, plan, Some((object_id(10028), &house)), line)
    };
    assert_eq!(
        list(&m, &mut plan),
        Ok(vec![
            object_id(9177),
            object_id(10028),
            object_id(5),
            object_id(6)
        ])
    );
    // a top level without a path, or a path of another update: none of the own list
    m.paths[1].has_path = false;
    assert_eq!(
        list(&m, &mut plan),
        Ok(vec![object_id(9177), object_id(10028)])
    );
    m.paths[1].has_path = true;
    m.paths[1].created = WorldTime::from_millis(16359);
    assert_eq!(
        list(&m, &mut plan),
        Ok(vec![object_id(9177), object_id(10028)])
    );
}

/// A 16 × 16 map of terrain `class` with player 2's drop-off walker 9177 standing at (5.5, 5.5),
/// its target the house 10028 at (12.5, 5.5), and the records `near` about it (id, place,
/// radius, class, player), each of the master numbered as its class.
fn drop_off_world(class: u8, near: &[(i32, Point, f32, ClassId, PlayerId)]) -> Spatial {
    let mut w = Spatial::with_tiles(
        GridSize::new(16, 16),
        crate::map::spatial::Tiles::new(crate::map::GridSize::new(16, 16), vec![class; 256]),
    );
    let placed = |at: Point, r: f32, class, player| Placed {
        position: at.with_height(0.0),
        radius: Footprint::new(r, r),
        class,
        player,
        master: master_id(class.get()),
        obstruction_type: ObstructionType::CIRCLE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: Some(Obstruction {
            kind: ObstructionType::CIRCLE,
            class: ObstructionClass::UNIT,
        }),
    };
    w.insert(
        object_id(9177),
        placed(Point::new(5.5, 5.5), 0.2, class::VILLAGER, PlayerId::new(2)),
    );
    w.insert(
        object_id(10028),
        Placed {
            obstruction: None,
            ..placed(
                Point::new(12.5, 5.5),
                1.0,
                class::BUILDING,
                PlayerId::new(2),
            )
        },
    );
    for &(id, at, r, class, player) in near {
        w.insert(object_id(id), placed(at, r, class, player));
    }
    w.relations = ByPlayer::new(vec![ByPlayer::default(); 5]);
    // player 4 is player 2's ally; player 3 is not
    w.relations
        .set(PlayerId::new(4), relation_row([1, 1, 0, 1, 1]));
    w
}

/// `FUN_14114ece0` for a standing unit whose action is in state 8 (a drop-off): with its
/// start blocked it also ignores the workers within its radius and a tenth that stand (their
/// path not this update's), of a worker class or a building with a task of action type 0x99,
/// allied, and not touching the target; with its start free, or walking, or in another state,
/// none of them.
#[test]
fn a_drop_off_walker_ignores_the_allied_workers_standing_beside_it() {
    let near = [
        (
            1,
            Point::new(5.6, 5.5),
            0.2,
            class::VILLAGER,
            PlayerId::new(2),
        ),
        (
            2,
            Point::new(5.6, 5.6),
            0.2,
            class::CLASS_7,
            PlayerId::new(2),
        ),
        (
            3,
            Point::new(5.4, 5.5),
            0.2,
            class::VILLAGER,
            PlayerId::new(2),
        ),
        (
            4,
            Point::new(5.5, 5.6),
            0.2,
            class::VILLAGER,
            PlayerId::new(2),
        ),
        (
            5,
            Point::new(5.5, 5.4),
            0.2,
            class::VILLAGER,
            PlayerId::new(3),
        ),
        (
            6,
            Point::new(5.4, 5.4),
            0.2,
            class::FISHING_SHIP,
            PlayerId::new(4),
        ),
        (7, Point::new(5.6, 5.4), 0.5, class::WALL, PlayerId::new(2)),
        (8, Point::new(5.4, 5.6), 0.5, class::GATE, PlayerId::new(2)),
        (
            9,
            Point::new(5.5, 5.5),
            7.0,
            class::VILLAGER,
            PlayerId::new(2),
        ),
    ];
    let mut world = drop_off_world(1, &near);
    let gait = |moving, path_made| {
        Some(Gait {
            moving,
            path_made,
            ..Gait::default()
        })
    };
    world.set_gait(object_id(3), gait(true, None));
    world.set_gait(object_id(4), gait(false, Some(WorldTime::from_millis(100))));
    world.set_gait(object_id(1), gait(false, Some(WorldTime::from_millis(99))));
    // player 2's master 27 (a building of class 27) has a task of action type 0x99; 39 none
    world.task_0x99 = vec![vec![], vec![], vec![master_id(27)]];
    let mut pathing = Pathing::default();
    pathing.start_mesh(16);
    let mut object = Well512::default();
    let groups = crate::groups::UnitGroups::default();
    let mut plan = Planning {
        pathing: &mut pathing,
        now: WorldTime::from_millis(100),
        computer: false,
        object: &mut object,
        groups: &groups,
    };
    let me = |action_state| Me {
        id: object_id(9177),
        group: None,
        action_state,
    };
    let mut m = builder_9177().placed(Position::new(5.5, 5.5, 0.0));
    m.paths.clear();
    m.moved_at = Position::new(5.5, 5.5, 0.0);
    let house = *world.get(object_id(10028)).unwrap();
    let line = (Point::new(5.5, 5.5), Point::new(12.5, 5.5));
    let list = |m: &Mover, world: &Spatial, plan: &mut Planning<'_>, state| {
        m.ignore_list(
            &me(state),
            world,
            plan,
            Some((object_id(10028), &house)),
            line,
        )
    };
    let mut ids = list(&m, &world, &mut plan, ActionState::new(8)).unwrap();
    ids[2..].sort_unstable();
    assert_eq!(
        ids,
        [
            object_id(9177),
            object_id(10028),
            object_id(1),
            object_id(6),
            object_id(7)
        ]
    );
    assert_eq!(
        list(&m, &world, &mut plan, ActionState::new(7)).unwrap(),
        [object_id(9177), object_id(10028)]
    );
    m.velocity.as_mut().unwrap().velocity = BVector::new(0.0, 0.0, 0.8);
    assert_eq!(
        list(&m, &world, &mut plan, ActionState::new(8)).unwrap(),
        [object_id(9177), object_id(10028)]
    );
    m.velocity.as_mut().unwrap().velocity = BVector::ZERO;
    let free = drop_off_world(2, &near);
    assert_eq!(
        list(&m, &free, &mut plan, ActionState::new(8)).unwrap(),
        [object_id(9177), object_id(10028)]
    );
}

/// `FUN_140fa0110`: the drop-off walker's start test along its line, with its radius, its
/// weapon range and the target's radii: blocked deep in terrain its mask lacks, free on
/// terrain it has.
#[test]
fn the_drop_off_walker_s_start_is_blocked_in_terrain_its_mask_lacks() {
    let test = |class: u8| {
        let world = Spatial::with_tiles(
            GridSize::new(16, 16),
            crate::map::spatial::Tiles::new(crate::map::GridSize::new(16, 16), vec![class; 256]),
        );
        let mut pathing = Pathing::default();
        pathing.start_mesh(16);
        let mut object = Well512::default();
        let groups = crate::groups::UnitGroups::default();
        let mut plan = Planning {
            pathing: &mut pathing,
            now: WorldTime::ZERO,
            computer: false,
            object: &mut object,
            groups: &groups,
        };
        let mut target = world_target();
        target.position = Position::new(12.5, 5.5, 0.0);
        let m = builder_9177();
        m.start_blocked(
            &world,
            &mut plan,
            &target,
            (Point::new(5.5, 5.5), Point::new(12.5, 5.5)),
        )
    };
    // class 1 lies outside the villager's mask (6): every row of tiles is a shape
    assert_eq!(test(1), Ok(true));
    assert_eq!(test(2), Ok(false));
}

/// A 20 × 20 map whose tiles (6, 4..=6) are of a terrain the mask 6 lacks (a wall), with the
/// map's links, and player 2's unit 9171 of radius `radius` at (4.5, 5.5).
fn walled(radius: Distance) -> Spatial {
    let mut classes = vec![4; 400];
    for y in 4..=6 {
        classes[y * 20 + 6] = 1;
    }
    let mut w = Spatial::with_tiles(
        GridSize::new(20, 20),
        crate::map::spatial::Tiles::new(crate::map::GridSize::new(20, 20), classes),
    );
    w.sight = Some(crate::vision::visibility::Visibility::new(
        GridSize::new(20, 20),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        0,
        Vec::new(),
    ));
    w.insert(
        object_id(9171),
        Placed {
            position: Position::new(4.5, 5.5, 0.0),
            radius: Footprint::new(radius.get(), radius.get()),
            class: class::VILLAGER,
            player: PlayerId::new(2),
            master: ids::ID_0,
            obstruction_type: ObstructionType::CIRCLE,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: Some(Obstruction {
                kind: ObstructionType::CIRCLE,
                class: ObstructionClass::UNIT,
            }),
        },
    );
    w
}

/// `FUN_140f9e540`'s dispatch: `COMISS 0.5, radius; JC` (0x140f9e71f): a radius above half a
/// tile, or an unordered one, takes the old pather.
#[test]
fn a_radius_above_half_a_tile_or_unordered_takes_the_old_pather() {
    assert!(!old_pather_for(Distance::new(0.2)));
    assert!(!old_pather_for(Distance::new(0.5)));
    assert!(old_pather_for(Distance::new(f32::from_bits(
        0.5f32.to_bits() + 1
    ))));
    assert!(old_pather_for(Distance::new(f32::NAN)));
}

/// `FUN_140f9e540` for a unit wider than half a tile: the path empties, then the old pather
/// (`FUN_140f89e40` with the caller's ignore list, `FUN_140f85580` with the include list and
/// the owner's number) writes it, its waypoints at height 0: here around the wall to the
/// point (its area, grown once, holds 96 tiles). A unit of half a tile takes the new pather,
/// which needs the game's mesh.
#[test]
fn a_big_unit_s_short_path_takes_the_old_pather() {
    let mut world = walled(Distance::new(0.6));
    let mut pathing = Pathing::default();
    let mut object = Well512::default();
    let groups = crate::groups::UnitGroups::default();
    let mut plan = Planning {
        pathing: &mut pathing,
        now: WorldTime::from_millis(100),
        computer: false,
        object: &mut object,
        groups: &groups,
    };
    let walker = Walker {
        id: object_id(9171),
        radius: Distance::new(0.6),
        restriction: RestrictionId::named(0),
        mask: TerrainMask::of_bits(6),
    };
    let mut out = Path::default();
    let _ = out.push(BVector::new(1.0, 0.0, 1.0));
    let goal = (BVector::new(8.5, 0.0, 5.5), Distance::ZERO, None);
    let r = short_path_for(
        walker,
        Point::new(4.5, 5.5),
        &mut world,
        &mut plan,
        goal,
        &[object_id(9171)],
        &mut out,
    );
    let mut pather = HullPather::for_list(
        &mut world,
        &[object_id(9171)],
        TerrainMask::of_bits(6),
        Distance::new(0.6),
        Some(PlayerId::from(2)),
    );
    let (result, path) = pather
        .find(
            Point::new(4.5, 5.5),
            Point::new(8.5, 5.5),
            None,
            Distance::ZERO,
        )
        .unwrap();
    assert_eq!(
        (r, result),
        (Ok(ShortFound::Reached), ShortFound::Reached),
        "{path:?}"
    );
    let points: Vec<BVector> = path
        .points
        .iter()
        .map(|p| BVector::new(p.x(), 0.0, p.y()))
        .collect();
    assert_eq!(out.points, points);
    assert_eq!(out.points.len(), 4);
    // off the map: emptied too, 4
    let r = short_path_for(
        walker,
        Point::new(4.5, 5.5),
        &mut world,
        &mut plan,
        (BVector::new(20.5, 0.0, 5.5), Distance::ZERO, None),
        &[object_id(9171)],
        &mut out,
    );
    assert_eq!((r, out.points.len()), (Ok(ShortFound::Invalid), 0));
    let small = Walker {
        radius: Distance::new(0.5),
        ..walker
    };
    let r = short_path_for(
        small,
        Point::new(4.5, 5.5),
        &mut world,
        &mut plan,
        goal,
        &[object_id(9171)],
        &mut out,
    );
    assert_eq!(
        r,
        Err(GenieError::unsupported(
            "a short path without the game's mesh"
        ))
    );
}

/// A step whose one hit is the walked level's target counts its waypoint reached; that hit,
/// kept, is kept all the same: the first hit loop takes it into the include list and gives
/// it its temporary shape (0x1411547c0..0x141154855) before the test (0x1411549ac).
#[test]
fn a_kept_hit_on_the_target_alone_is_kept_and_reaches_the_waypoint() {
    let mut w = world();
    let mut close = *w.get(object_id(9175)).unwrap();
    close.position = Position::new(28.3, 88.9, 0.0);
    w.insert(object_id(9175), close);
    w.set_gait(
        object_id(9175),
        Some(Gait {
            last_move: WorldTime::from_millis(16361),
            ..Gait::default()
        }),
    );
    let me = Me {
        id: object_id(9177),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    let mut m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    m.paths[0].target = Some(object_id(9175));
    let position = m.moved_at.vector();
    let waypoint = m.paths[0].point(WaypointIndex::of_data(1));
    let projected = m.projected(m.direction_to(position, waypoint), Seconds::new(DT));
    let avoided = m.avoid(
        &me,
        (&w, None),
        &[RecordId::from(object_id(9175))],
        projected,
        Seconds::new(DT),
        (WorldTime::from_millis(16361), false),
    );
    let avoided = avoided.unwrap();
    assert!(avoided.at_target);
    assert_eq!(
        (avoided.kept, avoided.pauses),
        (vec![object_id(9175)], Vec::new())
    );
}

/// Slot 0x5d0 ([`Mover::stop`]): the unit's speed and acceleration along its forward vector, its
/// wait to move and its ignore list go, and its animation speed (`+0x7b8`) is 0 within the call,
/// so the rest of the object's update and the objects after it see it standing (425789782's
/// villager 2785, stopped in update 404, which builder 2787 tests in update 405).
#[test]
fn a_stop_clears_the_animation_speed_within_the_call() {
    let mut m = builder_9177();
    m.animated.speed = 0.8;
    m.waiting = 1;
    m.ignore_units = vec![object_id(9175)];
    if let Some(v) = m.velocity.as_mut() {
        v.velocity = v.velocity.with_y(0.5);
        v.acceleration = v.acceleration.with_y(0.1);
    }
    m.stop().unwrap();
    assert_eq!(
        (m.animated.speed, m.waiting, m.ignore_units.len()),
        (0.0, 0, 0)
    );
    assert_eq!(
        m.velocity.map(|v| (v.velocity.y(), v.acceleration.y())),
        Some((0.0, 0.0))
    );
    // a master whose move algorithm has bit 0 also resets the sprite, which is not modelled
    let mut resets = builder_9177();
    resets.stats.move_algorithm |= MoveAlgorithm::ALTERNATES_SPRITES;
    assert!(resets.stop().is_err());
}

/// The walk's kept hits ([`Mover::go_round`]): with a parent the walked level goes and its
/// parent asks for a path again (state 2); a walk without one (a group member's own path,
/// level 0 from slot 0x540) stops the unit (slot 0x5d0) and the step gives up (1).
#[test]
fn kept_hits_replan_the_parent_or_stop_a_walk_without_one() {
    let w = world();
    let me = Me {
        id: object_id(9177),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    let mut m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    assert_eq!(m.paths.len(), 2);
    assert!(matches!(
        m.go_round(&me, &w, &[RecordId::from(object_id(9175))], false),
        Ok(Next::Again)
    ));
    assert_eq!((m.paths.len(), m.paths[0].state), (1, PathState::WantsPath));
    let mut lone = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    lone.paths.truncate(1);
    lone.animated.speed = 1.0;
    assert!(matches!(
        lone.go_round(&me, &w, &[RecordId::from(object_id(9175))], false),
        Ok(Next::Done(Motion::Done))
    ));
    assert_eq!(lone.animated.speed, 0.0);
    assert_eq!(lone.velocity.map(|v| v.velocity.y()), Some(0.0));
}

/// A map edge's record (id `x << 12 | y | 0xff000000`, [`Spatial::edge_records`]) is a hit
/// without an object: the hit loops pass over it (`FUN_1415417a0` finds none,
/// 0x141154709), while it counts among the hits (`local_1758`'s count, 0x14115488a): the
/// walked level's target alone among two hits is no lone target.
#[test]
fn a_map_edge_s_record_is_a_hit_without_an_object() {
    let w = world();
    let me = Me {
        id: object_id(9177),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    let mut m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    m.paths[0].target = Some(object_id(9175));
    let position = m.moved_at.vector();
    let waypoint = m.paths[0].point(WaypointIndex::of_data(1));
    let projected = m.projected(m.direction_to(position, waypoint), Seconds::new(DT));
    let edge = RecordId::Edge(Tile::new(5, 3));
    let avoided = m
        .avoid(
            &me,
            (&w, None),
            &[edge],
            projected,
            Seconds::new(DT),
            (WorldTime::from_millis(16361), false),
        )
        .unwrap();
    assert_eq!(
        (avoided.kept, avoided.pauses, avoided.at_target),
        (Vec::new(), Vec::new(), false)
    );
    let avoided = m
        .avoid(
            &me,
            (&w, None),
            &[edge, RecordId::from(object_id(9175))],
            projected,
            Seconds::new(DT),
            (WorldTime::from_millis(16361), false),
        )
        .unwrap();
    assert!(!avoided.at_target);
}

/// The walk's kept hits join the motion step's include list (`DAT_143f4a000`), at most 0x100
/// (`CMP RCX, 0x100; JGE` at 0x1411547cc): each it takes without a mesh shape of its own
/// gets a temporary one; one past the list's end gets neither.
#[test]
fn kept_hits_join_the_include_list_up_to_its_room() {
    let w = world();
    let mut pathing = Pathing::default();
    pathing.start_mesh(120);
    let mut include = vec![object_id(0); INCLUDED - 1];
    let shapes = |p: &Pathing| p.mesh().unwrap().shapes.len();
    let before = shapes(&pathing);
    assert_eq!(
        include_kept(
            &w,
            Some(&mut pathing),
            &[object_id(9175), object_id(9177)],
            &mut include
        ),
        Ok(())
    );
    assert_eq!(
        (include.len(), include.last()),
        (INCLUDED, Some(&object_id(9175)))
    );
    assert_eq!(shapes(&pathing), before + 1);
    // an object with a shape of its own gets none
    pathing.place_shape(object_id(9177), w.get(object_id(9177)).unwrap());
    let mut include = Vec::new();
    let before = shapes(&pathing);
    assert_eq!(
        include_kept(&w, Some(&mut pathing), &[object_id(9177)], &mut include),
        Ok(())
    );
    assert_eq!((include, shapes(&pathing)), (vec![object_id(9177)], before));
}

/// The motion step's path request of a unit wider than half a tile (master `+0x5d8` 0.6, no
/// planner: the straight way, too short): the short pather is the old one (`FUN_140f9e540`
/// at 0x1411524b6 -> `FUN_140f85580`) with the walk's kept hits as its include list
/// (`DAT_143f4a000`): a moving unit in the way counts only when included, and the new level
/// goes round it.
#[test]
fn a_big_unit_s_path_request_takes_the_old_pather_with_the_walk_s_kept_hits() {
    let mut w = Spatial::with_tiles(
        GridSize::new(20, 20),
        crate::map::spatial::Tiles::new(crate::map::GridSize::new(20, 20), vec![4; 400]),
    );
    let unit = |x: f32, r: f32| Placed {
        position: Position::new(x, 5.5, 0.0),
        radius: Footprint::new(r, r),
        class: class::VILLAGER,
        player: PlayerId::new(2),
        master: ids::ID_0,
        obstruction_type: ObstructionType::CIRCLE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: Some(Obstruction {
            kind: ObstructionType::CIRCLE,
            class: ObstructionClass::UNIT,
        }),
    };
    w.insert(object_id(9171), unit(4.5, 0.6));
    w.insert(object_id(50), unit(6.5, 0.3));
    w.set_gait(
        object_id(50),
        Some(Gait {
            moving: true,
            ..Gait::default()
        }),
    );
    let me = Me {
        id: object_id(9171),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    let mut pathing = Pathing::default();
    let mut object = Well512::default();
    let groups = crate::groups::UnitGroups::default();
    let mut plan = Planning {
        pathing: &mut pathing,
        now: WorldTime::from_millis(100),
        computer: false,
        object: &mut object,
        groups: &groups,
    };
    let goal = Goal {
        target: None,
        gap: Distance::new(FAR),
        to_end: FAR,
    };
    let mut request = |include: &[ObjectId]| {
        let mut m = builder_9177().placed(Position::new(4.5, 5.5, 0.0));
        m.stats.radius = Radii::from([0.6; 3]);
        m.moved_at = Position::new(4.5, 5.5, 0.0);
        m.request_path(
            Position::new(4.5, 5.5, 0.0),
            Position::new(8.5, 5.5, 0.0),
            Distance::ZERO,
            None,
            WorldTime::from_millis(100),
        );
        let mut plans = 0;
        assert!(
            m.request(&me, &mut w, &mut plan, goal, &mut plans, include)
                .is_ok()
        );
        m.paths[0].points.clone()
    };
    assert_eq!(
        request(&[]),
        [BVector::new(4.5, 0.0, 5.5), BVector::new(8.5, 0.0, 5.5)]
    );
    let points = request(&[object_id(50)]);
    let [start, by, end] = points[..] else {
        panic!("{points:?}")
    };
    assert_eq!(
        (start, end),
        (BVector::new(4.5, 0.0, 5.5), BVector::new(8.5, 0.0, 5.5))
    );
    let off = (0.3 + 0.6) * std::f32::consts::SQRT_2;
    assert!((by.x() - 6.5).abs() < 1e-5 && (by.y() - (5.5 - off)).abs() < 1e-5);
}

/// The house the builders of `world` walk to.
fn world_target() -> Placed {
    *world().get(object_id(10028)).unwrap()
}

/// `FUN_14114ffc0` -> `FUN_140fa0370`: the reach test starts at +0x824, where the unit last
/// moved to: a unit whose +0x824 lies off the map reaches nothing, whatever its place.
#[test]
fn the_reach_test_starts_where_the_unit_last_moved_to() {
    let mut world = Spatial::with_tiles(
        GridSize::new(40, 40),
        crate::map::spatial::Tiles::new(crate::map::GridSize::new(40, 40), vec![4; 40 * 40]),
    );
    world.insert(
        object_id(9177),
        Placed {
            position: Position::new(5.5, 5.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            player: PlayerId::new(2),
            master: ids::VILLAGER_MALE,
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: true,
            speed: Speed::new(0.8),
            unknown: None,
            obstruction: Some(Obstruction {
                kind: ObstructionType::CIRCLE,
                class: ObstructionClass::UNIT,
            }),
        },
    );
    let mut pathing = Pathing::default();
    let mut object = Well512::default();
    let groups = crate::groups::UnitGroups::default();
    let mut plan = Planning {
        pathing: &mut pathing,
        now: WorldTime::ZERO,
        computer: false,
        object: &mut object,
        groups: &groups,
    };
    let me = Me {
        id: object_id(9177),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    let mut m = builder_9177().placed(Position::new(5.5, 5.5, 0.0));
    m.moved_at = Position::new(5.5, 5.5, 0.0);
    // 25 tiles away on open land: the long pather's answer
    let far = Point::new(30.5, 5.5);
    assert_eq!(
        m.reaches(
            &me,
            &mut world,
            &mut plan,
            far,
            Distance::ZERO,
            ids::VILLAGER_MALE
        ),
        Ok(true)
    );
    // with ignore units the long pather lowers those that wait or move (none here)
    m.ignore_units = vec![object_id(5), object_id(6)];
    assert_eq!(
        m.reaches(
            &me,
            &mut world,
            &mut plan,
            far,
            Distance::ZERO,
            ids::VILLAGER_MALE
        ),
        Ok(true)
    );
    m.moved_at = Position::new(-1.0, 5.5, 0.0);
    assert_eq!(
        m.reaches(
            &me,
            &mut world,
            &mut plan,
            far,
            Distance::ZERO,
            ids::VILLAGER_MALE
        ),
        Ok(false)
    );
}

/// `FUN_14114f630`: an angle at most the turn's most, or a NaN most (an infinite turn rate
/// times dt 0), turns the whole angle and is not clipped (JBE on unordered).
#[test]
fn a_nan_most_turns_the_whole_angle_unclipped() {
    let mut m = builder_9177();
    m.forward = BVector::new(1.0, 0.0, 0.0);
    m.right = BVector::new(0.0, 0.0, -1.0);
    m.stats.turn_rate = TurnRate::new(f32::INFINITY);
    assert_eq!(
        m.steer(BVector::new(0.0, 0.0, 1.0), Seconds::ZERO),
        Ok(false)
    );
    assert!(m.forward.get().iter().all(|c| c.is_finite()));
    assert!(0.99 < m.forward.y());
    // a small most clips the turn
    m.forward = BVector::new(1.0, 0.0, 0.0);
    m.stats.turn_rate = TurnRate::new(1.0);
    assert_eq!(
        m.steer(BVector::new(0.0, 0.0, 1.0), Seconds::new(0.02)),
        Ok(true)
    );
    assert!(m.forward.y().abs() < 0.03);
}

/// Slot 195 (`FUN_14114f630`) from a forward vector it is given: a quarter turn to the left
/// of +x (toward +z) is negative, clipped to the turn rate times dt; the fast turn's dt is a
/// hundred times longer.
#[test]
fn a_turn_toward_a_direction_is_signed_and_clipped_by_the_turn_rate() {
    let mut m = builder_9177();
    m.stats.turn_rate = TurnRate::new(1.0);
    let (x, z) = (BVector::new(1.0, 0.0, 0.0), BVector::new(0.0, 0.0, 1.0));
    assert_eq!(
        m.turn_toward(x, z, Seconds::new(0.5)),
        (Angle::new(-0.5), true)
    );
    assert_eq!(
        m.turn_toward(z, x, Seconds::new(0.5)),
        (Angle::new(0.5), true)
    );
    let (turn, clipped) = m.turn_toward(x, z, Seconds::new(2.0));
    assert!(!clipped);
    assert_eq!(turn, Angle::new(-std::f32::consts::FRAC_PI_2));
    // a NaN most turns the whole angle (JBE on unordered)
    assert!(!m.turn_toward(x, z, Seconds::new(f32::NAN)).1);
    assert_eq!(m.turn_dt(Seconds::new(0.02)), Seconds::new(0.02));
    m.stats.fast_turn = true;
    assert_eq!(m.turn_dt(Seconds::new(0.02)), Seconds::new(0.02 * 100.0));
}

/// Slot 184 (`FUN_141163140`): the yaw turns the forward vector about the up axis and
/// rebuilds the right one (slot 173); the angle is the new facet's. Without facets it stops
/// after the vectors.
#[test]
fn a_yaw_turns_forward_and_right_and_takes_the_facet_s_angle() {
    let mut m = builder_9177();
    m.forward = BVector::new(1.0, 0.0, 0.0);
    m.right = BVector::new(0.0, 0.0, -1.0);
    let facets = crate::objects::facing::SpriteFacets {
        count: crate::objects::FacetCount::of_data(16),
        turns: true,
    };
    m.yaw(Angle::new(-std::f32::consts::FRAC_PI_2), Some(facets))
        .unwrap();
    assert!(m.forward.x().abs() < 1e-6 && (m.forward.y() - 1.0).abs() < 1e-6);
    assert_eq!(m.right, fmath::normalize(fmath::cross(UP, m.forward)));
    let facet = crate::objects::facing::facet_of_forward(m.forward, facets);
    assert_eq!(
        m.angle,
        crate::objects::facing::angle_of_facet(facet, facets)
    );
    let before = m.angle;
    assert!(m.yaw(Angle::new(0.1), None).is_err());
    assert_eq!(m.angle, before);
    assert!(m.forward.x() != 0.0);
}

/// Slot 173 (`FUN_14115d150`) sets the forward vector and the right one from the given up
/// vector, the angle left; slot 0x598 (`FUN_14115d050`) makes zeroed velocity data for a
/// speed other than 0 (a NaN too) and sets its forward speed; slot 0x600 (`FUN_14114d3b0`)
/// reads the walked level's pause.
#[test]
fn the_orientation_speed_and_pause_slots() {
    let mut m = builder_9177();
    m.angle = Angle::new(1.0);
    m.set_orientation(BVector::new(0.0, 0.0, 1.0), BVector::new(0.0, 1.0, 0.0));
    assert_eq!(m.forward, BVector::new(0.0, 0.0, 1.0));
    assert_eq!(m.right, BVector::new(1.0, 0.0, 0.0));
    assert_eq!(m.angle, Angle::new(1.0));
    m.velocity = None;
    m.set_speed(Speed::ZERO);
    assert_eq!(m.velocity, None);
    m.set_speed(Speed::new(f32::NAN));
    assert!(m.velocity.unwrap().velocity.y().is_nan());
    m.velocity = None;
    m.set_speed(Speed::new(1.2));
    assert_eq!(
        m.velocity,
        Some(Velocity {
            velocity: BVector::new(0.0, 0.0, 1.2),
            acceleration: BVector::ZERO,
        })
    );
    m.velocity.as_mut().unwrap().acceleration = BVector::new(0.0, 0.0, 5.0);
    m.set_speed(Speed::ZERO);
    assert_eq!(m.velocity.unwrap().velocity.y(), 0.0);
    assert_eq!(m.velocity.unwrap().acceleration.y(), 5.0);
    m.paths = Vec::new();
    assert!(!m.paused());
    m.paths = vec![PathData::without_path()];
    assert!(!m.paused());
    m.paths[0].pause_time = Seconds::new(0.5);
    assert!(m.paused());
    m.paths[0].pause_time = Seconds::new(f32::NAN);
    assert!(!m.paused());
}

/// `LAB_141152ec1`: a waypoint a tile away, or a target a tile from the unit, or a near one of
/// master 45, is the long pather's.
#[test]
fn a_far_waypoint_or_a_target_of_master_45_takes_the_long_pather() {
    assert!(far_waypoint(Distance::new(1.0), None) && !far_waypoint(Distance::new(0.5), None));
    assert!(far_waypoint(
        Distance::ZERO,
        Some((Distance::new(1.0), master_id(4)))
    ));
    assert!(!far_waypoint(
        Distance::new(30.0),
        Some((Distance::new(0.5), master_id(4)))
    ));
    assert!(far_waypoint(
        Distance::new(30.0),
        Some((Distance::new(0.5), ID_45))
    ));
    assert!(!far_waypoint(
        Distance::new(30.0),
        Some((Distance::new(f32::NAN), master_id(4)))
    ));
}

/// `FUN_141161950`: a moving unit's footprint stays 0.01 inside the map, its y by the map's
/// width as its x (the game reads `+0x18` for both).
#[test]
fn the_position_setter_holds_y_by_the_map_s_width() {
    let m = builder_9177();
    // a map 40 wide and 120 high: y 100 lies beyond the width less the radius and the gap
    let at = m.set_position(Position::new(10.0, 100.0, 0.0), GridSize::new(40, 120));
    assert_eq!(at.y(), (40.0f32 - 0.2) - EDGE_GAP);
    // a unit that does not move is not held
    let mut still = builder_9177();
    still.stats.speed = Speed::ZERO;
    assert_eq!(
        still
            .set_position(Position::new(10.0, 100.0, 0.0), GridSize::new(40, 120))
            .y(),
        100.0
    );
}

/// `FUN_140f98bc0` with 1: a loop's next point or 0 past its end; a back-and-forth path turns
/// at its ends and flips bit 2; a path that does not loop has none.
#[test]
fn a_looping_path_s_next_waypoint_goes_round_or_back_and_forth() {
    let mut l = level(
        1,
        &[
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0),
        ],
        PathState::WantsPath,
    );
    l.path_flags = PathFlags::NONE;
    assert_eq!(
        l.looping_next(WaypointIndex::of_data(1)),
        WaypointIndex::of_data(-1)
    );
    l.path_flags = PathFlags::LOOPS;
    assert_eq!(
        (
            l.looping_next(WaypointIndex::of_data(1)).get(),
            l.looping_next(WaypointIndex::of_data(2)).get()
        ),
        (2, 0)
    );
    l.path_flags = PathFlags::LOOPS | PathFlags::BACK_AND_FORTH;
    assert_eq!(
        l.looping_next(WaypointIndex::of_data(1)),
        WaypointIndex::of_data(0)
    );
    assert_eq!(
        (
            l.looping_next(WaypointIndex::of_data(0)).get(),
            l.path_flags
        ),
        (
            1,
            PathFlags::LOOPS | PathFlags::BACK_AND_FORTH | PathFlags::FORWARD
        )
    );
    assert_eq!(
        l.looping_next(WaypointIndex::of_data(1)),
        WaypointIndex::of_data(2)
    );
    assert_eq!(
        (
            l.looping_next(WaypointIndex::of_data(2)).get(),
            l.path_flags
        ),
        (1, PathFlags::LOOPS | PathFlags::BACK_AND_FORTH)
    );
}

/// `LAB_141151b6d`: an arrived level of link 2 hands its looping parent its next waypoint;
/// one of another link stays and the step walks on.
#[test]
fn an_arrived_level_hands_a_looping_parent_its_next_waypoint() {
    let goal = Goal {
        target: None,
        gap: Distance::new(FAR),
        to_end: FAR,
    };
    let mut m = builder_9177();
    m.paths[0].linked_type = 2;
    m.paths[1].path_flags = PathFlags::LOOPS;
    m.paths[1].waypoint = WaypointIndex::of_data(1);
    assert!(matches!(m.arrived(goal), Ok(Next::Again)));
    assert_eq!(
        (m.paths.len(), m.paths[0].state, m.paths[0].waypoint.get()),
        (1, PathState::WantsPath, 0)
    );
    let mut m = builder_9177();
    m.paths[0].linked_type = 0;
    assert!(matches!(m.arrived(goal), Ok(Next::Dispatch)));
    assert_eq!(m.paths.len(), 2);
}

/// `LAB_14115136c` on a loop (the path's flags bit 0): the level moves on from the waypoint
/// reached (`FUN_140f98bc0` with 1) and waits for a path (state 2), on to its state; back at
/// its first waypoint the step ends (`LAB_141155637`): the unit turns toward the direction
/// the step's walk last took (zero without one) and, its level in state 2, stops.
#[test]
fn a_loop_s_reached_waypoint_moves_on_and_its_first_ends_the_step() {
    let looping = |waypoint| {
        let mut m = builder_9177().placed(Position::new(30.0, 88.0, 0.0));
        m.paths = vec![level(
            2,
            &[
                Point::new(28.0, 89.0),
                Point::new(30.0, 88.0),
                Point::new(32.0, 89.0),
            ],
            PathState::Walking,
        )];
        let l = &mut m.paths[0];
        l.target = None;
        l.path_flags = PathFlags::LOOPS;
        l.waypoint = waypoint;
        l.flags = LevelFlags::REACHED;
        m
    };
    let mut world = Spatial::new(GridSize::new(120, 120));
    let me = Me {
        id: object_id(9177),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    let goal = Goal {
        target: None,
        gap: Distance::new(FAR),
        to_end: FAR,
    };
    let mut m = looping(WaypointIndex::of_data(1));
    let next = m.reached(&me, &mut world, None, goal, WorldTime::ZERO);
    assert!(matches!(next, Ok(Next::Dispatch)));
    assert_eq!(
        (m.paths[0].waypoint.get(), m.paths[0].state),
        (2, PathState::WantsPath)
    );
    let mut m = looping(WaypointIndex::of_data(2));
    let next = m.reached(&me, &mut world, None, goal, WorldTime::ZERO);
    assert!(matches!(next, Ok(Next::LoopEnd)));
    assert_eq!(
        (m.paths[0].waypoint.get(), m.paths[0].state),
        (0, PathState::WantsPath)
    );
    let mut m = looping(WaypointIndex::of_data(2));
    m.place_for_test(Position::new(32.0, 89.0, 0.0));
    m.moved_at = m.position();
    // no walk ran in the step: the direction it turns toward is zero, at π/2 (426048706's
    // scout 7227 in update 5429 turns by π/2 there)
    let mut expect = m.clone();
    let flat = fmath::normalize(expect.forward.with_height(0.0));
    let (turn, _) = expect.turn_toward(flat, BVector::ZERO, expect.turn_dt(Seconds::new(DT)));
    expect.yaw(turn, expect.facets()).unwrap();
    assert_ne!(turn, Angle::ZERO);
    assert_eq!(
        m.motion(&me, Seconds::new(DT), &mut world, None),
        Ok(Motion::Moving)
    );
    assert_eq!(
        (m.paths[0].waypoint.get(), m.paths[0].state),
        (0, PathState::WantsPath)
    );
    assert_eq!(m.velocity.unwrap().velocity, BVector::ZERO);
    assert_eq!(m.forward, expect.forward);
}

/// `0x141150f00`: a level whose target is gone walks as to a point (`FUN_1415417a0` finds
/// none, `LAB_1411512df`), as one with no target does.
#[test]
fn a_walk_whose_target_is_gone_walks_as_to_a_point() {
    let walk = |target: i32| {
        let mut m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
        for l in &mut m.paths {
            l.target = ObjectId::of_field(target);
            l.range = Distance::ZERO;
        }
        let me = Me {
            id: object_id(9926),
            group: None,
            action_state: ActionState::NO_ACTION,
        };
        let motion = m.motion(
            &me,
            Seconds::new(DT),
            &mut Spatial::new(GridSize::new(120, 120)),
            None,
        );
        (motion, m.forward, m.velocity)
    };
    assert_eq!(walk(77), walk(-1));
    assert_eq!(walk(77).0, Ok(Motion::Moving));
}

/// The motion step's gate gaps (0x1411512b3, 0x141151a62, 0x141154f22): a gate's gap is the
/// least of its own and its first two annexes' through the site's terms (`MINSS`); any other
/// target's is its own.
#[test]
fn a_gate_s_gap_is_the_least_over_it_and_its_annexes() {
    let mut world = Spatial::new(GridSize::new(40, 40));
    let wall = |x: f32, class: ClassId| Placed {
        position: Position::new(x, 10.0, 0.0),
        radius: Footprint::new(0.5, 0.5),
        class,
        player: PlayerId::new(1),
        master: ids::ID_0,
        obstruction_type: ObstructionType::TYPE_2,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: Some(Obstruction {
            kind: ObstructionType::TYPE_2,
            class: ObstructionClass::RESOURCE,
        }),
    };
    world.insert(object_id(100), wall(10.0, class::GATE));
    world.insert(object_id(101), wall(12.0, class::WALL));
    world.insert(object_id(102), wall(8.0, class::WALL));
    world.set_annex(object_id(101), Some(object_id(100)));
    world.set_annex(object_id(102), Some(object_id(100)));
    let near = (Position::new(13.5, 10.0, 0.0), Footprint::new(0.2, 0.2));
    let gate = *world.get(object_id(100)).unwrap();
    let own = footprint_gap(near.0, near.1, gate.position, gate.radius);
    assert!((own - Distance::new(2.8)).get().abs() < 1e-6);
    // annex 101 lies 0.8 away, less 0.1
    let gap = gate_gap(&world, near, (object_id(100), &gate), own, |g| {
        g - ANNEX_GAP
    });
    let annex = footprint_gap(
        near.0,
        near.1,
        Position::new(12.0, 10.0, 0.0),
        Footprint::new(0.5, 0.5),
    );
    assert!((annex - Distance::new(0.8)).get().abs() < 1e-6);
    assert_eq!(gap, (annex - Distance::new(ANNEX_GAP)).get());
    // not a gate: its own
    let mut post = gate;
    post.class = class::WALL;
    assert_eq!(
        gate_gap(&world, near, (object_id(100), &post), own, |g| g - 1.0),
        own.get()
    );
}

/// `FUN_140f98d50` and `FUN_140f98dd0`: a point goes, or the path ends before it and is
/// measured again; an index outside the path, or an empty path, changes nothing.
#[test]
fn a_path_loses_a_point_or_its_end_only_for_an_index_inside_it() {
    let mut points = vec![
        BVector::ZERO,
        BVector::new(1.0, 0.0, 0.0),
        BVector::new(1.0, 0.0, 2.0),
    ];
    assert!(!remove_point(&mut points, WaypointIndex::of_data(-1)));
    assert!(!remove_point(&mut points, WaypointIndex::of_data(3)));
    assert!(remove_point(&mut points, WaypointIndex::of_data(1)));
    assert_eq!(points, [BVector::ZERO, BVector::new(1.0, 0.0, 2.0)]);
    let mut path = Path {
        points: vec![
            BVector::ZERO,
            BVector::new(3.0, 0.0, 0.0),
            BVector::new(3.0, 0.0, 4.0),
            BVector::ZERO,
        ],
        length: Distance::new(12.0),
    };
    assert!(!cut_path(&mut path, -1));
    assert!(!cut_path(&mut path, 4));
    assert_eq!(path.length, Distance::new(12.0));
    assert!(cut_path(&mut path, 3));
    assert_eq!((path.points.len(), path.length), (3, Distance::new(7.0)));
    assert!(cut_path(&mut path, 0));
    assert_eq!((path.points.len(), path.length), (0, Distance::ZERO));
    assert!(!cut_path(&mut path, 0));
}

/// `FUN_140f96050`: the top level decides, not the walked one; a level without a path has
/// no more; else `count - 1 <= waypoint` in 32 bits says none.
#[test]
fn the_top_level_has_more_waypoints_unless_its_count_less_one_reaches_its_index() {
    let mut m = builder_9177();
    // the top level's two points, its waypoint 1: none after it, whatever the walked level has
    m.paths[0].waypoint = WaypointIndex::of_data(0);
    assert!(!m.top_has_more());
    m.paths[1].waypoint = WaypointIndex::of_data(0);
    assert!(m.top_has_more());
    // without a path: none
    m.paths[1].has_path = false;
    assert!(!m.top_has_more());
    // an empty path: -1 <= waypoint, but a waypoint below -1 has more
    m.paths[1].has_path = true;
    m.paths[1].points.clear();
    assert!(!m.top_has_more());
    m.paths[1].waypoint = WaypointIndex::of_data(-2);
    assert!(m.top_has_more());
    m.paths.clear();
    assert!(!m.top_has_more());
}

/// `FUN_140f9b8f0` compares `i` with the point count less one, signed: an empty path or the
/// last point has no stretch to split; any other stretch gets its midpoint.
#[test]
fn a_split_takes_the_midpoint_of_a_stretch_and_skips_an_empty_path() {
    let mut empty = Path::default();
    split(&mut empty, 0);
    assert_eq!(empty.points, Vec::<BVector>::new());
    let mut path = Path {
        points: vec![BVector::ZERO, BVector::new(4.0, 0.0, 0.0)],
        length: Distance::new(4.0),
    };
    split(&mut path, 1);
    assert_eq!(path.points.len(), 2);
    split(&mut path, 0);
    assert_eq!(
        path.points,
        [
            BVector::ZERO,
            BVector::new(2.0, 0.0, 0.0),
            BVector::new(4.0, 0.0, 0.0)
        ]
    );
}

/// Slot 196 (`FUN_14114f400`) at 0x14114f520..0x14114f52d: a NaN distance to the waypoint
/// goes past the first test and stops at the second (JBE on unordered): short.
#[test]
fn a_nan_waypoint_distance_is_short() {
    let w = BVector::new(f32::NAN, 0.0, 5.0);
    assert!(matches!(
        reach(w, BVector::ZERO, BVector::new(1.0, 0.0, 0.0)),
        Reach::Short
    ));
    assert!(matches!(
        reach(
            BVector::new(5.0, 0.0, 0.0),
            BVector::ZERO,
            BVector::new(1.0, 0.0, 0.0)
        ),
        Reach::Short
    ));
    assert!(matches!(
        reach(
            BVector::new(5.0, 0.0, 0.0),
            BVector::ZERO,
            BVector::new(5.0, 0.0, 0.0)
        ),
        Reach::Reached
    ));
}

/// The ground direction (`FUN_14114f210`): a NaN length divides (JBE on unordered), so the
/// direction is NaN, not the forward vector.
#[test]
fn a_direction_of_a_nan_length_is_nan() {
    let m = crate::gathering::hunt::tests::walker();
    let d = m.direction_to(
        BVector::new(f32::NAN, 0.0, 0.0),
        BVector::new(1.0, 0.0, 1.0),
    );
    assert!(d.x().is_nan() && d.y().is_nan());
    let d = m.direction_to(BVector::new(0.0, 0.0, 0.0), BVector::new(3.0, 0.0, 4.0));
    assert_eq!(d, BVector::new(0.6, 0.0, 0.8));
}

#[test]
fn two_boxes_overlap_by_the_lesser_of_their_axes() {
    // the cow 9517 at (21.8, 92.2) and Gaia's cow 9519 at (22.5, 91.5), radii 0.5 and 0.5
    let a = BVector::new(21.8, 0.0, 92.2);
    let b = BVector::new(22.5, 0.0, 91.5);
    let d = overlap(a, Footprint::new(0.5, 0.5), b, Footprint::new(0.5, 0.5));
    assert_eq!(d, (21.8f32 + 0.5) - (22.5 - 0.5));
    assert_eq!(
        overlap(a, Footprint::new(0.2, 0.2), b, Footprint::new(0.2, 0.2)),
        0.0
    );
    // a NaN goes past the four tests (COMISS/JNC) into MINSS/MAXSS, whose second operand wins
    let nan = overlap(
        BVector::new(f32::NAN, 0.0, 92.2),
        Footprint::new(0.5, 0.5),
        b,
        Footprint::new(0.5, 0.5),
    );
    assert_eq!(
        nan,
        fmath::minss(
            (22.5f32 + 0.5) - (22.5 - 0.5),
            (91.5f32 + 0.5) - (92.2 - 0.5)
        )
    );
}

#[test]
fn level_0_looks_ahead_to_the_farthest_parent_waypoint_it_walks_to_straight() {
    // a cow at its level 0 waypoint (21.8, 92.2), its level 1 parent's waypoint 1 of three
    let mut m = builder_9177().placed(Position::new(21.8, 92.2, 0.0));
    m.moved_at = Position::new(21.8, 92.2, 0.0);
    m.paths = vec![
        level(
            0,
            &[
                Point::new(21.5, 91.5),
                Point::new(21.8, 92.2),
                Point::new(25.15, 92.85),
            ],
            PathState::Walking,
        ),
        level(
            1,
            &[
                Point::new(21.5, 91.5),
                Point::new(28.5, 93.5),
                Point::new(28.9, 94.1),
                Point::new(40.0, 94.0),
            ],
            PathState::Idle,
        ),
    ];
    m.paths[1].waypoint = WaypointIndex::of_data(1);
    let me = Me {
        id: object_id(9517),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    // land of a terrain class the unit's mask (6) has
    let mut w = Spatial::with_tiles(
        GridSize::new(120, 120),
        crate::map::spatial::Tiles::new(crate::map::GridSize::new(120, 120), vec![2; 14400]),
    );
    // on open ground: waypoint 3 is beyond 10 tiles, waypoint 2 within, so it takes 2
    assert_eq!(m.look_ahead(&me, &mut w), Ok(true));
    assert_eq!(m.paths.len(), 1);
    assert_eq!(
        (
            m.paths[0].level.get(),
            m.paths[0].waypoint.get(),
            m.paths[0].state
        ),
        (1, 2, PathState::WantsPath)
    );
    // a tree in the way of both leaves the levels as they are
    let mut m = builder_9177().placed(Position::new(21.8, 92.2, 0.0));
    m.moved_at = Position::new(21.8, 92.2, 0.0);
    m.paths = vec![
        level(
            0,
            &[Point::new(21.5, 91.5), Point::new(21.8, 92.2)],
            PathState::Walking,
        ),
        level(
            1,
            &[
                Point::new(21.5, 91.5),
                Point::new(28.5, 93.5),
                Point::new(28.9, 94.1),
            ],
            PathState::Idle,
        ),
    ];
    m.paths[1].waypoint = WaypointIndex::of_data(1);
    w.insert(
        object_id(9357),
        Placed {
            position: Position::new(25.0, 93.0, 0.0),
            radius: Footprint::new(0.5, 0.5),
            class: class::TREE,
            player: PlayerId::new(0),
            master: ids::ID_0,
            obstruction_type: ObstructionType::TYPE_2,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: Some(Obstruction {
                kind: ObstructionType::TYPE_2,
                class: ObstructionClass::RESOURCE,
            }),
        },
    );
    assert_eq!(m.look_ahead(&me, &mut w), Ok(false));
    assert_eq!(m.paths.len(), 2);
}

/// A trade cart plans only within its owner's budget; any unit at most three times a step.
#[test]
fn a_trade_cart_plans_within_its_owners_budget() {
    let spent = PlanBudget {
        attempt_cap: 6,
        delay_cap: 2,
        plans: 6,
    };
    assert!(!may_plan(true, Some(spent), 0, 0));
    // a villager is not held to the budget
    assert!(may_plan(false, Some(spent), 0, 0));
    // past the delay cap the budget lets the cart plan again
    assert!(may_plan(true, Some(spent), 3, 0));
    assert!(may_plan(true, Some(PlanBudget { plans: 5, ..spent }), 0, 0));
    assert!(may_plan(true, None, 0, 0));
    assert!(!may_plan(false, None, 0, PLANS));
}

/// `FUN_140f9eb70`'s list for `FUN_140f60440`: a walker in no group lists itself and its
/// ignore units that move (not one that stands, not the target, not one the world lacks),
/// then the target; a walker in a group lists its group's differentiator units instead.
#[test]
fn the_long_pather_lowers_the_walker_or_its_groups_units_and_the_target() {
    let mut s = world();
    let moving = Gait {
        moving: true,
        ..Gait::default()
    };
    s.set_gait(object_id(9175), Some(moving));
    s.set_gait(object_id(10028), Some(moving));
    let ignore = object_id_list([9175, 9177, 10028, 4242]);
    let mut walker = LongWalker {
        id: object_id(9177),
        position: Position::new(28.0, 89.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        mask: TerrainMask::of_bits(1),
        moves: true,
        ignore_units: &ignore,
        group_units: None,
    };
    // 10028 moves but is the target: it comes last, once
    assert_eq!(
        lowered_units(&walker, &s, Some(object_id(10028))),
        [object_id(9177), object_id(9175), object_id(10028)]
    );
    assert_eq!(
        lowered_units(&walker, &s, None),
        [object_id(9177), object_id(9175), object_id(10028)]
    );
    walker.group_units = Some(vec![object_id(9177), object_id(31), object_id(32)]);
    assert_eq!(
        lowered_units(&walker, &s, Some(object_id(10028))),
        [
            object_id(9177),
            object_id(31),
            object_id(32),
            object_id(10028)
        ]
    );
    // a group whose differentiator groups are gone lists none, not even the walker
    walker.group_units = Some(Vec::new());
    assert_eq!(lowered_units(&walker, &s, None), Vec::new());
}

pub(crate) fn world() -> Spatial {
    let mut s = Spatial::new(GridSize::new(120, 120));
    let unit = |x, y| Placed {
        position: Position::new(x, y, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class: class::VILLAGER,
        player: PlayerId::new(2),
        master: ids::ID_0,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: Some(Obstruction {
            kind: ObstructionType::CIRCLE,
            class: ObstructionClass::UNIT,
        }),
    };
    s.insert(object_id(9175), unit(30.0, 89.0));
    s.insert(object_id(9177), unit(28.0, 89.0));
    s.insert(
        object_id(10028),
        Placed {
            position: Position::new(30.0, 88.0, 0.0),
            radius: Footprint::new(1.0, 1.0),
            class: class::BUILDING,
            player: PlayerId::new(2),
            master: ids::ID_0,
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
    s
}

/// [`world`]'s objects on a 120 × 120 map of tiles of `classes` (row by row), which the
/// pather reads.
pub(crate) fn tiled_world(classes: Vec<u8>) -> Spatial {
    let objects = world();
    let tiles = crate::map::spatial::Tiles::new(crate::map::GridSize::new(120, 120), classes);
    let mut s = Spatial::with_tiles(GridSize::new(120, 120), tiles);
    for id in [9175, 9177, 10028] {
        s.insert(
            object_id(id),
            *objects.get(object_id(id)).expect("in the world"),
        );
    }
    s
}

/// `FUN_14114cc60` from where the unit last moved (`+0x824`) to `p` (x, height, y): a clear
/// line closes in; a point off the map, a line through terrain the unit's mask lacks, or one
/// through another unit (9175 at (30, 89)) does not, but for the target, which the query
/// ignores.
#[test]
fn a_unit_closes_in_along_a_clear_straight_line() {
    let m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    let me = Me {
        id: object_id(9177),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    let mut open = tiled_world(vec![4; 120 * 120]);
    assert_eq!(
        m.close_in(&me, BVector::new(28.0, 0.0, 87.0), None, &mut open),
        Ok(true)
    );
    assert_eq!(
        m.close_in(&me, BVector::new(32.0, 0.0, 89.0), None, &mut open),
        Ok(false)
    );
    assert_eq!(
        m.close_in(
            &me,
            BVector::new(32.0, 0.0, 89.0),
            Some(object_id(9175)),
            &mut open
        ),
        Ok(true)
    );
    assert_eq!(
        m.close_in(&me, BVector::new(-1.0, 0.0, 89.0), None, &mut open),
        Ok(false)
    );
    assert_eq!(
        m.close_in(&me, BVector::new(28.0, 0.0, 200.0), None, &mut open),
        Ok(false)
    );
    let mut classes = vec![4; 120 * 120];
    for x in 26..31 {
        classes[88 * 120 + x] = 1;
    }
    let mut walled = tiled_world(classes);
    assert_eq!(
        m.close_in(&me, BVector::new(28.0, 0.0, 87.0), None, &mut walled),
        Ok(false)
    );
}

/// The long path's stretches (`0x141150f00` after `FUN_140f9eb70`): one whose query meets a
/// wall, a gate, a tower or a building that does not move ends the path before it, or, the
/// first, empties it (false); a moving building and a clear path leave it; a villager's path
/// is never cut.
#[test]
fn a_long_path_ends_before_its_first_walled_stretch() {
    let m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    let me = Me {
        id: object_id(9177),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    let path = || Path {
        points: vec![
            BVector::new(28.0, 0.0, 89.0),
            BVector::new(28.0, 0.0, 85.0),
            BVector::new(28.0, 0.0, 80.0),
        ],
        length: Distance::new(9.0),
    };
    let obstacle = |class, speed, y| Placed {
        position: Position::new(28.0, y, 0.0),
        radius: Footprint::new(0.5, 0.5),
        class,
        player: PlayerId::new(1),
        master: master_id(72),
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: true,
        speed,
        unknown: None,
        obstruction: Some(Obstruction {
            kind: ObstructionType::CIRCLE,
            class: ObstructionClass::UNIT,
        }),
    };
    let walled = |soldier: bool, block: Option<Placed>| {
        let mut world = tiled_world(vec![4; 120 * 120]);
        if soldier {
            let mut me = *world.get(object_id(9177)).unwrap();
            me.class = class::CLASS_6;
            world.insert(object_id(9177), me);
        }
        if let Some(b) = block {
            world.insert(object_id(500), b);
        }
        let mut p = path();
        let r = m.walled_off(&me, &mut world, None, &mut p);
        (r, p.points.len())
    };
    assert_eq!(walled(true, None), (Ok(true), 3));
    assert_eq!(
        walled(true, Some(obstacle(class::WALL, Speed::ZERO, 82.0))),
        (Ok(true), 2)
    );
    assert_eq!(
        walled(true, Some(obstacle(class::GATE, Speed::ZERO, 87.0))),
        (Ok(false), 0)
    );
    assert_eq!(
        walled(true, Some(obstacle(class::TOWER, Speed::ZERO, 82.0))),
        (Ok(true), 2)
    );
    let still = obstacle(class::BUILDING, Speed::ZERO, 82.0);
    assert_eq!(walled(true, Some(still)), (Ok(true), 2));
    let moving = obstacle(class::BUILDING, Speed::new(1.0), 82.0);
    assert_eq!(walled(true, Some(moving)), (Ok(true), 3));
    let wall = obstacle(class::WALL, Speed::ZERO, 87.0);
    assert_eq!(walled(false, Some(wall)), (Ok(true), 3));
}

/// The long pather's plan for a far waypoint (`0x141150f00` at `LAB_141152f9c`) from 9177's
/// tile: on open tiles it reaches the waypoint's tile (true); a waypoint on its own tile
/// gives no path, which stops the unit (None); a waypoint in a band its mask lacks (rows 70
/// to 74) ends at the band's edge (false), and farther from the waypoint than `way` allows the
/// path empties and the gap of its end comes back (3.54 here).
#[test]
fn a_long_plan_reaches_its_waypoint_stops_short_or_ends_at_a_band() {
    let mut band = vec![4; 120 * 120];
    for y in 70..75 {
        for x in 20..40 {
            band[y * 120 + x] = 1;
        }
    }
    let plan = |classes: &[u8], waypoint: BVector, way: f32| {
        let mut world = tiled_world(classes.to_vec());
        let mut pathing = Pathing::default();
        pathing.start_mesh(120);
        let mut object = Well512::default();
        let groups = crate::groups::UnitGroups::default();
        let plan = Planning {
            pathing: &mut pathing,
            now: WorldTime::ZERO,
            computer: false,
            object: &mut object,
            groups: &groups,
        };
        let me = Me {
            id: object_id(9177),
            group: None,
            action_state: ActionState::NO_ACTION,
        };
        let mut m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
        let mut path = Path::default();
        let r = m.long_plan(
            &me,
            &mut world,
            &plan,
            LongAsk {
                waypoint,
                way,
                range: Distance::ZERO,
                target: None,
            },
            &mut path,
        );
        (r, path.points, path.length)
    };
    let open = vec![4; 120 * 120];
    assert_eq!(
        plan(&open, BVector::new(28.0, 0.0, 60.0), 100.0),
        (
            Ok(Some((true, None))),
            vec![BVector::new(28.5, 0.0, 89.5), BVector::new(28.5, 0.0, 60.5)],
            Distance::new(29.0)
        )
    );
    assert_eq!(
        plan(&open, BVector::new(28.0, 0.0, 89.0), 100.0).0,
        Ok(None)
    );
    assert_eq!(
        plan(&band, BVector::new(28.0, 0.0, 72.0), 100.0),
        (
            Ok(Some((false, None))),
            vec![BVector::new(28.5, 0.0, 89.5), BVector::new(28.5, 0.0, 75.5)],
            Distance::new(14.0)
        )
    );
    let short = plan(&band, BVector::new(28.0, 0.0, 72.0), 0.5);
    assert_eq!(short.0, Ok(Some((false, Some(12.5f32.sqrt())))));
    assert_eq!((short.1.len(), short.2), (0, Distance::ZERO));
}

/// The new level's path is no path (`0x141150f00` after the plans), on 9177's walked level:
/// a found path in range of its end has its waypoint marked reached on a looping level (else
/// it is not modelled); not found, or out of range, off its last waypoint the waypoint is
/// marked too; at its last waypoint while its continue counter runs the unit pauses for
/// `STUCK_PAUSE` and stops; with the counter out a level 0 under a top path with more waits
/// at its end (state 5); a looping level, an attack move, a level above 1 and a top path
/// without more are not modelled. With a target, bit 2 moves from the level to its parents.
#[test]
fn a_level_without_a_path_marks_its_waypoint_pauses_or_waits() {
    let target = *world().get(object_id(10028)).unwrap();
    let mut m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    m.paths[0].flags = LevelFlags::MET_TARGET;
    assert!(matches!(
        m.no_path(false, Some(&target), 0.5),
        Ok(AfterRequest::Paused)
    ));
    assert_eq!(m.paths[0].pause_time, STUCK_PAUSE);
    assert_eq!(
        (m.paths[0].flags, m.paths[1].flags),
        (LevelFlags::NONE, LevelFlags::MET_TARGET)
    );
    let out = |edit: &dyn Fn(&mut Mover)| {
        let mut m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
        m.paths[0].continue_counter = 0;
        edit(&mut m);
        let r = m.no_path(false, None, 0.5);
        (r, m)
    };
    let (r, m) = out(&|m| m.paths[1].waypoint = WaypointIndex::of_data(0));
    assert!(matches!(r, Ok(AfterRequest::Again)));
    assert_eq!(m.paths[0].state, PathState::Unplanned);
    assert!(out(&|_| {}).0.is_err());
    assert!(
        out(&|m| m.paths[0].path_flags |= PathFlags::LOOPS)
            .0
            .is_err()
    );
    assert!(out(&|m| m.attack_moving = true).0.is_err());
    assert!(out(&|m| m.paths[0].level = WaypointLevel::TOP).0.is_err());
    let (r, m) = out(&|m| m.paths[0].waypoint = WaypointIndex::of_data(0));
    assert!(matches!(r, Ok(AfterRequest::Again)));
    assert_eq!(m.paths[0].flags, LevelFlags::REACHED);
    let mut m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    assert!(m.no_path(true, None, 0.05).is_err());
    m.paths[0].path_flags |= PathFlags::LOOPS;
    assert!(matches!(
        m.no_path(true, None, 0.05),
        Ok(AfterRequest::Again)
    ));
    assert_eq!(m.paths[0].flags, LevelFlags::REACHED);
}

/// A unit's walk for the actions' tests: MarvinBlue/426048731's builder 9177 on its first
/// path toward house 10028 ([`builder_9177`]), its `MoveTo`'s point the house's, on open
/// tiles ([`tiled_world`]) with the pather's mesh, a fresh Object stream and no unit groups.
pub(crate) struct WalkRig {
    pub(crate) body: crate::objects::layers::Body,
    pub(crate) world: Spatial,
    pathing: Pathing,
    object: Well512,
    groups: crate::groups::UnitGroups,
}

impl WalkRig {
    pub(crate) fn on_the_path() -> Self {
        let mover = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
        let mut pathing = Pathing::default();
        pathing.start_mesh(120);
        Self {
            body: crate::objects::layers::Body::new(mover.position(), None, Some(mover)),
            world: tiled_world(vec![4; 120 * 120]),
            pathing,
            object: Well512::default(),
            groups: crate::groups::UnitGroups::default(),
        }
    }

    /// The walking unit's movement state.
    pub(crate) fn mover_mut(&mut self) -> &mut Mover {
        self.body.mover_mut().expect("the rig's unit moves")
    }

    /// The walk of unit 9177 at (28, 89) in action state `state`, a tenth of a second long,
    /// with the world's planners.
    pub(crate) fn walk(&mut self, state: u8) -> Walk<'_> {
        Walk {
            body: &mut self.body,
            me: Me {
                id: object_id(9177),
                group: None,
                action_state: ActionState::new(state),
            },
            dt: Seconds::new(0.1),
            world: &mut self.world,
            plan: Some(Planning {
                pathing: &mut self.pathing,
                now: WorldTime::ZERO,
                computer: false,
                object: &mut self.object,
                groups: &self.groups,
            }),
        }
    }
}

/// [`WalkRig::on_the_path`] as a world for an action's update on the step: Gaia and `owner` as
/// player 1, whose first object is the builder 9177 in state 2 (no pasture) on its path toward
/// house 10028, on the open tiles of [`tiled_world`] with the pather's mesh.
pub(crate) fn path_world(owner: crate::world::Player) -> crate::world::World {
    let players = vec![crate::world::Player::default(), owner];
    let mut t = crate::world::World::new(
        WorldTime::ZERO,
        0,
        PlayerId::new(0),
        vec![Millis::ZERO; 2],
        players,
    );
    t.spatial = tiled_world(vec![4; 120 * 120]);
    t.pathing.start_mesh(120);
    let mover = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    let mut unit = Unit {
        id: object_id(9177),
        pasture: None,
        my_pasture: None,
        body: crate::objects::layers::Body::new(mover.position(), None, Some(mover)),
        ..Unit::default()
    };
    unit.life.state = ObjectState::Alive;
    t.players[1].objects.push(unit);
    // the villager's master (the gatherer tests set it) and the default one
    let master = |id| crate::objects::masters::Master {
        id,
        kind: crate::objects::masters::master_type::COMBAT,
        class: class::VILLAGER,
        ..crate::objects::masters::Master::default()
    };
    let table = crate::objects::masters::Masters::new([
        master(ids::VILLAGER_MALE),
        master(MasterId::default()),
    ]);
    t.masters = vec![crate::objects::masters::Masters::new([]), table];
    t
}

/// One tick of the walker: the velocity step, then the motion step from where it got.
fn tick(m: &mut Mover, at: Position, world: &mut Spatial) -> (Position, Motion) {
    let at = m.step_velocity(
        at,
        Seconds::new(DT),
        world.map,
        None,
        (WorldTime::ZERO, None),
    );
    m.new_position(object_id(9177), at, (world, None)).unwrap();
    let motion = m
        .motion(
            &Me {
                id: object_id(9177),
                group: None,
                action_state: ActionState::NO_ACTION,
            },
            Seconds::new(DT),
            world,
            None,
        )
        .unwrap();
    (at, motion)
}

/// `LAB_141153bd1`'s waypoint tests: a unit on its waypoint, or one whose step would lead
/// away from it, counts it reached and the motion step starts again (on the path's last
/// waypoint the walk ends: arrived, the levels reset); with more than 2 substitutes a short
/// step counts it reached too (flags 3) and the walk goes on.
#[test]
fn a_walk_on_or_past_its_waypoint_counts_it_reached() {
    let me = || Me {
        id: object_id(9177),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    let levels = |m: &Mover| {
        m.paths
            .iter()
            .map(|l| (l.waypoint.get(), l.flags, l.state))
            .collect::<Vec<_>>()
    };
    for at in [
        Position::new(30.0, 88.0, 0.0),
        Position::new(31.0, 87.5, 0.0),
    ] {
        let mut w = world();
        let mut m = builder_9177();
        m.moved_at = at;
        m.place_for_test(at);
        let motion = m.motion(&me(), Seconds::new(DT), &mut w, None).unwrap();
        assert_eq!(motion, Motion::Arrived);
        assert_eq!(levels(&m), [(0, LevelFlags::NONE, PathState::Idle)]);
    }
    let mut w = world();
    let mut m = builder_9177();
    m.substitute_count = 3;
    let (_, motion) = tick(&mut m, Position::new(28.0, 89.0, 0.0), &mut w);
    assert_eq!(motion, Motion::Moving);
    assert_eq!(
        levels(&m),
        [
            (
                1,
                LevelFlags::REACHED | LevelFlags::AT_WAYPOINT,
                PathState::Walking
            ),
            (1, LevelFlags::NONE, PathState::Idle)
        ]
    );
}

#[test]
fn the_first_step_reaches_top_speed_and_moves_along_the_saved_forward_vector() {
    let mut m = builder_9177();
    let mut w = world();
    let (at, motion) = tick(&mut m, Position::new(28.0, 89.0, 0.0), &mut w);
    // the tick oracle's positions after update 681 (snapshot 680)
    assert_eq!(at, Position::new(28.01431, 88.99284, 0.0));
    assert_eq!(motion, Motion::Moving);
    assert_eq!(
        m.velocity.as_ref().unwrap().velocity,
        BVector::new(0.0, 0.0, 0.8)
    );
    assert_eq!(
        m.velocity.as_ref().unwrap().acceleration,
        BVector::new(0.0, 0.0, f32::MAX)
    );
    assert_eq!(m.forward, BVector::new(0.894_427_2, 0.0, -0.447_213_62));
}

/// Slot 0x5e0 (`FUN_14114f300`): outside a group the speed toward `wanted` stays between 0 and
/// the top speed (a NaN stays). A member that moved aside stands. A free member below 10 is
/// not capped by the top speed (from 10 on, and a NaN, it is). A member on an own move takes
/// the group's speed while its walked level walks unpaused (a NaN pause too), else it stands.
#[test]
fn the_speed_slot_caps_the_speed_unless_the_unit_s_group_steers_it() {
    let mut m = builder_9177();
    m.stats.speed = Speed::new(1.2);
    assert_eq!(m.next_speed(5.0, None), 1.2);
    assert_eq!(m.next_speed(-1.0, None), 0.0);
    assert_eq!(m.next_speed(0.7, None), 0.7);
    assert!(m.next_speed(f32::NAN, None).is_nan());
    assert_eq!(m.next_speed(5.0, Some(GroupSpeed::Aside)), 0.0);
    assert_eq!(m.next_speed(5.0, Some(GroupSpeed::Free)), 5.0);
    assert_eq!(m.next_speed(-1.0, Some(GroupSpeed::Free)), 0.0);
    assert_eq!(m.next_speed(10.0, Some(GroupSpeed::Free)), 1.2);
    assert!(m.next_speed(f32::NAN, Some(GroupSpeed::Free)).is_nan());
    let own = Some(GroupSpeed::OwnMove(Speed::new(1.44)));
    assert_eq!(m.paths[0].state, PathState::Walking);
    assert_eq!(m.next_speed(-1.0, own), 1.44);
    m.paths[0].pause_time = Seconds::new(0.028);
    assert_eq!(m.next_speed(0.5, own), 0.0);
    m.paths[0].pause_time = Seconds::new(f32::NAN);
    assert_eq!(m.next_speed(0.5, own), 1.44);
    m.paths[0].pause_time = Seconds::ZERO;
    m.paths[0].state = PathState::WantsPath;
    assert_eq!(m.next_speed(0.5, own), 0.0);
    m.paths.clear();
    assert_eq!(m.next_speed(0.5, own), 0.0);
}

/// The velocity step (`FUN_140f9ba80`) takes slot 0x5e0's speed: a member on an own move whose
/// walked level is paused stands where it is, whatever its acceleration.
#[test]
fn the_velocity_step_of_a_paused_member_on_its_own_move_stands() {
    let mut m = builder_9177();
    m.paths[0].pause_time = Seconds::new(0.028);
    let at = m.step_velocity(
        Position::new(28.0, 89.0, 0.0),
        Seconds::new(DT),
        GridSize::new(120, 120),
        None,
        (WorldTime::ZERO, Some(GroupSpeed::OwnMove(Speed::new(1.2)))),
    );
    assert_eq!(at, Position::new(28.0, 89.0, 0.0));
    assert_eq!(m.velocity.unwrap().velocity.y(), 0.0);
}

/// `moveForward` (`FUN_141158490`), slot 199 (`FUN_141161860`) and slot 198
/// (`FUN_141150a80`) start from `+0x824`, where the unit last moved to, not from the place
/// the caller holds.
#[test]
fn the_step_and_the_speed_control_start_where_the_unit_last_moved_to() {
    let mut m = builder_9177();
    m.forward = BVector::new(1.0, 0.0, 0.0);
    m.moved_at = Position::new(10.0, 20.0, 0.0);
    m.velocity.as_mut().unwrap().velocity = BVector::new(0.0, 0.0, 0.5);
    m.velocity.as_mut().unwrap().acceleration = BVector::ZERO;
    // the step: 0.5 tiles a second for a second, from (10, 20) whatever the caller holds
    let at = m.step_velocity(
        Position::new(40.0, 40.0, 0.0),
        Seconds::new(1.0),
        GridSize::new(120, 120),
        None,
        (WorldTime::ZERO, None),
    );
    assert_eq!(at, Position::new(10.5, 20.0, 0.0));
    assert_eq!(m.moved_at, Position::new(10.5, 20.0, 0.0));
    // the projection along +x: 0.5 tiles from (10.5, 20)
    assert_eq!(
        m.projected(BVector::new(1.0, 0.0, 0.0), Seconds::new(1.0)),
        BVector::new(11.0, 0.0, 20.0)
    );
    // the speed control's turning circle around (10.5, 20) grown to the right by 1: a
    // waypoint inside it brakes, one beyond it speeds up
    m.stats.turn_radius = Distance::new(2.0);
    m.stats.turn_radius_speed = Speed::new(1.0);
    m.right = BVector::new(0.0, 0.0, -1.0);
    m.control_speed(BVector::new(10.5, 0.0, 19.2), false);
    assert_eq!(m.velocity.as_ref().unwrap().acceleration.y(), -f32::MAX);
    m.velocity.as_mut().unwrap().acceleration = BVector::ZERO;
    m.control_speed(BVector::new(12.0, 0.0, 17.0), false);
    assert_eq!(m.velocity.as_ref().unwrap().acceleration.y(), f32::MAX);
}

#[test]
fn the_second_tick_turns_toward_the_waypoint_from_where_the_first_step_ended() {
    let mut m = builder_9177();
    let mut w = world();
    let (at, _) = tick(&mut m, Position::new(28.0, 89.0, 0.0), &mut w);
    let (at, _) = tick(&mut m, at, &mut w);
    assert_eq!(at, Position::new(28.028_622, 88.98569, 0.0));
    // after update 682 the game's forward vector points from there to (30, 88)
    assert_eq!(m.forward, BVector::new(0.894_581_56, 0.0, -0.446_904_78));
    assert_eq!(m.right, BVector::new(-0.446_904_78, 0.0, -0.894_581_56));
    let (at, _) = tick(&mut m, at, &mut w);
    assert_eq!(at, Position::new(28.042_934, 88.97854, 0.0));
}

#[test]
fn the_gap_is_the_distance_between_the_footprints() {
    // MarvinBlue/426048731: a house at (30, 88), builders at (30, 89), (28, 89), (29, 99)
    let (villager, house) = (Footprint::new(0.2, 0.2), Footprint::new(1.0, 1.0));
    let gap = |x, y| {
        footprint_gap(
            Position::new(x, y, 0.0),
            villager,
            Position::new(30.0, 88.0, 0.0),
            house,
        )
    };
    assert_eq!(gap(30.0, 89.0), Distance::ZERO);
    assert_eq!(gap(28.0, 89.0), Distance::new(2.0f32 - 1.2));
    assert_eq!(gap(29.0, 99.0), Distance::new((11.0f32 - 1.2).hypot(0.0)));
}

/// MarvinBlue/426048731's hawk 9926 after tick 679: a flight to a point, no range.
#[test]
fn a_walk_to_a_point_goes_on_until_its_range() {
    let mut hawk = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    for l in &mut hawk.paths {
        l.target = None;
        l.range = Distance::ZERO;
    }
    let me = Me {
        id: object_id(9926),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    assert_eq!(
        hawk.motion(
            &me,
            Seconds::new(DT),
            &mut Spatial::new(GridSize::new(120, 120)),
            None
        ),
        Ok(Motion::Moving)
    );
    // a deer's walk with range 0.5 whose top path ends within it
    let mut deer = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    for l in &mut deer.paths {
        l.target = None;
        l.range = Distance::new(0.5);
    }
    deer.paths[1].points = vec![BVector::new(28.0, 0.0, 89.0), BVector::new(28.6, 0.0, 89.0)];
    deer.velocity.as_mut().unwrap().velocity = BVector::new(0.0, 0.0, 0.8);
    assert_eq!(
        deer.motion(
            &me,
            Seconds::new(DT),
            &mut Spatial::new(GridSize::new(120, 120)),
            None
        ),
        Ok(Motion::Arrived)
    );
    // the walk ends: one level, reset, and the deer stops
    assert_eq!(deer.paths.len(), 1);
    assert_eq!(
        (
            deer.paths[0].state,
            deer.paths[0].target,
            &deer.paths[0].points
        ),
        (PathState::Idle, None, &vec![])
    );
    assert_eq!(
        deer.velocity
            .as_ref()
            .map(|v| (v.velocity.y(), v.acceleration.y())),
        Some((0.0, 0.0))
    );
}

/// `0x141154f8a`: a top level (2) at its last waypoint, short of its target and off the
/// waypoint it left: a walker without a weapon range whose target moved after the path was
/// made lets the path go and arrives (2); one whose target did not, or a walker with a weapon
/// range, takes state 6 and is done (1); on a tie of times the higher player, then the higher
/// id yields (427222982, 428355643, 428376971).
#[test]
fn a_top_level_short_of_its_moved_target_arrives() {
    let walk = |weapon_range: Distance| {
        let mut m = builder_9177();
        m.paths = vec![level(
            2,
            &[Point::new(28.0, 89.0), Point::new(30.0, 88.0)],
            PathState::Walking,
        )];
        let l = &mut m.paths[0];
        l.flags = LevelFlags::REACHED;
        l.target = Some(object_id(500));
        l.created = WorldTime::from_millis(100);
        m.stats.weapon_range = weapon_range;
        m
    };
    let run = |m: &mut Mover, last_move: u32, target_player: PlayerId, target_id: i32| {
        let mut world = Spatial::new(GridSize::new(120, 120));
        let placed = Placed {
            position: Position::new(40.0, 88.0, 0.0),
            radius: Footprint::new(0.5, 0.5),
            class: class::CLASS_6,
            player: target_player,
            master: master_id(74),
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: false,
            speed: Speed::new(1.0),
            unknown: None,
            obstruction: None,
        };
        world.insert(object_id(target_id), placed);
        world.set_gait(
            object_id(target_id),
            Some(Gait {
                last_move: WorldTime::from_millis(last_move),
                ..Gait::default()
            }),
        );
        let me = Me {
            id: object_id(9177),
            group: None,
            action_state: ActionState::NO_ACTION,
        };
        m.place_for_test(Position::new(30.0, 88.0, 0.0));
        m.paths[0].target = ObjectId::of_field(target_id);
        let goal = Goal {
            target: Some((object_id(target_id), &placed)),
            gap: Distance::new(9.0),
            to_end: 9.0,
        };
        m.reached(&me, &mut world, None, goal, WorldTime::ZERO)
    };
    let mut m = walk(Distance::ZERO);
    assert!(matches!(
        run(&mut m, 150, PlayerId::new(0), 500),
        Ok(Next::Done(Motion::Arrived))
    ));
    assert_eq!(m.paths.len(), 1);
    assert_eq!(m.paths[0].created, WorldTime::ZERO);
    let mut m = walk(Distance::ZERO);
    assert!(matches!(
        run(&mut m, 50, PlayerId::new(0), 500),
        Ok(Next::Done(Motion::Done))
    ));
    assert_eq!(m.paths[0].state, PathState::GaveUp);
    let mut m = walk(Distance::new(2.0));
    assert!(matches!(
        run(&mut m, 150, PlayerId::new(0), 500),
        Ok(Next::Done(Motion::Done))
    ));
    // a tie: a target of a higher id yields, a lower one does not
    let mut m = walk(Distance::ZERO);
    assert!(matches!(
        run(&mut m, 100, PlayerId::new(0), 9500),
        Ok(Next::Done(Motion::Arrived))
    ));
    let mut m = walk(Distance::ZERO);
    assert!(matches!(
        run(&mut m, 100, PlayerId::new(0), 500),
        Ok(Next::Done(Motion::Done))
    ));
}

/// `LAB_14115136c` on a level 1 or 2 whose next waypoint lies within reach (its squared
/// distance below [`TARGET_SHORT_REACH`]): the manager's point query at the waypoint it moves
/// on to (`FUN_141051900`, flags 0x20) drops that waypoint (`FUN_140f98d50`) when an object's
/// record holds it, unless the manager's ignore list as the last `FUN_1410562e0` left it
/// holds the object (426916909's p0 in update 4036, 428265662's in 4931).
#[test]
fn a_next_waypoint_an_object_stands_on_goes() {
    let walk = || {
        let mut m = builder_9177().placed(Position::new(30.0, 88.0, 0.0));
        m.paths = vec![level(
            2,
            &[
                Point::new(28.0, 89.0),
                Point::new(30.0, 88.0),
                Point::new(31.0, 88.0),
                Point::new(32.0, 88.0),
            ],
            PathState::Walking,
        )];
        let l = &mut m.paths[0];
        l.target = None;
        l.flags = LevelFlags::REACHED;
        m
    };
    let mut world = Spatial::new(GridSize::new(120, 120));
    world.insert(
        object_id(500),
        Placed {
            position: Position::new(31.0, 88.5, 0.0),
            radius: Footprint::new(0.5, 0.5),
            class: class::BUILDING,
            player: PlayerId::new(2),
            master: ids::HOUSE,
            obstruction_type: ObstructionType::TYPE_2,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: Some(Obstruction {
                kind: ObstructionType::TYPE_2,
                class: ObstructionClass::UNIT,
            }),
        },
    );
    let me = Me {
        id: object_id(9177),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    let goal = Goal {
        target: None,
        gap: Distance::new(FAR),
        to_end: FAR,
    };
    let mut m = walk();
    let _ = m
        .reached(&me, &mut world, None, goal, WorldTime::ZERO)
        .unwrap();
    assert_eq!(
        m.paths[0].points,
        vec![
            BVector::new(28.0, 0.0, 89.0),
            BVector::new(30.0, 0.0, 88.0),
            BVector::new(32.0, 0.0, 88.0)
        ]
    );
    assert_eq!(m.paths[0].waypoint.get(), 2);
    // the house in the manager's ignore list: the waypoint stays
    world.set_manager_ignore(&[RecordId::from(object_id(500))]);
    let mut m = walk();
    let _ = m
        .reached(&me, &mut world, None, goal, WorldTime::ZERO)
        .unwrap();
    assert_eq!(m.paths[0].points.len(), 4);
    // the next waypoint out of reach: no query
    world.set_manager_ignore(&[]);
    let mut m = walk();
    m.paths[0].points[3] = BVector::new(40.0, 0.0, 88.0);
    let _ = m
        .reached(&me, &mut world, None, goal, WorldTime::ZERO)
        .unwrap();
    assert_eq!(m.paths[0].points.len(), 4);
}

/// MarvinBlue/426048731's 9175 in update 724..726: the step that reaches its waypoint marks it
/// reached; the next motion step puts the unit onto it, the level arrives and hands over to the
/// top level, which arrives too: the walk ends.
#[test]
fn a_reached_waypoint_ends_the_walk_at_the_end_of_the_path() {
    let mut m = builder_9177();
    m.paths[0].points = vec![BVector::new(30.0, 0.0, 89.0), BVector::new(29.5, 0.0, 89.5)];
    m.paths[1].points = vec![BVector::new(30.0, 0.0, 89.0), BVector::new(29.5, 0.0, 89.5)];
    for l in &mut m.paths {
        l.target = None;
        l.range = Distance::ZERO;
    }
    m.forward = BVector::new(
        -std::f32::consts::FRAC_1_SQRT_2,
        0.0,
        std::f32::consts::FRAC_1_SQRT_2,
    );
    m.velocity.as_mut().unwrap().velocity = BVector::new(0.0, 0.0, 0.8);
    let mut world = Spatial::new(GridSize::new(120, 120));
    m.moved_at = Position::new(29.51, 89.49, 0.0);
    m.place_for_test(Position::new(29.51, 89.49, 0.0));
    let me = Me {
        id: object_id(9175),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    assert_eq!(
        m.motion(&me, Seconds::new(DT), &mut world, None),
        Ok(Motion::Moving)
    );
    assert_eq!(
        m.paths[0].flags,
        LevelFlags::REACHED | LevelFlags::AT_WAYPOINT
    );
    assert_eq!(
        m.motion(&me, Seconds::new(DT), &mut world, None),
        Ok(Motion::Arrived)
    );
    assert_eq!(m.position(), Position::new(29.5, 89.5, 0.0));
    assert_eq!(m.paths.len(), 1);
    assert_eq!(m.paths[0].state, PathState::Idle);
}

/// MarvinBlue/426048731's 9175 in update 682: the path request of its `MoveTo` off the house
/// footprint, which the game saves as the top level of its path data.
#[test]
fn a_path_request_makes_the_top_level_from_the_unit_to_its_goal() {
    let mut m = builder_9177().placed(Position::new(30.0, 89.0, 0.0));
    m.request_path(
        Position::new(30.0, 89.0, 0.0),
        Position::new(29.5, 89.5, 0.0),
        Distance::ZERO,
        None,
        WorldTime::from_millis(16400),
    );
    assert_eq!(m.paths.len(), 1);
    let top = &m.paths[0];
    assert_eq!(
        (
            top.level.get(),
            top.path_flags,
            top.waypoint.get(),
            top.state,
            top.target
        ),
        (2, PathFlags::FORWARD, 1, PathState::WantsPath, None)
    );
    assert_eq!(
        top.points,
        vec![BVector::new(30.0, 0.0, 89.0), BVector::new(29.5, 0.0, 89.5)]
    );
    // the motion step then plans the path to walk, which the slice does not model
    let me = Me {
        id: object_id(9175),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    assert_eq!(
        m.motion(
            &me,
            Seconds::new(DT),
            &mut Spatial::new(GridSize::new(120, 120)),
            None
        ),
        Err(GenieError::unsupported(
            "a path request without the world's planners"
        ))
    );
}

#[test]
fn a_unit_with_no_speed_or_no_path_is_done() {
    let mut m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    m.stats.speed = Speed::ZERO;
    let me = Me {
        id: object_id(9177),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    assert_eq!(
        m.motion(&me, Seconds::new(DT), &mut world(), None),
        Ok(Motion::Done)
    );
    let mut m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    m.paths.clear();
    assert_eq!(
        m.motion(&me, Seconds::new(DT), &mut world(), None),
        Ok(Motion::Done)
    );
}

/// The 4v4's hawk 30594 in update 2007: its level 0 arrived under a top level without a path
/// (a bird's); the parent takes the reached mark and the step ends there, the unit flying on.
#[test]
fn an_arrived_level_under_one_without_a_path_ends_the_step() {
    let mut m = builder_9177().placed(Position::new(30.0, 88.0, 0.0));
    m.velocity.as_mut().unwrap().velocity = BVector::new(0.0, 0.0, 1.32);
    m.paths = vec![
        level(
            0,
            &[Point::new(28.0, 89.0), Point::new(30.0, 88.0)],
            PathState::Arrived,
        ),
        PathData {
            has_path: false,
            path_flags: PathFlags::NONE,
            created: WorldTime::ZERO,
            ..level(2, &[], PathState::Idle)
        },
    ];
    for l in &mut m.paths {
        l.target = None;
    }
    let me = Me {
        id: object_id(9177),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    assert_eq!(
        m.motion(&me, Seconds::new(DT), &mut world(), None),
        Ok(Motion::Done)
    );
    assert_eq!(m.paths.len(), 1);
    assert_eq!(
        (m.paths[0].flags, m.paths[0].state),
        (LevelFlags::REACHED, PathState::Idle)
    );
    assert_eq!(
        m.velocity.as_ref().unwrap().velocity,
        BVector::new(0.0, 0.0, 1.32)
    );
}

#[test]
fn a_human_walker_pauses_a_met_unit_of_an_older_human_order() {
    // 9177 meets 9175 of its own player: ordered at the same time, the lower id outranks
    // and 9177 walks on; once 9175's last human order is the older, 9177 pauses it for dt
    // more than its pause (slot 0x5f0)
    let mut w = world();
    let mut close = *w.get(object_id(9175)).unwrap();
    close.position = Position::new(28.3, 88.9, 0.0);
    w.insert(object_id(9175), close);
    let me = Me {
        id: object_id(9177),
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    let walker = |human_order| Gait {
        human_order,
        ..Gait::default()
    };
    w.set_gait(object_id(9177), Some(walker(WorldTime::from_millis(17000))));
    w.set_gait(
        object_id(9175),
        Some(Gait {
            pause: Some(Seconds::ZERO),
            ..walker(WorldTime::from_millis(17000))
        }),
    );
    let mut walker_9177 = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    assert!(
        walker_9177
            .motion(&me.clone(), Seconds::new(DT), &mut w, None)
            .is_ok()
    );
    assert_eq!(walker_9177.pauses, Vec::new());
    w.set_gait(
        object_id(9175),
        Some(Gait {
            pause: Some(Seconds::ZERO),
            ..walker(WorldTime::from_millis(16000))
        }),
    );
    let mut walker_9177 = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    assert!(
        walker_9177
            .motion(&me.clone(), Seconds::new(DT), &mut w, None)
            .is_ok()
    );
    assert_eq!(
        walker_9177.pauses,
        [(object_id(9175), Seconds::ZERO + Seconds::new(DT))]
    );
}

/// `0x141150f00`'s pause loop between two members of one unit group (`+0x6d0`) ordered at
/// the same time: the group's member order decides (`FUN_1410175f0`), not the ids. 9177,
/// the group's first member, pauses 9175, the second; listed the other way round it walks
/// on; a group the world does not hold (`FUN_14152ff40`) pauses neither, as two units of no
/// group rank by id (the higher id 9177 walks on).
#[test]
fn two_members_of_one_group_rank_by_their_places_in_it() {
    let mut w = world();
    let mut close = *w.get(object_id(9175)).unwrap();
    close.position = Position::new(28.3, 88.9, 0.0);
    w.insert(object_id(9175), close);
    let walker = |group| Gait {
        human_order: WorldTime::from_millis(17000),
        pause: Some(Seconds::ZERO),
        group,
        ..Gait::default()
    };
    w.set_gait(object_id(9177), Some(walker(Some(group_id(4)))));
    w.set_gait(object_id(9175), Some(walker(Some(group_id(4)))));
    let me = Me {
        id: object_id(9177),
        group: Some(group_id(4)),
        action_state: ActionState::NO_ACTION,
    };
    let groups = |ids: &[i32]| crate::groups::UnitGroups {
        groups: vec![crate::groups::UnitGroup {
            id: group_id(4),
            members: ids
                .iter()
                .map(|&id| crate::groups::Member {
                    id: object_id(id),
                    ..crate::groups::Member::default()
                })
                .collect(),
            ..crate::groups::UnitGroup::default()
        }],
        ..crate::groups::UnitGroups::default()
    };
    let pauses = |w: &Spatial, me: &Me, gs: Option<&crate::groups::UnitGroups>| {
        let m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
        let position = m.moved_at.vector();
        let waypoint = m.paths[0].point(WaypointIndex::of_data(1));
        let projected = m.projected(m.direction_to(position, waypoint), Seconds::new(DT));
        m.avoid(
            me,
            (w, gs),
            &[RecordId::from(object_id(9175))],
            projected,
            Seconds::new(DT),
            (WorldTime::from_millis(16361), false),
        )
        .unwrap()
        .pauses
    };
    let first = groups(&[9177, 9175]);
    assert_eq!(
        pauses(&w, &me, Some(&first)),
        [(object_id(9175), Seconds::new(DT))]
    );
    let second = groups(&[9175, 9177]);
    assert_eq!(pauses(&w, &me, Some(&second)), []);
    let none = crate::groups::UnitGroups::default();
    assert_eq!(pauses(&w, &me, Some(&none)), []);
    w.set_gait(object_id(9175), Some(walker(None)));
    let loose = Me { group: None, ..me };
    assert_eq!(pauses(&w, &loose, Some(&first)), []);
}

/// `0x141150f00`'s first hit loop (0x1411543f1..0x1411544bc): a walker whose own move in its
/// unit group (`FUN_14152ff40` on `+0x6d0`) has reason 0 (`FUN_141017590`) keeps no hit but
/// one of class 3, 27, 39 or 52, however its step meets it. Reason 1, a walker the group does
/// not list (−1) and a group the world does not hold keep the hit as any walker does.
/// (430356142's scout cavalry 9585 walking its own move through goat 34792 at update 291.)
#[test]
fn a_group_member_on_an_own_move_of_reason_0_keeps_only_buildings_walls_and_gates() {
    let mut w = world();
    let mut close = *w.get(object_id(9175)).unwrap();
    close.position = Position::new(28.3, 88.9, 0.0);
    w.insert(object_id(9175), close);
    w.set_gait(
        object_id(9175),
        Some(Gait {
            last_move: WorldTime::from_millis(16361),
            ..Gait::default()
        }),
    );
    let me = Me {
        id: object_id(9177),
        group: Some(group_id(4)),
        action_state: ActionState::NO_ACTION,
    };
    let groups = |member: i32, reason: i32| crate::groups::UnitGroups {
        groups: vec![crate::groups::UnitGroup {
            id: group_id(4),
            members: vec![crate::groups::Member {
                id: object_id(member),
                specific_move: crate::groups::SpecificMove {
                    reason: MoveReason::of_data(reason),
                    ..crate::groups::SpecificMove::default()
                },
                ..crate::groups::Member::default()
            }],
            ..crate::groups::UnitGroup::default()
        }],
        ..crate::groups::UnitGroups::default()
    };
    let kept = |w: &Spatial, gs: &crate::groups::UnitGroups| {
        let m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
        let position = m.moved_at.vector();
        let waypoint = m.paths[0].point(WaypointIndex::of_data(1));
        let projected = m.projected(m.direction_to(position, waypoint), Seconds::new(DT));
        m.avoid(
            &me,
            (w, Some(gs)),
            &[RecordId::from(object_id(9175))],
            projected,
            Seconds::new(DT),
            (WorldTime::from_millis(16361), false),
        )
        .unwrap()
        .kept
    };
    assert_eq!(kept(&w, &groups(9177, 0)), Vec::new());
    assert_eq!(kept(&w, &groups(9177, 1)), [object_id(9175)]);
    assert_eq!(kept(&w, &groups(9176, 0)), [object_id(9175)]);
    assert_eq!(
        kept(&w, &crate::groups::UnitGroups::default()),
        [object_id(9175)]
    );
    for class in [class::BUILDING, class::WALL, class::GATE, class::TOWER] {
        w.insert(object_id(9175), Placed { class, ..close });
        assert_eq!(
            kept(&w, &groups(9177, 0)),
            [object_id(9175)],
            "class {class}"
        );
    }
}

/// `FUN_14114e920` (slot 0x448): a unit's own waypoint is held as (x, height, y); a
/// coordinate less than 1e-7 above a whole number moves to the tile's middle, the height
/// stays; a NaN stays (`COMISS`, `JBE`). Slot 0x450 (`FUN_141070ce0`) empties the list.
#[test]
fn a_user_waypoint_on_a_tile_edge_moves_to_the_tile_s_middle() {
    let mut m = builder_9177();
    m.add_user_waypoint(25.0, 20.0, 1.0);
    m.add_user_waypoint(25.25, 20.000_000_1, 0.0);
    m.add_user_waypoint(f32::NAN, 7.75, 2.0);
    assert_eq!(m.user_waypoints[0], BVector::new(25.5, 1.0, 20.5));
    // 20.000_000_1 is 20.0 as an f32: on the edge
    assert_eq!(m.user_waypoints[1], BVector::new(25.25, 0.0, 20.5));
    assert!(m.user_waypoints[2].x().is_nan());
    assert_eq!(
        (m.user_waypoints[2].height(), m.user_waypoints[2].y()),
        (2.0, 7.75)
    );
    m.clear_user_waypoints();
    assert_eq!(m.user_waypoints, Vec::<BVector>::new());
}

/// `FUN_14114e410`: the unit's own waypoints come between its place and the goal and go;
/// a goal less than 1e-7 on the ground from the last of them is left out. A patrol's path
/// loops (flags bit 0), a far one goes back and forth (bit 1); an attack move (1) clears bit
/// 0 and sets `+0x7fb`, 0 clears it, none leaves it.
#[test]
fn a_patrol_s_path_request_walks_the_own_waypoints_and_takes_the_patrol_flags() {
    let mut m = builder_9177();
    m.user_waypoints = vec![BVector::new(25.5, 0.0, 20.5), BVector::new(26.5, 0.0, 20.5)];
    let ask = PathAsk {
        patrol: true,
        back_and_forth: true,
        attack_move: Some(false),
        ..PathAsk::default()
    };
    m.attack_moving = true;
    m.request_path_asked(
        Position::new(20.0, 20.0, 0.0),
        Position::new(30.0, 20.0, 0.0),
        Distance::ZERO,
        None,
        WorldTime::from_millis(500),
        ask,
    );
    let top = &m.paths[0];
    assert_eq!(
        top.points,
        vec![
            BVector::new(20.0, 0.0, 20.0),
            BVector::new(25.5, 0.0, 20.5),
            BVector::new(26.5, 0.0, 20.5),
            BVector::new(30.0, 0.0, 20.0)
        ]
    );
    assert_eq!(
        (top.path_flags, top.waypoint.get(), top.state),
        (
            PathFlags::LOOPS | PathFlags::BACK_AND_FORTH | PathFlags::FORWARD,
            1,
            PathState::WantsPath
        )
    );
    assert_eq!(m.user_waypoints, Vec::<BVector>::new());
    assert!(!m.attack_moving);
    // the goal on the last waypoint is left out; an attack move does not loop
    m.user_waypoints = vec![BVector::new(30.0, 0.0, 20.0)];
    let ask = PathAsk {
        patrol: true,
        attack_move: Some(true),
        ..PathAsk::default()
    };
    m.request_path_asked(
        Position::new(20.0, 20.0, 0.0),
        Position::new(30.0, 20.0, 0.0),
        Distance::ZERO,
        None,
        WorldTime::from_millis(500),
        ask,
    );
    assert_eq!(
        m.paths[0].points,
        vec![BVector::new(20.0, 0.0, 20.0), BVector::new(30.0, 0.0, 20.0)]
    );
    assert_eq!(m.paths[0].path_flags, PathFlags::FORWARD);
    assert!(m.attack_moving);
    // none leaves the attack move as it was
    m.request_path(
        Position::new(20.0, 20.0, 0.0),
        Position::new(30.0, 20.0, 0.0),
        Distance::ZERO,
        None,
        WorldTime::from_millis(500),
    );
    assert!(m.attack_moving);
}

/// `FUN_14114e410` with a saved patrol path's waypoint (`param_9`): the path is the own
/// waypoints alone and the level walks on from that waypoint; without any, the unit's place
/// and the goal on the ground (height 0).
#[test]
fn a_resumed_patrol_s_path_is_its_waypoints_from_the_saved_one() {
    let mut m = builder_9177();
    m.user_waypoints = vec![
        BVector::new(20.0, 0.0, 20.0),
        BVector::new(25.5, 0.0, 20.5),
        BVector::new(30.0, 0.0, 20.0),
    ];
    let ask = PathAsk {
        patrol: true,
        back_and_forth: true,
        resume: WaypointIndex::of_data(2),
        ..PathAsk::default()
    };
    m.request_path_asked(
        Position::new(22.0, 21.0, 0.0),
        Position::new(30.0, 20.0, 1.0),
        Distance::ZERO,
        None,
        WorldTime::from_millis(500),
        ask,
    );
    assert_eq!(m.paths[0].points.len(), 3);
    assert_eq!(
        (m.paths[0].waypoint.get(), m.paths[0].path_flags),
        (
            2,
            PathFlags::LOOPS | PathFlags::BACK_AND_FORTH | PathFlags::FORWARD
        )
    );
    m.request_path_asked(
        Position::new(22.0, 21.0, 0.0),
        Position::new(30.0, 20.0, 1.0),
        Distance::ZERO,
        None,
        WorldTime::from_millis(500),
        ask,
    );
    assert_eq!(
        m.paths[0].points,
        vec![BVector::new(22.0, 0.0, 21.0), BVector::new(30.0, 0.0, 20.0)]
    );
}

/// The path request's first exits ([`Mover::in_reach`]): a target within the walked level's
/// range (at least `RANGE_FLOOR` for a range of 0) puts the level in state 4; a unit on its
/// waypoint marks it reached (flag 1), and the waypoint itself (flag 2) without a target or
/// away from the target's footprint. Both go again; elsewhere the request goes on.
#[test]
fn a_request_in_reach_of_its_target_or_on_its_waypoint_goes_again() {
    let house = world_target();
    let start = BVector::new(28.0, 0.0, 89.0);
    let near = Goal {
        target: Some((object_id(10028), &house)),
        gap: Distance::new(0.1),
        to_end: FAR,
    };
    let mut m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    assert_eq!(m.in_reach(near, start), Some(AfterRequest::Again));
    assert_eq!(m.paths[0].state, PathState::Arrived);
    let mut m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    m.paths[0].range = Distance::ZERO;
    let floor = Goal {
        gap: Distance::new(RANGE_FLOOR),
        ..near
    };
    assert_eq!(m.in_reach(floor, start), Some(AfterRequest::Again));
    assert_eq!(m.paths[0].state, PathState::Arrived);
    let far = Goal {
        target: None,
        gap: Distance::new(FAR),
        to_end: FAR,
    };
    let mut m = builder_9177().placed(Position::new(28.0, 89.0, 0.0));
    assert_eq!(m.in_reach(far, start), None);
    assert_eq!(
        (m.paths[0].state, m.paths[0].flags),
        (PathState::Walking, LevelFlags::NONE)
    );
    let waypoint = BVector::new(30.0, 0.0, 88.0);
    assert_eq!(m.in_reach(far, waypoint), Some(AfterRequest::Again));
    assert_eq!(
        m.paths[0].flags,
        LevelFlags::REACHED | LevelFlags::AT_WAYPOINT
    );
    let mut m = builder_9177().placed(house.position);
    let on_house = Goal {
        gap: Distance::new(FAR),
        ..near
    };
    assert_eq!(m.in_reach(on_house, waypoint), Some(AfterRequest::Again));
    assert_eq!(m.paths[0].flags, LevelFlags::REACHED);
}

/// The request's try ([`Mover::spend_try`]): a try left goes on, one fewer; a looping top
/// path or an attack move spends none. The last try of level 0 under a top with more points
/// takes state 5; without more the level resets to state 6 and the unit stops (it gives up),
/// as does a level 2 on its last waypoint; a level 2 short of it gets its tries again
/// (`CONTINUE`) and goes on (flag 1).
#[test]
fn a_request_spends_a_try_and_gives_up_or_goes_on_without_more() {
    let mut m = builder_9177();
    assert_eq!(m.spend_try(), Ok(None));
    assert_eq!(m.paths[0].continue_counter, 9);
    let mut looping = builder_9177();
    looping.paths[1].path_flags |= PathFlags::LOOPS;
    let mut attacking = builder_9177();
    attacking.attack_moving = true;
    for mut m in [looping, attacking] {
        assert_eq!(m.spend_try(), Ok(None));
        assert_eq!(m.paths[0].continue_counter, 10);
    }
    let last_try = || {
        let mut m = builder_9177();
        m.paths[0].continue_counter = 1;
        m
    };
    let mut m = last_try();
    m.paths[1].waypoint = WaypointIndex::of_data(0);
    assert_eq!(m.spend_try(), Ok(Some(AfterRequest::Again)));
    assert_eq!(m.paths[0].state, PathState::Unplanned);
    let mut m = last_try();
    assert_eq!(m.spend_try(), Ok(Some(AfterRequest::GaveUp)));
    assert_eq!((m.paths.len(), m.paths[0].state), (1, PathState::GaveUp));
    assert_eq!(m.animated.speed, 0.0);
    let mut m = last_try();
    m.paths[0].level = WaypointLevel::TOP;
    assert_eq!(m.spend_try(), Ok(Some(AfterRequest::GaveUp)));
    let mut m = last_try();
    m.paths[0].level = WaypointLevel::TOP;
    m.paths[0].waypoint = WaypointIndex::of_data(0);
    assert_eq!(m.spend_try(), Ok(Some(AfterRequest::Again)));
    assert_eq!(
        (m.paths[0].flags, m.paths[0].continue_counter),
        (LevelFlags::REACHED, CONTINUE)
    );
}

/// The request's way from where [`builder_9177`] last moved to (28, 89) to its waypoint
/// (30, 88), toward `goal` within `range`.
fn way_9177(goal: Goal<'_>, range: Distance) -> Way<'_> {
    Way {
        goal,
        include: &[],
        position: BVector::new(28.0, 0.0, 89.0),
        waypoint: BVector::new(30.0, 0.0, 88.0),
        distance: Distance::new(ground(
            BVector::new(30.0, 0.0, 88.0),
            BVector::new(28.0, 0.0, 89.0),
        )),
        radius: Distance::new(0.2),
        level: WaypointLevel::WALKED,
        range,
        target_id: None,
        trade_cart: false,
    }
}

/// A short path of no length ([`Mover::waypoint_in_range`]): within the range (at least
/// `RANGE_FLOOR` for a target and no range) the level takes state 4; beyond it the level goes
/// on (flag 1), but on the last waypoint of a path made this update it resets to state 6 and
/// the unit stops (it gives up).
#[test]
fn a_waypoint_in_range_ends_the_level_or_gives_up_on_a_fresh_path() {
    let point = |gap| Goal {
        target: None,
        gap,
        to_end: FAR,
    };
    let mut m = builder_9177();
    assert_eq!(
        m.waypoint_in_range(
            &way_9177(point(Distance::new(0.05)), Distance::new(0.1)),
            WorldTime::from_millis(16360)
        ),
        Ok(AfterRequest::Again)
    );
    assert_eq!(m.paths[0].state, PathState::Arrived);
    let mut m = builder_9177();
    assert_eq!(
        m.waypoint_in_range(
            &way_9177(point(Distance::new(0.5)), Distance::new(0.1)),
            WorldTime::from_millis(16000)
        ),
        Ok(AfterRequest::Again)
    );
    assert_eq!(
        (m.paths[0].state, m.paths[0].flags),
        (PathState::Walking, LevelFlags::REACHED)
    );
    let mut m = builder_9177();
    assert_eq!(
        m.waypoint_in_range(
            &way_9177(point(Distance::new(0.5)), Distance::new(0.1)),
            WorldTime::from_millis(16360)
        ),
        Ok(AfterRequest::GaveUp)
    );
    assert_eq!((m.paths.len(), m.paths[0].state), (1, PathState::GaveUp));
    let house = world_target();
    let target = Goal {
        target: Some((object_id(10028), &house)),
        gap: Distance::new(RANGE_FLOOR),
        to_end: FAR,
    };
    let mut m = builder_9177();
    assert_eq!(
        m.waypoint_in_range(
            &way_9177(target, Distance::ZERO),
            WorldTime::from_millis(16360)
        ),
        Ok(AfterRequest::Again)
    );
    assert_eq!(m.paths[0].state, PathState::Arrived);
}

/// A unit without a footprint takes the straight way ([`Mover::straight_way`]) to its
/// waypoint, found; past the level's last waypoint the way has only its start.
#[test]
fn a_unit_without_a_footprint_takes_the_straight_way() {
    let m = builder_9177();
    let start = BVector::new(28.0, 0.0, 89.0);
    let mut np = NewPath::new(WorldTime::ZERO);
    m.straight_way(start, &mut np);
    assert_eq!(np.path.points, [start, BVector::new(30.0, 0.0, 88.0)]);
    assert!(np.found);
    let mut m = builder_9177();
    m.paths[0].waypoint = WaypointIndex::of_data(2);
    let mut np = NewPath::new(WorldTime::ZERO);
    m.straight_way(start, &mut np);
    assert_eq!(
        (np.path.points, np.path.length),
        (vec![start], Distance::ZERO)
    );
}

/// A new `MoveTo`'s wait (`FUN_140aa0d30`, `FUN_140aa0df0`): never waited, no time waited, and
/// `kDefaultWaitTime` (`+0x74`) 1 s, which a give-up replaces by the stance's.
#[test]
fn a_new_move_to_s_wait_lasts_a_second() {
    use crate::movement::move_to::MoveWait;
    let new = MoveWait::default();
    assert_eq!(
        (new.state, new.timer, new.delay),
        (MoveWaitState::Never, Seconds::ZERO, Seconds::new(1.0))
    );
    let mut wait = new;
    let _ = wait.give_up(true, UnitStance::STAND_GROUND);
    assert_eq!(
        (wait.state, wait.delay),
        (MoveWaitState::Waiting, Seconds::new(2.0))
    );
}

/// The unit's top speed with the sprite it shows (slot 0x208, `FUN_14114d1d0`): the master's
/// speed times the sprite's factor when that is above 0, else the master's.
#[test]
fn a_mover_that_shows_a_sprite_has_the_top_speed_of_its_factor() {
    use crate::objects::sprite::Graphic;
    let with = |factor: f32| Graphic {
        id: SpriteId::named(1),
        frames: crate::objects::Frame::of_data(1),
        facets: crate::objects::FacetCount::of_data(8),
        speed_multiplier: factor,
        frame_duration: Seconds::ZERO,
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    };
    let mut m = crate::gathering::hunt::tests::walker();
    m.stats.base = Speed::new(0.8);
    m.shows(Some(&with(1.5)));
    assert_eq!(m.stats.speed, Speed::new(0.8 * 1.5));
    m.shows(Some(&with(0.0)));
    assert_eq!(m.stats.speed, Speed::new(0.8));
    m.shows(Some(&with(f32::NAN)));
    assert_eq!(m.stats.speed, Speed::new(0.8));
    m.shows(Some(&with(2.0)));
    m.shows(None);
    assert_eq!(m.stats.speed, Speed::new(0.8));
}
