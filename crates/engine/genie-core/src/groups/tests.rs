use super::*;
use crate::combat::attack::ATTACK;
use crate::gathering::gather::GATHER;
use crate::gathering::hunt::tests::walker;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::map::spatial::Placed;
use crate::math::rng::Stream;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::Static;
use crate::objects::masters::ActionKind;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatPart;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::FormationClass;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::TargetOwner;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::{Master, Masters, Task, WorkValue};
use crate::objects::unit::Unit;
use crate::pathing::DelayCount;
use crate::players::Amount;
use crate::test_ids::class_id;
use crate::test_ids::group_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_ids;
use crate::testing::class;
use crate::testing::ids;
use crate::unit_ai::UnitAi;
use crate::world::World;
use crate::world::{FrameSeed, Player};
use std::sync::Arc;

/// `BMotion`'s constructor (`FUN_141041850`): the 0x7f7fffff turn limits at `+0x9c`,
/// `+0xa0`, `+0xa8`, zero speeds and accelerations, the lead (`+0xc4`) -1, flags 3, the
/// frame at the origin facing x, one level without a path.
#[test]
fn new_motion_holds_the_constructor_values() {
    let m = Motion::new();
    assert_eq!(
        (
            m.turn_radius_speed,
            m.maximum_yaw_per_second_moving,
            m.maximum_yaw_per_second_stationary
        ),
        (Speed::new(f32::MAX), TurnRate::MAX, TurnRate::MAX)
    );
    assert_eq!(
        (m.flags, m.lead, m.number_wait_delays),
        (
            (MotionFlags::MOVED | MotionFlags::NEW),
            None,
            DelayCount::ZERO
        )
    );
    assert_eq!((m.velocity, m.acceleration), (BVector::ZERO, BVector::ZERO));
    let frame = [
        BVector::ZERO,
        BVector::new(1.0, 0.0, 0.0),
        BVector::new(0.0, 1.0, 0.0),
        BVector::new(0.0, 0.0, -1.0),
    ];
    assert_eq!((m.position, m.new_position), (frame, frame));
    assert_eq!(m.paths, vec![PathData::without_path()]);
}

/// `FUN_14152ece0` over units 1, 2 (group 7), 3 (group 8), 4 (no group), 9 (gone) and a null
/// entry.
#[test]
fn the_common_group_needs_every_held_unit_in_one_group_and_the_first_held() {
    let link = |id: Option<GroupId>| GroupLink {
        id,
        ..GroupLink::default()
    };
    let group_of = |id: ObjectId| match id.get() {
        1 | 2 => Some(link(Some(group_id(7)))),
        3 => Some(link(Some(group_id(8)))),
        4 => Some(link(None)),
        _ => None,
    };
    let common = |ids: &[i32]| common_group(&object_ids(ids.iter().copied()), group_of);
    assert_eq!(common(&[1, 2]), Some(group_id(7)));
    assert_eq!(common(&[1, 9, 2]), Some(group_id(7)));
    assert_eq!(common(&[1, -1, 2]), Some(group_id(7)));
    assert_eq!(common(&[1, 3]), None);
    assert_eq!(common(&[1, 4]), None);
    // the first unit gone or a null: the others are compared with no group
    assert_eq!(common(&[9, 1, 2]), None);
    assert_eq!(common(&[-1, 1, 2]), None);
    assert_eq!(common(&[]), None);
    // the splitter's groups list ids without nulls
    assert_eq!(
        common_group(&[object_id(1), object_id(2)], group_of),
        Some(group_id(7))
    );
}

fn member(id: i32) -> Member {
    Member {
        id: object_id(id),
        ..Member::default()
    }
}

/// `FUN_1410175f0`: a member's place in the list, none for a unit that is no member, which
/// orders before every member as the game's −1 does.
#[test]
fn member_index_is_its_place_and_stranger_none() {
    let mut g = UnitGroup::new(group_id(1));
    g.members = vec![member(10), member(11)];
    assert_eq!(g.index_of(object_id(11)), Some(1));
    assert_eq!(g.index_of(object_id(12)), None);
    assert!(g.index_of(object_id(12)) < g.index_of(object_id(10)));
}

/// `FUN_141017590`: a member's own move's reason (`+0xf8`'s record at the member's index,
/// its `+8`), whatever its place in the list; −1 for a unit that is no member.
#[test]
fn member_reason_is_its_own_move_s() {
    let mut group = UnitGroup::new(group_id(1));
    let with_reason = |id, reason| Member {
        id,
        specific_move: SpecificMove {
            reason,
            ..SpecificMove::default()
        },
        ..Member::default()
    };
    group.members = vec![
        with_reason(object_id(10), MoveReason::ASIDE),
        with_reason(object_id(11), MoveReason::DETOUR),
    ];
    assert_eq!(group.member_reason(object_id(10)), MoveReason::ASIDE);
    assert_eq!(group.member_reason(object_id(11)), MoveReason::DETOUR);
    assert_eq!(group.member_reason(object_id(12)), MoveReason::NONE);
}

/// `FUN_1410177f0`: the first slot of the current formation (`+0x128`) that holds the unit
/// decides: in place when its state (`+0x2c`) is 3. A unit without a slot, and a group without
/// a current formation, is not in place; the previous formation is not read.
#[test]
fn unit_is_in_place_when_its_first_slot_in_the_current_formation_is() {
    let slot = |unit_id, state| Slot {
        unit_id,
        state,
        ..Slot::default()
    };
    let mut group = UnitGroup::new(group_id(1));
    group.previous_formation = Some(Formation {
        slots: vec![slot(Some(object_id(10)), SlotState::InPlace)],
        ..Formation::default()
    });
    assert!(!group.slot_in_place(object_id(10)));
    group.current_formation = Some(Formation {
        slots: vec![
            slot(Some(object_id(11)), SlotState::InPlace),
            slot(Some(object_id(10)), SlotState::Stopping),
            slot(Some(object_id(10)), SlotState::InPlace),
        ],
        ..Formation::default()
    });
    assert!(group.slot_in_place(object_id(11)));
    assert!(!group.slot_in_place(object_id(10)));
    assert!(!group.slot_in_place(object_id(12)));
}

/// `FUN_14100bbe0`: group 1's differentiator groups 1, 2 and 9 (gone) list 10, 11 and 12, the
/// gone one nothing; the cap stops the list.
#[test]
fn the_differentiator_units_are_the_members_of_the_listed_groups() {
    let mut one = UnitGroup::new(group_id(1));
    one.members = vec![member(10), member(11)];
    one.differentiator_groups = vec![group_id(1), group_id(2), group_id(9)];
    let mut two = UnitGroup::new(group_id(2));
    two.members = vec![member(12)];
    let groups = UnitGroups {
        groups: vec![one.clone(), two],
        ..UnitGroups::default()
    };
    assert_eq!(
        groups.differentiator_units(&one, 255),
        [object_id(10), object_id(11), object_id(12)]
    );
    assert_eq!(
        groups.differentiator_units(&one, 2),
        [object_id(10), object_id(11)]
    );
    let alone = UnitGroup::new(group_id(3));
    assert_eq!(groups.differentiator_units(&alone, 255), Vec::new());
}

/// `AddPointAtFront` (`FUN_141008040`) on a line of desired length 2: the newest point goes
/// first with its height 0, the length grows by each stretch, and past the desired length the
/// oldest points go while the line without its last stretch stays longer.
/// `FUN_14102bae0`: the points go, the lengths stay.
#[test]
fn clearing_lines_points_keeps_its_lengths() {
    let mut line = PointLine {
        points: vec![LinePoint::default(); 3],
        length: Distance::new(2.5),
        desired_length: Distance::new(4.0),
    };
    line.clear_points();
    assert_eq!(
        line,
        PointLine {
            points: Vec::new(),
            length: Distance::new(2.5),
            desired_length: Distance::new(4.0),
        }
    );
}

#[test]
fn the_stored_positions_keep_their_desired_length() {
    let at = |x: f32, h: f32| LinePoint {
        point: BVector::new(x, h, 0.0),
        ..LinePoint::default()
    };
    let mut line = PointLine {
        desired_length: Distance::new(2.0),
        ..PointLine::default()
    };
    line.add_at_front(at(0.0, 0.0)).unwrap();
    assert_eq!((line.points.len(), line.length), (1, Distance::ZERO));
    line.add_at_front(at(1.0, 5.0)).unwrap();
    assert_eq!(line.points[0].point, BVector::new(1.0, 0.0, 0.0));
    assert_eq!(line.length, Distance::new(1.0));
    line.add_at_front(at(2.0, 0.0)).unwrap();
    // 3 is longer than 2, but not without its last stretch (2): all stay
    line.add_at_front(at(3.0, 0.0)).unwrap();
    assert_eq!((line.points.len(), line.length), (4, Distance::new(3.0)));
    // 4 less 1 is 3: the oldest goes; 3 less 1 is 2: the rest stay
    line.add_at_front(at(4.0, 0.0)).unwrap();
    let xs: Vec<f32> = line.points.iter().map(|p| p.point.x()).collect();
    assert_eq!(xs, [4.0, 3.0, 2.0, 1.0]);
    assert_eq!(line.length, Distance::new(3.0));
}

#[test]
fn unit_sub_formation_follows_its_category_and_its_hero_flag() {
    assert_eq!(
        sub_formation(
            MasterKind::MISSILE,
            HeroFlags::NONE,
            FormationClass::of_data(3)
        ),
        None
    );
    assert_eq!(
        sub_formation(
            MasterKind::COMBAT,
            HeroFlags::NONE,
            FormationClass::of_data(3)
        ),
        Some(FormationCategory::named(3))
    );
    assert_eq!(
        sub_formation(
            MasterKind::COMBAT,
            HeroFlags::NONE,
            FormationClass::UNGROUPED
        ),
        Some(FormationCategory::PROTECTED)
    );
    assert_eq!(
        sub_formation(MasterKind::COMBAT, HeroFlags::NONE, FormationClass::FIRST),
        Some(FormationCategory::named(0))
    );
    assert_eq!(
        sub_formation(
            MasterKind::COMBAT,
            HeroFlags::NONE,
            FormationClass::of_data(255)
        ),
        None
    );
    assert_eq!(
        sub_formation(
            MasterKind::COMBAT,
            HeroFlags::FULL_HERO,
            FormationClass::of_data(3)
        ),
        Some(FormationCategory::PROTECTED)
    );
    assert_eq!(
        sub_formation(
            MasterKind::COMBAT,
            HeroFlags::PROTECTED_FORMATION | HeroFlags::INVERTED,
            FormationClass::of_data(3)
        ),
        Some(FormationCategory::named(3))
    );
    assert_eq!(
        sub_formation(
            MasterKind::BUILDING,
            HeroFlags::FULL_HERO,
            FormationClass::of_data(3)
        ),
        Some(FormationCategory::named(3))
    );
}

/// `FUN_140fe1550`: every value the constructor writes that the group holds.
#[test]
fn new_group_holds_the_constructor_values() {
    let g = UnitGroup::new(group_id(7));
    assert_eq!((g.id, g.owner, g.stance), (group_id(7), None, None));
    assert_eq!(
        (g.flags, g.current_command, g.differentiator_id),
        (
            GroupFlags::TURN,
            GroupCommandKind::NONE,
            Differentiator::NONE
        )
    );
    assert_eq!(g.current_target.target, GroupTarget::None);
    assert_eq!(
        (
            g.hibernating,
            g.replaces_queue,
            g.attack_plan_type,
            g.idle_timer
        ),
        (0, 1, 0, Millis::ZERO)
    );
    assert_eq!((g.previous_formation, g.current_formation), (None, None));
    assert!(g.members.is_empty() && g.unit_types.is_empty() && g.command_queue.is_empty());
    assert_eq!(
        (g.formation_locked_type, g.desired_spacing),
        (None, Distance::ZERO)
    );
    assert_eq!(g.motion, Motion::new());
}

#[test]
fn new_group_takes_the_next_id_at_the_list_end() {
    let mut groups = UnitGroups {
        counter: group_id(4),
        ..UnitGroups::default()
    };
    assert_eq!(groups.create(Some(PlayerId::new(2))), group_id(4));
    assert_eq!(groups.create(Some(PlayerId::new(1))), group_id(5));
    assert_eq!(groups.counter, group_id(6));
    assert_eq!(
        groups
            .groups
            .iter()
            .map(|g| (g.id, g.owner))
            .collect::<Vec<_>>(),
        [
            (group_id(4), Some(PlayerId::new(2))),
            (group_id(5), Some(PlayerId::new(1)))
        ]
    );
}

/// A world of player 1's unit 11 (master 83, a villager: type 70, class 4, swap group 1, state 2,
/// 25 hit points) and a building 60 (master 70, state 0) of player `owner`, the masters
/// `masters` for both players, player 1's attribute 90 at `owner_range`.
fn tasked_world(masters: Vec<Master>, owner_range: f32) -> World {
    use crate::objects::lifecycle::Lifecycle;
    let unit = |id, master, state, hp| Unit {
        id,
        life: Lifecycle {
            master_id: master,
            state,
            hp,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut attributes = vec![Amount::ZERO; 100];
    attributes[90] = Amount::new(owner_range);
    let mut t = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![
                    unit(
                        object_id(11),
                        ids::VILLAGER_MALE,
                        ObjectState::Alive,
                        HitPoints::new(25.0),
                    ),
                    unit(
                        object_id(60),
                        ids::HOUSE,
                        ObjectState::Foundation,
                        HitPoints::new(550.0),
                    ),
                ]
                .into(),
                attributes,
                ..Player::default()
            },
        ],
    );
    t.model.masters = vec![Arc::new(Masters::new([])), Arc::new(Masters::new(masters))];
    t
}

fn task_master(id: MasterId, tasks: Vec<Task>) -> Master {
    Master {
        id,
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        task_by_group: TaskGroup::MALE_VILLAGERS,
        hit_points: MasterHitPoints::of_data(25),
        tasks,
        ..Master::default()
    }
}

fn building_master() -> Master {
    Master {
        id: ids::HOUSE,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        hit_points: MasterHitPoints::of_data(550),
        ..Master::default()
    }
}

fn plain_task(action_type: ActionKind, range: Distance) -> Task {
    Task {
        action_kind: action_type,
        work_range: range,
        work_val1: WorkValue::new(1.0),
        ..Task::default()
    }
}

fn query_on(target: i32) -> task::TaskQuery {
    task::TaskQuery {
        target: ObjectId::try_from(target).unwrap(),
        point: Point::new(20.0, 20.0),
        filter: None,
        queued: false,
        direct: true,
    }
}

fn member_11() -> ObjectId {
    ObjectId::try_from(11).unwrap()
}

/// `FUN_14106b8b0`: the unit's own master's task is the command, with the task's range.
#[test]
fn member_own_task_is_the_command_with_the_task_range() {
    let t = tasked_world(
        vec![
            task_master(
                ids::VILLAGER_MALE,
                vec![plain_task(ActionKind::BUILD, Distance::new(2.5))],
            ),
            building_master(),
        ],
        9.0,
    );
    let got = t.member_task(member_11(), &query_on(60)).unwrap();
    assert_eq!(
        got,
        Some(task::TargetTask {
            command: ActionKind::BUILD,
            range: Some(Distance::new(2.5))
        })
    );
}

/// The slot answers none (-1) for a target that is the member itself.
#[test]
fn member_has_no_task_on_itself() {
    let t = tasked_world(
        vec![
            task_master(
                ids::VILLAGER_MALE,
                vec![plain_task(ActionKind::BUILD, Distance::new(2.5))],
            ),
            building_master(),
        ],
        9.0,
    );
    assert_eq!(t.member_task(member_11(), &query_on(11)).unwrap(), None);
}

/// Task 0x69's range is the target owner's attribute 90; task 0x68's is the unit's slot 0x230,
/// which the command never reads (none here); any other task's is its own. (The finder does not
/// find 0x68 and 0x69 yet, so the answer is built from a task given.)
#[test]
fn the_ranges_of_tasks_0x69_and_0x68_do_not_come_from_the_task() {
    let t = tasked_world(
        vec![task_master(ids::VILLAGER_MALE, vec![]), building_master()],
        9.0,
    );
    let target = t.task_target(object_id(60), 1).unwrap();
    let range = |action| {
        t.task_range(&plain_task(action, Distance::new(2.5)), &target)
            .map(|g| (g.command, g.range))
    };
    assert_eq!(
        range(ActionKind::HEAL).unwrap(),
        (ActionKind::HEAL, Some(Distance::new(9.0)))
    );
    assert_eq!(
        range(ActionKind::CONVERT).unwrap(),
        (ActionKind::CONVERT, None)
    );
    assert_eq!(range(GATHER).unwrap(), (GATHER, Some(Distance::new(2.5))));
}

/// A master without a task for the target takes the first master of its class, type and swap
/// group that has one, searched with the same filter.
#[test]
fn member_without_task_takes_the_swap_group_first_with_one() {
    let t = tasked_world(
        vec![
            task_master(ids::VILLAGER_MALE, vec![]),
            task_master(
                ids::BUILDER,
                vec![plain_task(ActionKind::BUILD, Distance::new(1.5))],
            ),
            task_master(
                master_id(119),
                vec![plain_task(ActionKind::BUILD, Distance::new(1.0))],
            ),
            building_master(),
        ],
        9.0,
    );
    let got = t.member_task(member_11(), &query_on(60)).unwrap();
    assert_eq!(
        got.map(|g| (g.command, g.range)),
        Some((ActionKind::BUILD, Some(Distance::new(1.5))))
    );
}

/// A master of a swap group 0 does not search the others; none found is none.
#[test]
fn master_without_swap_group_has_no_other_to_try() {
    let mut own = task_master(ids::VILLAGER_MALE, vec![]);
    own.task_by_group = TaskGroup::NONE;
    let t = tasked_world(
        vec![
            own,
            task_master(
                ids::BUILDER,
                vec![plain_task(ActionKind::BUILD, Distance::new(1.5))],
            ),
            building_master(),
        ],
        9.0,
    );
    assert_eq!(t.member_task(member_11(), &query_on(60)).unwrap(), None);
}

/// A master of type 0x28 or above that holds a board task (0x84) asks the player's commands
/// when it finds no task, which is not modelled; a type below does not.
#[test]
fn master_with_board_task_and_no_task_found_is_not_modelled() {
    let mut own = task_master(
        ids::VILLAGER_MALE,
        vec![plain_task(ActionKind::PICK_UP, Distance::new(1.0))],
    );
    own.task_by_group = TaskGroup::NONE;
    // its own list finds the board task for any target, so give it one for another object
    own.tasks[0].object_id = Some(master_id(999));
    let t = tasked_world(vec![own.clone(), building_master()], 9.0);
    assert!(t.member_task(member_11(), &query_on(60)).is_err());
    own.kind = MasterKind::of_data(0x27);
    let t = tasked_world(vec![own, building_master()], 9.0);
    assert_eq!(t.member_task(member_11(), &query_on(60)).unwrap(), None);
}

/// Player 1's group 1 of the members `members` (`(id, master, x, y)`, 25 hit points, state 2) and
/// player 2's building 60 (master 70, type 80, class 3, radii 2, attackable, state 2) at
/// (40, 45); master 4 is a combat unit of weapon range 4, 5 one of class 46 and 329 a listed
/// master. The group's top speed is 1 and its smallest unit range 2.5.
fn attack_world(members: &[(i32, i16, f32, f32)]) -> World {
    use crate::objects::lifecycle::Lifecycle;
    use crate::objects::masters::{Master, Masters};
    let objects: Vec<Unit> = members
        .iter()
        .map(|&(id, master, x, y)| Unit {
            id: object_id(id),
            place: Static::at(Position::new(x, y, 0.0)),
            group: GroupLink {
                id: Some(group_id(1)),
                flag: -2,
            },
            life: Lifecycle {
                master_id: master_id(master),
                state: ObjectState::Alive,
                hp: HitPoints::new(25.0),
                ..Lifecycle::default()
            },
            ..Unit::default()
        })
        .collect();
    let target = Unit {
        id: object_id(60),
        place: Static::at(Position::new(40.0, 45.0, 0.0)),
        attackable: true,
        life: Lifecycle {
            master_id: ids::HOUSE,
            state: ObjectState::Alive,
            hp: HitPoints::new(550.0),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::from_millis(50),
        FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: objects.into(),
                ..Player::default()
            },
            Player {
                objects: vec![target].into(),
                ..Player::default()
            },
        ],
    );
    let soldier = |id, class| Master {
        id,
        kind: MasterKind::COMBAT,
        class,
        radius: Footprint::new(0.3, 0.3),
        creatable: Some(CreatablePart {
            formation_category: FormationClass::of_data(1),
            ..CreatablePart::default()
        }),
        combat: Some(CombatPart {
            weapon_range: Distance::new(4.0),
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let building = Master {
        radius: Footprint::new(2.0, 2.0),
        ..building_master()
    };
    let masters = Masters::new([
        soldier(master_id(4), class::CLASS_6),
        soldier(master_id(5), class::CLASS_46),
        soldier(ids::ID_329, class::CLASS_6),
        building,
    ]);
    t.model.masters = vec![
        Arc::new(Masters::new([])),
        Arc::new(masters.clone()),
        Arc::new(masters),
    ];
    let mut g = UnitGroup::new(group_id(1));
    g.owner = Some(PlayerId::new(1));
    g.members = members
        .iter()
        .map(|&(id, ..)| Member {
            id: object_id(id),
            ..Member::default()
        })
        .collect();
    g.motion.flags = MotionFlags::NEW;
    g.motion.maximum_velocity_forward = Speed::new(1.0);
    g.smallest_unit_range = Distance::new(2.5);
    t.groups = UnitGroups {
        groups: vec![g],
        ..UnitGroups::default()
    };
    t
}

/// Every object of every player in the spatial index at its place (`FUN_14118f8c0`-style gaps and
/// the work command's reach read it).
fn register(t: &mut World) {
    let placed: Vec<(i32, Placed)> = PlayerId::each(t.players.len())
        .zip(&t.players)
        .flat_map(|(p, player)| player.objects.iter().map(move |u| (p, u)))
        .filter_map(|(p, u)| {
            let m = t.model.masters.of(p)?.get(u.life.master_id)?;
            Some((
                u.id.get(),
                Placed {
                    position: u.position(),
                    radius: m.radius,
                    class: m.class,
                    player: p,
                    master: m.id,
                    obstruction_kind: ObstructionKind::NONE,
                    buildable_on: BuildableOn::NO,
                    tall: false,
                    moves: false,
                    unknown: None,
                    obstruction: None,
                },
            ))
        })
        .collect();
    for (id, placed) in placed {
        t.spatial.insert(object_id(id), placed);
    }
}

fn attack_on(filter: Option<ActionKind>) -> attack::AttackOrder {
    attack::AttackOrder {
        disable_retargeting: false,
        filter,
    }
}

fn building_60() -> ObjectId {
    ObjectId::try_from(60).unwrap()
}

/// `FUN_14100f7f0`: a group commanded on a target out of its members' reach resets the target's
/// data, takes the largest weapon range as its range, then sets the target (kind 1, the place the
/// target last moved to as x, height, y, the range, the radii's length, the order's bytes) and
/// moves to it: command 9, [`crate::groups::GroupFlags::ARRIVED`] off and [`GroupFlags::ATTACKING`]
/// on.
#[test]
fn group_commanded_on_target_out_of_reach_moves_to_it() {
    let mut t = attack_world(&[(7, 4, 10.0, 10.0), (8, 4, 11.0, 10.0)]);
    assert!(
        t.command_attack(
            group_id(1),
            building_60(),
            attack_on(Some(ActionKind::named(0x1234)))
        )
        .unwrap()
    );
    let g = t.groups.get(group_id(1)).unwrap();
    let radius = sqrt(8.0);
    assert_eq!(
        g.current_target,
        TargetData {
            target: GroupTarget::Object(object_id(60)),
            original_position: BVector::new(40.0, 0.0, 45.0),
            range: Distance::new(4.0),
            target_radius: Distance::new(radius),
            disable_retargeting: 0,
            certain_action: Some(ActionKind::named(0x1234)),
        }
    );
    assert_eq!(g.current_command, GroupCommandKind::ATTACK);
    assert_eq!(
        (
            g.flags & GroupFlags::ARRIVED,
            g.flags & GroupFlags::ATTACKING
        ),
        (GroupFlags::NONE, GroupFlags::ATTACKING)
    );
    assert!(
        g.members
            .iter()
            .all(|m| m.seconds_since_attacking == Seconds::MAX)
    );
}

/// A group already on the target whose members all work on it, without
/// [`crate::groups::GroupFlags::JOINED`], only notes where the target is (false); with it, or a
/// member on another target, it commands again.
#[test]
fn group_already_on_its_target_only_notes_the_place() {
    let mut t = attack_world(&[(7, 4, 10.0, 10.0)]);
    assert!(
        t.command_attack(group_id(1), building_60(), attack_on(None))
            .unwrap()
    );
    t.players[2].objects[ListIndex::new(0)].place = Static::at(Position::new(41.0, 45.0, 0.0));
    assert!(
        !t.command_attack(group_id(1), building_60(), attack_on(None))
            .unwrap()
    );
    assert_eq!(
        t.groups
            .get(group_id(1))
            .unwrap()
            .current_target
            .original_position,
        BVector::new(41.0, 0.0, 45.0)
    );
    t.groups.get_mut(group_id(1)).unwrap().flags |= GroupFlags::JOINED;
    assert!(
        t.command_attack(group_id(1), building_60(), attack_on(None))
            .unwrap()
    );
}

/// With two commands queued the members' pass does not run: the range is the group's smallest unit
/// range, and a member within reach attacks nothing here.
#[test]
fn group_with_two_queued_commands_takes_its_smallest_unit_range() {
    let mut t = attack_world(&[(7, 4, 38.0, 45.0)]);
    t.groups.get_mut(group_id(1)).unwrap().command_queue =
        vec![GroupCommand::default(), GroupCommand::default()];
    assert!(
        t.command_attack(group_id(1), building_60(), attack_on(None))
            .unwrap()
    );
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().current_target.range,
        Distance::new(2.5)
    );
}

/// `FUN_14102bbd0`: the pass raises the range to the largest a member works at; a member
/// within its weapon's reach of the target and not nearer than its minimum range is sent at it.
#[test]
fn the_reach_pass_raises_the_range_and_sends_member_in_reach() {
    let mut t = attack_world(&[(7, 4, 10.0, 10.0), (8, 4, 38.0, 45.0)]);
    register(&mut t);
    for u in &mut t.players[1].objects {
        t.parts.set_unit_ai(
            u.id,
            Some(UnitAi {
                retarget_timer: Millis::new(7),
                ..UnitAi::default()
            }),
        );
        t.parts.put_mover(u.id, Some(walker()));
    }
    let mut reference = t.streams.clone();
    let draw = reference.get(Stream::Ai).rand15();
    let mut range = -1.0;
    t.reach_pass(group_id(1), object_id(60), attack_on(None), &mut range)
        .unwrap();
    assert_eq!(range, 4.0);
    // member 8 touches the target (a gap 0, so the timer's span is the least, 1000); 7 is far
    let timers: Vec<u32> = t.players[1]
        .objects
        .iter()
        .map(|u| t.parts.unit_ai.get(u.id).unwrap().retarget_timer.get())
        .collect();
    assert_eq!(timers, [7, 750 + draw % 500]);
    let mut t = attack_world(&[(7, 4, 10.0, 10.0)]);
    let mut range = -1.0;
    t.reach_pass(group_id(1), object_id(60), attack_on(None), &mut range)
        .unwrap();
    assert_eq!(range, 4.0);
}

/// `FUN_1415cd0d0`: no filter is -1; a filter takes the range of the master's task of that
/// action type (the conversion's adds attribute 280); no such task -1; a master the player does
/// not have 0.
#[test]
fn filter_takes_the_range_of_the_members_task_of_that_type() {
    let mut t = attack_world(&[(7, 4, 10.0, 10.0)]);
    for master in t
        .model
        .masters
        .iter_mut()
        .filter_map(|m| Arc::make_mut(m).get_mut(master_id(4)))
    {
        master.tasks = vec![
            plain_task(GATHER, Distance::new(6.0)),
            plain_task(ActionKind::CONVERT, Distance::new(1.5)),
        ];
    }
    t.players[1].attributes = vec![Amount::ZERO; 300];
    t.players[1].attributes[280] = Amount::new(2.0);
    assert_eq!(
        t.task_reach(PlayerId::new(1), master_id(4), None).unwrap(),
        -1.0
    );
    assert_eq!(
        t.task_reach(PlayerId::new(1), master_id(4), Some(GATHER))
            .unwrap(),
        6.0
    );
    assert_eq!(
        t.task_reach(PlayerId::new(1), master_id(4), Some(ActionKind::CONVERT))
            .unwrap(),
        3.5
    );
    assert_eq!(
        t.task_reach(PlayerId::new(1), master_id(4), Some(ATTACK))
            .unwrap(),
        -1.0
    );
    assert_eq!(
        t.task_reach(PlayerId::new(1), master_id(99), Some(GATHER))
            .unwrap(),
        0.0
    );
    // the range is the task's with the filter: the pass raises the group's range to it
    let mut range = -1.0;
    t.reach_pass(
        group_id(1),
        object_id(60),
        attack_on(Some(GATHER)),
        &mut range,
    )
    .unwrap();
    assert_eq!(range, 6.0);
}

fn general(queued: bool, direct: bool) -> CommandArgs {
    CommandArgs {
        queued,
        direct,
        disable_retargeting: false,
        filter: None,
    }
}

/// `FUN_14100fc50`, not queued: the group moves to the target within the range and takes the
/// command (5) with the target's data (kind 1, where it last moved to, the range, the radii's
/// length, no retargeting, certain action -1); the members' seconds since attacking are the
/// maximum.
#[test]
fn general_command_moves_the_group_to_the_target_and_takes_the_command() {
    let mut t = attack_world(&[(7, 4, 10.0, 10.0)]);
    let moved = t
        .general_command(
            group_id(1),
            (GroupCommandKind::named(5), building_60()),
            Point::new(40.0, 45.0),
            Distance::new(3.0),
            general(false, true),
        )
        .unwrap();
    assert!(moved);
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(g.current_command, GroupCommandKind::named(5));
    assert_eq!(
        g.current_target,
        TargetData {
            target: GroupTarget::Object(object_id(60)),
            original_position: BVector::new(40.0, 0.0, 45.0),
            range: Distance::new(3.0),
            target_radius: Distance::new(sqrt(8.0)),
            disable_retargeting: 0,
            certain_action: None,
        }
    );
    assert!(
        g.members
            .iter()
            .all(|m| m.seconds_since_attacking == Seconds::MAX)
    );
}

/// A group that cannot move (its top speed below the tolerance) takes no command (false).
#[test]
fn general_command_of_group_that_cannot_move_does_nothing() {
    let mut t = attack_world(&[(7, 4, 10.0, 10.0)]);
    t.groups
        .get_mut(group_id(1))
        .unwrap()
        .motion
        .maximum_velocity_forward = Speed::ZERO;
    assert!(
        !t.general_command(
            group_id(1),
            (GroupCommandKind::named(5), building_60()),
            Point::new(40.0, 45.0),
            Distance::new(3.0),
            general(false, true)
        )
        .unwrap()
    );
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().current_command,
        GroupCommandKind::NONE
    );
}

/// Queued, the command joins the queue approved by `direct` and a group without a command told
/// directly hibernates; not direct behind queued commands it joins unapproved, the queue is
/// approved and runs, and a general command on a held target in it is run (the group takes it).
#[test]
fn general_command_queues_or_runs_the_queue() {
    let mut t = attack_world(&[(7, 4, 10.0, 10.0)]);
    assert!(
        !t.general_command(
            group_id(1),
            (GroupCommandKind::named(5), building_60()),
            Point::new(40.0, 45.0),
            Distance::new(3.0),
            general(true, true)
        )
        .unwrap()
    );
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!((g.command_queue.len(), g.hibernating), (1, 1));
    let node = g.command_queue[0];
    assert_eq!(
        (
            node.command,
            node.target_id,
            node.range,
            node.point,
            node.approved,
            node.approved_2
        ),
        (
            GroupCommandKind::named(5),
            Some(object_id(60)),
            Distance::new(3.0),
            BVector::new(40.0, 0.0, 45.0),
            1,
            u16::MAX
        )
    );
    assert!(
        t.general_command(
            group_id(1),
            (GroupCommandKind::named(5), building_60()),
            Point::new(40.0, 45.0),
            Distance::new(3.0),
            general(false, false)
        )
        .unwrap()
    );
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(g.command_queue, []);
    assert_eq!(g.current_command, GroupCommandKind::named(5));
}

/// `FUN_14100c6c0`: a group of fewer than five members lists one special unit, of five or more
/// four. A target of class 24 (and 57, 12, 47) takes the listed masters' members then the class
/// 46 ones; a target of master 6 or 7 the class 46 ones only; a target of a villager's class the
/// units by formation category; any other target none.
#[test]
fn the_special_units_follow_the_target_class() {
    let units = [
        (7, 4, 0.0, 0.0),
        (8, 5, 0.0, 0.0),
        (9, 329, 0.0, 0.0),
        (10, 329, 0.0, 0.0),
        (11, 5, 0.0, 0.0),
    ];
    let special = |target_class: ClassId, target_master: MasterId, take: usize| {
        let mut t = attack_world(&units[..take]);
        for b in t
            .model
            .masters
            .iter_mut()
            .filter_map(|m| Arc::make_mut(m).get_mut(ids::HOUSE))
        {
            b.class = target_class;
        }
        let _ = target_master;
        t.refill_special_units(group_id(1), object_id(60)).unwrap();
        t.groups.get(group_id(1)).unwrap().special_units.clone()
    };
    assert_eq!(
        special(class::CLASS_24, ids::HOUSE, 5),
        [object_id(9), object_id(10), object_id(8), object_id(11)]
    );
    assert_eq!(special(class::CLASS_24, ids::HOUSE, 4), [object_id(9)]);
    assert_eq!(special(class::BUILDING, ids::HOUSE, 5), Vec::new());
}

/// `FUN_141007ba0`: each formation category's attack range rises to the largest range its members
/// have against the target (never down); a category without units stays.
#[test]
fn the_categories_attack_ranges_rise_to_their_members_ranges() {
    let mut t = attack_world(&[(7, 4, 0.0, 0.0), (8, 4, 0.0, 0.0)]);
    t.groups.get_mut(group_id(1)).unwrap().unit_types = vec![
        UnitTypeStatistic {
            formation_category: FormationCategory::named(1),
            units: vec![object_id(7), object_id(8)],
            attack_range: Distance::new(1.0),
            ..UnitTypeStatistic::default()
        },
        UnitTypeStatistic {
            formation_category: FormationCategory::named(2),
            units: vec![object_id(7)],
            attack_range: Distance::new(9.0),
            ..UnitTypeStatistic::default()
        },
        UnitTypeStatistic {
            formation_category: FormationCategory::named(3),
            attack_range: Distance::new(0.5),
            ..UnitTypeStatistic::default()
        },
    ];
    t.raise_attack_ranges(group_id(1), object_id(60)).unwrap();
    let ranges: Vec<f32> = t
        .groups
        .get(group_id(1))
        .unwrap()
        .unit_types
        .iter()
        .map(|s| s.attack_range.get())
        .collect();
    assert_eq!(ranges, [4.0, 9.0, 0.5]);
}

/// An attack task for a building of another player (task 7, owner type 2, class 3).
fn attack_task() -> Task {
    Task {
        action_kind: ActionKind::COMBAT,
        owner_type: TargetOwner::ENEMY,
        object_group: Some(class_id(3)),
        work_range: Distance::new(1.5),
        ..Task::default()
    }
}

/// [`attack_world`] whose soldiers (master 4) have the attack task.
fn attack_world_with_task(members: &[(i32, i16, f32, f32)]) -> World {
    let mut t = attack_world(members);
    for master in t
        .model
        .masters
        .iter_mut()
        .filter_map(|m| Arc::make_mut(m).get_mut(master_id(4)))
    {
        master.tasks = vec![attack_task()];
    }
    t
}

fn command_args(queued: bool, direct: bool) -> CommandArgs {
    CommandArgs {
        queued,
        direct,
        disable_retargeting: false,
        filter: None,
    }
}

/// `FUN_14100efd0`: a group's order on a building of another player runs the attack path (task 7):
/// the group moves to it in command 9 and the move stops the members.
#[test]
fn group_ordered_on_an_enemy_building_runs_the_attack_path() {
    let mut t = attack_world_with_task(&[(7, 4, 10.0, 10.0), (8, 4, 11.0, 10.0)]);
    let moved = t
        .group_command(
            group_id(1),
            Some(object_id(60)),
            Point::new(40.0, 45.0),
            command_args(false, true),
        )
        .unwrap();
    assert!(moved);
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(
        (g.current_command, g.current_target.target),
        (GroupCommandKind::ATTACK, GroupTarget::Object(object_id(60)))
    );
    assert_eq!(g.flags & GroupFlags::JOINED, GroupFlags::NONE);
}

/// `FUN_14100efd0`: a member whose task on the building is action 5 gives the general command: the
/// group moves to it within the task's range and is in command 5 with the target's data.
#[test]
fn group_ordered_with_general_task_runs_the_general_command() {
    let mut t = attack_world_with_task(&[(7, 4, 10.0, 10.0)]);
    for master in t
        .model
        .masters
        .iter_mut()
        .filter_map(|m| Arc::make_mut(m).get_mut(master_id(4)))
    {
        master.tasks = vec![Task {
            action_kind: GATHER,
            work_range: Distance::new(6.0),
            ..attack_task()
        }];
    }
    assert!(
        t.group_command(
            group_id(1),
            Some(object_id(60)),
            Point::new(40.0, 45.0),
            command_args(false, true)
        )
        .unwrap()
    );
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(
        (g.current_command, g.current_target.target),
        (
            GroupCommandKind::named(5),
            GroupTarget::Object(object_id(60))
        )
    );
    assert_eq!(g.current_target.range, Distance::new(6.0));
}

/// A target the world does not hold is a null object for the command: the plain move.
#[test]
fn group_ordered_on_an_object_the_world_lacks_moves_to_the_point() {
    let mut t = attack_world_with_task(&[(7, 4, 10.0, 10.0)]);
    assert!(
        t.group_command(
            group_id(1),
            Some(object_id(99)),
            Point::new(40.0, 45.0),
            command_args(false, true)
        )
        .unwrap()
    );
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(
        (g.current_command, g.current_target.target),
        (GroupCommandKind::MOVE, GroupTarget::None)
    );
}

/// A queued attack joins the command queue (approved with `direct`, its range the task's, the
/// retargeting byte and the filter's 16 bits kept) and nothing moves; a group without a command
/// that is told directly hibernates.
#[test]
fn queued_attack_joins_the_command_queue() {
    let mut t = attack_world_with_task(&[(7, 4, 10.0, 10.0)]);
    let args = CommandArgs {
        queued: true,
        direct: true,
        disable_retargeting: true,
        filter: None,
    };
    assert!(
        !t.group_command(
            group_id(1),
            Some(object_id(60)),
            Point::new(40.0, 45.0),
            args
        )
        .unwrap()
    );
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(
        g.command_queue,
        [GroupCommand {
            command: GroupCommandKind::named(7),
            target_id: Some(object_id(60)),
            range: Distance::new(1.5),
            point: BVector::new(40.0, 0.0, 45.0),
            approved: 1,
            disable_retargeting: 1,
            approved_2: 0xffff,
        }]
    );
    assert_eq!(
        (g.current_command, g.hibernating, g.replaces_queue),
        (GroupCommandKind::NONE, 1, 0)
    );
}

/// An order neither queued nor direct behind queued commands joins them unapproved; the queue is
/// approved and run: the first attack on the target (held) is the group's command and ends the run,
/// the second stays queued.
#[test]
fn an_indirect_attack_behind_queued_commands_runs_the_queue() {
    let mut t = attack_world_with_task(&[(7, 4, 10.0, 10.0)]);
    t.groups.get_mut(group_id(1)).unwrap().command_queue = vec![GroupCommand {
        command: GroupCommandKind::named(7),
        target_id: Some(object_id(60)),
        range: Distance::new(1.5),
        point: BVector::new(40.0, 0.0, 45.0),
        approved: 0,
        disable_retargeting: 0,
        approved_2: 0xffff,
    }];
    assert!(
        t.group_command(
            group_id(1),
            Some(object_id(60)),
            Point::new(40.0, 45.0),
            command_args(false, false)
        )
        .unwrap()
    );
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(g.command_queue.len(), 1);
    assert_eq!(
        (g.current_command, g.current_target.target),
        (GroupCommandKind::ATTACK, GroupTarget::Object(object_id(60)))
    );
}

/// `FUN_14100d460`: an approved queued attack on a target the world holds is the group's
/// command (`FUN_14100f7f0` with the node's retargeting byte and filter) and ends the run
/// without the move to the last command's point.
#[test]
fn queued_attack_on_held_target_ends_the_run_with_the_attack() {
    let mut t = attack_world(&[(7, 4, 10.0, 10.0)]);
    t.groups.get_mut(group_id(1)).unwrap().command_queue = vec![GroupCommand {
        command: GroupCommandKind::named(7),
        target_id: Some(object_id(60)),
        range: Distance::new(1.5),
        point: BVector::new(40.0, 0.0, 45.0),
        approved: 1,
        disable_retargeting: 1,
        approved_2: 0x0102,
    }];
    t.check_queued_orders(group_id(1)).unwrap();
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(g.command_queue, []);
    assert_eq!(g.current_command, GroupCommandKind::ATTACK);
    assert_eq!(
        (
            g.current_target.disable_retargeting,
            g.current_target.certain_action
        ),
        (1, ActionKind::of_data(0x0102))
    );
}

/// `FUN_14100c6c0`: a target of master 6 or 7 takes the class 46 members only; a target of class 4
/// the members by formation category in the group's statistics (up to the limit); one of class 57
/// the masters' list first.
#[test]
fn the_special_units_of_masters_6_and_7_and_of_the_category_classes() {
    let units = [
        (7, 4, 0.0, 0.0),
        (8, 5, 0.0, 0.0),
        (9, 329, 0.0, 0.0),
        (10, 5, 0.0, 0.0),
        (11, 4, 0.0, 0.0),
    ];
    let world = |target_class: ClassId, target_master: MasterId| {
        let mut t = attack_world(&units);
        t.players[2].objects[ListIndex::new(0)].life.master_id = target_master;
        for masters in &mut t.model.masters {
            let mut b = building_master();
            b.id = target_master;
            b.class = target_class;
            *masters = Arc::new(Masters::new(
                masters
                    .iter()
                    .filter(|m| m.id != ids::HOUSE)
                    .cloned()
                    .chain([b]),
            ));
        }
        t
    };
    let mut t = world(class::BUILDING, ids::ID_7);
    t.refill_special_units(group_id(1), object_id(60)).unwrap();
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().special_units,
        [object_id(8), object_id(10)]
    );
    let mut t = world(class::VILLAGER, ids::HOUSE);
    t.groups.get_mut(group_id(1)).unwrap().unit_types = vec![
        UnitTypeStatistic {
            formation_category: FormationCategory::named(2),
            units: vec![object_id(9), object_id(10)],
            ..UnitTypeStatistic::default()
        },
        UnitTypeStatistic {
            formation_category: FormationCategory::named(1),
            units: vec![object_id(7), object_id(8), object_id(11)],
            ..UnitTypeStatistic::default()
        },
    ];
    t.refill_special_units(group_id(1), object_id(60)).unwrap();
    // category 1 first, then 2, up to four
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().special_units,
        [object_id(7), object_id(8), object_id(11), object_id(9)]
    );
    let mut t = world(class::CLASS_57, ids::HOUSE);
    t.refill_special_units(group_id(1), object_id(60)).unwrap();
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().special_units,
        [object_id(9), object_id(8), object_id(10)]
    );
}
