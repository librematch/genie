use super::*;
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::RestrictionId;
use crate::map::TerrainMask;
use crate::movement::NEW_PATH_FLAGS;
use crate::movement::PathState;
use crate::movement::TurnRate;
use crate::movement::WaypointLevel;
use crate::objects::actions::ActionList;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::test_ids::object_id;
use crate::world::Millis;
use crate::world::WorldTime;

/// Slot 0x280 (`FUN_141082d60`) passes only a target id above -1 the world holds: a
/// recycled object's negative id gives -1 like none.
#[test]
fn a_top_target_below_minus_one_is_none() {
    use crate::objects::unit::Unit;
    use crate::world::Player;
    let hunter = |target: i32| Unit {
        id: object_id(5),
        actions: ActionList::default()
            .with_hunt(Some(crate::gathering::hunt::tests::hunting(target))),
        ..Unit::default()
    };
    let held = |id: i32| Unit {
        id: object_id(id),
        ..Unit::default()
    };
    let player = Player {
        objects: vec![held(7), held(-3)].into(),
        ..Player::default()
    };
    let t = World::new(
        WorldTime::ZERO,
        0,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![Player::default(), player],
    );
    assert_eq!(t.top_target_object(&hunter(7)), Some(object_id(7)));
    assert_eq!(t.top_target_object(&hunter(-3)), None);
    assert_eq!(t.top_target_object(&hunter(9)), None);
    assert_eq!(t.top_target_object(&held(1)), None);
}

/// `FUN_14104c150` turns only when 1e-7 is below the yaw's size (an ordered compare): a NaN
/// yaw leaves the vectors and the motion flags alone.
#[test]
fn a_nan_yaw_does_not_turn() {
    let mut m = Motion::new();
    let before = m.clone();
    m.turn(f32::NAN);
    assert_eq!(m.flags, before.flags);
    assert_eq!(
        format!("{:?}", m.new_position),
        format!("{:?}", before.new_position)
    );
}
use crate::groups::UnitGroups;
use crate::map::spatial::Placed;
use crate::math::rng::Well512;
use crate::pathing::Pathing;

fn level(level: u8, points: &[Point], waypoint: i32, state: PathState) -> PathData {
    PathData {
        level: WaypointLevel::of_byte(level),
        points: points
            .iter()
            .map(|p| BVector::new(p.x(), 0.0, p.y()))
            .collect(),
        has_path: true,
        path_flags: NEW_PATH_FLAGS,
        waypoint,
        state,
        ..PathData::without_path()
    }
}

/// `FUN_141046600` and `FUN_141046640`: a motion moves with any velocity, and waits to move
/// while its walked level wants a path (state 1 or 2).
#[test]
fn a_motion_moves_with_a_velocity_and_waits_for_a_path() {
    let mut m = Motion::new();
    assert!(!m.moving() && !m.waits_to_move());
    m.velocity = BVector::new(0.0, 0.0, -0.5);
    assert!(m.moving());
    m.paths = vec![level(0, &[], 0, PathState::WantsPath)];
    assert!(m.waits_to_move());
    m.paths[0].state = PathState::Walking;
    assert!(!m.waits_to_move());
}

/// `FUN_14104a050`: the forward speed and its acceleration go to 0 (the other components stay)
/// and the walked level resets, its parents gone.
#[test]
fn stopping_the_walk_stands_the_motion_and_resets_its_level() {
    let mut m = Motion::new();
    m.velocity = BVector::new(1.0, 2.0, 3.0);
    m.acceleration = BVector::new(4.0, 5.0, 6.0);
    m.paths = vec![
        level(
            0,
            &[Point::new(1.0, 1.0), Point::new(2.0, 2.0)],
            1,
            PathState::Walking,
        ),
        level(1, &[Point::new(2.0, 2.0)], 1, PathState::Idle),
    ];
    m.stop_walk().unwrap();
    assert_eq!(
        (m.velocity, m.acceleration),
        (BVector::new(1.0, 2.0, 0.0), BVector::new(4.0, 5.0, 0.0))
    );
    assert_eq!(m.paths, [PathData::without_path()]);
}

/// `FUN_1410481f0`: levels 0 and 1 below the top (2) go and the top walks from waypoint 1
/// (state 2, 10 tries); a top of another level resets; a lone level stays.
#[test]
fn the_walked_levels_below_the_high_level_go() {
    let mut m = Motion::new();
    let mut top = level(
        2,
        &[
            Point::new(0.0, 0.0),
            Point::new(5.0, 5.0),
            Point::new(9.0, 9.0),
        ],
        2,
        PathState::Arrived,
    );
    top.continue_counter = 3;
    m.paths = vec![
        level(0, &[Point::new(1.0, 1.0)], 1, PathState::Walking),
        level(1, &[Point::new(2.0, 2.0)], 1, PathState::Idle),
        top,
    ];
    m.drop_to_high_level().unwrap();
    assert_eq!(m.paths.len(), 1);
    let l = &m.paths[0];
    assert_eq!(
        (l.level.get(), l.waypoint, l.state, l.continue_counter),
        (2, 1, PathState::WantsPath, 10)
    );
    assert_eq!(l.points.len(), 3);
    m.paths = vec![
        level(0, &[Point::new(1.0, 1.0)], 1, PathState::Walking),
        level(1, &[Point::new(2.0, 2.0)], 1, PathState::Idle),
    ];
    m.paths[0].repath_time = 4.0;
    m.drop_to_high_level().unwrap();
    assert_eq!(m.paths, [PathData::without_path()]);
    let lone = vec![level(0, &[Point::new(1.0, 1.0)], 1, PathState::Walking)];
    m.paths.clone_from(&lone);
    m.drop_to_high_level().unwrap();
    assert_eq!(m.paths, lone);
}

/// `FUN_140f9b4d0`: a bit on or off the walked level; bit 2 also goes on for its parents.
#[test]
fn a_levels_flag_and_its_parents_bit_2() {
    let mut paths = vec![
        level(0, &[], 0, PathState::Walking),
        level(1, &[], 0, PathState::Idle),
        level(2, &[], 0, PathState::Idle),
    ];
    set_flag(&mut paths, 0, true);
    assert_eq!(paths[0].flags, 1);
    set_flag(&mut paths, 2, false);
    assert_eq!((paths[0].flags, paths[1].flags, paths[2].flags), (1, 4, 4));
    set_flag(&mut paths, 0, false);
    assert_eq!(paths[0].flags, 0);
    assert_eq!(paths[1].flags, 4);
}

/// `FUN_140f98e80`: the walked level and its parents go; the level keeps its repath time.
#[test]
fn a_reset_level_drops_its_parents_and_keeps_its_repath_time() {
    let mut paths = vec![
        level(0, &[Point::new(1.0, 1.0)], 1, PathState::Walking),
        level(2, &[], 0, PathState::Idle),
    ];
    paths[0].repath_time = 2.5;
    reset_level(&mut paths).unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(
        paths[0],
        PathData {
            repath_time: 2.5,
            ..PathData::without_path()
        }
    );
}

/// `FUN_140f98bc0`: a loop goes round; a path back and forth turns at its ends.
#[test]
fn a_looping_path_goes_round_or_back_and_forth() {
    let mut l = level(
        1,
        &[
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0),
        ],
        0,
        PathState::WantsPath,
    );
    assert_eq!(looping_next(&mut l, 0), -1);
    l.path_flags = 1;
    assert_eq!(looping_next(&mut l, 1), 2);
    assert_eq!(looping_next(&mut l, 2), 0);
    l.path_flags = 1 | 2;
    assert_eq!(looping_next(&mut l, 1), 0);
    assert_eq!(looping_next(&mut l, 0), 1);
    assert_eq!(l.path_flags & 4, 4);
    assert_eq!(looping_next(&mut l, 1), 2);
    assert_eq!(looping_next(&mut l, 2), 1);
    assert_eq!(l.path_flags & 4, 0);
}

/// `FUN_141046130`: short of the waypoint, on it, or past it (or a step under 0.1).
#[test]
fn a_step_falls_short_of_reaches_or_passes_its_waypoint() {
    let w = BVector::new(10.0, 0.0, 0.0);
    assert_eq!(
        passes_waypoint(w, BVector::ZERO, BVector::new(5.0, 0.0, 0.0)),
        Passing::Short
    );
    // a step that ends on the waypoint is not past it
    assert_eq!(
        passes_waypoint(
            w,
            BVector::new(9.95, 0.0, 0.0),
            BVector::new(10.0, 0.0, 0.0)
        ),
        Passing::Short
    );
    // one past it by less than 0.1 is
    assert_eq!(
        passes_waypoint(
            w,
            BVector::new(9.95, 0.0, 0.0),
            BVector::new(10.02, 0.0, 0.0)
        ),
        Passing::Passes
    );
    assert_eq!(
        passes_waypoint(w, BVector::new(9.0, 0.0, 0.0), BVector::new(10.0, 0.0, 0.0)),
        Passing::Short
    );
    // past it, but no farther beyond than it was before: on it
    assert_eq!(
        passes_waypoint(w, BVector::new(9.0, 0.0, 0.0), BVector::new(10.5, 0.0, 0.0)),
        Passing::Reaches
    );
    assert_eq!(
        passes_waypoint(w, BVector::new(9.0, 0.0, 0.0), BVector::new(11.0, 0.0, 0.0)),
        Passing::Reaches
    );
    assert_eq!(
        passes_waypoint(w, BVector::new(9.0, 0.0, 0.0), BVector::new(12.0, 0.0, 0.0)),
        Passing::Passes
    );
    // a NaN step is short (COMISS step, to the waypoint; JBE at 0x14104621d), where an
    // ordered compare would pass it on to the next tests
    assert_eq!(
        passes_waypoint(
            w,
            BVector::new(9.0, 0.0, 0.0),
            BVector::new(f32::NAN, 0.0, 0.0)
        ),
        Passing::Short
    );
}

/// `FUN_1411570c0`'s ends held inside the map.
#[test]
fn a_querys_ends_stay_inside_the_map() {
    assert_eq!(
        hold(BVector::new(-1.0, 5.0, 3.0), GridSize::new(10, 20)),
        Point::new(0.0, 3.0)
    );
    assert_eq!(
        hold(BVector::new(10.0, 0.0, 25.0), GridSize::new(10, 20)),
        Point::new(10.0 - NEAR, 20.0 - NEAR)
    );
}

/// `calculateYawAngle` with its clamp: a quarter turn to the left is negative, and limited
/// by the yaw per second times dt; without the clamp (`param_5` 0) the whole angle.
#[test]
fn a_turn_is_limited_by_the_yaw_per_second() {
    let mut m = Motion::new();
    let (x, z) = (BVector::new(1.0, 0.0, 0.0), BVector::new(0.0, 0.0, 1.0));
    let free = m.calculate_yaw_angle(x, z, Seconds::new(0.1), true);
    assert_eq!(free, -std::f32::consts::FRAC_PI_2);
    m.maximum_yaw_per_second_moving = TurnRate::new(1.0);
    assert_eq!(m.calculate_yaw_angle(x, z, Seconds::new(0.1), true), -0.1);
    assert_eq!(m.calculate_yaw_angle(z, x, Seconds::new(0.1), true), 0.1);
    assert_eq!(
        m.calculate_yaw_angle(z, x, Seconds::new(0.1), false),
        std::f32::consts::FRAC_PI_2
    );
}

/// The clamp is `MINSS limit, angle` (0x1410463e2): a NaN limit leaves the angle, a NaN
/// angle stays NaN (an ordered `<=` would take the limit both times); a NaN cross product
/// keeps the sign (`COMISS`/`JBE`).
#[test]
fn the_yaw_clamp_keeps_the_angle_for_a_nan_operand() {
    let mut m = Motion::new();
    let (x, z) = (BVector::new(1.0, 0.0, 0.0), BVector::new(0.0, 0.0, 1.0));
    m.maximum_yaw_per_second_moving = TurnRate::new(f32::NAN);
    assert_eq!(
        m.calculate_yaw_angle(x, z, Seconds::new(0.1), true),
        -std::f32::consts::FRAC_PI_2
    );
    m.maximum_yaw_per_second_moving = TurnRate::new(1.0);
    let nan = m.calculate_yaw_angle(BVector::new(f32::NAN, 0.0, 0.0), z, Seconds::new(0.1), true);
    assert!(nan.is_nan(), "{nan}");
}

/// `FUN_14104c150` turns the new position's forward and right about its up; a turn of 1e-7
/// or less is none. The integration then moves the group along its forward and copies the
/// new position.
#[test]
fn the_motion_turns_then_moves_along_its_forward() {
    let mut m = Motion::new();
    m.flags = 0;
    m.turn(1e-8);
    assert_eq!((m.new_position, m.flags), (m.position, 0));
    m.turn(-std::f32::consts::FRAC_PI_2);
    assert_eq!(m.flags, 1);
    let f = m.new_position[1];
    assert!((f.y() - 1.0).abs() < 1e-6 && f.x().abs() < 1e-6, "{f:?}");
    m.flags = 0;
    m.velocity = BVector::new(0.0, 0.0, 2.0);
    m.acceleration = BVector::new(0.0, 0.0, 1.0);
    m.integrate(Seconds::new(0.5)).unwrap();
    assert_eq!(m.velocity.y(), 2.5);
    let p = m.position[0];
    assert!((p.y() - 1.25).abs() < 1e-6 && p.x().abs() < 1e-6, "{p:?}");
    assert_eq!(m.position, m.new_position);
    assert_eq!(m.flags, 1);
}

struct Fixture {
    spatial: Spatial,
    pathing: Pathing,
    object: Well512,
    groups: UnitGroups,
}

impl Fixture {
    fn new() -> Self {
        Self {
            spatial: Spatial::new(GridSize::new(40, 40)),
            pathing: Pathing::default(),
            object: Well512::default(),
            groups: UnitGroups::default(),
        }
    }

    fn step(&mut self, m: &mut Motion, group: &GroupView, lead: &Lead) -> bool {
        let mut plan = Planning {
            pathing: &mut self.pathing,
            now: WorldTime::from_millis(100),
            computer: false,
            object: &mut self.object,
            construction_rate: 0.0,
            groups: &self.groups,
        };
        let mut world = StepWorld {
            spatial: &mut self.spatial,
            plan: &mut plan,
            dt: Seconds::new(0.1),
        };
        path_step(m, group, lead, &mut world, false).unwrap()
    }
}

fn lead() -> Lead {
    Lead {
        walker: Walker {
            id: object_id(7),
            radius: Distance::new(0.2),
            restriction: RestrictionId::named(0),
            mask: TerrainMask::ALL,
        },
        position: Position::new(5.0, 5.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        moves: true,
        solid: true,
        ignore_units: Vec::new(),
        player: PlayerId::new(1),
        computer: false,
        group: None,
    }
}

/// A walked level heads for its waypoint: the group turns to it and takes its top speed.
#[test]
fn a_walked_level_turns_the_group_toward_its_waypoint() {
    let mut fx = Fixture::new();
    let mut m = Motion::new();
    m.position[0] = BVector::new(5.0, 0.0, 5.0);
    m.new_position[0] = BVector::new(5.0, 0.0, 5.0);
    m.maximum_velocity_forward = Speed::new(1.2);
    m.paths = vec![level(
        0,
        &[Point::new(5.0, 5.0), Point::new(5.0, 9.0)],
        1,
        PathState::Walking,
    )];
    let group = GroupView::default();
    assert!(fx.step(&mut m, &group, &lead()));
    assert_eq!(m.velocity.y(), 1.2);
    let f = m.new_position[1];
    assert!((f.y() - 1.0).abs() < 1e-6 && f.x().abs() < 1e-6, "{f:?}");
}

/// A reached last waypoint of a top level without a parent arrives: the level resets and
/// the group stands.
#[test]
fn the_last_waypoint_of_the_top_level_ends_the_walk() {
    let mut fx = Fixture::new();
    let mut m = Motion::new();
    m.position[0] = BVector::new(5.0, 0.0, 9.0);
    m.velocity = BVector::new(0.0, 0.0, 1.2);
    let mut top = level(
        2,
        &[Point::new(5.0, 5.0), Point::new(5.0, 9.0)],
        1,
        PathState::Walking,
    );
    top.flags = 1;
    m.paths = vec![top];
    assert!(fx.step(&mut m, &GroupView::default(), &lead()));
    assert_eq!(m.paths, vec![PathData::without_path()]);
    assert_eq!(m.velocity.y(), 0.0);
}

/// An arrived walked level linked as found hands over to its parent, whose waypoint then
/// counts as reached; a pause makes the group stand until it ran out.
#[test]
fn an_arrived_level_hands_over_and_a_pause_waits() {
    let mut fx = Fixture::new();
    let mut m = Motion::new();
    m.position[0] = BVector::new(5.0, 0.0, 9.0);
    let mut walked = level(
        0,
        &[Point::new(5.0, 5.0), Point::new(5.0, 9.0)],
        1,
        PathState::Arrived,
    );
    walked.linked_type = 1;
    let parent = level(
        2,
        &[Point::new(5.0, 5.0), Point::new(5.0, 9.0)],
        1,
        PathState::Idle,
    );
    m.paths = vec![walked, parent];
    // the parent's reached last waypoint ends the walk in the same step
    assert!(fx.step(&mut m, &GroupView::default(), &lead()));
    assert_eq!(m.paths.len(), 1);
    assert_eq!(m.paths[0].state, PathState::Idle);
    let mut m = Motion::new();
    m.velocity = BVector::new(0.0, 0.0, 1.0);
    let mut paused = level(
        0,
        &[Point::new(5.0, 5.0), Point::new(5.0, 9.0)],
        1,
        PathState::Walking,
    );
    paused.pause_time = Seconds::new(0.3);
    m.paths = vec![paused];
    assert!(!fx.step(&mut m, &GroupView::default(), &lead()));
    assert_eq!(m.velocity.y(), 0.0);
    assert!((m.paths[0].pause_time - Seconds::new(0.2)).get().abs() < 1e-6);
}

impl Fixture {
    /// The request of `m`'s walked level (state 2) for `lead`, at tick 100.
    fn request(
        &mut self,
        m: &mut Motion,
        lead: &Lead,
        budget: bool,
        target: Option<&Placed>,
        target_gap2: DistanceSquared,
    ) -> Option<Pass> {
        let mut plan = Planning {
            pathing: &mut self.pathing,
            now: WorldTime::from_millis(100),
            computer: false,
            object: &mut self.object,
            construction_rate: 0.0,
            groups: &self.groups,
        };
        let mut world = StepWorld {
            spatial: &mut self.spatial,
            plan: &mut plan,
            dt: Seconds::new(0.1),
        };
        request(m, lead, &mut world, budget, target, target_gap2).unwrap()
    }
}

/// A group at (5, 5) whose walked level (level `n`, state 2) heads for (5, `y`).
fn requesting(n: u8, y: f32) -> Motion {
    let mut m = Motion::new();
    m.position[0] = BVector::new(5.0, 0.0, 5.0);
    m.paths = vec![level(
        n,
        &[Point::new(5.0, 5.0), Point::new(5.0, y)],
        1,
        PathState::WantsPath,
    )];
    m
}

/// A target of radius 0.5 at (5, `y`).
fn target_at(y: f32) -> Placed {
    Placed {
        position: Position::new(5.0, y, 0.0),
        radius: Footprint::new(0.5, 0.5),
        class: class::CLASS_0,
        player: PlayerId::new(0),
        master: ids::ID_0,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: false,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: None,
    }
}

/// A lead in a group: `flags` 0x800 patrols.
fn grouped(flags: u32) -> Lead {
    Lead {
        group: Some(LeadGroup {
            flags,
            members: vec![object_id(7)],
            ..LeadGroup::default()
        }),
        ..lead()
    }
}

/// The request's turn (`0x14104a80b`): a computer player over its path budget waits, its wait
/// delays counted; else its plan counts and the delays go back to 0. A level whose top does not
/// loop, of a group that does not patrol, spends a plan and gives up with the last; a patrol's
/// level, or a lone lead's, keeps its plans.
#[test]
fn a_request_over_the_budget_waits_and_the_last_plan_spent_gives_up() {
    use crate::pathing::PlanBudget;
    let mut fx = Fixture::new();
    let budget = |plans| PlanBudget {
        attempt_cap: 1,
        delay_cap: -1,
        plans,
    };
    fx.pathing.budgets = vec![budget(0), budget(1)];
    let mut m = requesting(0, 30.0);
    m.velocity = BVector::new(0.0, 0.0, 1.0);
    assert!(matches!(
        fx.request(&mut m, &lead(), true, None, DistanceSquared::new(f32::MAX)),
        Some(Pass::Done(false))
    ));
    assert_eq!((m.number_wait_delays, m.velocity.y()), (1, 0.0));
    assert_eq!(fx.pathing.budgets[1].plans, 1);
    let mut m = requesting(0, 6.0);
    m.paths[0].continue_counter = 1;
    m.number_wait_delays = 4;
    let p = fx.request(
        &mut m,
        &grouped(0),
        false,
        None,
        DistanceSquared::new(f32::MAX),
    );
    assert!(matches!(p, Some(Pass::Done(false))));
    assert_eq!(fx.pathing.budgets[1].plans, 2);
    assert_eq!(
        (m.number_wait_delays, m.paths.clone()),
        (0, vec![PathData::without_path()])
    );
    // (a lead the short pather does not serve: the near waypoint counts as reached)
    for lead in [grouped(0x800), lead()] {
        let mut m = requesting(0, 6.0);
        m.paths[0].continue_counter = 1;
        let lead = Lead {
            solid: false,
            ..lead
        };
        let _ = fx.request(&mut m, &lead, false, None, DistanceSquared::new(f32::MAX));
        assert_eq!(m.paths.last().map(|l| l.continue_counter), Some(1));
    }
}

/// Without a tile planner for the lead's restriction a top level's plan is the straight way:
/// it becomes level 1 in state 2 under the top (state 0, its plans back to 10), linked as
/// found, and the next pass makes its stretches.
#[test]
fn a_top_level_without_a_planner_plans_the_straight_way() {
    let mut fx = Fixture::new();
    let mut m = requesting(2, 30.0);
    m.paths[0].continue_counter = 4;
    assert!(matches!(
        fx.request(&mut m, &lead(), false, None, DistanceSquared::new(f32::MAX)),
        Some(Pass::Next)
    ));
    assert_eq!(m.paths.len(), 2);
    let (new, top) = (&m.paths[0], &m.paths[1]);
    assert_eq!(
        (new.level.get(), new.state, new.linked_type, new.waypoint),
        (1, PathState::WantsPath, 1, 1)
    );
    assert_eq!(
        new.points,
        [BVector::new(5.0, 0.0, 5.0), BVector::new(5.0, 0.0, 30.0)]
    );
    assert_eq!(
        (new.length, new.created),
        (25.0, WorldTime::from_millis(100))
    );
    assert_eq!((top.state, top.continue_counter), (PathState::Idle, 10));
}

/// A near waypoint the short pather finds no way to: a group without a target counts it
/// reached, and at the last waypoint of a level made in this tick the walk ends there (the
/// level resets in state 6, the group stands); a group within reach of its target arrived
/// (state 4), the reach [`NEAR`] for a level without a range.
#[test]
fn a_near_waypoint_without_a_way_is_reached_or_arrived_at() {
    let mut fx = Fixture::new();
    let lead = Lead {
        solid: false,
        ..lead()
    };
    let mut m = requesting(0, 6.0);
    assert!(matches!(
        fx.request(&mut m, &lead, false, None, DistanceSquared::new(f32::MAX)),
        Some(Pass::Reached)
    ));
    m.paths[0].created = WorldTime::from_millis(100);
    m.velocity = BVector::new(0.0, 0.0, 1.0);
    assert!(matches!(
        fx.request(&mut m, &lead, false, None, DistanceSquared::new(f32::MAX)),
        Some(Pass::Done(true))
    ));
    assert_eq!(
        (m.paths[0].state, m.paths[0].has_path),
        (PathState::GaveUp, false)
    );
    assert_eq!(m.velocity.y(), 0.0);
    let mut m = requesting(0, 6.0);
    let placed = target_at(6.0);
    assert!(matches!(
        fx.request(
            &mut m,
            &lead,
            false,
            Some(&placed),
            DistanceSquared::new(NEAR)
        ),
        Some(Pass::Next)
    ));
    assert_eq!(m.paths[0].state, PathState::Arrived);
    let mut m = requesting(0, 6.0);
    assert!(matches!(
        fx.request(
            &mut m,
            &lead,
            false,
            Some(&placed),
            DistanceSquared::new(0.2)
        ),
        Some(Pass::Reached)
    ));
}

/// A far waypoint whose target is near takes the straight way when nothing stands on it: a
/// walked level 0 (state 3), linked as found, steers at once (no pass ends the request).
#[test]
fn a_near_target_takes_the_straight_way() {
    let mut fx = open_fixture();
    let mut m = requesting(0, 30.0);
    let placed = target_at(30.0);
    assert!(
        fx.request(
            &mut m,
            &lead(),
            false,
            Some(&placed),
            DistanceSquared::new(0.5)
        )
        .is_none()
    );
    let new = &m.paths[0];
    assert_eq!(
        (new.level.get(), new.state, new.linked_type),
        (0, PathState::Walking, 1)
    );
    assert_eq!(
        new.points,
        [BVector::new(5.0, 0.0, 5.0), BVector::new(5.0, 0.0, 30.0)]
    );
}

/// [`Fixture::new`] on an open map of 40 by 40 tiles.
fn open_fixture() -> Fixture {
    let mut fx = Fixture::new();
    fx.spatial = Spatial::with_tiles(
        GridSize::new(40, 40),
        crate::map::spatial::Tiles::new(GridSize::new(40, 40), vec![4; 40 * 40]),
    );
    fx
}

/// A near waypoint takes the short pather's path (a lead wider than half a tile: the old
/// pather): a walked level 0 linked as found, which steers at once.
#[test]
fn a_near_waypoint_takes_the_short_pather_s_path() {
    let mut fx = open_fixture();
    let mut m = requesting(0, 9.0);
    let mut lead = lead();
    lead.walker.radius = Distance::new(0.6);
    assert!(
        fx.request(&mut m, &lead, false, None, DistanceSquared::new(f32::MAX))
            .is_none()
    );
    let new = &m.paths[0];
    assert_eq!(
        (new.level.get(), new.state, new.linked_type),
        (0, PathState::Walking, 1)
    );
    assert_eq!(new.points.first(), Some(&BVector::new(5.0, 0.0, 5.0)));
    assert_eq!(new.points.last(), Some(&BVector::new(5.0, 0.0, 9.0)));
    assert_eq!(m.paths[1].state, PathState::Idle);
}

/// A far waypoint without a near target takes the long pather's path through the tiles'
/// centres: a walked level 0 linked as found.
#[test]
fn a_far_waypoint_takes_the_long_pather_s_path() {
    let mut fx = open_fixture();
    let mut m = requesting(0, 30.0);
    assert!(
        fx.request(&mut m, &lead(), false, None, DistanceSquared::new(f32::MAX))
            .is_none()
    );
    let new = &m.paths[0];
    assert_eq!(
        (new.level.get(), new.state, new.linked_type),
        (0, PathState::Walking, 1)
    );
    assert_eq!(
        new.points,
        [BVector::new(5.5, 0.0, 5.5), BVector::new(5.5, 0.0, 30.5)]
    );
}

/// The request's end (`LAB_14104b4c5`) without a path to link (a top level's not found, or
/// fewer than two points): with fewer than three plans left, at its last waypoint a patrol or a
/// looping path pauses, a level below the top waits for a top with waypoints left (state 5), any
/// other gives up; before its last waypoint the level counts it reached. With more plans it
/// pauses a moment. A found path of fewer than two points arrived (the level resets), or counts
/// the waypoint reached on a looping path.
#[test]
fn a_request_without_a_path_pauses_waits_gives_up_or_counts_its_waypoint() {
    use super::request::link;
    use crate::pathing::tile_path::Path;
    let at = |n: u8, waypoints: &[Point], counter| {
        let mut m = Motion::new();
        m.velocity = BVector::new(0.0, 0.0, 1.0);
        m.paths = vec![level(
            n,
            waypoints,
            waypoints.len() as i32 - 1,
            PathState::WantsPath,
        )];
        m.paths[0].continue_counter = counter;
        m
    };
    let last = [Point::new(5.0, 5.0), Point::new(5.0, 9.0)];
    let end = |m: &mut Motion, lead: &Lead, found| {
        link(
            m,
            lead,
            Path::default(),
            (found, false),
            WorldTime::from_millis(100),
        )
        .unwrap()
    };
    let mut m = at(2, &last, 2);
    assert!(matches!(
        end(&mut m, &grouped(0x800), false),
        Some(Pass::Done(false))
    ));
    assert_eq!(m.paths[0].pause_time, PATROL_PAUSE);
    let mut m = at(2, &last, 2);
    m.paths[0].path_flags |= 1;
    assert!(matches!(
        end(&mut m, &lead(), false),
        Some(Pass::Done(false))
    ));
    assert_eq!(m.paths[0].pause_time, PATROL_PAUSE);
    let mut m = at(0, &last, 2);
    m.paths.push(level(
        2,
        &[
            Point::new(0.0, 0.0),
            Point::new(5.0, 5.0),
            Point::new(9.0, 9.0),
        ],
        1,
        PathState::Idle,
    ));
    assert!(matches!(end(&mut m, &lead(), false), Some(Pass::Next)));
    assert_eq!(m.paths[0].state, PathState::Unplanned);
    let mut m = at(2, &last, 2);
    assert!(matches!(
        end(&mut m, &lead(), false),
        Some(Pass::Done(false))
    ));
    assert_eq!(
        (m.paths.clone(), m.velocity.y()),
        (vec![PathData::without_path()], 0.0)
    );
    let mut m = at(2, &last, 2);
    m.paths[0].waypoint = 0;
    assert!(matches!(end(&mut m, &lead(), false), Some(Pass::Reached)));
    let mut m = at(2, &last, 3);
    assert!(matches!(
        end(&mut m, &lead(), false),
        Some(Pass::Done(false))
    ));
    assert_eq!((m.paths[0].pause_time, m.velocity.y()), (STUCK_PAUSE, 0.0));
    let mut m = at(0, &last, 5);
    assert!(matches!(end(&mut m, &lead(), true), Some(Pass::Done(true))));
    assert_eq!(m.paths, vec![PathData::without_path()]);
    let mut m = at(0, &last, 5);
    m.paths[0].path_flags |= 1;
    assert!(matches!(end(&mut m, &lead(), true), Some(Pass::Reached)));
}

/// The reached waypoint (`0x14104a4bf`): with bit 1 the group takes the waypoint's place, and
/// bits 0 and 1 clear; with waypoints left a level above 0 asks for the next stretch (state 2),
/// level 0 walks on; a looping path takes its next waypoint and asks again.
#[test]
fn a_reached_waypoint_moves_the_level_on() {
    use step::reached;
    let three = [
        Point::new(5.0, 5.0),
        Point::new(5.0, 9.0),
        Point::new(9.0, 9.0),
    ];
    let mut m = Motion::new();
    m.paths = vec![level(1, &three, 1, PathState::Walking)];
    m.paths[0].flags = 3;
    reached(&mut m, None).unwrap();
    assert_eq!(
        (m.position[0], m.new_position[0]),
        (BVector::new(5.0, 0.0, 9.0), BVector::new(5.0, 0.0, 9.0))
    );
    let l = &m.paths[0];
    assert_eq!(
        (l.flags & 3, l.waypoint, l.state),
        (0, 2, PathState::WantsPath)
    );
    m.paths = vec![level(0, &three, 1, PathState::Walking)];
    reached(&mut m, None).unwrap();
    assert_eq!(
        (m.paths[0].waypoint, m.paths[0].state),
        (2, PathState::Walking)
    );
    m.paths = vec![level(1, &three, 2, PathState::Walking)];
    m.paths[0].path_flags = 1;
    reached(&mut m, None).unwrap();
    assert_eq!(
        (m.paths[0].waypoint, m.paths[0].state),
        (0, PathState::WantsPath)
    );
}

/// Past the last waypoint of a path that does not loop: a group farther than the range plus
/// [`NEAR`] from the waypoint it left steps back to it and asks again (state 2) when that is
/// level 0's, or at least [`CLOSE`]; else it arrived (state 4). With a target the gap to its
/// footprint counts; a level that reached its target with bit 2 arrived, the bit cleared.
#[test]
fn past_its_last_waypoint_a_level_steps_back_or_arrived() {
    use step::reached;
    let two = [Point::new(5.0, 5.0), Point::new(5.0, 9.0)];
    let state = |n: u8, at: Point, target: Option<&Placed>, flags: u32| {
        let mut m = Motion::new();
        m.position[0] = BVector::new(at.x(), 0.0, at.y());
        m.paths = vec![level(n, &two, 1, PathState::Walking)];
        m.paths[0].flags = flags;
        reached(&mut m, target).unwrap();
        (m.paths[0].waypoint, m.paths[0].state, m.paths[0].flags & 4)
    };
    assert_eq!(
        state(0, Point::new(5.0, 5.0), None, 0),
        (1, PathState::WantsPath, 0)
    );
    assert_eq!(
        state(0, Point::new(5.0, 9.0), None, 0),
        (2, PathState::Arrived, 0)
    );
    // 0.6 short: beyond 0.5, below CLOSE: level 0 steps back, level 1 arrived
    assert_eq!(
        state(0, Point::new(5.0, 8.4), None, 0),
        (1, PathState::WantsPath, 0)
    );
    assert_eq!(
        state(1, Point::new(5.0, 8.4), None, 0),
        (2, PathState::Arrived, 0)
    );
    let target = target_at(12.0);
    assert_eq!(
        state(1, Point::new(5.0, 8.0), Some(&target), 0),
        (1, PathState::WantsPath, 0)
    );
    assert_eq!(
        state(1, Point::new(5.0, 8.0), Some(&target), 4),
        (2, PathState::Arrived, 0)
    );
    // on the waypoint it left the group arrived, its target however far
    assert_eq!(
        state(1, Point::new(5.0, 9.0), Some(&target), 0),
        (2, PathState::Arrived, 0)
    );
}

impl Fixture {
    /// One pass of `m`'s path step for `lead`, at tick 100.
    fn pass(&mut self, m: &mut Motion, lead: &Lead) -> Pass {
        let mut plan = Planning {
            pathing: &mut self.pathing,
            now: WorldTime::from_millis(100),
            computer: false,
            object: &mut self.object,
            construction_rate: 0.0,
            groups: &self.groups,
        };
        let mut world = StepWorld {
            spatial: &mut self.spatial,
            plan: &mut plan,
            dt: Seconds::new(0.1),
        };
        step::pass(m, &GroupView::default(), lead, &mut world, false).unwrap()
    }
}

/// An arrived level (state 4) linked as not found (2) hands over to its parent, which asks
/// again (state 2): a looping parent from its next waypoint, a patrol's from the one after
/// (arrived past its last); linked as given up (3) the parent only asks again. A level whose
/// bit 0 is set counts its waypoint reached first; a level in state 2 ends the pass as its
/// request does, its target the world's object.
#[test]
fn an_arrived_level_hands_over_by_its_link() {
    let three = [
        Point::new(5.0, 5.0),
        Point::new(5.0, 9.0),
        Point::new(9.0, 9.0),
    ];
    let mut fx = Fixture::new();
    let handed = |fx: &mut Fixture, linked, path_flags, lead: &Lead, waypoint| {
        let mut m = Motion::new();
        let mut walked = level(
            0,
            &[Point::new(5.0, 5.0), Point::new(5.0, 9.0)],
            2,
            PathState::Arrived,
        );
        walked.linked_type = linked;
        let mut parent = level(2, &three, waypoint, PathState::Idle);
        parent.path_flags = path_flags;
        m.paths = vec![walked, parent];
        assert!(matches!(fx.pass(&mut m, lead), Pass::Next));
        assert_eq!(m.paths.len(), 1);
        (m.paths[0].waypoint, m.paths[0].state)
    };
    assert_eq!(handed(&mut fx, 2, 1, &lead(), 1), (2, PathState::WantsPath));
    assert_eq!(
        handed(&mut fx, 2, 0, &grouped(0x800), 1),
        (2, PathState::WantsPath)
    );
    assert_eq!(
        handed(&mut fx, 2, 0, &grouped(0x800), 2),
        (3, PathState::Arrived)
    );
    assert_eq!(
        handed(&mut fx, 3, 1, &grouped(0x800), 1),
        (1, PathState::WantsPath)
    );
    // bit 0: the waypoint counts reached (past the last, 0.5 from the target: back to it, state
    // 2), then the request (a lead the short pather does not serve, short of the target's
    // reach: the waypoint counts reached again)
    let lead = Lead {
        solid: false,
        ..lead()
    };
    let mut m = requesting(0, 6.0);
    m.paths[0].flags = 1;
    m.paths[0].target = Some(object_id(7));
    fx.spatial.insert(object_id(7), target_at(6.0));
    assert!(matches!(fx.pass(&mut m, &lead), Pass::Reached));
    assert_eq!(
        (m.paths[0].waypoint, m.paths[0].state),
        (1, PathState::WantsPath)
    );
}

/// A target of radius 0.2 at (`x`, `y`) with its obstruction record (a circle).
fn placed_at(x: f32, y: f32) -> Placed {
    Placed {
        position: Position::new(x, y, 0.0),
        radius: Footprint::new(0.2, 0.2),
        obstruction: Some(crate::map::spatial::Obstruction {
            kind: ObstructionType::CIRCLE,
            class: ObstructionClass::UNIT,
        }),
        ..target_at(y)
    }
}

/// What stands in the way of the step: the walked level's target alone sets bits 0 and 2; an
/// object moved after the level was made sends the walk back to its parent (state 2), and a
/// level without a parent resets, the group standing; one that moved before does not stop it.
#[test]
fn what_stands_in_the_way_stops_the_walk() {
    use crate::map::spatial::Gait;
    let mut fx = open_fixture();
    let walking = |target| {
        let mut m = requesting(0, 9.0);
        m.paths[0].state = PathState::Walking;
        m.paths[0].target = target;
        m.paths[0].created = WorldTime::from_millis(50);
        m
    };
    fx.spatial.insert(object_id(9), placed_at(5.0, 5.3));
    let mut m = walking(Some(object_id(9)));
    assert!(matches!(fx.pass(&mut m, &lead()), Pass::Next));
    assert_eq!(m.paths[0].flags & 5, 5);
    // 8 in the way moved at 60
    fx.spatial.set_gait(
        object_id(8),
        Some(Gait {
            last_move: WorldTime::from_millis(60),
            ..Gait::default()
        }),
    );
    fx.spatial.remove(object_id(9));
    fx.spatial.insert(object_id(8), placed_at(5.0, 5.3));
    let mut m = walking(None);
    m.paths.push(level(
        2,
        &[Point::new(5.0, 5.0), Point::new(9.0, 9.0)],
        1,
        PathState::Idle,
    ));
    assert!(matches!(fx.pass(&mut m, &lead()), Pass::Next));
    assert_eq!(
        (m.paths.len(), m.paths[0].level.get(), m.paths[0].state),
        (1, 2, PathState::WantsPath)
    );
    let mut m = walking(None);
    m.velocity = BVector::new(0.0, 0.0, 1.0);
    assert!(matches!(fx.pass(&mut m, &lead()), Pass::Done(true)));
    assert_eq!(
        (m.paths.clone(), m.velocity.y()),
        (vec![PathData::without_path()], 0.0)
    );
    // one that moved before the level was made: the walk goes on at its top speed
    let mut m = walking(None);
    m.paths[0].created = WorldTime::from_millis(70);
    m.maximum_velocity_forward = Speed::new(1.2);
    assert!(matches!(fx.pass(&mut m, &lead()), Pass::Done(true)));
    assert_eq!(
        (m.paths[0].state, m.velocity.y()),
        (PathState::Walking, 1.2)
    );
}

/// The walk's end in range: a group within the walked level's range of the top level's last
/// point arrived (the level resets); one whose gap to its target is the range counts the
/// waypoint reached.
#[test]
fn a_walk_in_range_of_its_end_stops() {
    let mut fx = open_fixture();
    let mut m = requesting(0, 9.0);
    m.paths[0].state = PathState::Walking;
    m.paths[0].range = Distance::new(1.0);
    m.paths.push(level(
        2,
        &[Point::new(5.0, 5.0), Point::new(5.0, 5.5)],
        1,
        PathState::Idle,
    ));
    assert!(matches!(fx.pass(&mut m, &lead()), Pass::Done(true)));
    assert_eq!(m.paths, vec![PathData::without_path()]);
    fx.spatial.insert(
        object_id(7),
        Placed {
            radius: Footprint::new(0.5, 0.5),
            ..placed_at(5.0, 3.0)
        },
    );
    let mut m = requesting(0, 9.0);
    m.paths[0].state = PathState::Walking;
    (m.paths[0].range, m.paths[0].target) = (Distance::new(1.5), Some(object_id(7)));
    assert!(matches!(fx.pass(&mut m, &lead()), Pass::Reached));
}

/// The members' centre pulls the group while every member acts on a target: from 3 or more
/// away its new place takes a share of the centre (on the ground); a member without a target
/// pulls nothing.
#[test]
fn the_members_centre_pulls_a_group_whose_members_all_act() {
    let mut fx = open_fixture();
    let walk = |fx: &mut Fixture, members: Vec<Option<(Option<ObjectId>, BVector)>>| {
        let mut m = requesting(0, 9.0);
        m.paths[0].state = PathState::Walking;
        m.new_position[0] = BVector::new(5.0, 0.0, 5.0);
        let group = GroupView {
            computer: false,
            members,
        };
        let mut plan = Planning {
            pathing: &mut fx.pathing,
            now: WorldTime::from_millis(100),
            computer: false,
            object: &mut fx.object,
            construction_rate: 0.0,
            groups: &fx.groups,
        };
        let mut world = StepWorld {
            spatial: &mut fx.spatial,
            plan: &mut plan,
            dt: Seconds::new(0.1),
        };
        assert!(matches!(
            step::pass(&mut m, &group, &lead(), &mut world, false).unwrap(),
            Pass::Done(true)
        ));
        m.new_position[0]
    };
    let far = walk(
        &mut fx,
        vec![
            Some((Some(object_id(20)), BVector::new(15.0, 1.0, 5.0))),
            Some((Some(object_id(20)), BVector::new(15.0, 3.0, 5.0))),
        ],
    );
    let pulled = |c: f32, own: f32| PULL_SHARE * c + PULL_KEEP * own;
    assert_eq!(
        far,
        BVector::new(pulled(15.0, 5.0), pulled(0.0, 0.0), pulled(5.0, 5.0))
    );
    let idle = walk(
        &mut fx,
        vec![
            Some((Some(object_id(20)), BVector::new(15.0, 1.0, 5.0))),
            Some((None, BVector::new(15.0, 3.0, 5.0))),
        ],
    );
    assert_eq!(idle, BVector::new(5.0, 0.0, 5.0));
}
