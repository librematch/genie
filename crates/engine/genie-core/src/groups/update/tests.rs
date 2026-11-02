use crate::combat::Charge;
use crate::combat::attack::AttackState;
use crate::construction::MainBuildingId;
use crate::garrison::ContainerId;
use crate::groups::FormationCategory;
use crate::groups::FormationKind;
use crate::groups::GroupCommandKind;
use crate::groups::GroupFlags;
use crate::groups::MotionFlags;
use crate::groups::MovePath;
use crate::groups::MoveReason;
use crate::groups::SlotState;
use crate::groups::TransformType;
use crate::groups::UnitType;
use crate::groups::{GroupTarget, Member, UnitGroup, UnitGroups};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Restriction;
use crate::map::TerrainFlags;
use crate::map::terrain::TileTerrain;
use crate::math::fmath::BVector;
use crate::movement::GroupSpeed;
use crate::movement::PathState;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::ClassId;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::FormationClass;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::SpecialAttack;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::master_type;
use crate::objects::masters::{Master, Masters, Task};
use crate::objects::unit::Unit;
use crate::pathing::DelayCount;
use crate::players::ByPlayer;
use crate::players::PlayerId;
use crate::players::relation_row;
use crate::players::relations;
use crate::test_ids::group_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_id_list;
use crate::unit_ai::ActionKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAi;
use crate::vision::VisionBits;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::parts::Object;
use crate::world::{Player, World};

/// A unit of master 4 at (`x`, `y`) in state 2 with 50 hit points (its unit AI [`soldier_ai`]).
fn soldier(id: i32, x: f32, y: f32) -> Unit {
    let mut u = Unit {
        id: object_id(id),
        place: crate::objects::layers::Static::at(Position::new(x, y, 0.0)),
        ..Unit::default()
    };
    u.life.master_id = master_id(4);
    u.life.state = ObjectState::Alive;
    u.life.hp = HitPoints::new(50.0);
    u
}

/// A soldier's unit AI: no order, action or target.
fn soldier_ai() -> UnitAi {
    UnitAi {
        current_order: None,
        current_action: None,
        current_target: None,
        ..UnitAi::default()
    }
}

/// Player 1 holds soldiers 10, 11 and 12 (master 4: a combat unit of class 6, 60 hit
/// points, line of sight 4, radius 0.2) in group 1; player 2, its enemy, holds 20 at (3, 3)
/// and player 3, its ally, 30 at (4, 4). An 8 by 8 map whose tiles player 1 sees none of.
fn world() -> World {
    let players = vec![
        Player::default(),
        Player {
            objects: vec![
                soldier(10, 1.0, 1.0),
                soldier(11, 2.0, 1.0),
                soldier(12, 1.0, 2.0),
            ]
            .into(),
            ..Player::default()
        },
        Player {
            objects: vec![soldier(20, 3.5, 3.5)].into(),
            ..Player::default()
        },
        Player {
            objects: vec![soldier(30, 4.5, 4.5)].into(),
            ..Player::default()
        },
    ];
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO; 4],
        players,
    )
    .with_ais([10, 11, 12, 20, 30].map(|id| (object_id(id), soldier_ai())));
    let master = Master {
        id: master_id(4),
        terrain_restriction: Some(crate::map::RestrictionId::named(0)),
        kind: master_type::COMBAT,
        class: class::CLASS_6,
        hit_points: MasterHitPoints::of_data(60),
        line_of_sight: Distance::new(4.0),
        search_radius: Distance::new(3.0),
        radius: Footprint::new(0.2, 0.2),
        standing_sprite: None,
        formation_category: FormationClass::of_data(1),
        ..Master::default()
    };
    t.masters = vec![Masters::new([master]); 4];
    let mut s = crate::map::spatial::Spatial::new(GridSize::new(8, 8));
    s.relations = ByPlayer::new(vec![
        relation_row([0, 3, 3, 3]),
        relation_row([0, 0, 3, 0]),
        relation_row([0, 3, 0, 3]),
        relation_row([0, 0, 3, 0]),
    ]);
    s.unit_diplomacy = ByPlayer::new(vec![
        relations([1, 4, 4, 4]),
        relations([0, 1, 4, 2]),
        relations([0, 4, 1, 4]),
        relations([0, 2, 4, 1]),
    ]);
    s.sight = Some(crate::vision::visibility::Visibility::new(
        GridSize::new(8, 8),
        Vec::new(),
        vec![VisionBits::NONE; 64],
        Vec::new(),
        0,
        Vec::new(),
    ));
    t.spatial = s;
    let member = |id| Member {
        id,
        ..Member::default()
    };
    let mut g = UnitGroup::new(group_id(1));
    g.owner = Some(PlayerId::new(1));
    g.members = vec![
        member(object_id(10)),
        member(object_id(11)),
        member(object_id(12)),
    ];
    for u in &mut t.players[1].objects {
        u.group.id = Some(group_id(1));
    }
    t.groups = UnitGroups {
        groups: vec![g],
        ..UnitGroups::default()
    };
    t
}

/// Player `p` sees tile (`x`, `y`) (`seen`) or only explored it.
fn light(t: &mut World, p: usize, x: usize, y: usize, seen: bool) {
    let v = t.spatial.sight.as_mut().unwrap();
    v.unified[y * 8 + x] |= if seen {
        VisionBits::from_bits_retain(1 << p)
    } else {
        VisionBits::from_bits_retain(1 << (32 + p))
    };
}

/// The members' check of `FUN_14103d910`: 11 below 1 hit point, 12 of another player, 13
/// without a unit AI and 99 the world does not hold leave the group (their links clear and
/// the held ones stop, slot 0x5d0), which sets [`GroupFlags::STATISTICS`]; 10 stays.
#[test]
fn lost_members_leave_the_group_and_stop() {
    let mut t = world();
    t.players[1].objects[ListIndex::new(1)].life.hp = HitPoints::new(0.5);
    let mut stray = soldier(12, 1.0, 2.0);
    stray.group.id = Some(group_id(1));
    let _ = t.remove_live(PlayerId::new(1), ListIndex::new(2)).unwrap();
    let _ = t
        .push_live(PlayerId::new(2), Object::with_ai(stray, soldier_ai()))
        .unwrap();
    let mut mute = soldier(13, 2.0, 2.0);
    mute.group.id = Some(group_id(1));
    let _ = t.push_live(PlayerId::new(1), mute).unwrap();
    for u in &mut t.players[1].objects {
        t.parts
            .put_mover(u.id, Some(crate::gathering::hunt::tests::walker()));
    }
    let g = t.groups.get_mut(group_id(1)).unwrap();
    for id in [13, 99] {
        g.members.push(Member {
            id: object_id(id),
            ..Member::default()
        });
    }
    g.flags = GroupFlags::NONE;
    t.drop_lost_members(group_id(1)).unwrap();
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(
        g.members.iter().map(|m| m.id).collect::<Vec<_>>(),
        [object_id(10)]
    );
    assert_eq!(g.flags & GroupFlags::STATISTICS, GroupFlags::STATISTICS);
    let unit = |t: &World, id: ObjectId| t.object(id).unwrap().1.clone();
    assert_eq!(unit(&t, object_id(10)).group.id, Some(group_id(1)));
    for id in [11, 12, 13] {
        assert_eq!(unit(&t, object_id(id)).group.id, None, "{id}");
    }
    let stopped = unit(&t, object_id(11));
    let stopped = t.parts.mover_of(stopped.id).unwrap();
    assert_eq!(stopped.animated.speed, Speed::new(0.0));
    assert_eq!(stopped.velocity.unwrap().velocity.y(), 0.0);
    // a NaN's hit points stay (COMISS then JA)
    let mut t = world();
    t.players[1].objects[ListIndex::new(0)].life.hp = HitPoints::new(f32::NAN);
    t.drop_lost_members(group_id(1)).unwrap();
    assert_eq!(t.groups.get(group_id(1)).unwrap().members.len(), 3);
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().flags & GroupFlags::STATISTICS,
        GroupFlags::NONE
    );
}

/// A hibernating group by its command: without one or on a move it runs its queued commands
/// when it has any; a build waits on its own target below state 2; a repair waits on an
/// allied target below its hit points (a NaN too) and runs the queue otherwise; an attack
/// waits while its target is pending; a follow (2) and a command above 106 but the relics'
/// go.
#[test]
fn a_hibernating_group_waits_runs_its_queue_or_goes() {
    let mut t = world();
    let set = |t: &mut World, command: i32, target: i32, queued: bool| {
        let g = t.groups.get_mut(group_id(1)).unwrap();
        g.current_command = GroupCommandKind::named(command);
        // the target alone: the constructor's kind −1 stays
        g.current_target.target = GroupTarget::of_saved(target, -1);
        g.command_queue = if queued {
            vec![crate::groups::GroupCommand::default()]
        } else {
            Vec::new()
        };
    };
    for command in [-1, 1] {
        set(&mut t, command, -1, false);
        assert_eq!(t.hibernation(group_id(1)).unwrap(), Some(false));
        set(&mut t, command, -1, true);
        assert_eq!(t.hibernation(group_id(1)).unwrap(), Some(true));
    }
    for command in [2, 3, 0x69, 0x6b, 0x2711] {
        set(&mut t, command, -1, false);
        assert_eq!(t.hibernation(group_id(1)).unwrap(), None, "{command}");
    }
    // a build on the group's own 11 in state 1 waits, in state 2 or of another player not
    set(&mut t, 0x65, 11, false);
    t.players[1].objects[ListIndex::new(1)].life.state = ObjectState::Unfinished;
    assert_eq!(t.hibernation(group_id(1)).unwrap(), Some(false));
    t.players[1].objects[ListIndex::new(1)].life.state = ObjectState::Alive;
    assert_eq!(t.hibernation(group_id(1)).unwrap(), Some(true));
    set(&mut t, 0x65, 20, false);
    t.players[2].objects[ListIndex::new(0)].life.state = ObjectState::Foundation;
    assert_eq!(t.hibernation(group_id(1)).unwrap(), Some(true));
    // a repair of the ally's 30 below its 60 hit points waits; at them, or of the enemy's
    // 20, or of nothing, the queue runs
    set(&mut t, 0x6a, 30, false);
    assert_eq!(t.hibernation(group_id(1)).unwrap(), Some(false));
    t.players[3].objects[ListIndex::new(0)].life.hp = HitPoints::new(f32::NAN);
    assert_eq!(t.hibernation(group_id(1)).unwrap(), Some(false));
    t.players[3].objects[ListIndex::new(0)].life.hp = HitPoints::new(60.0);
    assert_eq!(t.hibernation(group_id(1)).unwrap(), Some(true));
    set(&mut t, 0x6a, 20, false);
    assert_eq!(t.hibernation(group_id(1)).unwrap(), Some(true));
    set(&mut t, 0x6a, 77, false);
    assert_eq!(t.hibernation(group_id(1)).unwrap(), Some(true));
    // an attack on the enemy's 20 waits while player 1 sees it
    set(&mut t, 9, 20, false);
    assert_eq!(t.hibernation(group_id(1)).unwrap(), Some(true));
    light(&mut t, 1, 3, 3, true);
    assert_eq!(t.hibernation(group_id(1)).unwrap(), Some(false));
}

/// `FUN_1410176b0`: a target the world holds, in state 2 at most (0 for a build), inside no
/// other object, of a player the owner does not count an ally, that the owner sees (or only
/// explored, for a master with `fog_visibility`).
#[test]
fn a_target_is_pending_while_seen_and_no_ally_s() {
    let mut t = world();
    let target = |t: &mut World, command: i32, id: i32| {
        let g = t.groups.get_mut(group_id(1)).unwrap();
        g.current_command = GroupCommandKind::named(command);
        g.current_target.target = GroupTarget::of_saved(id, -1);
    };
    target(&mut t, 9, 20);
    assert!(!t.target_pending(group_id(1)).unwrap());
    light(&mut t, 1, 3, 3, false);
    assert!(!t.target_pending(group_id(1)).unwrap());
    for m in &mut t.masters {
        let mut master = m.get(master_id(4)).unwrap().clone();
        master.fog_visibility = FogVisibility::VISIBLE;
        *m = Masters::new([master]);
    }
    assert!(t.target_pending(group_id(1)).unwrap());
    t.players[2].objects[ListIndex::new(0)].life.state = ObjectState::Dying;
    assert!(!t.target_pending(group_id(1)).unwrap());
    t.players[2].objects[ListIndex::new(0)].life.state = ObjectState::Alive;
    t.players[2].objects[ListIndex::new(0)].life.inside = Some(ContainerId::new(object_id(5)));
    assert!(!t.target_pending(group_id(1)).unwrap());
    t.players[2].objects[ListIndex::new(0)].life.inside = None;
    assert!(t.target_pending(group_id(1)).unwrap());
    // an ally's: not pending, but for a build in state 0
    light(&mut t, 1, 4, 4, true);
    target(&mut t, 9, 30);
    assert!(!t.target_pending(group_id(1)).unwrap());
    target(&mut t, 0x65, 30);
    assert!(!t.target_pending(group_id(1)).unwrap());
    t.players[3].objects[ListIndex::new(0)].life.state = ObjectState::Foundation;
    assert!(t.target_pending(group_id(1)).unwrap());
    // without members, none
    t.groups.get_mut(group_id(1)).unwrap().members.clear();
    assert!(!t.target_pending(group_id(1)).unwrap());
    assert!(!t.target_pending(group_id(2)).unwrap());
}

/// `FUN_141017c70` and `FUN_141017850`: a member whose unit AI works on a relic (order
/// `0x2db` or action `0x277`) keeps a pick-up pending while its master picks up relics and
/// its target is a relic (class `0x2a`), or delivers them and it has a relic or no target;
/// a delivery pending on a monastery (master 104).
#[test]
fn relic_carriers_keep_their_command_pending() {
    let mut t = world();
    let relic = Master {
        id: ids::RELIC,
        kind: master_type::COMBAT,
        class: class::RELIC,
        ..Master::default()
    };
    let monastery = Master {
        id: ids::MONASTERY,
        kind: master_type::BUILDING,
        class: class::BUILDING,
        ..Master::default()
    };
    let task = |action| Task {
        action_type: action,
        ..Task::default()
    };
    let monk = |tasks: Vec<Task>| Master {
        id: master_id(4),
        kind: master_type::COMBAT,
        class: class::MONK,
        tasks,
        ..Master::default()
    };
    let set_masters = |t: &mut World, tasks: Vec<Task>| {
        t.masters = vec![Masters::new([monk(tasks), relic.clone(), monastery.clone()]); 4];
    };
    let mut r = soldier(40, 6.0, 6.0);
    r.life.master_id = ids::RELIC;
    let mut m = soldier(41, 7.0, 7.0);
    m.life.master_id = ids::MONASTERY;
    t.extend_live(
        PlayerId::new(0),
        [r, m].map(|u| Object::with_ai(u, soldier_ai())),
    )
    .unwrap();
    let ai = |t: &mut World, order: Option<OrderKind>, action: Option<ActionKind>, target: i32| {
        let a = t
            .parts
            .unit_ai
            .get_mut(t.players[1].objects[ListIndex::new(0)].id)
            .unwrap();
        a.current_order = order;
        a.current_action = action;
        a.current_target = ObjectId::of_field(target);
    };
    t.groups.get_mut(group_id(1)).unwrap().current_command = GroupCommandKind::named(0x84);
    set_masters(
        &mut t,
        vec![task(crate::objects::masters::action_type::PICK_UP)],
    );
    ai(&mut t, Some(OrderKind::OnTarget), None, 40);
    assert!(t.target_pending(group_id(1)).unwrap());
    ai(&mut t, None, Some(ActionKind::OnTarget), 40);
    assert!(t.target_pending(group_id(1)).unwrap());
    ai(&mut t, None, None, 40);
    assert!(!t.target_pending(group_id(1)).unwrap());
    ai(&mut t, Some(OrderKind::OnTarget), None, -1);
    assert!(!t.target_pending(group_id(1)).unwrap());
    set_masters(
        &mut t,
        vec![task(crate::objects::masters::action_type::DEPOSIT)],
    );
    assert!(t.target_pending(group_id(1)).unwrap());
    ai(&mut t, Some(OrderKind::OnTarget), None, 41);
    assert!(!t.target_pending(group_id(1)).unwrap());
    // the delivery: on the monastery, with either task
    t.groups.get_mut(group_id(1)).unwrap().current_command = GroupCommandKind::named(0x88);
    assert!(t.target_pending(group_id(1)).unwrap());
    set_masters(
        &mut t,
        vec![task(crate::objects::masters::action_type::PICK_UP)],
    );
    assert!(t.target_pending(group_id(1)).unwrap());
    set_masters(&mut t, vec![task(crate::gathering::gather::GATHER)]);
    assert!(!t.target_pending(group_id(1)).unwrap());
    set_masters(
        &mut t,
        vec![task(crate::objects::masters::action_type::DEPOSIT)],
    );
    ai(&mut t, Some(OrderKind::OnTarget), None, 40);
    assert!(!t.target_pending(group_id(1)).unwrap());
}

/// `FUN_141017f00`: a member is free by its top action's state and its unit AI; an object
/// the world does not hold is not.
#[test]
fn a_member_is_free_by_its_action_and_its_unit_ai() {
    let mut t = world();
    assert!(t.member_idle(object_id(10)).unwrap());
    assert!(!t.member_idle(object_id(99)).unwrap());
    let ai = |t: &mut World, order: Option<OrderKind>, action: Option<ActionKind>| {
        let a = t
            .parts
            .unit_ai
            .get_mut(t.players[1].objects[ListIndex::new(0)].id)
            .unwrap();
        a.current_order = order;
        a.current_action = action;
    };
    ai(&mut t, None, Some(ActionKind::Move));
    assert!(!t.member_idle(object_id(10)).unwrap());
    ai(&mut t, Some(OrderKind::Wait), Some(ActionKind::GroupAside));
    assert!(t.member_idle(object_id(10)).unwrap());
    ai(&mut t, None, Some(ActionKind::Heal));
    assert!(t.member_idle(object_id(10)).unwrap());
    ai(&mut t, Some(OrderKind::Move), Some(ActionKind::Heal));
    assert!(!t.member_idle(object_id(10)).unwrap());
    ai(&mut t, None, None);
    t.parts
        .set_unit_ai(t.players[1].objects[ListIndex::new(0)].id, None);
    assert!(t.member_idle(object_id(10)).unwrap());
    // an Attack in state 2 is not free, in state 13 it is; a state 4 is not
    let attack = |state| crate::combat::attack::Attack {
        target: None,
        target2: None,
        state,
        timer: Seconds::ZERO,
        range: Distance::ZERO,
        min_range: Distance::ZERO,
        point: Position::ORIGIN,
        seen: Position::ORIGIN,
        move_to: None,
        pending: false,
        swing: Seconds::ZERO,
        rest: Seconds::ZERO,
        arms: crate::combat::attack::Arms {
            move_sprite: None,
            sprite: None,
            second_sprite: None,
            idle_sprite: None,
            idle_attack_sprite: None,
            missile: None,
            fire_frame: crate::objects::Frame::of_data(-1),
            interrupt_frame: crate::objects::Frame::of_data(-1),
        },
        sub_action: false,
        on_target_moved: crate::movement::move_to::TargetMoved::Follows,
        same_owner: false,
    };
    for (state, free) in [
        (AttackState::Killed, false),
        (AttackState::Failed, true),
        (AttackState::Move, false),
    ] {
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .list
            .set_attack(Some(attack(state)));
        assert_eq!(t.member_idle(object_id(10)).unwrap(), free, "{state}");
    }
    // a trade cart's state 2 is free, whatever its action
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .set_attack(Some(attack(AttackState::Killed)));
    for m in &mut t.masters {
        let mut master = m.get(master_id(4)).unwrap().clone();
        master.class = class::TRADE_CART;
        *m = Masters::new([master]);
    }
    assert!(t.member_idle(object_id(10)).unwrap());
}

/// `FUN_141007a50`: a group rests without a command, its motion standing and no held
/// member moving.
#[test]
fn a_group_rests_without_command_motion_or_moving_member() {
    let mut t = world();
    t.groups.get_mut(group_id(1)).unwrap().current_command = GroupCommandKind::NONE;
    assert!(t.at_rest(group_id(1)));
    t.parts.put_mover(
        t.players[1].objects[ListIndex::new(1)].id,
        Some(crate::gathering::hunt::tests::walker()),
    );
    assert!(!t.at_rest(group_id(1)));
    t.parts
        .put_mover(t.players[1].objects[ListIndex::new(1)].id, None);
    t.groups.get_mut(group_id(1)).unwrap().motion.velocity = BVector::new(0.0, 0.0, 1.0);
    assert!(!t.at_rest(group_id(1)));
    t.groups.get_mut(group_id(1)).unwrap().motion.velocity = BVector::ZERO;
    t.groups.get_mut(group_id(1)).unwrap().current_command = GroupCommandKind::MOVE;
    assert!(!t.at_rest(group_id(1)));
    assert!(!t.at_rest(group_id(2)));
}

/// `FUN_141007ca0`: a kept-apart unit the world does not hold leaves the list; one that may
/// not be interrupted (its unit AI has queued orders) stays; any other leaves and stands,
/// as does an attacker with no member's target within its reach.
#[test]
fn kept_apart_units_leave_unless_busy() {
    let mut t = world();
    let apart = soldier(15, 6.0, 6.0);
    t.parts
        .put_mover(apart.id, Some(crate::gathering::hunt::tests::walking()));
    let apart_ai = UnitAi {
        orders: vec![crate::unit_ai::QueuedOrder::default()],
        ..soldier_ai()
    };
    let free = soldier(16, 6.0, 6.0);
    t.parts
        .put_mover(free.id, Some(crate::gathering::hunt::tests::walking()));
    let attacker = soldier(17, 7.0, 7.0);
    let attacker_ai = UnitAi {
        current_action: Some(ActionKind::Attack),
        ..soldier_ai()
    };
    t.extend_live(
        PlayerId::new(1),
        [
            Object::with_ai(apart, apart_ai),
            Object::with_ai(free, soldier_ai()),
            Object::with_ai(attacker, attacker_ai),
        ],
    )
    .unwrap();
    t.groups.get_mut(group_id(1)).unwrap().objects =
        vec![object_id(99), object_id(15), object_id(16), object_id(17)];
    t.keep_apart(group_id(1)).unwrap();
    assert_eq!(t.groups.get(group_id(1)).unwrap().objects, [object_id(15)]);
    assert_eq!(t.parts.anim_speed_of(object_id(16)), Some(Speed::ZERO));
    assert_ne!(t.parts.anim_speed_of(object_id(15)), Some(Speed::ZERO));
    // a member's group moving keeps the unit too
    t.parts
        .unit_ai
        .get_mut(t.players[1].objects[ListIndex::new(3)].id)
        .unwrap()
        .orders
        .clear();
    t.players[1].objects[ListIndex::new(3)].group.id = Some(group_id(1));
    t.groups.get_mut(group_id(1)).unwrap().current_command = GroupCommandKind::MOVE;
    t.keep_apart(group_id(1)).unwrap();
    assert_eq!(t.groups.get(group_id(1)).unwrap().objects, [object_id(15)]);
    t.groups.get_mut(group_id(1)).unwrap().current_command = GroupCommandKind::NONE;
    t.keep_apart(group_id(1)).unwrap();
    assert_eq!(t.groups.get(group_id(1)).unwrap().objects, Vec::new());
}

/// The group's deletion: its held members whose link names it leave it; an idle one stands
/// and shows standing, a busy one walks on. A group without members goes at its update.
#[test]
fn a_deleted_group_lets_its_members_go() {
    let mut t = world();
    for u in &mut t.players[1].objects {
        t.parts
            .put_mover(u.id, Some(crate::gathering::hunt::tests::walking()));
    }
    t.parts
        .unit_ai
        .get_mut(t.players[1].objects[ListIndex::new(1)].id)
        .unwrap()
        .current_action = Some(ActionKind::Move);
    t.players[1].objects[ListIndex::new(2)].group.id = Some(group_id(7));
    t.delete_group(group_id(1)).unwrap();
    assert!(t.groups.get(group_id(1)).is_none());
    let unit = |t: &World, id: ObjectId| t.object(id).unwrap().1.clone();
    assert_eq!(
        [10, 11, 12].map(|id| unit(&t, object_id(id)).group.id),
        [None, None, Some(group_id(7))]
    );
    assert_eq!(
        t.parts.mover_of(object_id(10)).unwrap().animated.speed,
        Speed::new(0.0)
    );
    assert_ne!(
        t.parts.mover_of(object_id(11)).unwrap().animated.speed,
        Speed::ZERO
    );
    assert_ne!(
        t.parts.mover_of(object_id(12)).unwrap().animated.speed,
        Speed::ZERO
    );
    // the update of a group without members deletes it
    let mut t = world();
    t.groups.get_mut(group_id(1)).unwrap().members.clear();
    assert!(!t.group_update(group_id(1)).unwrap());
    t.update_groups(PlayerId::new(1)).unwrap();
    assert!(t.groups.get(group_id(1)).is_none());
}

/// Slots 0x660, 0x658 and 0x650: a unit with the sprite takes its speed (and the sprite,
/// where the slice holds the unit's), one without (-1) shows nothing; a trade cart's
/// standing and walking sprites are its goods', which the slice does not hold, its running
/// one the master's.
#[test]
fn a_unit_shows_its_standing_walking_or_running_sprite() {
    let mut t = world();
    let walker = crate::gathering::hunt::tests::walker();
    t.parts
        .put_mover(t.players[1].objects[ListIndex::new(0)].id, Some(walker));
    let speed = |t: &World| t.parts.mover_of(object_id(10)).unwrap().stats.speed;
    let master = |t: &mut World, class: ClassId, sprites: [i16; 3]| {
        let m = Master {
            id: master_id(4),
            kind: master_type::COMBAT,
            class,
            hit_points: MasterHitPoints::of_data(60),
            radius: Footprint::new(0.2, 0.2),
            standing_sprite: SpriteId::of_data(sprites[0]),
            move_sprite: SpriteId::of_data(sprites[1]),
            run_sprite: SpriteId::of_data(sprites[2]),
            formation_category: FormationClass::of_data(1),
            ..Master::default()
        };
        t.masters = vec![Masters::new([m]); 4];
    };
    master(&mut t, class::CLASS_6, [-1; 3]);
    assert_eq!(t.show_walking(object_id(10)), Ok(false));
    assert_eq!(t.show_running(object_id(10)), Ok(false));
    t.show_standing(object_id(10)).unwrap();
    assert_eq!(speed(&t), Speed::new(0.8));
    master(&mut t, class::CLASS_6, [6, 7, 8]);
    // the sprites' factors on the walker's top speed 0.8
    for (id, factor) in [(6, 0.125), (7, 1.125), (8, 1.875)] {
        let graphic = crate::objects::sprite::Graphic {
            id: SpriteId::named(id),
            frames: crate::objects::Frame::of_data(1),
            facets: crate::objects::FacetCount::of_data(8),
            speed_multiplier: SpeedMultiplier::of_data(factor),
            frame_duration: Seconds::ZERO,
            replay_delay: Seconds::ZERO,
            flags: SpriteFlags::NONE,
        };
        let _ = t.graphics.insert(graphic.id, graphic);
    }
    assert_eq!(t.show_walking(object_id(10)), Ok(true));
    assert_eq!(speed(&t), Speed::new(0.8 * 1.125));
    assert_eq!(t.show_running(object_id(10)), Ok(true));
    assert_eq!(speed(&t), Speed::new(0.8 * 1.875));
    t.show_standing(object_id(10)).unwrap();
    assert_eq!(speed(&t), Speed::new(0.8 * 0.125));
    // a held sprite needs the graphic
    t.graphics.clear();
    let graphic = crate::objects::sprite::Graphic {
        id: SpriteId::named(1),
        frames: crate::objects::Frame::of_data(1),
        facets: crate::objects::FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::ZERO,
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    };
    t.players[1].objects[ListIndex::new(0)].sprite = Some(crate::objects::sprite::ActiveSprite {
        graphic,
        frame: crate::objects::Frame::of_data(0),
        interval: Seconds::ZERO,
        last: Seconds::ZERO,
        last_frame: crate::objects::Frame::of_data(0),
        changed: false,
        looped: false,
        animate: crate::objects::AnimateState::of_data(0),
        speed: Speed::new(0.0),
    });
    assert!(t.show_walking(object_id(10)).is_err());
    master(&mut t, class::TRADE_CART, [6, 7, 8]);
    let trade = crate::groups::update::TRADE_CART_SPRITES;
    assert_eq!(t.show_walking(object_id(10)), Err(trade.clone()));
    assert_eq!(t.show_standing(object_id(10)), Err(trade));
    t.players[1].objects[ListIndex::new(0)].sprite = None;
    assert_eq!(t.show_running(object_id(10)), Ok(true));
    assert_eq!(t.show_walking(object_id(99)), Ok(false));
}

/// The formation's turn: past π/4 (not a NaN), or with [`MotionFlags::NEW`] or
/// [`GroupFlags::RELAYOUT_AT_TURN`].
#[test]
fn the_formation_turns_past_a_quarter_turn() {
    let mut t = world();
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.flags = GroupFlags::NONE;
    g.motion.flags = MotionFlags::NONE;
    g.motion.position[1] = BVector::new(1.0, 0.0, 0.0);
    assert!(
        !t.turns_formation(group_id(1), BVector::new(0.72, 0.0, 0.693_974_1))
            .unwrap()
    );
    assert!(
        t.turns_formation(group_id(1), BVector::new(0.70, 0.0, 0.714_142_8))
            .unwrap()
    );
    assert!(
        t.turns_formation(group_id(1), BVector::new(0.0, 0.0, -1.0))
            .unwrap()
    );
    assert!(
        !t.turns_formation(group_id(1), BVector::new(f32::NAN, 0.0, 0.0))
            .unwrap()
    );
    t.groups.get_mut(group_id(1)).unwrap().flags = GroupFlags::RELAYOUT_AT_TURN;
    assert!(
        t.turns_formation(group_id(1), BVector::new(1.0, 0.0, 0.0))
            .unwrap()
    );
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.flags = GroupFlags::NONE;
    g.motion.flags = MotionFlags::NEW;
    assert!(
        t.turns_formation(group_id(1), BVector::new(1.0, 0.0, 0.0))
            .unwrap()
    );
}

/// `movePositionToFrontUsingCentroid`: the members' mean place (x, height, y) plus the
/// forward times 0, as the group's place and new place.
#[test]
fn the_centroid_front_is_the_members_mean_place() {
    let mut t = world();
    t.players[1].objects[ListIndex::new(2)].place_for_test(Position::new(1.0, 4.0, 3.0));
    t.groups.get_mut(group_id(1)).unwrap().motion.position[1] = BVector::new(1.0, 0.0, 0.0);
    t.front_by_centroid(group_id(1)).unwrap();
    let g = t.groups.get(group_id(1)).unwrap();
    let mean = BVector::new(4.0 / 3.0, 1.0, 2.0);
    assert_eq!(g.motion.position[0], mean);
    assert_eq!(g.motion.new_position[0], mean);
    // an infinite forward makes the place NaN (inf times 0)
    t.groups.get_mut(group_id(1)).unwrap().motion.position[1] =
        BVector::new(f32::INFINITY, 0.0, 0.0);
    t.front_by_centroid(group_id(1)).unwrap();
    assert!(
        t.groups.get(group_id(1)).unwrap().motion.position[0]
            .x()
            .is_nan()
    );
}

/// The formation lines' attack ranges: each its slots' units' least weapon range (a combat
/// master's; 0 for another type), `f32::MAX` without a held one; a spare slot's index reads
/// the spare slot; a place without a slot is not modelled.
#[test]
fn each_line_takes_its_units_least_range() {
    let mut t = world();
    let combat = |id: MasterId, kind: MasterKind, range: Distance| Master {
        id,
        kind,
        weapon_range: range,
        ..Master::default()
    };
    t.masters = vec![
        Masters::new([
            combat(master_id(4), master_type::COMBAT, Distance::new(3.0)),
            combat(master_id(5), master_type::COMBAT, Distance::new(1.5)),
            combat(ids::ID_6, master_type::MOVING, Distance::new(2.0))
        ]);
        4
    ];
    t.players[1].objects[ListIndex::new(1)].life.master_id = master_id(5);
    t.players[1].objects[ListIndex::new(2)].life.master_id = ids::ID_6;
    let slot = |unit| crate::groups::Slot {
        unit_id: unit,
        ..crate::groups::Slot::default()
    };
    let line = |indices: Vec<u32>| crate::groups::FormationLine {
        indices,
        ..crate::groups::FormationLine::default()
    };
    t.groups.get_mut(group_id(1)).unwrap().current_formation = Some(crate::groups::Formation {
        slots: vec![
            slot(Some(object_id(10))),
            slot(Some(object_id(11))),
            slot(Some(object_id(99))),
        ],
        spare_slots: vec![slot(Some(object_id(12)))],
        lines: vec![
            line(vec![0, 1]),
            line(vec![2]),
            line(vec![3, 0]),
            line(vec![0]),
        ],
        number_lines: 3,
        ..crate::groups::Formation::default()
    });
    t.line_ranges(group_id(1)).unwrap();
    let f = t
        .groups
        .get(group_id(1))
        .unwrap()
        .current_formation
        .clone()
        .unwrap();
    // the fourth line is not in use: it keeps its range
    assert_eq!(
        f.lines
            .iter()
            .map(|l| l.attack_range.get())
            .collect::<Vec<_>>(),
        [1.5, f32::MAX, 0.0, 0.0]
    );
    t.groups
        .get_mut(group_id(1))
        .unwrap()
        .current_formation
        .as_mut()
        .unwrap()
        .lines[1]
        .indices = vec![u32::MAX];
    assert!(t.line_ranges(group_id(1)).is_err());
}

/// The relayout's choice: a changed count sets [`GroupFlags::RELAYOUT_SPREAD`] at a spacing of at
/// most 6 (a NaN too) and relays out above it; an attack's kind-6 formation refills instead; bit 7
/// above 6 relays out and clears bits 7 and 1; bit 1 above 6 relays out an idle group.
#[test]
fn the_relayout_follows_the_count_and_the_spacing() {
    let mut t = world();
    let set = |t: &mut World, flags: u32, spacing: f32| {
        let g = t.groups.get_mut(group_id(1)).unwrap();
        g.flags = GroupFlags::from_bits_retain(flags);
        g.total_spacing = Distance::new(spacing);
    };
    set(&mut t, 0, 6.0);
    assert_eq!(t.relayout_choice(group_id(1), 3).unwrap(), (false, false));
    assert_eq!(t.relayout_choice(group_id(1), 4).unwrap(), (false, false));
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().flags,
        GroupFlags::RELAYOUT_SPREAD
    );
    set(&mut t, 0, f32::NAN);
    assert_eq!(t.relayout_choice(group_id(1), 4).unwrap(), (false, false));
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().flags,
        GroupFlags::RELAYOUT_SPREAD
    );
    set(&mut t, 0, 6.5);
    assert_eq!(t.relayout_choice(group_id(1), 4).unwrap(), (true, false));
    set(&mut t, 0x82, 6.5);
    assert_eq!(t.relayout_choice(group_id(1), 3).unwrap(), (true, false));
    assert_eq!(t.groups.get(group_id(1)).unwrap().flags, GroupFlags::NONE);
    set(&mut t, 0x2, 6.5);
    assert_eq!(t.relayout_choice(group_id(1), 3).unwrap(), (true, false));
    assert_eq!(t.groups.get(group_id(1)).unwrap().flags, GroupFlags::NONE);
    t.groups.get_mut(group_id(1)).unwrap().motion.velocity = BVector::new(0.0, 0.0, 1.0);
    set(&mut t, 0x2, 6.5);
    assert_eq!(t.relayout_choice(group_id(1), 3).unwrap(), (false, false));
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().flags,
        GroupFlags::RELAYOUT
    );
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.current_command = GroupCommandKind::ATTACK;
    g.current_formation = Some(crate::groups::Formation {
        kind: FormationKind::TYPE_6,
        ..crate::groups::Formation::default()
    });
    set(&mut t, 0, 6.0);
    assert_eq!(t.relayout_choice(group_id(1), 4).unwrap(), (false, true));
    assert_eq!(t.groups.get(group_id(1)).unwrap().flags, GroupFlags::NONE);
}

/// A follow: its target near where the group's target was keeps the group
/// ([`GroupFlags::FOLLOWED`]); far, the group moves to it (`commandMoveTo`: the members' seconds
/// since attacking go to `f32::MAX`); without the target it ends as a finished move: deleted
/// without queued commands, else hibernating.
#[test]
fn a_follow_moves_to_its_target_when_it_strays() {
    let mut t = world();
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.current_command = GroupCommandKind::FOLLOW;
    g.current_target.target = GroupTarget::of_saved(20, -1);
    g.current_target.original_position = BVector::new(3.5, 0.0, 4.5);
    g.flags = GroupFlags::NONE;
    let mut relayout = false;
    // 1 from the target: within half the lead's line of sight (2)
    assert!(t.command_update(group_id(1), &mut relayout).unwrap());
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(
        (g.flags, g.members[0].seconds_since_attacking),
        (GroupFlags::FOLLOWED, Seconds::ZERO)
    );
    t.groups
        .get_mut(group_id(1))
        .unwrap()
        .current_target
        .original_position = BVector::new(3.5, 0.0, 5.5);
    assert!(t.command_update(group_id(1), &mut relayout).unwrap());
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(
        (g.flags, g.members[0].seconds_since_attacking),
        (GroupFlags::FOLLOWED, Seconds::MAX)
    );
    assert_eq!(g.motion.lead, Some(object_id(12)));
    t.groups.get_mut(group_id(1)).unwrap().current_target.target = GroupTarget::of_saved(77, -1);
    assert!(!t.command_update(group_id(1), &mut relayout).unwrap());
    t.groups.get_mut(group_id(1)).unwrap().command_queue =
        vec![crate::groups::GroupCommand::default()];
    assert!(t.command_update(group_id(1), &mut relayout).unwrap());
    assert_eq!(t.groups.get(group_id(1)).unwrap().hibernating, 1);
    assert!(!relayout);
    t.groups.get_mut(group_id(1)).unwrap().current_command = GroupCommandKind::named(3);
    assert!(t.command_update(group_id(1), &mut relayout).is_err());
}

/// `FUN_141041420`: [`MotionFlags::NEW`] and a moving group ask for a relayout; with flags bit
/// 12 a strayed target (a squared 1 from the top path's end) turns the move into a follow;
/// wait delays and a group waiting for a path do nothing.
#[test]
fn a_moving_group_s_move_waits_or_follows() {
    let mut t = world();
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.current_command = GroupCommandKind::MOVE;
    g.flags = GroupFlags::NONE;
    g.motion.flags = MotionFlags::NEW;
    let mut relayout = false;
    assert!(!t.move_update(group_id(1), &mut relayout).unwrap());
    assert!(relayout);
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.motion.flags = MotionFlags::NONE;
    g.motion.velocity = BVector::new(0.0, 0.0, 1.0);
    let mut relayout = false;
    assert!(!t.move_update(group_id(1), &mut relayout).unwrap());
    assert!(relayout);
    // a follow's target 20 at (3.5, 3.5) and the top path's end at (3.5, h, 3.5): no change
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.flags = GroupFlags::FOLLOWED;
    g.current_target.target = GroupTarget::of_saved(20, -1);
    let mut path = crate::movement::PathData::without_path();
    path.has_path = true;
    path.points = vec![BVector::ZERO, BVector::new(3.5, 0.0, 3.5)];
    g.motion.paths = vec![path];
    let mut relayout = false;
    assert!(!t.move_update(group_id(1), &mut relayout).unwrap());
    assert_eq!(
        (relayout, t.groups.get(group_id(1)).unwrap().current_command),
        (true, GroupCommandKind::MOVE)
    );
    t.groups.get_mut(group_id(1)).unwrap().motion.paths[0].points[1] = BVector::new(3.5, 0.0, 4.5);
    let mut relayout = false;
    assert!(!t.move_update(group_id(1), &mut relayout).unwrap());
    assert_eq!(
        (relayout, t.groups.get(group_id(1)).unwrap().current_command),
        (false, GroupCommandKind::FOLLOW)
    );
    // standing with bit 12: a follow at once; else wait delays or a level waiting do nothing
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.current_command = GroupCommandKind::MOVE;
    g.motion.velocity = BVector::ZERO;
    assert!(!t.move_update(group_id(1), &mut relayout).unwrap());
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().current_command,
        GroupCommandKind::FOLLOW
    );
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.current_command = GroupCommandKind::MOVE;
    g.flags = GroupFlags::NONE;
    g.motion.number_wait_delays = DelayCount::of_data(1);
    assert!(!t.move_update(group_id(1), &mut relayout).unwrap());
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.motion.number_wait_delays = DelayCount::ZERO;
    g.motion.paths[0].state = PathState::WantsPath;
    assert!(!t.move_update(group_id(1), &mut relayout).unwrap());
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().current_command,
        GroupCommandKind::MOVE
    );
}

/// `FUN_141041420` for a standing group: without a path walked and no formation the move finishes
/// (command -1, [`GroupFlags::LINE`] off and [`GroupFlags::MEMBERS_SENT`] on); with one walked it
/// goes on; on a patrol it does not finish; a formation whose slots are all in their places lets
/// their units go and ends the command; one with none in place locks to its kind.
#[test]
fn a_standing_group_s_move_ends_or_goes_on() {
    let mut t = world();
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.current_command = GroupCommandKind::MOVE;
    g.flags = GroupFlags::LINE;
    g.motion.flags = MotionFlags::NONE;
    let mut relayout = false;
    assert!(t.move_update(group_id(1), &mut relayout).unwrap());
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(
        (g.current_command, g.flags),
        (GroupCommandKind::NONE, GroupFlags::MEMBERS_SENT)
    );
    let walked = || {
        let mut path = crate::movement::PathData::without_path();
        path.has_path = true;
        path.points = vec![BVector::ZERO, BVector::new(3.5, 0.0, 3.5)];
        path
    };
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.current_command = GroupCommandKind::MOVE;
    g.motion.paths = vec![walked()];
    assert!(!t.move_update(group_id(1), &mut relayout).unwrap());
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().current_command,
        GroupCommandKind::MOVE
    );
    // on a patrol ([`GroupFlags::PATROL`]) without a path walked: the command ends, unfinished
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.motion.paths[0].has_path = false;
    g.flags = GroupFlags::PATROL;
    assert!(!t.move_update(group_id(1), &mut relayout).unwrap());
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().current_command,
        GroupCommandKind::NONE
    );
    // every slot in place: its units leave and the command ends
    let mut t = world();
    let slot = |unit, state| crate::groups::Slot {
        unit_id: unit,
        state,
        ..crate::groups::Slot::default()
    };
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.current_command = GroupCommandKind::MOVE;
    g.motion.flags = MotionFlags::NONE;
    g.current_formation = Some(crate::groups::Formation {
        kind: FormationKind::COLUMN,
        slots: vec![
            slot(Some(object_id(10)), SlotState::InPlace),
            slot(Some(object_id(11)), SlotState::InPlace),
        ],
        ..crate::groups::Formation::default()
    });
    assert!(t.move_update(group_id(1), &mut relayout).unwrap());
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(
        g.members.iter().map(|m| m.id).collect::<Vec<_>>(),
        [object_id(12)]
    );
    assert_eq!(g.current_command, GroupCommandKind::NONE);
    // none in place but no free unit: the formation locks to its kind, the command ends
    let mut t = world();
    for u in &mut t.players[1].objects {
        t.parts.unit_ai.get_mut(u.id).unwrap().current_action = Some(ActionKind::Move);
    }
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.current_command = GroupCommandKind::MOVE;
    g.motion.flags = MotionFlags::NONE;
    g.current_formation = Some(crate::groups::Formation {
        kind: FormationKind::COLUMN,
        slots: vec![slot(Some(object_id(10)), SlotState::Steering)],
        ..crate::groups::Formation::default()
    });
    assert!(!t.move_update(group_id(1), &mut relayout).unwrap());
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().current_command,
        GroupCommandKind::NONE
    );
}

/// `AutomaticMoveUnitsToDesiredPositions`: a slot without a unit, of a unit the world does
/// not hold or that is busy, or in its place (on no attack), is passed; a free unit out of
/// its place gets a path to it, not modelled.
#[test]
fn units_out_of_their_places_get_paths() {
    let mut t = world();
    let slot = |unit, state| crate::groups::Slot {
        unit_id: unit,
        state,
        ..crate::groups::Slot::default()
    };
    t.parts
        .unit_ai
        .get_mut(t.players[1].objects[ListIndex::new(1)].id)
        .unwrap()
        .current_action = Some(ActionKind::Move);
    t.groups.get_mut(group_id(1)).unwrap().current_formation = Some(crate::groups::Formation {
        slots: vec![
            slot(None, SlotState::Steering),
            slot(Some(object_id(99)), SlotState::Steering),
            slot(Some(object_id(11)), SlotState::Steering),
            slot(Some(object_id(10)), SlotState::InPlace),
        ],
        ..crate::groups::Formation::default()
    });
    t.move_to_desired_positions(group_id(1), GroupCommandKind::MOVE, None)
        .unwrap();
    t.move_to_desired_positions(group_id(1), GroupCommandKind::ATTACK, None)
        .unwrap();
    assert!(
        t.move_to_desired_positions(group_id(1), GroupCommandKind::ATTACK, Some(object_id(20)))
            .is_err()
    );
    t.groups
        .get_mut(group_id(1))
        .unwrap()
        .current_formation
        .as_mut()
        .unwrap()
        .slots[3]
        .state = SlotState::Steering;
    assert!(
        t.move_to_desired_positions(group_id(1), GroupCommandKind::MOVE, None)
            .is_err()
    );
}

/// The members' own moves: a member moved aside (reason 2) that is free stops, counts its
/// timer down and, at 0, drops its move and stands; a busy one or one without an active
/// move is passed; another reason's step is not modelled.
#[test]
fn a_member_moved_aside_waits_out_its_timer() {
    let mut t = world();
    t.dt_seconds = Seconds::new(0.5);
    for u in &mut t.players[1].objects {
        t.parts
            .put_mover(u.id, Some(crate::gathering::hunt::tests::walking()));
    }
    let aside = crate::groups::SpecificMove {
        active: 1,
        path_id: MovePath::of_data(4),
        reason: MoveReason::ASIDE,
        timer: Seconds::new(0.75),
        ..crate::groups::SpecificMove::default()
    };
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.members[0].specific_move = aside;
    g.members[1].specific_move = aside;
    t.parts
        .unit_ai
        .get_mut(t.players[1].objects[ListIndex::new(1)].id)
        .unwrap()
        .current_action = Some(ActionKind::Move);
    t.specific_moves_step(group_id(1)).unwrap();
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(g.members[0].specific_move.timer, Seconds::new(0.25));
    assert_eq!(g.members[1].specific_move, aside);
    assert_eq!(t.parts.anim_speed_of(object_id(10)), Some(Speed::ZERO));
    assert_ne!(t.parts.anim_speed_of(object_id(11)), Some(Speed::ZERO));
    t.specific_moves_step(group_id(1)).unwrap();
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(
        g.members[0].specific_move,
        crate::groups::SpecificMove {
            path_id: MovePath::NONE,
            reason: MoveReason::NONE,
            ..crate::groups::SpecificMove::default()
        }
    );
    // a NaN timer keeps the move
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.members[0].specific_move = crate::groups::SpecificMove {
        timer: Seconds::new(f32::NAN),
        ..aside
    };
    t.specific_moves_step(group_id(1)).unwrap();
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().members[0]
            .specific_move
            .active,
        1
    );
    // any other reason takes the unit's own motion step: without a path it is done at
    // once, and the move goes
    t.groups.get_mut(group_id(1)).unwrap().members[0]
        .specific_move
        .reason = MoveReason::of_data(1);
    t.specific_moves_step(group_id(1)).unwrap();
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().members[0]
            .specific_move
            .active,
        0
    );
}

/// A member's own move of the way round (reason 0) steps the unit along its path (slot
/// 0x608): moving on (the step turns it toward its waypoint; the velocity update moves it
/// later), the move's distance loses the ground distance from its last position to where the
/// unit last moved to (here 1 tile short of it on y), which becomes its last position.
#[test]
fn a_member_s_own_move_walks_its_path() {
    let mut t = world();
    t.dt_seconds = Seconds::new(0.5);
    let mut walker = crate::gathering::hunt::tests::walker();
    walker.moved_at = Position::new(1.0, 1.0, 0.0);
    walker.take_record_path(
        vec![BVector::new(1.0, 0.0, 1.0), BVector::new(5.0, 0.0, 1.0)],
        Distance::new(4.0),
        WorldTime::ZERO,
        false,
    );
    t.parts
        .put_mover(t.players[1].objects[ListIndex::new(0)].id, Some(walker));
    t.spatial.insert(
        object_id(10),
        crate::map::spatial::Placed {
            position: Position::new(1.0, 1.0, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::CLASS_6,
            player: PlayerId::new(1),
            master: master_id(4),
            obstruction_type: ObstructionType::NONE,
            buildable_on: BuildableOn::NO,
            tall: false,
            moves: true,
            unknown: None,
            obstruction: None,
        },
    );
    t.groups.get_mut(group_id(1)).unwrap().members[0].specific_move = crate::groups::SpecificMove {
        active: 1,
        path_id: MovePath::NONE,
        reason: MoveReason::DETOUR,
        timer: Seconds::new(3.0),
        distance: Distance::new(3.0),
        last_position: BVector::new(1.0, 0.0, 0.0),
        ..crate::groups::SpecificMove::default()
    };
    t.specific_moves_step(group_id(1)).unwrap();
    let (_, u) = t.object(object_id(10)).unwrap();
    let at = u.moved_vector(t.parts.mover_of(u.id));
    let own = t.groups.get(group_id(1)).unwrap().members[0].specific_move;
    assert_eq!(own.active, 1);
    assert_eq!(
        (at, own.last_position),
        (BVector::new(1.0, 0.0, 1.0), BVector::new(1.0, 0.0, 1.0))
    );
    assert_eq!(own.distance, Distance::new(2.0));
}

/// The walks read a unit's gait from the unit itself ([`crate::world::World::objects`]): after a
/// group update that changes its members' movers, they read the movers as it left them. Here the
/// group goes (its members have no unit AI) and its free members stand, no longer moving.
#[test]
fn a_group_update_leaves_its_members_gaits_current() {
    let mut t = world();
    let walker = crate::gathering::hunt::tests::walker();
    for u in &mut t.players[1].objects {
        t.parts.put_mover(u.id, Some(walker.clone()));
        t.parts.mover_mut(u.id).unwrap().moved_at = u.position();
        t.parts.set_unit_ai(u.id, None);
    }
    assert!(t.objects().gait(object_id(10)).unwrap().moving);
    t.update_groups(PlayerId::new(1)).unwrap();
    for id in 10..=12 {
        assert!(!t.objects().gait(object_id(id)).unwrap().moving, "{id}");
    }
}

/// `FUN_14100d940`: the orders whose time has come go; later ones stay.
#[test]
fn due_orders_leave_the_queue() {
    let mut t = world();
    t.world_time = WorldTime::from_millis(100);
    let order = |execute_time| crate::groups::GroupOrder {
        execute_time,
        ..crate::groups::GroupOrder::default()
    };
    t.groups.get_mut(group_id(1)).unwrap().order_queue = vec![
        order(WorldTime::from_millis(99)),
        order(WorldTime::from_millis(101)),
        order(WorldTime::from_millis(100)),
    ];
    t.drop_due_orders(group_id(1));
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().order_queue,
        [order(WorldTime::from_millis(101))]
    );
}

/// The idle timer: an idle group adds the update's milliseconds and draws anew past 3000; a
/// busy one draws anew. The milliseconds come back exactly from the update's seconds.
#[test]
fn the_idle_timer_counts_while_idle() {
    let mut t = world();
    t.dt_seconds = Millis::new(69).seconds();
    t.groups.get_mut(group_id(1)).unwrap().idle_timer = Millis::new(2931);
    t.idle_timer_step(group_id(1)).unwrap();
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().idle_timer,
        Millis::new(3000)
    );
    let mut streams = t.streams.clone();
    let draw = streams.get(crate::math::rng::Stream::World).rand15();
    t.idle_timer_step(group_id(1)).unwrap();
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().idle_timer,
        Millis::new(draw % 750)
    );
    t.groups.get_mut(group_id(1)).unwrap().motion.velocity = BVector::new(0.0, 0.0, 1.0);
    t.groups.get_mut(group_id(1)).unwrap().idle_timer = Millis::new(5);
    t.idle_timer_step(group_id(1)).unwrap();
    assert!(t.groups.get(group_id(1)).unwrap().idle_timer < Millis::new(750));
    for dt in 0..=100_000 {
        t.dt_seconds = Millis::new(dt).seconds();
        assert_eq!(t.dt_ms(), Millis::new(dt));
    }
}

/// The awake update's ends: an idle group (no command, none queued) whose formation's slots are all
/// in state 3 goes, one with [`GroupFlags::NO_PATH_UNIT`] goes, and one whose lead the world does
/// not hold goes; with every member kept apart and nothing pending, each leaves the group.
#[test]
fn an_idle_group_goes() {
    let mut t = world();
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.current_command = GroupCommandKind::NONE;
    g.flags = GroupFlags::NONE;
    g.current_formation = Some(crate::groups::Formation {
        kind: FormationKind::COLUMN,
        slots: vec![crate::groups::Slot {
            state: SlotState::InPlace,
            ..crate::groups::Slot::default()
        }],
        ..crate::groups::Formation::default()
    });
    assert!(!t.group_update(group_id(1)).unwrap());
    let mut t = world();
    t.groups.get_mut(group_id(1)).unwrap().flags = GroupFlags::NO_PATH_UNIT;
    assert!(!t.group_update(group_id(1)).unwrap());
    // every member kept apart: each leaves, then the group has none
    let mut t = world();
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.flags = GroupFlags::NONE;
    g.objects = vec![object_id(10), object_id(11), object_id(12)];
    g.current_command = GroupCommandKind::MOVE;
    assert!(!t.group_update(group_id(1)).unwrap());
    assert_eq!(t.groups.get(group_id(1)).unwrap().members, Vec::new());
    assert_eq!(t.groups.get(group_id(1)).unwrap().objects, Vec::new());
    assert_eq!(t.object(object_id(11)).unwrap().1.group.id, None);
}

/// `BaseWorld::update` runs a player's groups (`FUN_14103d910` at `0x14155179b`) before the
/// player's own update (slot 0x100 at `0x1415518f0`, which updates its objects): a group
/// whose member leaves it (no unit AI) stops the member, which then does not move in the
/// same update.
#[test]
fn a_player_s_groups_update_before_its_objects() {
    let mut t = world();
    let walker = crate::gathering::hunt::tests::walker();
    for u in &mut t.players[1].objects {
        t.parts.put_mover(u.id, Some(walker.clone()));
        t.parts.mover_mut(u.id).unwrap().moved_at = u.position();
        t.parts.set_unit_ai(u.id, None);
    }
    let before = t.object(object_id(10)).unwrap().1.position();
    t.tick(Millis::new(69));
    let u = t.object(object_id(10)).unwrap().1;
    assert_eq!(u.group.id, None);
    assert_eq!(u.position(), before);
    assert_eq!(
        t.parts
            .mover_of(object_id(10))
            .unwrap()
            .velocity
            .unwrap()
            .velocity
            .y(),
        0.0
    );
}

/// Slot 0x5e0 (`FUN_14114f300`) in the members' own update: the group has no formation, so no
/// member is in its place and the group steers its free members' speeds (its owner is no
/// player, so that no group update moves them first). 10, on an own move (reason 1) whose walked level is paused, stands; 11, without one
/// (reason −1), keeps its speed of 1 above its top speed of 0.8 (below 10) and moves 1 a second
/// (0.1 in the update); 12, on an own move (reason 0) without a walked level, stands.
#[test]
fn a_group_steers_its_free_members_speeds_in_their_own_update() {
    let mut t = world();
    let walker = crate::gathering::hunt::tests::walker();
    for u in &mut t.players[1].objects {
        let mut m = walker.clone();
        m.moved_at = u.position();
        m.velocity = Some(crate::movement::Velocity {
            velocity: BVector::new(0.0, 0.0, 1.0),
            acceleration: BVector::ZERO,
        });
        t.parts.put_mover(u.id, Some(m));
    }
    t.parts
        .mover_mut(t.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .paths = vec![crate::movement::PathData {
        state: PathState::Walking,
        pause_time: Seconds::new(0.5),
        ..crate::movement::PathData::without_path()
    }];
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.owner = Some(PlayerId::new(9));
    g.motion.maximum_velocity_forward = Speed::new(0.9);
    g.members[0].specific_move.reason = MoveReason::of_data(1);
    g.members[1].specific_move.reason = MoveReason::NONE;
    g.members[2].specific_move.reason = MoveReason::DETOUR;
    let before: Vec<_> = object_id_list(10..=12)
        .into_iter()
        .map(|id| t.object(id).unwrap().1.position())
        .collect();
    t.tick(Millis::new(100));
    assert_eq!(t.players[1].unsupported, None);
    let speed = |t: &World, id| {
        t.parts
            .mover_of(object_id(id))
            .unwrap()
            .velocity
            .unwrap()
            .velocity
            .y()
    };
    assert_eq!(speed(&t, 10), 0.0);
    assert_eq!(t.object(object_id(10)).unwrap().1.position(), before[0]);
    assert_eq!(speed(&t, 11), 1.0);
    assert_eq!(
        t.object(object_id(11)).unwrap().1.position(),
        Position::new(1.9, 1.0, 0.0)
    );
    assert_eq!(speed(&t, 12), 0.0);
    assert_eq!(t.object(object_id(12)).unwrap().1.position(), before[2]);
}

/// [`Unit::group_speed`]: the group steers a free member while its motion moves or the member
/// is not in its place; one in its place in a standing group, one not free (its unit AI acts),
/// one outside a group the world holds and one in none are not steered. The reason picks the
/// answer: 2 aside, −1 free, any other an own move at the group's top speed.
#[test]
fn the_speed_slot_reads_the_group_that_steers_a_free_member() {
    let t = world();
    let mut g = t.groups.get(group_id(1)).unwrap().clone();
    g.motion.maximum_velocity_forward = Speed::new(0.9);
    g.current_formation = Some(crate::groups::Formation {
        slots: vec![crate::groups::Slot {
            unit_id: Some(object_id(10)),
            state: SlotState::InPlace,
            ..crate::groups::Slot::default()
        }],
        ..crate::groups::Formation::default()
    });
    g.members[0].specific_move.reason = MoveReason::ASIDE;
    g.members[1].specific_move.reason = MoveReason::NONE;
    g.members[2].specific_move.reason = MoveReason::of_data(4);
    let groups = UnitGroups {
        groups: vec![g],
        ..UnitGroups::default()
    };
    let masters =
        crate::objects::masters::OwnerMasters::new(&t.parts.own_master, Some(&t.masters[1]));
    let of = |groups: &UnitGroups, id| {
        let u = t.object(object_id(id)).unwrap().1;
        u.group_speed(groups, masters, t.parts.unit_ai.get(u.id))
    };
    assert_eq!(of(&groups, 10), Ok(None));
    assert_eq!(of(&groups, 11), Ok(Some(GroupSpeed::Free)));
    assert_eq!(
        of(&groups, 12),
        Ok(Some(GroupSpeed::OwnMove(Speed::new(0.9))))
    );
    let mut moving = groups.clone();
    moving.get_mut(group_id(1)).unwrap().motion.velocity = BVector::new(0.0, 0.0, 1.0);
    assert_eq!(of(&moving, 10), Ok(Some(GroupSpeed::Aside)));
    let busy = t.object(object_id(11)).unwrap().1.clone();
    let mut busy_ai = t.parts.unit_ai.get(busy.id).unwrap().clone();
    busy_ai.current_action = Some(ActionKind::Attack);
    assert_eq!(busy.group_speed(&groups, masters, Some(&busy_ai)), Ok(None));
    let mut gone = busy;
    gone.group.id = Some(group_id(7));
    assert_eq!(gone.group_speed(&groups, masters, Some(&busy_ai)), Ok(None));
    gone.group.id = None;
    assert_eq!(gone.group_speed(&groups, masters, Some(&busy_ai)), Ok(None));
    let lost = t.object(object_id(11)).unwrap().1;
    assert_eq!(
        lost.group_speed(
            &groups,
            crate::objects::masters::OwnerMasters::new(&t.parts.own_master, None),
            t.parts.unit_ai.get(lost.id)
        ),
        Err(crate::world::OBJECT_MASTER)
    );
}

/// `FUN_141040b50`: a motion that moved (bit 0) stores its new position at the front; bit 1
/// moves to the group's flags, and both motion bits clear. A motion that did not move keeps
/// its stored positions.
#[test]
fn the_update_end_stores_a_moved_group_s_new_position() {
    let mut g = UnitGroup::new(group_id(1));
    g.motion.new_position[0] = BVector::new(3.0, 0.0, 4.0);
    g.motion.flags = MotionFlags::MOVED | MotionFlags::NEW;
    g.flags = GroupFlags::NONE;
    g.update_end().unwrap();
    assert_eq!(
        (g.motion.flags, g.flags & GroupFlags::RELAYOUT),
        (MotionFlags::NONE, GroupFlags::RELAYOUT)
    );
    assert_eq!(
        g.stored_positions.points.first().map(|p| p.point),
        Some(BVector::new(3.0, 0.0, 4.0))
    );
    let mut still = UnitGroup::new(group_id(2));
    still.motion.flags = MotionFlags::NONE;
    let before = still.stored_positions.points.len();
    still.update_end().unwrap();
    assert_eq!(still.stored_positions.points.len(), before);
}

/// `FUN_141016870`: the lead is the held member nearest (in 3D, where it last moved to) the
/// target's place, the first of equals; -1 without a held member.
#[test]
fn the_lead_is_the_member_nearest_the_targets_place() {
    let at = |id, x: f32, y: f32| Unit {
        id,
        place: crate::objects::layers::Static::at(Position::new(x, y, 0.0)),
        ..Unit::default()
    };
    let player = Player {
        objects: vec![
            at(object_id(10), 5.0, 5.0),
            at(object_id(11), 2.0, 1.0),
            at(object_id(12), 1.0, 2.0),
        ]
        .into(),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    let member = |id| Member {
        id,
        ..Member::default()
    };
    let mut g = UnitGroup::new(group_id(1));
    // 99 is not held; 11 and 12 are as near (1, 1) as each other: 11 comes first
    g.members = vec![
        member(object_id(99)),
        member(object_id(10)),
        member(object_id(11)),
        member(object_id(12)),
    ];
    g.current_target.original_position = BVector::new(1.0, 0.0, 1.0);
    t.groups = UnitGroups {
        groups: vec![g],
        ..UnitGroups::default()
    };
    assert_eq!(t.nearest_to_target(group_id(1)), Some(object_id(11)));
    t.groups.groups[0].members = vec![member(object_id(99))];
    assert_eq!(t.nearest_to_target(group_id(1)), None);
    assert_eq!(t.nearest_to_target(group_id(2)), None);
}

/// `FUN_141017630`, `resetIdleTimer` (`FUN_14102d370`) and `IsIdle` (`FUN_141017ad0`).
#[test]
fn a_group_holds_members_draws_its_idle_timer_and_tells_when_it_is_idle() {
    let held = Unit {
        id: object_id(10),
        ..Unit::default()
    };
    let player = Player {
        objects: vec![held].into(),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player],
    );
    let member = |id| Member {
        id,
        ..Member::default()
    };
    let mut g = UnitGroup::new(group_id(1));
    g.members = vec![member(object_id(99)), member(object_id(10))];
    t.groups = UnitGroups {
        groups: vec![g],
        ..UnitGroups::default()
    };
    assert!(t.any_member_held(group_id(1)));
    t.groups.groups[0].members = vec![member(object_id(99))];
    assert!(!t.any_member_held(group_id(1)));
    assert!(!t.any_member_held(group_id(2)));
    // the World stream's next draw, modulo 750
    let mut streams = t.streams.clone();
    let draw = streams.get(crate::math::rng::Stream::World).rand15();
    t.reset_idle_timer(group_id(1));
    assert_eq!(t.groups.groups[0].idle_timer, Millis::new(draw % 750));
    assert_eq!(t.streams, streams);
    // idle: standing; not while moving or waiting for a path; idle again with bit 5, or on a
    // move with bit 11
    assert!(t.is_idle(group_id(1)));
    t.groups.groups[0].motion.velocity = BVector::new(0.0, 0.0, 1.0);
    assert!(!t.is_idle(group_id(1)));
    t.groups.groups[0].current_command = GroupCommandKind::MOVE;
    t.groups.groups[0].flags |= GroupFlags::PATROL;
    assert!(t.is_idle(group_id(1)));
    t.groups.groups[0].flags = GroupFlags::MEMBERS_SENT;
    assert!(t.is_idle(group_id(1)));
    assert!(!t.is_idle(group_id(2)));
}

/// `FUN_14102bb40`: the command queue empties; a hibernating group (1) wakes with flags
/// `0x4c000`, any other hibernation value wakes without them.
#[test]
fn emptying_the_command_queue_wakes_a_hibernating_group() {
    let mut g = UnitGroup::new(group_id(1));
    g.command_queue = vec![crate::groups::GroupCommand::default()];
    g.hibernating = 1;
    g.clear_command_queue();
    assert_eq!(g.command_queue, Vec::new());
    assert_eq!(
        (g.hibernating, g.flags),
        (0, GroupFlags::from_bits_retain(0x100 | 0x4_c000))
    );
    let mut g = UnitGroup::new(group_id(1));
    g.hibernating = 2;
    g.clear_command_queue();
    assert_eq!((g.hibernating, g.flags), (0, GroupFlags::TURN));
}

/// The long pather's list of a unit in group 1 (whose differentiator groups are 1 and 2) is
/// 10, 11 and 12; a unit of no group, or of a group the world does not hold, has none.
#[test]
fn a_grouped_units_list_is_its_groups_differentiator_units() {
    let member = |id| Member {
        id,
        ..Member::default()
    };
    let mut one = UnitGroup::new(group_id(1));
    one.members = vec![member(object_id(10)), member(object_id(11))];
    one.differentiator_groups = vec![group_id(1), group_id(2)];
    let mut two = UnitGroup::new(group_id(2));
    two.members = vec![member(object_id(12))];
    let groups = UnitGroups {
        groups: vec![one, two],
        ..UnitGroups::default()
    };
    assert_eq!(
        groups.group_units(Some(group_id(1))),
        Some(vec![object_id(10), object_id(11), object_id(12)])
    );
    assert_eq!(groups.group_units(Some(group_id(2))), Some(Vec::new()));
    assert_eq!(groups.group_units(Some(group_id(9))), None);
    assert_eq!(groups.group_units(None), None);
}

/// [`world`] on its 8 by 8 map of one terrain every master stands on, its soldiers placed.
fn grounded() -> World {
    use crate::map::terrain::{Terrain, TerrainKind};
    let mut t = world();
    let kinds = vec![TerrainKind {
        shown: true,
        flags: TerrainFlags::WATER,
        forest: false,
    }];
    t.spatial.terrain = Some(Terrain::new(
        GridSize::new(8, 8),
        vec![TileTerrain::of_save(0, 255, -1); 64],
        kinds,
        vec![Restriction::new(vec![1.0])],
    ));
    t.spatial.sight.as_mut().unwrap().registry =
        Some(crate::vision::registry::Registry::new(4, 65));
    for p in PlayerId::each(4).skip(1) {
        for u in &mut t.players[p.index()].objects {
            t.parts.unit_ai.get_mut(u.id).unwrap().ai_type = crate::unit_ai::SOLDIER_AI;
            t.parts.put_mover(
                u.id,
                Some(crate::movement::Mover {
                    moved_at: u.position(),
                    ..crate::gathering::hunt::tests::walker()
                }),
            );
        }
        for u in t.players[p.index()].objects.clone() {
            t.spatial.insert(
                u.id,
                crate::map::spatial::Placed {
                    position: u.position(),
                    radius: Footprint::new(0.2, 0.2),
                    class: class::CLASS_6,
                    player: p,
                    master: master_id(4),
                    obstruction_type: ObstructionType::NONE,
                    buildable_on: BuildableOn::NO,
                    tall: true,
                    moves: false,
                    unknown: None,
                    obstruction: None,
                },
            );
        }
    }
    t
}

/// An awake group without a command (`BUnitGroup::update` from `0x14103dd65`) runs on: its lead is
/// the member nearest its target's place (10). With [`MotionFlags::NEW`] it places itself
/// ([`GroupFlags::PLACED`]) facing its members' mean forward, and its formation turns (bit 8
/// clears): the walked level holds no path, so the group moves to its members' mean place. Without
/// bit 1 nothing turns it, unless [`GroupFlags::RELAYOUT_AT_TURN`] does;
/// [`GroupFlags::LEAD_TO_TARGET`] puts it at its centroid; [`GroupFlags::PLACE_AGAIN`] places it
/// too, and clears.
#[test]
fn an_awake_group_without_a_command_places_itself_and_turns_its_formation() {
    let mean = BVector::new(4.0 / 3.0, 0.0, 4.0 / 3.0);
    let run = |flags, motion_flags| {
        let mut t = grounded();
        let g = t.groups.get_mut(group_id(1)).unwrap();
        (g.flags, g.motion.flags) = (flags, motion_flags);
        assert_eq!(t.group_update(group_id(1)), Ok(true));
        let g = t.groups.get(group_id(1)).unwrap().clone();
        assert_eq!((g.motion.lead, g.centroid), (Some(object_id(10)), mean));
        g
    };
    let g = run(GroupFlags::TURN, MotionFlags::MOVED | MotionFlags::NEW);
    assert_eq!(g.flags, GroupFlags::PLACED);
    assert_eq!(g.motion.position[1], BVector::new(-1.0, 0.0, 0.0));
    assert_eq!(g.motion.position[0], mean);
    let g = run(GroupFlags::TURN, MotionFlags::MOVED);
    assert_eq!(g.flags, GroupFlags::NONE);
    assert_eq!(
        g.motion.position[..2],
        [BVector::ZERO, BVector::new(1.0, 0.0, 0.0)]
    );
    let g = run(
        GroupFlags::TURN | GroupFlags::RELAYOUT_AT_TURN,
        MotionFlags::MOVED,
    );
    assert_eq!(g.flags, GroupFlags::NONE);
    assert_eq!(g.motion.position[0], mean);
    let g = run(GroupFlags::LEAD_TO_TARGET, MotionFlags::MOVED);
    assert_eq!(g.flags, GroupFlags::NONE);
    assert_eq!(g.motion.position[0], mean);
    let g = run(GroupFlags::PLACE_AGAIN, MotionFlags::MOVED);
    assert_eq!(g.flags, GroupFlags::PLACED);
    assert_eq!(g.motion.position[1], BVector::new(-1.0, 0.0, 0.0));
}

/// The awake update of a group in a formation of three slots (`BUnitGroup::update` after
/// `BMotion`'s update): its members walk to their slots (`FUN_141041810` as laid out: 10 to
/// (3, 4), 11 to (4, 4), 12 to (5, 4)) and its due order goes. With [`MotionFlags::NEW`] the
/// formation turns: the group stands at its members' mean place facing their forward (-x),
/// the slots turn with it about that place and take the nearest units anew.
#[test]
fn an_awake_group_s_members_walk_to_their_slots_and_its_due_orders_go() {
    let run = |flags, motion_flags| {
        let mut t = grounded();
        let slot = |x: f32| crate::groups::Slot {
            unit_id: ObjectId::of_field(9 + x as i32),
            offset: BVector::new(x - 2.0, 0.0, 0.0),
            transform_position: BVector::new(x + 2.0, 0.0, 4.0),
            desired_category: FormationCategory::named(1),
            desired_sub_category: UnitType::NONE,
            active: 1,
            transformed: 1,
            ..crate::groups::Slot::default()
        };
        let g = t.groups.get_mut(group_id(1)).unwrap();
        g.current_formation = Some(crate::groups::Formation {
            kind: FormationKind::LINE,
            transform_type: TransformType::TURNED,
            transform_orientation: BVector::new(0.0, 0.0, 1.0),
            slots: vec![slot(1.0), slot(2.0), slot(3.0)],
            ..crate::groups::Formation::new()
        });
        (g.flags, g.motion.flags) = (flags, motion_flags);
        g.order_queue.push(crate::groups::GroupOrder::default());
        t.build_mesh().unwrap();
        assert_eq!(t.group_update(group_id(1)), Ok(true));
        assert_eq!(t.groups.get(group_id(1)).unwrap().order_queue.len(), 0);
        let f = t
            .groups
            .get(group_id(1))
            .unwrap()
            .current_formation
            .clone()
            .unwrap();
        let walks = [10, 11, 12].map(|id| {
            let m = t.parts.mover_of(object_id(id)).cloned().unwrap();
            m.paths[0].points.clone()
        });
        (f, walks)
    };
    let (f, walks) = run(GroupFlags::NONE, MotionFlags::MOVED);
    let slots: Vec<_> = f
        .slots
        .iter()
        .map(|s| (s.unit_id, s.transform_position))
        .collect();
    assert_eq!(
        slots,
        [
            (Some(object_id(10)), BVector::new(3.0, 0.0, 4.0)),
            (Some(object_id(11)), BVector::new(4.0, 0.0, 4.0)),
            (Some(object_id(12)), BVector::new(5.0, 0.0, 4.0))
        ]
    );
    assert_eq!(
        walks,
        [
            vec![BVector::new(1.0, 0.0, 1.0), BVector::new(3.0, 0.0, 4.0)],
            vec![BVector::new(2.0, 0.0, 1.0), BVector::new(4.0, 0.0, 4.0)],
            vec![BVector::new(1.0, 0.0, 2.0), BVector::new(5.0, 0.0, 4.0)]
        ]
    );
    let (f, walks) = run(GroupFlags::TURN, MotionFlags::MOVED | MotionFlags::NEW);
    let ids: Vec<_> = f.slots.iter().map(|s| s.unit_id).collect();
    assert_eq!(
        ids,
        [
            Some(object_id(11)),
            Some(object_id(10)),
            Some(object_id(12))
        ]
    );
    let x: Vec<_> = f.slots.iter().map(|s| s.transform_position.x()).collect();
    assert!(
        (x[0] - 7.0 / 3.0).abs() < 1e-6
            && (x[1] - 4.0 / 3.0).abs() < 1e-6
            && (x[2] - 1.0 / 3.0).abs() < 1e-6,
        "{x:?}"
    );
    // 11 walks from (2, 1) to the first slot
    let [from, to] = [walks[1][0], walks[1][1]];
    assert_eq!(from, BVector::new(2.0, 0.0, 1.0));
    assert!(
        (to.x() - 7.0 / 3.0).abs() < 1e-6 && (to.y() - 4.0 / 3.0).abs() < 1e-6,
        "{to:?}"
    );
}

/// Group 1 attacks object 20 (command 9, an object target first seen at its place) with its
/// three members in category 1.
fn attacking(t: &mut World) {
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.current_command = GroupCommandKind::ATTACK;
    g.current_target = crate::groups::TargetData {
        target: GroupTarget::Object(object_id(20)),
        original_position: BVector::new(3.5, 0.0, 3.5),
        ..crate::groups::TargetData::default()
    };
    g.unit_types = vec![crate::groups::UnitTypeStatistic {
        formation_category: FormationCategory::named(1),
        units: vec![object_id(10), object_id(11), object_id(12)],
        ..crate::groups::UnitTypeStatistic::default()
    }];
    g.flags = GroupFlags::NONE;
    g.motion.maximum_velocity_forward = Speed::new(1.0);
}

/// Every object of every player in the spatial index at its place (`FUN_14118f8c0`-style gaps and
/// the work command's reach read it).
fn register(t: &mut World) {
    let placed: Vec<(i32, crate::map::spatial::Placed)> = PlayerId::each(t.players.len())
        .zip(&t.players)
        .flat_map(|(p, player)| player.objects.iter().map(move |u| (p, u)))
        .filter_map(|(p, u)| {
            let m = t.masters.of(p)?.get(u.life.master_id)?;
            Some((
                u.id.get(),
                crate::map::spatial::Placed {
                    position: u.position(),
                    radius: m.radius,
                    class: m.class,
                    player: p,
                    master: m.id,
                    obstruction_type: ObstructionType::NONE,
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

/// `FUN_14103f4e0` with the target out of sight (not pending): queued commands finish it; else
/// the group stops, forgets its command and its target (range too), and finishes when its nearest
/// member (11, a footprint gap of about 2.37) is within the range, which rises to that member's
/// line of sight (4); a target farther away sends it back to where it was first seen, asking
/// for a relayout.
#[test]
fn an_attack_whose_target_is_gone_from_sight_ends_or_goes_back() {
    let mut t = world();
    attacking(&mut t);
    t.groups.get_mut(group_id(1)).unwrap().command_queue =
        vec![crate::groups::GroupCommand::default()];
    let mut relayout = false;
    assert!(t.attack_update(group_id(1), &mut relayout).unwrap());
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().current_command,
        GroupCommandKind::ATTACK
    );
    t.groups.get_mut(group_id(1)).unwrap().command_queue.clear();
    assert!(t.attack_update(group_id(1), &mut relayout).unwrap());
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(
        (g.current_command, g.current_target),
        (
            GroupCommandKind::NONE,
            crate::groups::TargetData {
                target: GroupTarget::None,
                ..crate::groups::TargetData::default()
            }
        )
    );
    // the update then deletes the group: no command queued and the target gone
    attacking(&mut t);
    assert!(!t.command_update(group_id(1), &mut relayout).unwrap());
    assert!(!relayout);
    // a far target: back to the first place
    attacking(&mut t);
    t.players[2].objects[ListIndex::new(0)].place =
        crate::objects::layers::Static::at(Position::new(7.5, 7.5, 0.0));
    assert!(!t.attack_update(group_id(1), &mut relayout).unwrap());
    assert!(relayout);
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().current_target.target,
        GroupTarget::None
    );
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().current_command,
        GroupCommandKind::MOVE
    );
}

/// A first member the world does not hold ends the update without finishing it.
#[test]
fn an_attack_without_a_first_member_does_nothing() {
    let mut t = world();
    attacking(&mut t);
    t.groups.get_mut(group_id(1)).unwrap().members[0].id = object_id(99);
    let mut relayout = false;
    assert!(!t.attack_update(group_id(1), &mut relayout).unwrap());
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().current_command,
        GroupCommandKind::ATTACK
    );
}

/// The categories of a pending attack: a category none of whose units is within its range (the
/// footprint gap less 0.25 for no range) leaves the command unfinished and the group arrived (flags
/// bit 3) when its motion stands; once arrived the category is released at once: the group stops
/// and each unit is sent (its own move clears). A group arrived and on its way (bit 4) sends all
/// its members at the end of the update, clears bit 4 and is finished.
#[test]
fn an_attack_releases_a_category_in_reach_or_arrived() {
    let mut t = world();
    attacking(&mut t);
    register(&mut t);
    light(&mut t, 1, 3, 3, true);
    assert!(t.target_pending(group_id(1)).unwrap());
    let reasons = |t: &World, reason: i32| {
        t.groups
            .get(group_id(1))
            .unwrap()
            .members
            .iter()
            .all(|m| m.specific_move.reason == MoveReason::of_data(reason))
    };
    for m in &mut t.groups.get_mut(group_id(1)).unwrap().members {
        m.specific_move.reason = MoveReason::of_data(5);
    }
    let mut relayout = false;
    // nothing in reach: not finished, arrived
    assert!(!t.attack_update(group_id(1), &mut relayout).unwrap());
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().flags,
        GroupFlags::ARRIVED
    );
    assert!(reasons(&t, 5));
    // arrived: the category is released
    assert!(t.attack_update(group_id(1), &mut relayout).unwrap());
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().flags,
        GroupFlags::ARRIVED
    );
    assert!(reasons(&t, -1));
    // arrived and on its way: all are sent at the end
    for m in &mut t.groups.get_mut(group_id(1)).unwrap().members {
        m.specific_move.reason = MoveReason::of_data(5);
    }
    t.groups.get_mut(group_id(1)).unwrap().unit_types.clear();
    t.groups.get_mut(group_id(1)).unwrap().flags = GroupFlags::ATTACKING;
    assert!(t.attack_update(group_id(1), &mut relayout).unwrap());
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().flags,
        GroupFlags::ARRIVED
    );
    assert!(reasons(&t, -1));
}

/// A unit in contact (a footprint gap of 0.14 less 0.25 within no range) releases the category
/// without the group having arrived.
#[test]
fn a_unit_in_contact_releases_its_category() {
    let mut t = world();
    attacking(&mut t);
    t.players[1].objects[ListIndex::new(0)].place =
        crate::objects::layers::Static::at(Position::new(3.0, 3.0, 0.0));
    register(&mut t);
    light(&mut t, 1, 3, 3, true);
    for m in &mut t.groups.get_mut(group_id(1)).unwrap().members {
        m.specific_move.reason = MoveReason::of_data(5);
    }
    let mut relayout = false;
    assert!(t.attack_update(group_id(1), &mut relayout).unwrap());
    assert!(
        t.groups
            .get(group_id(1))
            .unwrap()
            .members
            .iter()
            .all(|m| m.specific_move.reason == MoveReason::NONE)
    );
}

/// A target that moved more than 1 from where it stood and is in sight is attacked anew; the
/// group's move to it begins, which ends the update (not finished).
#[test]
fn an_attack_follows_a_target_that_moved() {
    let mut t = world();
    attacking(&mut t);
    register(&mut t);
    light(&mut t, 1, 3, 3, true);
    t.groups
        .get_mut(group_id(1))
        .unwrap()
        .current_target
        .original_position = BVector::new(3.5, 0.0, 1.5);
    let mut relayout = false;
    assert!(!t.attack_update(group_id(1), &mut relayout).unwrap());
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(g.current_target.target, GroupTarget::Object(object_id(20)));
    assert_eq!(
        (
            g.current_command,
            g.flags & (GroupFlags::ARRIVED | GroupFlags::ATTACKING)
        ),
        (GroupCommandKind::ATTACK, GroupFlags::ATTACKING)
    );
}

/// Owner 2's masters: master 4 and a gate (master 5: a building of class 39) and the target 20
/// is the gate, with 21 its annex at (2.5, 2.5).
fn gate_world() -> World {
    let mut t = world();
    let m4 = t.masters[2].get(master_id(4)).unwrap().clone();
    let gate = Master {
        id: master_id(5),
        kind: master_type::BUILDING,
        class: class::GATE,
        radius: Footprint::new(0.5, 0.5),
        standing_sprite: None,
        ..Master::default()
    };
    t.masters[2] = Masters::new([m4, gate]);
    t.players[2].objects[ListIndex::new(0)].life.master_id = master_id(5);
    let mut annex = soldier(21, 2.5, 2.5);
    annex.life.master_id = master_id(5);
    let _ = t
        .push_live(PlayerId::new(2), Object::with_ai(annex, soldier_ai()))
        .unwrap();
    t.parts.building_mut(object_id(21)).unwrap().annex_of =
        Some(MainBuildingId::new(object_id(20)));
    t.link_test_annex(object_id(21), object_id(20));
    t
}

/// `FUN_14100dbe0`: a gate out of the unit's reach (slot 0x1e8) is struck at its nearest part by
/// the footprint gap squared, the gate first; a gate in reach, and any other target, stands for
/// itself.
#[test]
fn a_gate_out_of_reach_is_struck_at_its_nearest_part() {
    let mut t = gate_world();
    // 11 at (2, 1): the annex is nearer than the gate
    assert_eq!(
        t.gate_part(object_id(11), object_id(20)).unwrap(),
        object_id(21)
    );
    t.parts.building_mut(object_id(20)).unwrap().annexes.clear();
    assert_eq!(
        t.gate_part(object_id(11), object_id(20)).unwrap(),
        object_id(20)
    );
    // within its weapon range and above the least one: the gate itself
    t.link_test_annex(object_id(21), object_id(20));
    let mut m4 = t.masters[1].get(master_id(4)).unwrap().clone();
    m4.weapon_range = Distance::new(10.0);
    t.masters[1] = Masters::new([m4.clone()]);
    assert_eq!(
        t.gate_part(object_id(11), object_id(20)).unwrap(),
        object_id(20)
    );
    // below the least range (a squared 3.9 against 5 squared) it is out of reach again
    m4.min_range = Distance::new(5.0);
    t.masters[1] = Masters::new([m4]);
    assert_eq!(
        t.gate_part(object_id(11), object_id(20)).unwrap(),
        object_id(21)
    );
    // a target that is no gate
    t.players[2].objects[ListIndex::new(0)].life.master_id = master_id(4);
    assert_eq!(
        t.gate_part(object_id(11), object_id(20)).unwrap(),
        object_id(20)
    );
}

/// Slot 0x208 (`FUN_141202c70`) of a unit: its mover's speed (0 without a mover); a charger
/// (type 70 with `max_charge`) of special attack 3 is the charge's, which is not modelled.
#[test]
fn a_unit_s_speed_is_its_movers_unless_a_charge_runs() {
    let mut t = world();
    t.parts.put_mover(
        t.players[1].objects[ListIndex::new(0)].id,
        Some(crate::gathering::hunt::tests::walker()),
    );
    let speed = |t: &World, id| {
        let (p, u) = t.object(object_id(id)).unwrap();
        t.unit_speed(p, u)
    };
    assert_eq!(speed(&t, 11).unwrap(), 0.0);
    assert_eq!(speed(&t, 10).unwrap(), 0.8);
    let mut m4 = t.masters[1].get(master_id(4)).unwrap().clone();
    m4.max_charge = Charge::new(1.0);
    t.masters[1] = Masters::new([m4.clone()]);
    assert_eq!(speed(&t, 10).unwrap(), 0.8);
    m4.special_attack = SpecialAttack::CHARGE;
    t.masters[1] = Masters::new([m4.clone()]);
    assert!(speed(&t, 10).is_err());
    m4.max_charge = Charge::ZERO;
    t.masters[1] = Masters::new([m4]);
    assert_eq!(speed(&t, 10).unwrap(), 0.8);
}

/// `FUN_14103d460`: a released unit works on the part of the target (its order queue empties
/// with the group's retargeting byte set), its own move clears, and a unit on the special list
/// with a speed draws its retarget timer from the time to the target at that speed: the gap
/// (about 2.37) over 0.8 is 2 whole seconds, 4000 ms, so the timer is a draw below 2000 plus
/// 3000.
#[test]
fn a_released_special_unit_draws_its_retarget_timer() {
    let mut t = world();
    attacking(&mut t);
    register(&mut t);
    light(&mut t, 1, 3, 3, true);
    t.parts.put_mover(
        t.players[1].objects[ListIndex::new(1)].id,
        Some(crate::gathering::hunt::tests::walker()),
    );
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.special_units = vec![object_id(11)];
    g.current_target.disable_retargeting = 1;
    for u in &mut t.players[1].objects {
        t.parts.unit_ai.get_mut(u.id).unwrap().orders =
            vec![crate::unit_ai::QueuedOrder::default()];
        t.parts.unit_ai.get_mut(u.id).unwrap().retarget_timer = Millis::new(7);
    }
    let mut reference = t.streams.clone();
    let draw = reference.get(crate::math::rng::Stream::Ai).rand15();
    t.release_units(group_id(1), &[object_id(10), object_id(11)], object_id(20))
        .unwrap();
    let timers: Vec<u32> = t.players[1]
        .objects
        .iter()
        .map(|u| t.parts.unit_ai.get(u.id).unwrap().retarget_timer.get())
        .collect();
    assert_eq!(timers, [7, 3000 + draw % 2000, 7]);
    assert!(
        t.players[1].objects.iter().take(2).all(|u| t
            .parts
            .unit_ai
            .get(u.id)
            .unwrap()
            .orders
            .is_empty())
    );
    assert_eq!(
        t.parts
            .unit_ai
            .get(t.players[1].objects[ListIndex::new(2)].id)
            .unwrap()
            .orders
            .len(),
        1
    );
    // the target the world does not hold: nothing
    t.release_units(group_id(1), &[object_id(12)], object_id(77))
        .unwrap();
    assert_eq!(
        t.parts
            .unit_ai
            .get(t.players[1].objects[ListIndex::new(2)].id)
            .unwrap()
            .orders
            .len(),
        1
    );
}
