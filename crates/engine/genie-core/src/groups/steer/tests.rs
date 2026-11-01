use super::*;
use crate::groups::GroupFlags;
use crate::groups::MovePath;
use crate::groups::MoveReason;
use crate::groups::SlotState;
use crate::groups::{Formation, Member, Slot, UnitGroup, UnitGroups};
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::Radii;
use crate::map::RestrictionId;
use crate::map::TerrainMask;
use crate::map::spatial::{Obstruction, Placed, Spatial, Tiles};
use crate::math::fmath::BVector;
use crate::movement::PathState;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::master_type;
use crate::objects::masters::{Master, Masters};
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::test_ids::group_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::unit_ai::ActionKind;
use crate::unit_ai::UnitAi;
use crate::world::Millis;
use crate::world::Player;
use crate::world::Seconds;
use crate::world::WorldTime;

/// The walking and the running sprite's top speeds of the hunt tests' walker (speed 0.8): its
/// factors 1.125 and 1.875.
const WALKING: f32 = 0.8 * 1.125;
const RUNNING: f32 = 0.8 * 1.875;

/// The graphic `id` with the speed factor `factor`.
const fn graphic(id: i16, factor: f32) -> Graphic {
    Graphic {
        id: SpriteId::named(id),
        frames: 1,
        facets: 8,
        speed_multiplier: factor,
        frame_duration: Seconds::ZERO,
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    }
}

/// The update's seconds (69 ms).
const DT: f32 = 0.069;

/// A unit of master 4 at (`x`, `y`), where it last moved to, with a free unit AI and the
/// hunt tests' walker (top speed 0.8, radius 0.2, turn rate the largest).
fn member(id: i32, x: f32, y: f32) -> Unit {
    let mut mover = crate::gathering::hunt::tests::walker();
    mover.moved_at = Position::new(x, y, 0.0);
    let mut u = Unit {
        id: object_id(id),
        ai: Some(UnitAi {
            current_order: None,
            current_action: None,
            current_target: None,
            ..UnitAi::default()
        }),
        body: crate::objects::layers::Body::new(Position::new(x, y, 0.0), None, Some(mover)),
        ..Unit::default()
    };
    u.life.master_id = master_id(4);
    u.life.state = ObjectState::Alive;
    u.life.hp = HitPoints::new(50.0);
    u.group.id = Some(group_id(1));
    u
}

/// A slot of `unit` at (`x`, `y`) on the map, made at the world's time 100.
fn slot(unit: i32, x: f32, y: f32) -> Slot {
    Slot {
        unit_id: ObjectId::of_field(unit),
        transform_position: BVector::new(x, 0.0, y),
        time: WorldTime::from_millis(100),
        ..Slot::default()
    }
}

/// Player 1's units 10 at (5, 5) and 11 at (6, 5) in group 1 (its top speed 0.9), whose
/// formation's slots are at (10, 5) and (6, 9); master 4 a combat unit of class 6 with
/// standing sprite 6 (no factor), walking sprite 7 (factor 1.125) and running sprite 8 (factor
/// 1.875), which the slice's graphics hold (not the units' sprites); a 32 by 32
/// map of the terrain class bit 2 (in the walker's mask) at the world's time 100 and an update of 69 ms.
fn world() -> World {
    let players = vec![
        Player::default(),
        Player {
            objects: vec![member(10, 5.0, 5.0), member(11, 6.0, 5.0)].into(),
            ..Player::default()
        },
    ];
    let mut t = World::new(
        WorldTime::ZERO,
        0,
        PlayerId::new(0),
        vec![Millis::ZERO; 2],
        players,
    );
    t.world_time = WorldTime::from_millis(100);
    t.dt_seconds = Seconds::new(DT);
    let master = Master {
        id: master_id(4),
        kind: master_type::COMBAT,
        class: class::CLASS_6,
        hit_points: 60,
        radius: Footprint::new(0.2, 0.2),
        standing_sprite: Some(SpriteId::named(6)),
        move_sprite: Some(SpriteId::named(7)),
        run_sprite: Some(SpriteId::named(8)),
        formation_category: 1,
        ..Master::default()
    };
    t.masters = vec![Masters::new([master]); 2];
    for (id, factor) in [(6, 0.0), (7, 1.125), (8, 1.875)] {
        let _ = t.graphics.insert(SpriteId::named(id), graphic(id, factor));
    }
    t.spatial = Spatial::with_tiles(
        GridSize::new(32, 32),
        Tiles::new(GridSize::new(32, 32), vec![2; 32 * 32]),
    );
    let mut g = UnitGroup::new(group_id(1));
    g.owner = Some(PlayerId::new(1));
    g.flags = GroupFlags::NONE;
    g.motion.maximum_velocity_forward = Speed::new(0.9);
    g.members = [10, 11]
        .map(|id| Member {
            id: object_id(id),
            ..Member::default()
        })
        .to_vec();
    g.current_formation = Some(Formation {
        slots: vec![slot(10, 10.0, 5.0), slot(11, 6.0, 9.0)],
        ..Formation::default()
    });
    t.groups = UnitGroups {
        groups: vec![g],
        ..UnitGroups::default()
    };
    t
}

/// The members' master and movement with radius `r` on the ground.
fn widen(t: &mut World, r: f32) {
    for m in &mut t.masters {
        let mut all: Vec<Master> = m.iter().cloned().collect();
        for a in &mut all {
            a.radius = Footprint::new(r, r);
        }
        *m = Masters::new(all);
    }
    for u in &mut t.players[1].objects {
        if let Some(m) = u.body.mover_mut() {
            m.stats.radius = Radii::new(r, r, m.stats.radius.z());
        }
    }
}

/// An obstruction of player 2 at (`x`, `y`).
fn post(x: f32, y: f32) -> Placed {
    Placed {
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
    }
}

fn unit(t: &World, id: i32) -> &Unit {
    t.object(object_id(id)).unwrap().1
}

fn mover(t: &World, id: i32) -> &crate::movement::Mover {
    unit(t, id).body.mover().unwrap()
}

fn state(t: &World, slot: usize) -> SlotState {
    t.groups
        .get(group_id(1))
        .unwrap()
        .current_formation
        .as_ref()
        .unwrap()
        .slots[slot]
        .state
}

/// `FUN_141039f60`: a reason other than -1 makes the member's move active with its goal,
/// timer, distance and the place it last moved to, its failures left; -1 ends it; a unit
/// that is no member, or that the world does not hold, is refused.
#[test]
fn a_member_s_own_move_is_set_or_ended() {
    let mut t = world();
    t.groups.get_mut(group_id(1)).unwrap().members[0]
        .specific_move
        .consecutive_move_failures = 4;
    assert!(
        t.set_specific_move(
            group_id(1),
            object_id(10),
            (MovePath::of_data(7), MoveReason::DETOUR),
            (BVector::new(1.0, 2.0, 3.0), 3.0, 2.5)
        )
        .unwrap()
    );
    let own = t.groups.get(group_id(1)).unwrap().members[0].specific_move;
    assert_eq!(
        own,
        SpecificMove {
            active: 1,
            path_id: MovePath::of_data(7),
            reason: MoveReason::DETOUR,
            goal: BVector::new(1.0, 2.0, 3.0),
            timer: Seconds::new(3.0),
            consecutive_move_failures: 4,
            distance: Distance::new(2.5),
            last_position: BVector::new(5.0, 0.0, 5.0),
        }
    );
    assert!(
        t.set_specific_move(
            group_id(1),
            object_id(10),
            (MovePath::of_data(7), MoveReason::NONE),
            (BVector::from([1.0; 3]), 1.0, 1.0)
        )
        .unwrap()
    );
    let own = t.groups.get(group_id(1)).unwrap().members[0].specific_move;
    assert_eq!(
        own,
        SpecificMove {
            path_id: MovePath::NONE,
            reason: MoveReason::NONE,
            ..SpecificMove::default()
        }
    );
    assert!(
        !t.set_specific_move(
            group_id(1),
            object_id(12),
            (MovePath::of_data(0), MoveReason::DETOUR),
            (BVector::ZERO, 0.0, 0.0)
        )
        .unwrap()
    );
    t.groups.get_mut(group_id(1)).unwrap().members.push(Member {
        id: object_id(99),
        ..Member::default()
    });
    assert!(
        !t.set_specific_move(
            group_id(1),
            object_id(99),
            (MovePath::of_data(0), MoveReason::DETOUR),
            (BVector::ZERO, 0.0, 0.0)
        )
        .unwrap()
    );
}

/// A member on its slot (1e-7 or nearer, not a NaN) in state 3 clears its failures and
/// takes the slot as its unit AI's goal; one more than 10 failures in is moved aside: its
/// own move (reason 2, its place, 0.1 s), a stop and [`GroupFlags::CATCHING_UP`].
#[test]
fn a_member_in_place_or_failing_too_often() {
    let mut t = world();
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.current_formation.as_mut().unwrap().slots[0].transform_position = BVector::new(5.0, 0.0, 5.0);
    g.current_formation.as_mut().unwrap().slots[0].state = SlotState::InPlace;
    g.members[0].specific_move.consecutive_move_failures = 11;
    t.steer_to_slot(group_id(1), 0, 0, &[]).unwrap();
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(g.members[0].specific_move.consecutive_move_failures, 0);
    assert_eq!(
        unit(&t, 10).ai.as_ref().unwrap().state_position,
        Point::new(5.0, 5.0)
    );
    let mut t = world();
    t.groups.get_mut(group_id(1)).unwrap().members[0]
        .specific_move
        .consecutive_move_failures = 11;
    t.steer_to_slot(group_id(1), 0, 0, &[]).unwrap();
    let g = t.groups.get(group_id(1)).unwrap();
    let own = g.members[0].specific_move;
    assert_eq!(
        (own.active, own.path_id, own.reason),
        (1, MovePath::NONE, MoveReason::ASIDE)
    );
    assert_eq!(
        (own.goal, own.timer, own.distance),
        (
            BVector::new(5.0, 0.0, 5.0),
            Seconds::new(0.1),
            Distance::ZERO
        )
    );
    assert_eq!(own.consecutive_move_failures, 11);
    assert_eq!(g.flags & GroupFlags::CATCHING_UP, GroupFlags::CATCHING_UP);
    assert_eq!(mover(&t, 10).animated.speed, 0.0);
    assert_eq!(
        unit(&t, 10).ai.as_ref().unwrap().state_position,
        Point::ORIGIN
    );
    // a NaN gap is not in place
    let mut t = world();
    let m = t.players[1].objects[ListIndex::new(0)]
        .body
        .mover_mut()
        .unwrap();
    m.moved_at = m.moved_at.with_x(f32::NAN);
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.current_formation.as_mut().unwrap().slots[0].state = SlotState::InPlace;
    g.members[0].specific_move.consecutive_move_failures = 11;
    t.steer_to_slot(group_id(1), 0, 0, &[]).unwrap();
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().members[0]
            .specific_move
            .reason,
        MoveReason::ASIDE
    );
}

/// A member that is not free (its unit AI acts) stops while it moves without a `MoveTo`,
/// clears its failures, and its slot in state 1 or 2 takes state 3; a slot in another
/// state keeps it.
#[test]
fn a_busy_member_stops_and_its_slot_counts_it_there() {
    for (before, after) in [
        (SlotState::Arriving, SlotState::InPlace),
        (SlotState::Stopping, SlotState::InPlace),
        (SlotState::Steering, SlotState::Steering),
        (SlotState::InPlace, SlotState::InPlace),
    ] {
        let mut t = world();
        t.players[1].objects[ListIndex::new(0)]
            .ai
            .as_mut()
            .unwrap()
            .current_action = Some(ActionKind::Attack);
        let g = t.groups.get_mut(group_id(1)).unwrap();
        g.current_formation.as_mut().unwrap().slots[0].state = before;
        g.members[0].specific_move.consecutive_move_failures = 3;
        t.steer_to_slot(group_id(1), 0, 0, &[]).unwrap();
        assert_eq!(state(&t, 0), after);
        assert_eq!(
            t.groups.get(group_id(1)).unwrap().members[0]
                .specific_move
                .consecutive_move_failures,
            0
        );
        assert_eq!(mover(&t, 10).animated.speed, 0.0);
    }
}

/// A free member five tiles from its slot steers to it: the yaw turns it round, its speed
/// is its top speed and it shows its walking sprite (whose speed it takes), the group's
/// flags take bit 9 and its unit AI's goal is the slot.
#[test]
fn a_far_member_turns_and_walks_to_its_slot() {
    let mut t = world();
    t.steer_to_slot(group_id(1), 0, 0, &[]).unwrap();
    let m = mover(&t, 10);
    assert!(0.99 < m.forward.x());
    assert_eq!(m.velocity.unwrap().velocity.y(), 0.8);
    assert_eq!(m.stats.speed, Speed::new(WALKING));
    assert_eq!(
        t.groups.get(group_id(1)).unwrap().flags & GroupFlags::CATCHING_UP,
        GroupFlags::CATCHING_UP
    );
    assert_eq!(state(&t, 0), SlotState::Steering);
    assert_eq!(
        unit(&t, 10).ai.as_ref().unwrap().state_position,
        Point::new(10.0, 5.0)
    );
}

/// The speed by the gap to the slot: 4 tiles, 1.2 times the top speed, running; 1.5 tiles,
/// the top speed plus 0.2 of it times 1.5 / 3; within five radii (0.5), the top speed,
/// walking; within the radius (0.1), the group's top speed, walking.
#[test]
fn a_near_member_catches_up_or_takes_the_group_s_speed() {
    let speed = 0.8f32;
    let cases = [
        (4.0, speed * CATCH_UP, RUNNING, 0x200),
        (
            1.5,
            speed + ((speed * CATCH_UP - speed) * 1.5) / CATCH_UP_SPAN,
            RUNNING,
            0x200,
        ),
        (0.5, speed, WALKING, 0),
        (0.1, 0.9, WALKING, 0),
    ];
    for (gap, v, shown, flags) in cases {
        let mut t = world();
        let f = t
            .groups
            .get_mut(group_id(1))
            .unwrap()
            .current_formation
            .as_mut()
            .unwrap();
        f.slots[0].transform_position = BVector::new(5.0 + gap, 0.0, 5.0);
        t.steer_to_slot(group_id(1), 0, 0, &[]).unwrap();
        let m = mover(&t, 10);
        assert_eq!(m.velocity.unwrap().velocity.y(), v, "{gap}");
        assert_eq!(m.stats.speed, Speed::new(shown), "{gap}");
        assert_eq!(
            t.groups.get(group_id(1)).unwrap().flags & GroupFlags::CATCHING_UP,
            GroupFlags::from_bits_retain(flags),
            "{gap}"
        );
    }
}

/// A member whose step would pass its slot (state 2) stops and, its group neither
/// patrolling nor walking a path, shows its standing sprite.
#[test]
fn a_member_about_to_pass_its_slot_stops() {
    let mut t = world();
    let f = t
        .groups
        .get_mut(group_id(1))
        .unwrap()
        .current_formation
        .as_mut()
        .unwrap();
    f.slots[0].transform_position = BVector::new(5.05, 0.0, 5.0);
    t.steer_to_slot(group_id(1), 0, 0, &[]).unwrap();
    assert_eq!(state(&t, 0), SlotState::Stopping);
    let m = mover(&t, 10);
    assert_eq!(m.animated.speed, 0.0);
    assert_eq!(m.velocity.unwrap().velocity.y(), 0.0);
    // the standing sprite has no factor: the top speed
    assert_eq!(m.stats.speed, Speed::new(0.8));
}

/// A slot whose time is past: state 1 becomes 2 and the member stops; in state 2 a moving
/// group's member only shows standing and the slot takes state 3; a still group's member
/// with a way there (the long pather, the unit without a height) stands on it facing the
/// group's forward. A slot in state 3 shows standing.
#[test]
fn a_due_slot_takes_its_member() {
    let mut t = world();
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.motion.velocity = BVector::new(0.0, 0.0, 1.0);
    let s = &mut g.current_formation.as_mut().unwrap().slots[0];
    s.time = WorldTime::from_millis(50);
    s.state = SlotState::Arriving;
    t.steer_to_slot(group_id(1), 0, 0, &[]).unwrap();
    assert_eq!(state(&t, 0), SlotState::InPlace);
    assert_eq!(mover(&t, 10).animated.speed, 0.0);
    assert_eq!(unit(&t, 10).position(), Position::new(5.0, 5.0, 0.0));
    let mut t = world();
    let m = t.players[1].objects[ListIndex::new(0)]
        .body
        .mover_mut()
        .unwrap();
    m.stats.radius = m.stats.radius.with_z(0.0);
    t.spatial.insert(
        object_id(10),
        Placed {
            obstruction: None,
            player: PlayerId::new(1),
            ..post(5.0, 5.0)
        },
    );
    let s = &mut t
        .groups
        .get_mut(group_id(1))
        .unwrap()
        .current_formation
        .as_mut()
        .unwrap()
        .slots[0];
    s.time = WorldTime::from_millis(50);
    s.state = SlotState::Stopping;
    t.steer_to_slot(group_id(1), 0, 0, &[]).unwrap();
    assert_eq!(state(&t, 0), SlotState::InPlace);
    assert_eq!(unit(&t, 10).position(), Position::new(10.0, 5.0, 0.0));
    let m = mover(&t, 10);
    assert_eq!(m.moved_at, Position::new(10.0, 5.0, 0.0));
    assert_eq!(m.last_move, WorldTime::from_millis(100));
    assert_eq!(m.forward, BVector::new(1.0, 0.0, 0.0));
    assert_eq!(m.right, BVector::new(0.0, 0.0, -1.0));
    assert_eq!(
        t.spatial.get(object_id(10)).unwrap().position,
        Position::new(10.0, 5.0, 0.0)
    );
    // in state 3 the member only shows standing, its goal left
    let mut t = world();
    let s = &mut t
        .groups
        .get_mut(group_id(1))
        .unwrap()
        .current_formation
        .as_mut()
        .unwrap()
        .slots[0];
    s.time = WorldTime::from_millis(50);
    s.state = SlotState::InPlace;
    t.steer_to_slot(group_id(1), 0, 0, &[]).unwrap();
    assert_eq!(mover(&t, 10).stats.speed, Speed::new(0.8));
    assert_eq!(
        unit(&t, 10).ai.as_ref().unwrap().state_position,
        Point::ORIGIN
    );
}

/// A record on the line to the slot is the way round (`0x14102683b`): member 10, wide
/// enough for the old short pather (radius 1.2, its half 0.6), finds a way round the post to
/// the slot (10, 5): its path data walks that path (level 0, state 3, from waypoint 1), its own
/// move takes it (reason 0, timer and distance 3), the slot takes state 0, the member stops
/// and its failures count one more.
#[test]
fn a_blocked_way_to_the_slot_goes_round() {
    let mut t = world();
    widen(&mut t, 1.2);
    t.spatial.insert(object_id(50), post(7.5, 5.0));
    t.steer_to_slot(group_id(1), 0, 0, &[object_id(11)])
        .unwrap();
    let m = mover(&t, 10);
    let walked = &m.paths[0];
    assert_eq!(
        (
            m.paths.len(),
            walked.level.get(),
            walked.state,
            walked.waypoint,
            walked.target
        ),
        (1, 0, PathState::Walking, 1, None)
    );
    assert_eq!(
        walked.points.last().map(|p| p.ground()),
        Some(Point::new(10.0, 5.0))
    );
    let own = t.groups.get(group_id(1)).unwrap().members[0].specific_move;
    assert_eq!(
        (
            own.active,
            own.reason,
            own.timer,
            own.distance,
            own.consecutive_move_failures
        ),
        (
            1,
            MoveReason::DETOUR,
            Seconds::new(3.0),
            Distance::new(3.0),
            1
        )
    );
    assert_eq!(state(&t, 0), SlotState::Steering);
    // the update's list ignored: clear
    let mut t = world();
    t.spatial.insert(object_id(50), post(7.5, 5.0));
    assert_eq!(t.steer_to_slot(group_id(1), 0, 0, &[object_id(50)]), Ok(()));
}

/// The way round to a slot a record stands on: the slot fails its position test, so the
/// member tries one tile toward it (6, 5), which the short pather reaches within 1.01; the
/// way serves, its goal that step.
#[test]
fn a_slot_a_record_holds_takes_a_step_toward_it() {
    let mut t = world();
    widen(&mut t, 1.2);
    t.spatial.insert(object_id(50), post(10.0, 5.0));
    t.steer_to_slot(group_id(1), 0, 0, &[object_id(11)])
        .unwrap();
    let m = mover(&t, 10);
    assert_eq!(
        m.paths[0].points.last().map(|p| p.ground()),
        Some(Point::new(6.0, 5.0))
    );
    let own = t.groups.get(group_id(1)).unwrap().members[0].specific_move;
    assert_eq!((own.active, own.goal), (1, BVector::new(6.0, 0.0, 5.0)));
}

/// The members' moves: a member whose own move is active with a clear line to its slot
/// stands and its move ends before it steers; one with a record in the way counts the
/// move's timer down and waits while the timer and the distance are above 0, then ends
/// it too; a member in no slot that moves stops.
#[test]
fn the_members_moves_end_wait_or_stop() {
    let mut t = world();
    t.spatial.insert(object_id(50), post(6.0, 7.0));
    // the members walk: a stop clears their animation speed
    for u in &mut t.players[1].objects {
        u.body.set_anim_speed(1.0);
    }
    let mut stray = member(12, 20.0, 20.0);
    stray.body.set_anim_speed(1.0);
    stray.body.mover_mut().unwrap().velocity = Some(crate::movement::Velocity {
        velocity: BVector::new(0.0, 0.0, 0.5),
        acceleration: BVector::ZERO,
    });
    t.players[1].objects.push(stray);
    let g = t.groups.get_mut(group_id(1)).unwrap();
    g.members.push(Member {
        id: object_id(12),
        ..Member::default()
    });
    for m in &mut g.members[..2] {
        m.specific_move = SpecificMove {
            active: 1,
            path_id: MovePath::of_data(3),
            reason: MoveReason::DETOUR,
            timer: Seconds::new(1.0),
            distance: Distance::new(2.0),
            ..SpecificMove::default()
        };
    }
    t.members_to_slots(group_id(1), &[]).unwrap();
    let g = t.groups.get(group_id(1)).unwrap();
    assert_eq!(
        g.members[0].specific_move,
        SpecificMove {
            path_id: MovePath::NONE,
            reason: MoveReason::NONE,
            ..SpecificMove::default()
        }
    );
    assert_eq!(
        unit(&t, 10).ai.as_ref().unwrap().state_position,
        Point::new(10.0, 5.0)
    );
    assert_eq!(g.members[1].specific_move.timer, Seconds::new(1.0 - DT));
    assert_eq!(g.members[1].specific_move.reason, MoveReason::DETOUR);
    assert_eq!(
        unit(&t, 11).ai.as_ref().unwrap().state_position,
        Point::ORIGIN
    );
    assert_eq!(mover(&t, 12).animated.speed, 0.0);
    assert_ne!(mover(&t, 11).animated.speed, 0.0);
    // the timer run out (or the distance at 0) ends the move, and the member steers on
    let mut t2 = world();
    t2.spatial.insert(object_id(50), post(6.0, 7.0));
    let g = t2.groups.get_mut(group_id(1)).unwrap();
    g.members[1].specific_move = SpecificMove {
        active: 1,
        timer: Seconds::new(1.0),
        distance: Distance::ZERO,
        ..SpecificMove::default()
    };
    assert_eq!(t2.members_to_slots(group_id(1), &[object_id(50)]), Ok(()));
    let own = t2.groups.get(group_id(1)).unwrap().members[1].specific_move;
    assert_eq!(
        (own.active, own.path_id, own.reason),
        (0, MovePath::NONE, MoveReason::NONE)
    );
    assert_eq!(mover(&t2, 11).animated.speed, 0.0);
    assert_eq!(
        unit(&t2, 11).ai.as_ref().unwrap().state_position,
        Point::new(6.0, 9.0)
    );
}

/// What the way round reads of a member at `at` (x, height, y) of radius 0.2 and height
/// radius 0.5.
fn rounder(at: BVector) -> detour::Rounder<'static> {
    detour::Rounder {
        id: object_id(10),
        walker: crate::movement::Walker {
            id: object_id(10),
            radius: Distance::new(0.1),
            restriction: RestrictionId::named(0),
            mask: TerrainMask::of_bits(2),
        },
        computer: false,
        at,
        here: at.position(),
        mask: TerrainMask::of_bits(2),
        restriction: RestrictionId::named(0),
        grow: Distance::new(0.1),
        radius: Footprint::new(0.2, 0.2),
        height: 0.5,
        solid: true,
        differentiators: &[],
    }
}

/// The way round's step: one tile toward the slot on the ground, the group's forward when the
/// member is on it, held inside the map by the member's radius on x and its height radius on
/// y; its footprint is near a point within 10 of it (the squared gap below 100).
#[test]
fn the_way_round_s_step_goes_one_tile_toward_the_slot_inside_the_map() {
    use detour::one_step_toward;
    let (map, forward) = (GridSize::new(32, 32), BVector::new(0.0, 0.0, 1.0));
    let step = |at, to| one_step_toward(&rounder(at), to, forward, map);
    assert_eq!(
        step(BVector::new(5.0, 0.0, 5.0), BVector::new(9.0, 0.0, 5.0)),
        BVector::new(6.0, 0.0, 5.0)
    );
    assert_eq!(
        step(BVector::new(5.0, 0.0, 5.0), BVector::new(5.0, 0.0, 5.0)),
        BVector::new(5.0, 0.0, 6.0)
    );
    assert_eq!(
        step(BVector::new(0.5, 0.0, 5.0), BVector::new(-9.0, 0.0, 5.0)),
        BVector::new(0.2, 0.0, 5.0)
    );
    assert_eq!(
        step(BVector::new(31.5, 0.0, 5.0), BVector::new(40.0, 0.0, 5.0)),
        BVector::new(31.8, 0.0, 5.0)
    );
    assert_eq!(
        step(BVector::new(5.0, 0.0, 0.8), BVector::new(5.0, 0.0, -9.0)),
        BVector::new(5.0, 0.0, 0.5)
    );
    assert_eq!(
        step(BVector::new(5.0, 0.0, 31.2), BVector::new(5.0, 0.0, 40.0)),
        BVector::new(5.0, 0.0, 31.5)
    );
    let r = rounder(BVector::new(5.0, 0.0, 5.0));
    assert!(r.near(15.1, 5.0));
    assert!(!r.near(15.3, 5.0));
}

/// `FUN_140f9bcd0` outside the path: a way that ends farther from the slot than the gap
/// (on the ground) falls short; an empty one ends at the origin.
#[test]
fn a_way_round_that_ends_farther_than_the_gap_falls_short() {
    use crate::pathing::tile_path::Path;
    use detour::ends_farther;
    let mut way = Path::default();
    let _ = way.push(BVector::new(9.0, 3.0, 5.0));
    assert!(ends_farther(
        &way,
        BVector::new(10.0, 0.0, 5.0),
        Distance::new(0.5)
    ));
    assert!(!ends_farther(
        &way,
        BVector::new(10.0, 0.0, 5.0),
        Distance::new(1.5)
    ));
    assert!(ends_farther(
        &Path::default(),
        BVector::new(3.0, 0.0, 4.0),
        Distance::new(4.9)
    ));
    assert!(!ends_farther(
        &Path::default(),
        BVector::new(3.0, 0.0, 4.0),
        Distance::new(5.0)
    ));
}

/// The way round's third try: records on the slot and on the step toward it, the member's way
/// goes to the group's place (5, 8) by the short pather. That place lies farther from the slot
/// than the member does: while the group stands the member stops there instead (no path, its
/// failures cleared, the slot in state 3); while it moves the member walks the way, its own
/// move's goal that place.
#[test]
fn a_member_with_its_slot_and_step_held_goes_round_to_the_group() {
    let run = |moving: bool| {
        let mut t = world();
        widen(&mut t, 1.2);
        t.spatial.insert(object_id(50), post(10.0, 5.0));
        t.spatial.insert(object_id(51), post(6.0, 5.0));
        let g = t.groups.get_mut(group_id(1)).unwrap();
        g.motion.position[0] = BVector::new(5.0, 0.0, 8.0);
        if moving {
            g.motion.velocity = BVector::new(0.0, 0.0, 0.9);
        }
        t.steer_to_slot(group_id(1), 0, 0, &[object_id(11)])
            .unwrap();
        t
    };
    let t = run(false);
    assert_eq!(mover(&t, 10).paths.len(), 0);
    assert_eq!(state(&t, 0), SlotState::InPlace);
    let own = t.groups.get(group_id(1)).unwrap().members[0].specific_move;
    assert_eq!((own.active, own.consecutive_move_failures), (0, 0));
    let t = run(true);
    let m = mover(&t, 10);
    assert_eq!(
        m.paths[0].points.last().map(|p| p.ground()),
        Some(Point::new(5.0, 8.0))
    );
    let own = t.groups.get(group_id(1)).unwrap().members[0].specific_move;
    assert_eq!((own.active, own.goal), (1, BVector::new(5.0, 0.0, 8.0)));
    assert_eq!(state(&t, 0), SlotState::Steering);
}

/// The way round keeps the tile planner's result in the short pather's integer and reads it by
/// its code: the goal as reached, the nearest node as partial, nothing as blocked and off the
/// map as in range.
#[test]
fn the_way_round_reads_the_planner_s_result_by_its_code() {
    use crate::pathing::ShortFound;
    use crate::pathing::tile_path::Found;
    for found in [Found::OffMap, Found::Goal, Found::Nearest, Found::Nothing] {
        let read = detour::planned_as_short(found);
        assert_eq!(read.code(), found.code());
    }
    assert_eq!(detour::planned_as_short(Found::OffMap), ShortFound::InRange);
}
