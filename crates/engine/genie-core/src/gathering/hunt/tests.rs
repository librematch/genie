use super::*;
use crate::combat::attack::AttackState;
use crate::gathering::DropSiteId;
use crate::gathering::gather::Walks;
use crate::map::Footprint;
use crate::map::Radii;
use crate::map::RestrictionId;
use crate::map::TerrainMask;
use crate::math::Angle;
use crate::math::fmath::BVector;
use crate::movement::MoveAlgorithm;
use crate::movement::Speed;
use crate::movement::TurnRate;
use crate::movement::move_to::MoveWaitState;
use crate::movement::move_to::{
    MoveTo, MoveToAction, MoveToReads, MoveWait, SpriteReads, TargetMoved, WalkTo,
};
use crate::objects::ActionState;
use crate::objects::Graphics;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::Voice;
use crate::objects::lifecycle::Held;
use crate::objects::masters::FlyMode;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::master_type;
use crate::objects::sprite::Graphic;
use crate::players::AmountRate;
use crate::players::PlayerType;
use crate::players::attribute;
use crate::test_ids::carrying;
use crate::test_ids::class_of;
use crate::test_ids::object_id;
use crate::unit_ai::UnitStance;
use crate::world::WorldTime;

use crate::combat::attack::tests::Rig;
use crate::combat::attack::tests::reads;
use crate::map::spatial::Spatial;
use crate::movement::{Me, MoveStats, Velocity};
use crate::movement::{SubstituteCount, WaitDelays};
use crate::objects::facing::SpriteFacets;
use crate::objects::layers::{Animated, Motion, Static};
use crate::objects::unit::Unit;
use crate::objects::{FacetCount, Frame};
use crate::unit_ai::{NotifyKind, UnitAi};
use crate::world::World;

/// The reads of a human player's hunt ([`HuntReads`]): a work rate and a productivity of 1, a
/// capacity of 10, its unit AI's target class −1, no sprites, the Attack's of
/// [`crate::combat::attack::tests::reads`].
pub(crate) fn hunt_reads() -> HuntReads {
    HuntReads {
        defaults: MoveDefaults::default(),
        attack: reads(),
        work_rate: WorkRate::new(1.0),
        capacity: Amount::new(10.0),
        productivity: Amount::new(1.0),
        stops_decay: false,
        has_ai: true,
        ai_class: None,
        tree_searched: false,
        predator_hunts_end: false,
        sprites: HuntSprites::default(),
        herd: None,
    }
}

/// The sprites' table of a world without sprites.
static NO_GRAPHICS: Graphics = Graphics::new();

/// What the `MoveTo` constructors read of a hunter that carries `held`: no sprites, a unit in
/// state 2.
pub(crate) fn walks(held: Held) -> Walks<'static> {
    Walks {
        reads: MoveToReads {
            sprites: SpriteReads {
                defaults: MoveDefaults::default(),
                graphics: &NO_GRAPHICS,
            },
            state: ObjectState::Alive,
            speed: Speed::new(1.0),
            at: Position::ORIGIN,
            ground: Footprint::ZERO,
        },
        held,
    }
}

/// `FUN_140aa0df0` takes a target in state 6 or below and holds a later one as none
/// (`FUN_140a7a230`): the hunt's `MoveTo` then has no target and walks to its place.
#[test]
fn hunt_move_to_refuses_target_past_state_6() {
    let mover = walker();
    let h = hunting(31);
    let walk = |state| {
        let to = WalkTo {
            id: Some(object_id(31)),
            at: Position::new(5.0, 6.0, 0.0),
            gap: Distance::ZERO,
            state: Some(state),
        };
        h.move_to_object(to, Distance::new(1.0), &mover, &walks(Held::default()))
            .unwrap()
            .map(|m| (ObjectId::field(m.target), m.point, m.state))
    };
    assert_eq!(
        walk(ObjectState::Undead),
        Some((31, Position::new(5.0, 6.0, 0.0), MoveTo::Arrived))
    );
    assert_eq!(
        walk(ObjectState::Removed),
        Some((-1, Position::new(5.0, 6.0, 0.0), MoveTo::Start))
    );
}

/// The quiet `MoveTo` in `state` to the house 10028 at (30, 88) within `range`, which asks again
/// when its target moves (`FUN_140aa0df0`, `+0x61` 1).
fn to_house(state: MoveTo, range: Distance) -> MoveToAction {
    MoveToAction {
        point: Position::new(30.0, 88.0, 0.0),
        target: Some(object_id(10028)),
        range,
        on_target_moved: TargetMoved::AsksAgain,
        ..MoveToAction::sub_in(state)
    }
}

/// The quiet `MoveTo` in `state` to a point, which asks again when it moves (`+0x61` 1).
fn to_point(state: MoveTo, point: Position, range: Distance) -> MoveToAction {
    MoveToAction {
        state,
        on_target_moved: TargetMoved::AsksAgain,
        ..MoveToAction::sub_to_point(point, range, None)
    }
}

/// The hunter 9177 of [`Rig::on_the_path`] in action state `state`.
const fn me(state: &HuntState) -> Me {
    Me {
        human_order: WorldTime::ZERO,
        id: object_id(9177),
        group: None,
        action_state: ActionState::new(state.get()),
    }
}

/// Slot 0x78 (`FUN_140a78060`): a load of the task's resource, or any for a task of none,
/// asks for the drop-off; another resource (wood for a food task: kinds by
/// [`resource_kind`], meat 15 counting as food) does not, the caller's unit AI taking notify
/// 0x21c.
#[test]
fn load_of_another_resource_asks_no_drop_off() {
    assert!(asked_drop_off(
        Some(attribute::FOOD),
        Some(attribute::MEAT),
        300
    ));
    assert!(asked_drop_off(None, Some(attribute::WOOD), 300));
    assert!(!asked_drop_off(
        Some(attribute::FOOD),
        Some(attribute::WOOD),
        300
    ));
    assert!(!asked_drop_off(
        Some(attribute::STONE),
        Some(attribute::GOLD),
        300
    ));
}

/// A Hunt gathering `target` with the chopping task.
pub(crate) fn hunting(target: i32) -> HuntAction {
    HuntAction {
        target: HuntTargetId::of_field(target),
        state: HuntState::Gather,
        point: Position::ORIGIN,
        target_class: None,
        task: CHOP,
        drop_site: None,
        carry_to: None,
        timer: Seconds::ZERO,
        lead: Lead::default(),
    }
}

/// `FUN_140ac44b0`: the decay task (`attribute_type4` 0x124) under attribute 292 turns a
/// decaying target's decay negative once; any other task or a player without it leaves it.
#[test]
fn decay_task_stops_its_target_decay_once() {
    let mut hunt = hunting(7);
    let decaying = || Victim {
        decay: AmountRate::new(0.25),
        ..Victim::default()
    };
    let mut v = decaying();
    assert!(!hunt.stops_decay_of(true, &mut v));
    assert_eq!(v.decay, AmountRate::new(0.25));
    hunt.task.kind4 = Some(DECAY_TASK);
    assert!(!hunt.stops_decay_of(false, &mut v));
    assert_eq!(v.decay, AmountRate::new(0.25));
    assert!(hunt.stops_decay_of(true, &mut v));
    assert_eq!(v.decay, AmountRate::new(-0.25));
    assert!(!hunt.stops_decay_of(true, &mut v));
    assert_eq!(v.decay, AmountRate::new(-0.25));
    let mut none = Victim::default();
    assert!(!hunt.stops_decay_of(true, &mut none));
    assert_eq!(none.decay.get().to_bits(), 0);
}

/// The decay check on the step ([`HuntAction::decay_check`]): a target whose decay it stops
/// goes back to the world at once; one it leaves does not.
#[test]
fn stopped_decay_goes_back_to_the_world_at_once() {
    let decaying = Victim {
        id: object_id(10028),
        decay: AmountRate::new(0.25),
        ..Victim::default()
    };
    let mut hunt = hunting(10028);
    hunt.task.kind4 = Some(DECAY_TASK);
    let mut rig = Rig::on_the_path(decaying.clone());
    rig.run(100, |c, u, actions, f, _| {
        hunt.decay_check(c, u, actions, true, f)
    })
    .unwrap();
    assert_eq!(rig.f.victim.decay, AmountRate::new(-0.25));
    let mut rig = Rig::on_the_path(decaying);
    rig.run(100, |c, u, actions, f, _| {
        hunt.decay_check(c, u, actions, false, f)
    })
    .unwrap();
    assert_eq!(rig.f.victim.decay, AmountRate::new(0.25));
}

/// [`walker`] in motion: its animation runs at speed 1 (`+0x7b8`), which a stop clears.
pub(crate) fn walking() -> Mover {
    let mut m = walker();
    m.animated.speed = Speed::new(1.0);
    m
}

/// A villager walking at 0.8 tiles a second, radius 0.2.
pub(crate) fn walker() -> Mover {
    Mover {
        forward: BVector::new(-1.0, 0.0, 0.0),
        right: BVector::new(0.0, 0.0, 1.0),
        velocity: Some(Velocity {
            velocity: BVector::new(0.0, 0.0, 0.8),
            acceleration: BVector::new(0.0, 0.0, 1.0),
        }),
        paths: vec![],
        factor: 1.0,
        stats: MoveStats {
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
            facets: Some(SpriteFacets {
                count: FacetCount::of_data(16),
                turns: true,
            }),
            weapon_range: Distance::ZERO,
        },
        waiting: 1,
        wait_delays: WaitDelays::of_data(0),
        attack_moving: false,
        last_move: WorldTime::ZERO,
        moved_at: Position::ORIGIN,
        substitute_count: SubstituteCount::of_data(0),
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

/// MarvinBlue/426048731's tree 9365 (Gaia, class 15, 100 wood) as 9181 meets it in update 748.
fn tree() -> Quarry {
    Quarry {
        id: object_id(9365),
        position: Position::new(24.5, 97.5, 0.0),
        class: class::TREE,
        kind: master_type::STATIC,
        state: ObjectState::Alive,
        player: PlayerId::new(0),
        amount: Amount::new(100.0),
        allied: false,
        top_action: None,
        drop_site: false,
    }
}

fn engagement() -> Engagement {
    Engagement {
        gap_squared: DistanceSquared::new(3.726 * 3.726),
        gap: Distance::new(3.726),
        position: Position::new(20.774, 97.5, 0.0),
        target_position: Position::new(24.5, 97.5, 0.0),
        speed: Speed::new(0.8),
        range: Distance::ZERO,
        min_range: Distance::ZERO,
        can_move: true,
        stance: UnitStance::AGGRESSIVE,
        state_position: Point::ORIGIN,
        world_time: WorldTime::ZERO,
        master: ids::VILLAGER_MALE,
        target_class: class::TREE,
        same_owner: false,
    }
}

const ARMS: Arms = Arms {
    move_sprite: None,
    sprite: None,
    second_sprite: None,
    idle_sprite: None,
    idle_attack_sprite: None,
    missile: None,
    fire_frame: Frame::of_data(0),
    interrupt_frame: Frame::of_data(-1),
};

const CHOP: HuntTask = HuntTask {
    id: TaskId::test(0),
    range: Distance::ZERO,
    resource: Some(attribute::MEAT),
    sprites: TaskSprites::of_data([-1, 1239, 1961, 1960]),
    work_value: WorkValue::new(1.0),
    productivity: None,
    kind4: None,
};

#[test]
fn villager_sent_to_tree_attacks_it_first_and_walks_there() {
    let mut mover = walker();
    let mut held = Held::default();
    let (hunt, _) = HuntAction::start(
        HuntStart {
            task: CHOP,
            target: &tree(),
            player: PlayerId::new(2),
            player_type: PlayerType::HUMAN,
            carry_rule: false,
            held: &mut held,
            walks: walks(Held::default()),
        },
        &engagement(),
        ARMS,
        &mut mover,
    )
    .unwrap();
    assert_eq!(hunt.state.bare(), HuntState::Attack { attack: None });
    let attack = hunt.state.attack().copied().unwrap();
    assert_eq!(
        (
            attack.state.bare(),
            attack.state.move_to().copied().map(|m| m.state)
        ),
        (AttackState::Move { move_to: None }, Some(MoveTo::Start))
    );
    // the first MoveTo's teardown stopped the unit
    let v = mover.velocity.unwrap();
    assert_eq!(
        (v.velocity.y(), v.acceleration.y(), mover.waiting),
        (0.0, 0.0, 0)
    );
    // nothing carried, a human player and settings that keep it: the held kind stays
    assert_eq!(held, carrying(None, 0.0));
    // the constructor's lead: the animal's start at (-1, -1)
    assert_eq!(hunt.lead, Lead::NEW);
    assert_eq!(hunt.lead.start, Point::new(-1.0, -1.0));
}

/// Slot 0x48 on another tree: the hunt takes it and its Attack starts over; one within 0.01
/// starts arrived, which the caller shows (records/425906646's 31110 in update 7967). Its own
/// target it does not take.
#[test]
fn hunt_retargeted_to_tree_beside_it_starts_its_attack_arrived() {
    let mut mover = walker();
    let mut held = Held::default();
    let (mut hunt, _) = HuntAction::start(
        HuntStart {
            task: CHOP,
            target: &tree(),
            player: PlayerId::new(2),
            player_type: PlayerType::HUMAN,
            carry_rule: false,
            held: &mut held,
            walks: walks(Held::default()),
        },
        &engagement(),
        ARMS,
        &mut mover,
    )
    .unwrap();
    let retarget = |hunt: &mut HuntAction, q: &Quarry, e: &Engagement, m: &mut Mover| {
        hunt.retarget(
            HuntRetarget {
                target: q,
                player: PlayerId::new(2),
                walks: walks(Held::default()),
                drops_here: false,
            },
            e,
            ARMS,
            m,
        )
        .unwrap()
    };
    assert_eq!(
        retarget(&mut hunt, &tree(), &engagement(), &mut mover),
        None
    );
    let beside = Quarry {
        id: object_id(9366),
        position: Position::new(20.78, 97.5, 0.0),
        ..tree()
    };
    let close = Engagement {
        gap_squared: DistanceSquared::new(0.005 * 0.005),
        gap: Distance::new(0.005),
        target_position: beside.position,
        ..engagement()
    };
    assert_eq!(retarget(&mut hunt, &beside, &close, &mut mover), Some(true));
    let attack = hunt.state.attack().copied().unwrap();
    assert_eq!(
        (
            hunt.target,
            hunt.state.bare(),
            attack.state.bare(),
            attack.state.move_to().copied().map(|m| m.state)
        ),
        (
            Some(HuntTargetId::new(object_id(9366))),
            HuntState::Attack { attack: None },
            AttackState::Move { move_to: None },
            Some(MoveTo::Arrived)
        )
    );
}

/// Slot 0x68 (`FUN_140ac7980`) clears the lead with the target: a hunt that led one sheep and
/// is sent to another forgets it sent any (`moved_sheep` 0, the start −1). 429604460's
/// shepherd 30721 at update 184: its next sheep, more than 3 tiles from the town centre, is
/// sent there.
#[test]
fn hunt_new_target_clears_its_lead() {
    let mut mover = walker();
    let mut held = Held::default();
    let (mut hunt, _) = HuntAction::start(
        HuntStart {
            task: CHOP,
            target: &tree(),
            player: PlayerId::new(2),
            player_type: PlayerType::HUMAN,
            carry_rule: false,
            held: &mut held,
            walks: walks(Held::default()),
        },
        &engagement(),
        ARMS,
        &mut mover,
    )
    .unwrap();
    let led = Lead {
        moved: true,
        still_time: 3,
        start: Point::new(127.5, 30.4),
    };
    hunt.lead = led;
    let other = Quarry {
        id: object_id(9366),
        position: Position::new(20.78, 97.5, 0.0),
        ..tree()
    };
    // its own target changes nothing
    let _ = hunt
        .retarget(
            HuntRetarget {
                target: &tree(),
                player: PlayerId::new(2),
                walks: walks(Held::default()),
                drops_here: false,
            },
            &engagement(),
            ARMS,
            &mut mover,
        )
        .unwrap();
    assert_eq!(hunt.lead, led);
    let _ = hunt
        .retarget(
            HuntRetarget {
                target: &other,
                player: PlayerId::new(2),
                walks: walks(Held::default()),
                drops_here: false,
            },
            &engagement(),
            ARMS,
            &mut mover,
        )
        .unwrap();
    assert_eq!(
        (hunt.target, hunt.lead),
        (Some(HuntTargetId::new(object_id(9366))), Lead::NEW)
    );
}

#[test]
fn hunter_keeps_another_food_it_carries_and_drops_the_rest_by_the_settings() {
    let start = |held: Held, carry_rule: bool| {
        let mut held = held;
        let _ = HuntAction::start(
            HuntStart {
                task: CHOP,
                target: &tree(),
                player: PlayerId::new(2),
                player_type: PlayerType::HUMAN,
                carry_rule,
                held: &mut held,
                walks: walks(Held::default()),
            },
            &engagement(),
            ARMS,
            &mut walker(),
        )
        .unwrap();
        held
    };
    assert_eq!(
        start(carrying(Some(attribute::FOOD), 7.0), false),
        carrying(Some(attribute::MEAT), 7.0)
    );
    assert_eq!(
        start(carrying(Some(attribute::STONE), 7.0), false),
        carrying(Some(attribute::STONE), 7.0)
    );
    assert_eq!(
        start(carrying(Some(attribute::STONE), 7.0), true),
        carrying(Some(attribute::MEAT), 0.0)
    );
}

fn victim(amount: f32) -> Victim {
    Victim {
        id: object_id(9357),
        amount: Amount::new(amount),
        state: ObjectState::Dying,
        kind: master_type::STATIC,
        class: class::TREE,
        storage: Amount::new(100.0),
        ..Victim::default()
    }
}

#[test]
fn villager_gathers_work_rate_times_work_value_times_dt_until_its_load_is_full() {
    let hunt = HuntAction {
        target: Some(HuntTargetId::new(object_id(9357))),
        state: HuntState::Gather,
        point: Position::new(24.5, 94.5, 0.0),
        target_class: Some(class::TREE),
        task: HuntTask {
            work_value: WorkValue::new(0.39),
            resource: Some(attribute::WOOD),
            ..CHOP
        },
        drop_site: None,
        carry_to: None,
        timer: Seconds::ZERO,
        lead: Lead::default(),
    };
    let r = hunt_reads();
    let mut unit = Unit::default();
    unit.life.attribute_type = Some(attribute::WOOD);
    let mut tree = victim(100.0);
    assert_eq!(
        hunt.gather(Seconds::new(0.02), &mut unit, &mut tree, &r),
        Ok(false)
    );
    let w = (1.0f32 * 0.39) * 0.02;
    assert_eq!(
        (unit.life.amount, tree.amount),
        (Amount::new(w), Amount::new(100.0 - w))
    );
    // a load 0.1 short of the capacity rounds up, and the full load asks for set_state(8)
    unit.life.amount = Amount::new(9.95 - w);
    let mut tree = victim(50.0);
    assert_eq!(
        hunt.gather(Seconds::new(0.02), &mut unit, &mut tree, &r),
        Ok(true)
    );
    assert_eq!(unit.life.amount, Amount::new(10.0));
    // a tree's last bit goes with a thousandth of its master's storage, the empty target
    // asking for set_state(8)
    unit.life.amount = Amount::ZERO;
    let mut tree = victim(0.05);
    assert_eq!(
        hunt.gather(Seconds::new(0.02), &mut unit, &mut tree, &r),
        Ok(true)
    );
    assert_eq!(
        (unit.life.amount, tree.amount),
        (Amount::new(0.05 + 100.0 * 0.001), Amount::ZERO)
    );
}

/// Case 6 without a target: a load is carried off (`set_state(8)`), none or a NaN load searches
/// (`set_state(3)`, `COMISS` with `JBE`).
#[test]
fn hunt_without_target_carries_its_load_off_or_searches() {
    let mut h = HuntAction {
        target: None,
        state: HuntState::Gather,
        point: Position::ORIGIN,
        target_class: Some(class::TREE),
        task: CHOP,
        drop_site: None,
        carry_to: None,
        timer: Seconds::ZERO,
        lead: Lead::default(),
    };
    assert!(h.ends_without_target(Amount::new(0.5)));
    assert_eq!(h.state, HuntState::Gather);
    for held in [0.0, -1.0, f32::NAN] {
        h.state = HuntState::Gather;
        assert!(!h.ends_without_target(Amount::new(held)));
        assert_eq!(h.state, HuntState::Search);
    }
}

/// A live allied target fails the hunt at once (`set_state` 13, its timer cleared), as house
/// 9158 did MarvinBlue/426048731's 9181 after its drop-off in update 1897.
#[test]
fn hunt_on_live_ally_fails() {
    let ally = Quarry {
        allied: true,
        ..tree()
    };
    let mut held = Held::default();
    let (hunt, _) = HuntAction::start(
        HuntStart {
            task: CHOP,
            target: &ally,
            player: PlayerId::new(2),
            player_type: PlayerType::HUMAN,
            carry_rule: false,
            held: &mut held,
            walks: walks(Held::default()),
        },
        &engagement(),
        ARMS,
        &mut walker(),
    )
    .expect("a hunt");
    assert_eq!(
        (hunt.state, hunt.timer, hunt.target),
        (
            HuntState::Failed,
            Seconds::ZERO,
            Some(HuntTargetId::new(ally.id))
        )
    );
}

/// `set_state(4)` on a dead target, or on the hunter's own livestock, walks to it: a quiet
/// `MoveTo` within the task's range, or 1.5 to the livestock (MarvinBlue/426048731's 9171 to its
/// own sheep 9520 in update 1116).
#[test]
fn hunt_walks_to_dead_target_or_its_own_livestock() {
    let dead = Quarry {
        state: ObjectState::Dying,
        ..tree()
    };
    let sheep = Quarry {
        class: class::LIVESTOCK,
        player: PlayerId::new(2),
        ..tree()
    };
    for (q, range) in [(dead, CHOP.range), (sheep, Distance::new(1.5))] {
        let mut held = Held::default();
        let (hunt, _) = HuntAction::start(
            HuntStart {
                task: CHOP,
                target: &q,
                player: PlayerId::new(2),
                player_type: PlayerType::HUMAN,
                carry_rule: false,
                held: &mut held,
                walks: walks(Held::default()),
            },
            &engagement(),
            ARMS,
            &mut walker(),
        )
        .unwrap();
        // a quiet MoveTo to the target that asks again when it moves (`+0x61` 1)
        let walk = hunt.state.move_to().copied().map(|m| {
            (
                m.state,
                ObjectId::field(m.target),
                m.range,
                m.on_target_moved,
                m.voice,
            )
        });
        assert_eq!(
            (hunt.state.bare(), walk, hunt.state.attack().copied()),
            (
                HuntState::Walk { move_to: None },
                Some((
                    MoveTo::Start,
                    9365,
                    range,
                    TargetMoved::AsksAgain,
                    Voice::Quiet
                )),
                None
            )
        );
    }
}

/// 425906646's shepherd 31110 in update 3543: the carcass it walks to went past state 6, so
/// its walk ends and a new one to the hunt's point starts (state 11), the unit stopped; a
/// lumberjack (its unit AI's class 15) would search for another tree first.
#[test]
fn hunt_whose_target_went_walks_to_its_point() {
    let mut hunt = HuntAction {
        target: Some(HuntTargetId::new(object_id(28813))),
        state: HuntState::Walk {
            move_to: Some(to_house(MoveTo::Moving, CHOP.range)),
        },
        point: Position::new(177.272_75, 62.444_31, 0.0),
        target_class: Some(class::LIVESTOCK),
        task: CHOP,
        drop_site: None,
        carry_to: None,
        timer: Seconds::ZERO,
        lead: Lead::default(),
    };
    let mut rig = Rig::on_the_path(Victim::default());
    let mut r = HuntReads {
        ai_class: Some(class::TREE),
        ..hunt_reads()
    };
    let lost = |h: &HuntAction, r: &HuntReads, rig: &mut Rig| {
        let mut h = *h;
        let got = rig.run(100, |c, u, _, _, _| h.lost_on_the_way(c, u, r));
        (got, h)
    };
    assert!(lost(&hunt, &r, &mut rig).0.is_err());
    // the caller's search for the next tree found none: the hunt walks to its point
    r.tree_searched = true;
    assert!(lost(&hunt, &r, &mut rig).0.is_ok());
    (r.tree_searched, r.ai_class) = (false, None);
    (_, hunt) = lost(&hunt, &r, &mut rig);
    let to_point = to_point(
        MoveTo::Start,
        Position::new(177.272_75, 62.444_31, 0.0),
        CHOP.range,
    );
    assert_eq!(
        (
            hunt.state.bare(),
            hunt.target,
            hunt.state.move_to().copied(),
            hunt.timer
        ),
        (
            HuntState::WalkSearch { move_to: None },
            None,
            Some(to_point),
            Seconds::ZERO
        )
    );
    assert_eq!(rig.mover().velocity.unwrap().velocity.y(), 0.0);
}

/// `set_state(6)` on a target of class 49 (429381190's villager 15295 hunting its pasture's
/// dead Ibex 15371 in update 1453): the hunter's own goes on to the hunt's moving sprite, its
/// timer 0; another player's (a farm changes hands, a pasture's livestock ends the hunt) is
/// not modelled.
#[test]
fn hunt_faces_its_own_pasture_dead_livestock() {
    const MOVING: Graphic = Graphic {
        id: SpriteId::named(1880),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(16),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED.union(SpriteFlags::TURNS),
    };
    let ibex = Victim {
        id: object_id(15371),
        class: class::FARM,
        kind: master_type::MOVING,
        player: PlayerId::new(1),
        state: ObjectState::Dead,
        ..Victim::default()
    };
    let face = |victim: Victim| {
        let mut hunt = HuntAction {
            state: HuntState::Walk { move_to: None },
            target_class: Some(class::FARM),
            timer: Seconds::new(-3.0),
            ..hunting(15371)
        };
        let mut rig = Rig::on_the_path(victim);
        let r = HuntReads {
            sprites: HuntSprites {
                moving: Some(MOVING),
                ..HuntSprites::default()
            },
            ..hunt_reads()
        };
        let got = rig.run(50, |c, u, actions, f, _| {
            hunt.set_face(c, u, actions, f, &r)
        });
        (got, hunt, rig.sprite())
    };
    let (r, hunt, shown) = face(ibex.clone());
    assert_eq!(r, Ok(HuntNext::Stay));
    assert_eq!(
        (hunt.state, hunt.timer, hunt.state.move_to().copied()),
        (HuntState::Face, Seconds::ZERO, None)
    );
    assert_eq!(shown, Some(SpriteId::named(1880)));
    assert!(
        face(Victim {
            player: PlayerId::new(2),
            ..ibex
        })
        .0
        .is_err()
    );
}

/// A hunt's walk to its point (states 11 and 15, `0x140ac7a00` cases 0xa and 0xe): an ended
/// `MoveTo` (code 1) stops the unit, after which state 11 searches (`set_state` 3) and state 15
/// is done (`set_state` 2); a failed one (code 3) does the same for a human player, a computer
/// player's would end the hunt (`set_state` 13). A human player's villager whose motion step
/// gave up waits first (425906646's p4 villagers 27238, 31018, 31034 and 31042 in update 5724).
#[test]
fn walk_to_the_point_ends_in_search_or_done() {
    let hunt = |state, move_to| {
        let mut h = HuntAction {
            target: None,
            state,
            point: Position::new(177.272_75, 62.444_31, 0.0),
            target_class: Some(class::LIVESTOCK),
            task: CHOP,
            drop_site: None,
            carry_to: None,
            timer: Seconds::ZERO,
            lead: Lead::default(),
        };
        // a walk holds its MoveTo; a search holds none
        if let Some(m) = h.state.walk_mut() {
            *m = Some(MoveToAction::sub_in(move_to));
        }
        h
    };
    let walker_at = PointWalker {
        player_type: PlayerType::HUMAN,
        villager: true,
        stance: UnitStance::DEFENSIVE,
    };
    let run = |h: &mut HuntAction, who: PointWalker| {
        let mut rig = Rig::on(Spatial::default(), 0.8, Victim::default());
        let at = Position::new(171.888_89, 59.053_39, 0.0);
        rig.unit_mut().place = Static::at(at);
        let id = rig.unit().id;
        rig.t.set_motion(id, Some(Motion::Moving(walker())));
        let got = rig.run(500, |c, u, _, _, _| {
            h.update_point_walk(c, u, me(&h.state), who)
        });
        (got, rig.mover().velocity.unwrap().velocity.y())
    };
    let done = PointWalk::Done {
        target: None,
        second: None,
    };
    for (state, ends) in [
        (HuntState::WalkSearch { move_to: None }, PointWalk::Search),
        (HuntState::WalkPoint { move_to: None }, done),
    ] {
        // ended (code 1) or failed (code 3)
        for code in [MoveTo::Arrived, MoveTo::Failed] {
            let mut h = hunt(state, code);
            let (got, speed) = run(&mut h, walker_at);
            assert_eq!(
                (got.unwrap(), h.state.move_to().copied(), speed),
                (ends, None, 0.0)
            );
            let after = if ends == PointWalk::Search {
                HuntState::Search
            } else {
                HuntState::Idle
            };
            assert_eq!(h.state, after);
        }
        // a computer player's failed walk ends the hunt (set_state 13)
        let computer = PointWalker {
            player_type: PlayerType::COMPUTER,
            ..walker_at
        };
        assert!(run(&mut hunt(state, MoveTo::Failed), computer).0.is_err());
    }
    // set_state(2) lets the target and the second target go, in that order
    let mut h = hunt(HuntState::Search, MoveTo::Arrived);
    (h.target, h.carry_to, h.timer) = (
        Some(HuntTargetId::new(object_id(28813))),
        Some(DropSiteId::new(object_id(9156))),
        Seconds::new(-10.0),
    );
    assert_eq!(
        h.set_done(),
        [Some(object_id(28813)), Some(object_id(9156))]
    );
    assert_eq!(
        (h.state, h.target, h.carry_to, h.point, h.timer),
        (
            HuntState::Idle,
            None,
            None,
            Position::from([-1.0; 3]),
            Seconds::ZERO
        )
    );
    // the wait: a second for a stance below 2, then the MoveTo asks again
    let mut h = hunt(HuntState::WalkSearch { move_to: None }, MoveTo::Waiting);
    *h.state.walk_mut().unwrap() = h.state.move_to().copied().map(|m| MoveToAction {
        wait: MoveWait {
            state: MoveWaitState::Waiting,
            timer: Seconds::ZERO,
            delay: Seconds::new(1.0),
        },
        ..m
    });
    let waiting = |h: &HuntAction| h.state.move_to().copied().map(|m| (m.state, m.wait.timer));
    assert_eq!(run(&mut h, walker_at).0.unwrap(), PointWalk::Walking);
    assert_eq!(waiting(&h), Some((MoveTo::Waiting, Seconds::new(0.5))));
    assert_eq!(run(&mut h, walker_at).0.unwrap(), PointWalk::Walking);
    assert_eq!(waiting(&h), Some((MoveTo::Start, Seconds::ZERO)));
    // an empty list answers 1 ("There is no list."): the walk ended
    let mut h = hunt(HuntState::WalkSearch { move_to: None }, MoveTo::Arrived);
    *h.state.walk_mut().unwrap() = None;
    assert_eq!(run(&mut h, walker_at).0.unwrap(), PointWalk::Search);
}

/// The drop-off's and the point walk's `MoveTo` on the way (`0x140ac7a00` cases 7, 0xa and
/// 0xe, `FUN_140a992b0`): it starts, asking for a path (to the drop site, to the point;
/// after a wait `+0x6c` goes from 1 to 2); moving, it walks on, the drop-off's starting again
/// once its site left its point; a waiting drop-off starts again past its delay, a failed
/// one waits 3 seconds with the unit's animation stopped, and the timer's wait ends in
/// another try. A drop-off or a point walk without its `MoveTo`, a drop site gone and a
/// computer player's ended walk are not modelled.
#[test]
fn hunter_walks_to_its_drop_site_and_to_its_point() {
    let who = PointWalker {
        player_type: PlayerType::HUMAN,
        villager: true,
        stance: UnitStance::AGGRESSIVE,
    };
    let drop = |h: &mut HuntAction, rig: &mut Rig| {
        (
            rig.unit_mut().life.attribute_type,
            rig.unit_mut().life.amount,
        ) = (Some(attribute::WOOD), Amount::new(3.0));
        rig.run(100, |c, u, _, _, _| {
            h.update_drop(c, u, me(&HuntState::Drop { move_to: None }), None, who)
        })
    };
    let dropping = |move_to: Option<MoveToAction>| HuntAction {
        state: HuntState::Drop { move_to },
        carry_to: Some(DropSiteId::new(object_id(10028))),
        ..hunting(-1)
    };
    let state = |h: &HuntAction| h.state.move_to().copied().map(|m| m.state);
    let site = || to_house(MoveTo::Start, CHOP.range);
    let mut rig = Rig::on_the_path(Victim::default()).holding(10028, PlayerId::new(0));
    rig.mover_mut().paths.clear();
    let mut h = dropping(Some(site()));
    assert_eq!(drop(&mut h, &mut rig), Ok(Dropped::Going));
    assert_eq!(state(&h), Some(MoveTo::Moving));
    assert_eq!(rig.mover_mut().paths.len(), 2);
    let mut rig = Rig::on_the_path(Victim::default()).holding(10028, PlayerId::new(0));
    let mut h = dropping(Some(to_house(MoveTo::Moving, CHOP.range)));
    assert_eq!(drop(&mut h, &mut rig), Ok(Dropped::Going));
    assert_eq!(state(&h), Some(MoveTo::Moving));
    // the site left the point the MoveTo holds: it starts again
    *h.state.walk_mut().unwrap() = h.state.move_to().copied().map(|m| MoveToAction {
        point: Position::new(35.0, 88.0, 0.0),
        ..m
    });
    assert_eq!(drop(&mut h, &mut rig), Ok(Dropped::Going));
    assert_eq!(state(&h), Some(MoveTo::Start));
    let mut h = dropping(Some(MoveToAction {
        wait: MoveWait {
            delay: Seconds::new(0.05),
            ..MoveWait::NEW
        },
        ..to_house(MoveTo::Waiting, CHOP.range)
    }));
    assert_eq!(drop(&mut h, &mut rig), Ok(Dropped::Going));
    assert_eq!(state(&h), Some(MoveTo::Start));
    rig.mover_mut().animated.speed = Speed::new(1.0);
    let mut h = dropping(Some(to_house(MoveTo::Failed, CHOP.range)));
    assert_eq!(drop(&mut h, &mut rig), Ok(Dropped::Going));
    assert_eq!(
        (h.timer, rig.mover_mut().animated.speed),
        (DROP_WAIT, Speed::ZERO)
    );
    h.timer = Seconds::new(-0.05);
    assert_eq!(drop(&mut h, &mut rig), Ok(Dropped::Again));
    assert_eq!(h.timer, Seconds::ZERO);
    // an empty list answers 1: the drop site is gone from the call, which is not modelled
    assert!(drop(&mut dropping(None), &mut rig).is_err());
    let mut h = dropping(Some(to_house(MoveTo::Arrived, CHOP.range)));
    assert!(drop(&mut h, &mut rig).is_err());
    // the walk to a point
    let point_walk = |h: &mut HuntAction, rig: &mut Rig, who| {
        rig.run(100, |c, u, _, _, _| {
            h.update_point_walk(c, u, me(&h.state), who)
        })
    };
    let point = Position::new(35.5, 88.5, 0.0);
    let walking = |move_to: Option<MoveToAction>| HuntAction {
        state: HuntState::WalkPoint { move_to },
        point,
        ..hunting(-1)
    };
    let mut rig = Rig::on_the_path(Victim::default());
    rig.mover_mut().paths.clear();
    let mut h = walking(Some(MoveToAction {
        wait: MoveWait {
            state: MoveWaitState::Waiting,
            ..MoveWait::NEW
        },
        ..to_point(MoveTo::Start, point, CHOP.range)
    }));
    assert_eq!(point_walk(&mut h, &mut rig, who), Ok(PointWalk::Walking));
    assert_eq!(
        h.state.move_to().copied().map(|m| (m.state, m.wait.state)),
        Some((MoveTo::Moving, MoveWaitState::AskedAgain))
    );
    assert_eq!(rig.mover_mut().paths.len(), 2);
    let mut rig = Rig::on_the_path(Victim::default());
    let mut h = walking(Some(to_point(MoveTo::Moving, point, CHOP.range)));
    assert_eq!(point_walk(&mut h, &mut rig, who), Ok(PointWalk::Walking));
    assert_eq!(state(&h), Some(MoveTo::Moving));
    let computer = PointWalker {
        player_type: PlayerType::COMPUTER,
        ..who
    };
    let mut h = walking(Some(to_point(MoveTo::Arrived, point, CHOP.range)));
    assert!(point_walk(&mut h, &mut rig, computer).is_err());
}

/// The units in the way of the drop-off's and the point walk's `MoveTo` (notify 0x215, the
/// motion step's [`crate::movement::Mover::in_the_way`]) go to the unit AI at once, in their
/// order, as the Hunt's own walk passes them on.
#[test]
fn hunter_walks_pass_on_the_units_in_their_way() {
    let who = PointWalker {
        player_type: PlayerType::HUMAN,
        villager: true,
        stance: UnitStance::AGGRESSIVE,
    };
    let in_the_way = vec![
        (NotifyKind::IN_THE_WAY, [77, -1, -1]),
        (NotifyKind::IN_THE_WAY, [78, -1, -1]),
    ];
    for state in [
        HuntState::Drop { move_to: None },
        HuntState::WalkPoint { move_to: None },
    ] {
        for move_to in [MoveTo::Start, MoveTo::Moving] {
            let mut rig = Rig::on_the_path(Victim::default()).holding(10028, PlayerId::new(0));
            rig.mover_mut().in_the_way = vec![object_id(77), object_id(78)];
            let drops = matches!(state, HuntState::Drop { .. });
            let mut h = HuntAction {
                state,
                carry_to: Some(DropSiteId::new(object_id(10028))),
                point: Position::new(35.5, 88.5, 0.0),
                ..hunting(-1)
            };
            *h.state.walk_mut().unwrap() = Some(MoveToAction {
                target: drops.then(|| object_id(10028)),
                ..to_point(move_to, Position::new(35.5, 88.5, 0.0), CHOP.range)
            });
            rig.run(100, |c, u, _, _, _| {
                if drops {
                    h.update_drop(c, u, me(&state), None, who).map(|_| ())
                } else {
                    h.update_point_walk(c, u, me(&state), who).map(|_| ())
                }
            })
            .unwrap();
            assert_eq!(rig.posted(), in_the_way, "{state} {move_to:?}");
            assert_eq!(rig.mover().in_the_way, Vec::new());
        }
    }
}

/// `set_state(4)` (`0x140ac60e0` case 4) toward house 10028 from 9177: a dead target is
/// walked to within the task's range, the hunter's own live livestock within
/// `OWN_LIVESTOCK_RANGE`, the point the target's place; a target the hunter touches already
/// starts arrived. Another live target is not modelled; a unit without speed fails it at its start.
#[test]
fn hunt_walks_to_its_target_place_within_its_range() {
    let set_walk = |h: &mut HuntAction, rig: &mut Rig, victim: &Victim| {
        let World {
            spatial,
            players,
            parts,
            ..
        } = &mut rig.t;
        let u = &players[1].objects[ListIndex::new(0)];
        h.set_walk(
            (spatial, u.position()),
            parts.motion_mut(u.id),
            victim,
            PlayerId::new(1),
            &walks(Held::default()),
        )
    };
    let walk = |h: &HuntAction| {
        h.state
            .move_to()
            .copied()
            .map(|m| (m.state, m.range, m.point))
    };
    let dead = Victim {
        state: ObjectState::Dying,
        ..Victim::default()
    };
    let own = Victim {
        class: class::LIVESTOCK,
        player: PlayerId::new(1),
        state: ObjectState::Alive,
        ..Victim::default()
    };
    let mut rig = Rig::on_the_path(Victim::default());
    let mut h = HuntAction {
        timer: Seconds::new(-2.0),
        ..hunting(10028)
    };
    assert_eq!(set_walk(&mut h, &mut rig, &dead), Ok(()));
    let place = Position::new(30.0, 88.0, 0.0);
    assert_eq!(
        (h.state.bare(), h.timer, h.point, walk(&h)),
        (
            HuntState::Walk { move_to: None },
            Seconds::ZERO,
            place,
            Some((MoveTo::Start, CHOP.range, place))
        )
    );
    let mut h = hunting(10028);
    assert_eq!(set_walk(&mut h, &mut rig, &own), Ok(()));
    assert_eq!(
        walk(&h).map(|w| w.1),
        Some(Distance::new(OWN_LIVESTOCK_RANGE))
    );
    let mut h = hunting(9177);
    assert_eq!(set_walk(&mut h, &mut rig, &dead), Ok(()));
    assert_eq!(
        h.state.move_to().copied().map(|m| m.state),
        Some(MoveTo::Arrived)
    );
    let alive = Victim {
        player: PlayerId::new(2),
        ..own
    };
    assert!(set_walk(&mut hunting(10028), &mut rig, &alive).is_err());
    // a unit without speed: its MoveTo fails at its start (state 13)
    rig.mover_mut().stats.speed = Speed::ZERO;
    let mut h = hunting(10028);
    assert_eq!(set_walk(&mut h, &mut rig, &dead), Ok(()));
    assert_eq!(
        h.state.move_to().copied().map(|m| m.state),
        Some(MoveTo::Failed)
    );
}

/// State 10's update (`0x140ac7a00` case 9, [`HuntAction::update_lead`]): without a target the
/// hunt searches; an animal standing where it was sent is slaughtered (`set_state(12)`, its
/// Attack); else the timer runs, and at its end an animal on the start is slaughtered too,
/// one within 1.5 tiles becomes the start again with the hunter facing it and the timer at
/// −1 s, and one farther is walked to (`set_state(4)`). A hunt leading unprepared is not
/// modelled.
#[test]
fn herder_slaughters_its_animal_once_it_stands_or_walks_after_it() {
    let herd = |idle| Herd {
        own: true,
        drop_site: None,
        engagement: engagement(),
        arms: ARMS,
        idle,
        special_graphic: None,
    };
    let animal = |position| Victim {
        id: object_id(10028),
        class: class::LIVESTOCK,
        player: PlayerId::new(1),
        state: ObjectState::Alive,
        position,
        ..Victim::default()
    };
    let lead = |h: &mut HuntAction, victim: Victim, herd: Option<Herd>| {
        let mut rig = Rig::on_the_path(victim);
        let r = HuntReads {
            herd,
            ..hunt_reads()
        };
        rig.run(100, |c, u, _, f, _| h.update_lead(c, u, f, &r))
    };
    let leading = |timer| HuntAction {
        state: HuntState::Lead,
        timer,
        lead: Lead {
            start: Point::new(29.0, 89.0),
            ..Lead::default()
        },
        ..hunting(10028)
    };
    let mut h = HuntAction {
        target: None,
        ..leading(Seconds::ZERO)
    };
    assert_eq!(
        lead(&mut h, animal(Position::new(29.0, 89.0, 0.0)), None),
        Ok(HuntNext::Search)
    );
    assert_eq!(h.state.bare(), HuntState::Search);
    let mut h = leading(Seconds::new(-1.0));
    assert!(lead(&mut h, animal(Position::new(29.0, 89.0, 0.0)), None).is_err());
    let mut h = leading(Seconds::new(-1.0));
    assert_eq!(
        lead(
            &mut h,
            animal(Position::new(29.0, 89.0, 0.0)),
            Some(herd(true))
        ),
        Ok(HuntNext::Stay)
    );
    assert_eq!(
        (h.state.bare(), h.state.attack().copied().is_some()),
        (HuntState::Attack { attack: None }, true)
    );
    let mut h = leading(Seconds::new(-1.0));
    assert_eq!(
        lead(
            &mut h,
            animal(Position::new(29.0, 89.0, 0.0)),
            Some(herd(false))
        ),
        Ok(HuntNext::Stay)
    );
    assert_eq!(
        (h.state.bare(), h.timer),
        (HuntState::Lead, Seconds::new(-1.0 + 0.1))
    );
    let mut h = leading(Seconds::new(-0.05));
    assert_eq!(
        lead(
            &mut h,
            animal(Position::new(29.0, 89.0, 0.0)),
            Some(herd(false))
        ),
        Ok(HuntNext::Stay)
    );
    assert_eq!(h.state.bare(), HuntState::Attack { attack: None });
    let mut h = leading(Seconds::new(-0.05));
    assert_eq!(
        lead(
            &mut h,
            animal(Position::new(29.5, 89.0, 0.0)),
            Some(herd(false))
        ),
        Ok(HuntNext::Stay)
    );
    assert_eq!(
        (h.state.bare(), h.timer, h.lead.start),
        (HuntState::Lead, LEAD_WAIT, Point::new(29.5, 89.0))
    );
    let mut h = leading(Seconds::new(-0.05));
    assert_eq!(
        lead(
            &mut h,
            animal(Position::new(35.0, 89.0, 0.0)),
            Some(herd(false))
        ),
        Ok(HuntNext::Stay)
    );
    assert_eq!(
        (
            h.state.bare(),
            h.state.move_to().copied().map(|m| (m.state, m.range))
        ),
        (
            HuntState::Walk { move_to: None },
            Some((MoveTo::Start, Distance::new(OWN_LIVESTOCK_RANGE)))
        )
    );
}

/// One update (`0x140ac7a00`) by the hunt's state: a computer player's and a state without a
/// case are not modelled; a walk whose target went on the way walks to the point (state 11); a
/// walk that gave up waits, then walks there; a walk under way steps on, or asks again once its
/// target left the `MoveTo`'s point; a lead and a face on nothing search; a gather without a
/// target carries its load off (full) or searches.
#[test]
fn hunt_update_runs_the_step_of_its_state() {
    let update = |h: &mut HuntAction, rig: &mut Rig, player_type: PlayerType, held: f32| {
        (
            rig.unit_mut().life.attribute_type,
            rig.unit_mut().life.amount,
        ) = (Some(attribute::FOOD), Amount::new(held));
        let mut r = hunt_reads();
        r.attack.player_type = player_type;
        let me = me(&h.state.bare());
        rig.run(100, |c, u, actions, f, _| {
            h.update(c, u, actions, me, f, &r)
        })
    };
    let mut rig = Rig::on_the_path(Victim::default()).holding(10028, PlayerId::new(0));
    let reason = |r: Result<HuntNext, crate::GenieError>| r.unwrap_err().reason();
    let r = update(&mut hunting(10028), &mut rig, PlayerType::COMPUTER, 0.0);
    assert_eq!(reason(r), "a computer player's hunt (FUN_140fde4f0)");
    let mut h = HuntAction {
        state: HuntState::New,
        ..hunting(10028)
    };
    let r = update(&mut h, &mut rig, PlayerType::HUMAN, 0.0);
    assert_eq!(
        reason(r),
        "a Hunt action in a state the slice does not model"
    );
    // the target lost on the way
    let walking = |target, timer, move_to| HuntAction {
        state: HuntState::Walk {
            move_to: Some(to_house(move_to, Distance::ZERO)),
        },
        timer,
        point: Position::new(35.5, 88.5, 0.0),
        ..hunting(target)
    };
    let state = |h: &HuntAction| (h.state.bare(), h.state.move_to().copied().map(|m| m.state));
    // its MoveTo's first check finds the house past state 6 (code 5): the target goes and the
    // hunt walks to its point
    let mut lost = Rig::on_the_path(Victim::default()).holding(10028, PlayerId::new(0));
    lost.t.players[0]
        .objects
        .iter_mut()
        .for_each(|u| u.life.state = ObjectState::Removed);
    let mut h = walking(-1, Seconds::ZERO, MoveTo::Moving);
    assert_eq!(
        update(&mut h, &mut lost, PlayerType::HUMAN, 0.0),
        Ok(HuntNext::Stay)
    );
    assert_eq!(
        (
            h.state.bare(),
            h.target,
            h.state
                .move_to()
                .copied()
                .map(|m| (m.state, ObjectId::field(m.target)))
        ),
        (
            HuntState::WalkSearch { move_to: None },
            None,
            Some((MoveTo::Start, -1))
        )
    );
    // a walk that gave up waits its time, then walks to the point
    let mut h = walking(10028, Seconds::new(-1.0), MoveTo::Failed);
    assert_eq!(
        update(&mut h, &mut rig, PlayerType::HUMAN, 0.0),
        Ok(HuntNext::Stay)
    );
    assert_eq!(
        (h.state.bare(), h.timer),
        (HuntState::Walk { move_to: None }, Seconds::new(-1.0 + 0.1))
    );
    let mut h = walking(10028, Seconds::new(-0.05), MoveTo::Failed);
    assert_eq!(
        update(&mut h, &mut rig, PlayerType::HUMAN, 0.0),
        Ok(HuntNext::Stay)
    );
    assert_eq!(
        (h.state.bare(), h.timer),
        (HuntState::WalkSearch { move_to: None }, Seconds::ZERO)
    );
    // a walk under way to the house at the MoveTo's point steps on; off it, it asks again
    let mut h = walking(10028, Seconds::ZERO, MoveTo::Moving);
    assert_eq!(
        update(&mut h, &mut rig, PlayerType::HUMAN, 0.0),
        Ok(HuntNext::Stay)
    );
    assert_eq!(
        state(&h),
        (HuntState::Walk { move_to: None }, Some(MoveTo::Moving))
    );
    let mut h = walking(10028, Seconds::ZERO, MoveTo::Moving);
    *h.state.walk_mut().unwrap() = h.state.move_to().copied().map(|m| MoveToAction {
        point: Position::new(35.0, 88.0, 0.0),
        ..m
    });
    assert_eq!(
        update(&mut h, &mut rig, PlayerType::HUMAN, 0.0),
        Ok(HuntNext::Stay)
    );
    assert_eq!(
        state(&h),
        (HuntState::Walk { move_to: None }, Some(MoveTo::Start))
    );
    // a lead without a target and a face on an empty target search
    for state in [HuntState::Lead, HuntState::Face] {
        let mut h = HuntAction {
            state,
            ..hunting(-1)
        };
        assert_eq!(
            update(&mut h, &mut rig, PlayerType::HUMAN, 0.0),
            Ok(HuntNext::Search)
        );
        assert_eq!(h.state.bare(), HuntState::Search, "state {state}");
    }
    // a gather without a target: empty it searches, loaded it is full
    let mut h = HuntAction {
        target: None,
        ..hunting(-1)
    };
    assert_eq!(
        update(&mut h, &mut rig, PlayerType::HUMAN, 0.0),
        Ok(HuntNext::Search)
    );
    assert_eq!(h.state.bare(), HuntState::Search);
    let mut h = hunting(-1);
    assert_eq!(
        update(&mut h, &mut rig, PlayerType::HUMAN, 5.0),
        Ok(HuntNext::Drop)
    );
    assert_eq!(h.state.bare(), HuntState::Gather);
}

/// State 12's Attack and its end (`0x140ac7a00` case 0xb): under way (code 0) the hunt stays;
/// ended (code 1) on its target the hunt walks to it, without one it searches, and on a predator
/// of a unit AI with orders queued under the game's setting it would end (not modelled); failed
/// (3) it tells the caller, whose search for the next tree follows; its target inside another (4)
/// goes and the hunt walks to its point. A hunt in state 12 without its Attack is not modelled.
#[test]
fn hunt_attack_ends_into_walk_search_or_its_point() {
    use crate::combat::attack::tests::own_attack;
    let sheep = || Victim {
        id: object_id(10028),
        class: class::LIVESTOCK,
        player: PlayerId::new(1),
        state: ObjectState::Alive,
        position: Position::new(30.0, 88.0, 0.0),
        ..Victim::default()
    };
    let attack = |h: &mut HuntAction, state, victim, queue_empty, ends| {
        let mut a = own_attack(10028, state, Distance::new(1.0));
        (a.voice, a.seen) = (Voice::Quiet, Position::new(30.0, 88.0, 0.0));
        a.arms.idle_sprite = Some(Graphic {
            id: SpriteId::named(1),
            frames: Frame::of_data(10),
            facets: FacetCount::of_data(8),
            speed_multiplier: SpeedMultiplier::ONE,
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags: SpriteFlags::NONE,
        });
        *h.state.attack_mut().unwrap() = Some(a);
        let mut rig = Rig::on_the_path(victim).holding(10028, PlayerId::new(1));
        let mut r = hunt_reads();
        (r.attack.queue_empty, r.predator_hunts_end) = (queue_empty, ends);
        let got = rig.run(100, |c, u, actions, f, _| {
            h.update(
                c,
                u,
                actions,
                me(&HuntState::Attack { attack: None }),
                f,
                &r,
            )
        });
        (got, rig.workers(10028, PlayerId::new(1)) == Some(0))
    };
    let fresh = |target| HuntAction {
        state: HuntState::Attack { attack: None },
        ..hunting(target)
    };
    let mut h = fresh(10028);
    assert_eq!(
        attack(&mut h, AttackState::New, sheep(), true, false),
        (Ok(HuntNext::Stay), false)
    );
    assert_eq!(
        (h.state.bare(), h.state.attack().copied().map(|a| a.state)),
        (HuntState::Attack { attack: None }, Some(AttackState::New))
    );
    let done = AttackState::Done;
    let mut h = fresh(10028);
    assert_eq!(
        attack(&mut h, done, sheep(), true, false),
        (Ok(HuntNext::Stay), false)
    );
    assert_eq!(
        (h.state.bare(), h.state.move_to().copied().map(|m| m.range)),
        (
            HuntState::Walk { move_to: None },
            Some(Distance::new(OWN_LIVESTOCK_RANGE))
        )
    );
    let mut h = fresh(-1);
    assert_eq!(
        attack(&mut h, done, sheep(), true, false),
        (Ok(HuntNext::Search), false)
    );
    assert_eq!(
        (h.state.bare(), h.state.attack().copied()),
        (HuntState::Search, None)
    );
    let wolf = Victim {
        class: class::PREDATOR,
        ..sheep()
    };
    let (r, _) = attack(&mut fresh(10028), done, wolf, false, true);
    assert_eq!(
        r.unwrap_err().reason(),
        "a hunt that ends with its Attack on a predator (set_state 1)"
    );
    let mut h = fresh(10028);
    let failed = AttackState::Failed;
    assert_eq!(
        attack(&mut h, failed, sheep(), true, false),
        (Ok(HuntNext::AttackFailed), false)
    );
    assert_eq!(h.state.bare(), HuntState::Attack { attack: None });
    let inside = Victim {
        inside: true,
        ..sheep()
    };
    let mut h = fresh(10028);
    assert_eq!(
        attack(&mut h, done, inside, true, false),
        (Ok(HuntNext::Stay), true)
    );
    assert_eq!(
        (h.state.bare(), h.target),
        (HuntState::WalkSearch { move_to: None }, None)
    );
    let mut rig = Rig::on_the_path(sheep());
    let r = hunt_reads();
    let r = rig.run(100, |c, u, actions, f, _| {
        fresh(10028).update(
            c,
            u,
            actions,
            me(&HuntState::Attack { attack: None }),
            f,
            &r,
        )
    });
    assert_eq!(
        r.unwrap_err().reason(),
        "a Hunt in state 12 without its Attack"
    );
}

/// `set_state(6)` on livestock (`0x140ac60e0` case 6): the hunter's own live animal more
/// than 3 tiles from its drop site and nearer than 30 is led there (`set_state(10)`; an
/// animal that stands is sent first, which the world test
/// `hunter_sheep_walks_before_the_hunter_update_ends` shows), and once led it is led on
/// however far; a nearer or a farther one is slaughtered (`set_state(12)`). A dead animal out
/// of the task's range plus `GAP` is walked to (`set_state(4)`); one at hand is faced. A live
/// one unprepared is not modelled.
#[test]
fn hunter_facing_livestock_leads_slaughters_or_walks_to_it() {
    let herd = |site: Option<(DropSiteId, DistanceSquared)>, idle| Herd {
        own: true,
        drop_site: site,
        engagement: engagement(),
        arms: ARMS,
        idle,
        special_graphic: None,
    };
    let sheep = |state| Victim {
        id: object_id(10028),
        class: class::LIVESTOCK,
        player: PlayerId::new(1),
        state,
        position: Position::new(29.5, 89.0, 0.0),
        ..Victim::default()
    };
    let face = |h: &mut HuntAction, victim: Victim, herd: Option<Herd>| {
        let mut rig = Rig::on_the_path(victim);
        let r = HuntReads {
            herd,
            ..hunt_reads()
        };
        rig.run(100, |c, u, actions, f, _| h.set_face(c, u, actions, f, &r))
    };
    let walking = |target, moved| HuntAction {
        state: HuntState::Walk { move_to: None },
        lead: Lead {
            moved,
            ..Lead::default()
        },
        ..hunting(target)
    };
    assert!(face(&mut walking(10028, false), sheep(ObjectState::Alive), None).is_err());
    let mut h = walking(10028, false);
    let led = face(
        &mut h,
        sheep(ObjectState::Alive),
        Some(herd(
            Some((DropSiteId::new(object_id(500)), DistanceSquared::new(16.0))),
            false,
        )),
    );
    assert_eq!(led, Ok(HuntNext::Stay));
    assert_eq!(
        (
            h.state.bare(),
            h.drop_site,
            h.lead.moved,
            h.lead.start,
            h.timer
        ),
        (
            HuntState::Lead,
            Some(DropSiteId::new(object_id(500))),
            true,
            Point::new(29.5, 89.0),
            LEAD_WAIT
        )
    );
    for (gap_squared, moved, state) in [
        (
            DistanceSquared::new(4.0),
            false,
            HuntState::Attack { attack: None },
        ),
        (HERD_FAR, false, HuntState::Attack { attack: None }),
        (HERD_FAR, true, HuntState::Lead),
    ] {
        let mut h = walking(10028, moved);
        let herd = herd(Some((DropSiteId::new(object_id(500)), gap_squared)), false);
        assert_eq!(
            face(&mut h, sheep(ObjectState::Alive), Some(herd)),
            Ok(HuntNext::Stay)
        );
        assert_eq!(h.state.bare(), state, "{gap_squared} {moved}");
    }
    let mut h = walking(10028, false);
    assert_eq!(
        face(&mut h, sheep(ObjectState::Alive), Some(herd(None, false))),
        Ok(HuntNext::Stay)
    );
    assert_eq!(h.state.bare(), HuntState::Attack { attack: None });
    let mut h = walking(10028, false);
    assert_eq!(
        face(&mut h, sheep(ObjectState::Dying), None),
        Ok(HuntNext::Stay)
    );
    assert_eq!(
        (h.state.bare(), h.state.move_to().copied().map(|m| m.state)),
        (HuntState::Walk { move_to: None }, Some(MoveTo::Start))
    );
    let mut h = walking(9177, false);
    assert_eq!(
        face(&mut h, sheep(ObjectState::Dying), None),
        Ok(HuntNext::Stay)
    );
    assert_eq!(
        (h.state.bare(), h.state.move_to().copied(), h.timer),
        (HuntState::Face, None, Seconds::ZERO)
    );
}

/// MarvinBlue/426048731's lumberjack 9181 in update 1897: its `MoveTo` to the TC 9156 ended
/// (state 1); the TC takes wood within a tile, so the load goes to player 2 as wood (1) and
/// the unit carries nothing after; the hunt has no target to go back to.
#[test]
fn drop_off_gives_the_load_to_the_player() {
    let mut hunt = HuntAction {
        target: None,
        state: HuntState::Drop {
            move_to: Some(MoveToAction::sub_in(MoveTo::Arrived)),
        },
        point: Position::from([-1.0; 3]),
        target_class: Some(class::BUILDING),
        task: CHOP,
        drop_site: None,
        carry_to: Some(DropSiteId::new(object_id(9156))),
        timer: Seconds::ZERO,
        lead: Lead::default(),
    };
    let mut rig = Rig::on(Spatial::default(), 0.8, Victim::default());
    let at = Position::new(26.820_372, 94.718_056, 0.0);
    rig.unit_mut().place = Static::at(at);
    let id = rig.unit().id;
    rig.t.set_motion(id, Some(Motion::Moving(walker())));
    rig.t.players[1].attributes = vec![Amount::ZERO; 230];
    let site = DropOff {
        takes: true,
        gap_squared: DistanceSquared::new(0.5),
    };
    let who = PointWalker {
        player_type: PlayerType::HUMAN,
        villager: true,
        stance: UnitStance::AGGRESSIVE,
    };
    let drop = |rig: &mut Rig, hunt: &mut HuntAction, held: Held, site| {
        (
            rig.unit_mut().life.attribute_type,
            rig.unit_mut().life.amount,
        ) = (held.kind, held.amount);
        rig.run(28, |c, u, _, _, _| {
            hunt.update_drop(
                c,
                u,
                me(&HuntState::Drop { move_to: None }),
                Some(site),
                who,
            )
        })
    };
    let got = drop(
        &mut rig,
        &mut hunt,
        carrying(Some(attribute::WOOD), 7.057_053_6),
        site,
    )
    .unwrap();
    assert_eq!(
        got,
        Dropped::Load(attribute::WOOD, Amount::new(7.057_053_6))
    );
    let life = &rig.unit().life;
    assert_eq!(
        (
            life.attribute_type,
            life.amount,
            hunt.state.move_to().copied()
        ),
        (None, Amount::ZERO, None)
    );
    // carried food of a carcass (15) goes as food; a site that does not take it stops
    *hunt.state.walk_mut().unwrap() = Some(MoveToAction::sub_in(MoveTo::Arrived));
    let got = drop(
        &mut rig,
        &mut hunt,
        carrying(Some(attribute::MEAT), 2.0),
        site,
    )
    .unwrap();
    assert_eq!(got, Dropped::Load(attribute::FOOD, Amount::new(2.0)));
    *hunt.state.walk_mut().unwrap() = Some(MoveToAction::sub_in(MoveTo::Arrived));
    let refused = DropOff {
        takes: false,
        ..site
    };
    assert!(
        drop(
            &mut rig,
            &mut hunt,
            carrying(Some(attribute::MEAT), 2.0),
            refused
        )
        .is_err()
    );
    // a site out of a tile's reach: the hunt waits 3 seconds, the unit's animation stopped;
    // at the wait's end it tries again
    *hunt.state.walk_mut().unwrap() = Some(MoveToAction::sub_in(MoveTo::Arrived));
    let far = DropOff {
        gap_squared: DistanceSquared::new(1.5),
        ..site
    };
    let id = rig.unit().id;
    rig.t.parts.set_anim_speed(id, Speed::new(1.0));
    assert_eq!(
        drop(
            &mut rig,
            &mut hunt,
            carrying(Some(attribute::MEAT), 2.0),
            far
        ),
        Ok(Dropped::Going)
    );
    assert_eq!(
        (hunt.timer, rig.t.parts.anim_speed_of(rig.unit().id)),
        (DROP_WAIT, Some(Speed::ZERO))
    );
    hunt.timer = Seconds::new(-0.01);
    assert_eq!(
        drop(
            &mut rig,
            &mut hunt,
            carrying(Some(attribute::MEAT), 2.0),
            far
        ),
        Ok(Dropped::Again)
    );
    assert_eq!(hunt.timer, Seconds::ZERO);
}

/// State 4's `MoveTo` start ([`HuntAction::update_list`] in [`HuntAction::update`]) toward house 10028: after
/// a wait `+0x6c` goes from 1 to 2, the walk asks for its path and the unit shows the hunt's
/// moving sprite; a target within the walk range is reached at once.
#[test]
fn hunt_walk_starts_toward_its_target() {
    const MOVING: Graphic = Graphic {
        id: SpriteId::named(1880),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(16),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED.union(SpriteFlags::TURNS),
    };
    let start = |range: Distance| {
        let house = Victim {
            id: object_id(10028),
            state: ObjectState::Dying,
            ..Victim::default()
        };
        let mut rig = Rig::on_the_path(house).holding(10028, PlayerId::new(0));
        rig.mover_mut().paths.clear();
        let mut h = HuntAction {
            state: HuntState::Walk {
                move_to: Some(MoveToAction {
                    sprite: Some(MOVING),
                    wait: MoveWait {
                        state: MoveWaitState::Waiting,
                        ..MoveWait::NEW
                    },
                    ..to_house(MoveTo::Start, range)
                }),
            },
            ..hunting(10028)
        };
        let r = hunt_reads();
        let got = rig.run(100, |c, u, actions, f, _| {
            h.update(c, u, actions, me(&HuntState::Walk { move_to: None }), f, &r)
        });
        let paths = rig.mover().paths.len();
        let walk = h.state.move_to().copied().map(|m| (m.state, m.wait.state));
        (got, walk, paths, rig.sprite())
    };
    assert_eq!(
        start(Distance::new(0.1)),
        (
            Ok(HuntNext::Stay),
            Some((MoveTo::Moving, MoveWaitState::AskedAgain)),
            2,
            Some(SpriteId::named(1880))
        )
    );
    let (got, walk, ..) = start(Distance::new(5.0));
    assert_eq!(
        (got, walk.map(|w| w.0)),
        (Ok(HuntNext::Stay), Some(MoveTo::Arrived))
    );
}

/// State 4's walk that ended after its target went from the world (the `MoveTo`'s check cleared
/// its target and it walked on to its point, code 1): the decay check does nothing and
/// `set_state(6)` without a target only shows the walking sprite, timer 0.
#[test]
fn walk_that_ended_without_its_target_only_turns_to_the_face() {
    let mut rig = Rig::on_the_path(Victim::default());
    let mut h = HuntAction {
        state: HuntState::Walk {
            move_to: Some(MoveToAction::sub_in(MoveTo::Arrived)),
        },
        ..hunting(-1)
    };
    let r = hunt_reads();
    let got = rig.run(100, |c, u, actions, f, _| {
        h.update(c, u, actions, me(&HuntState::Walk { move_to: None }), f, &r)
    });
    assert_eq!(got, Ok(HuntNext::Stay));
    assert_eq!(
        (h.state, h.timer, h.state.move_to().copied()),
        (HuntState::Face, Seconds::ZERO, None)
    );
}

/// A `MoveTo` whose target was let go (code 5: the house went past state 6) ends the drop-off's
/// walk as a failure does (the hunt waits 3 seconds, the animation stopped, cases 3 to 5) and
/// the point walk as an arrival does (state 11 searches).
#[test]
fn move_to_that_lost_its_target_ends_the_drop_off_and_the_point_walk() {
    let who = PointWalker {
        player_type: PlayerType::HUMAN,
        villager: true,
        stance: UnitStance::AGGRESSIVE,
    };
    let lost = || {
        let mut rig = Rig::on_the_path(Victim::default()).holding(10028, PlayerId::new(0));
        rig.t.players[0]
            .objects
            .iter_mut()
            .for_each(|u| u.life.state = ObjectState::Removed);
        rig
    };
    let mut rig = lost();
    let mut h = HuntAction {
        state: HuntState::Drop {
            move_to: Some(to_house(MoveTo::Moving, CHOP.range)),
        },
        carry_to: Some(DropSiteId::new(object_id(10028))),
        ..hunting(-1)
    };
    let got = rig.run(100, |c, u, _, _, _| {
        h.update_drop(c, u, me(&HuntState::Drop { move_to: None }), None, who)
    });
    assert_eq!(got, Ok(Dropped::Going));
    assert_eq!(h.timer, DROP_WAIT);
    let mut rig = lost();
    let mut h = HuntAction {
        state: HuntState::WalkSearch {
            move_to: Some(to_house(MoveTo::Moving, CHOP.range)),
        },
        ..hunting(-1)
    };
    let got = rig.run(100, |c, u, _, _, _| {
        h.update_point_walk(c, u, me(&HuntState::WalkSearch { move_to: None }), who)
    });
    assert_eq!(
        (got, h.state, h.state.move_to().copied()),
        (Ok(PointWalk::Search), HuntState::Search, None)
    );
}

/// A hunt of a tree's class searches for the next tree when its `MoveTo` failed (code 3) or its
/// first check finds the target it holds past state 6 (code 5); a target the world no longer
/// holds is not a loss (the `MoveTo` walks on to its point), and nor is a `MoveTo` on its way.
#[test]
fn hunt_of_tree_searches_on_failed_walk_or_target_let_go() {
    let tree_lost = |state: ObjectState, move_to: MoveTo, holding: bool| {
        let mut rig = Rig::on_the_path(Victim::default());
        if holding {
            rig = rig.holding(10028, PlayerId::new(0));
            rig.t.players[0]
                .objects
                .iter_mut()
                .for_each(|u| u.life.state = state);
        }
        rig.set_ai(Some(UnitAi {
            target_class: class_of(15),
            ..UnitAi::default()
        }));
        rig.actions_mut().list.set_hunt(Some(HuntAction {
            state: HuntState::Walk {
                move_to: Some(to_house(move_to, Distance::ZERO)),
            },
            ..hunting(-1)
        }));
        rig.run(100, |c, u, actions, _, _| Ok(u.tree_lost(actions, c)))
    };
    assert_eq!(
        tree_lost(ObjectState::Removed, MoveTo::Moving, true),
        Ok(true)
    );
    assert_eq!(
        tree_lost(ObjectState::Undead, MoveTo::Moving, true),
        Ok(false)
    );
    assert_eq!(
        tree_lost(ObjectState::Alive, MoveTo::Failed, true),
        Ok(true)
    );
    assert_eq!(
        tree_lost(ObjectState::Alive, MoveTo::Moving, false),
        Ok(false)
    );
}

/// A unit in a state of 3 or more adds no sub-action (`FUN_140a977e0` deletes the new
/// `MoveTo`): the hunt's `set_state` leaves it without one.
#[test]
fn dying_hunter_makes_no_move_to() {
    let mut mover = walker();
    let mut h = hunting(-1);
    h.point = Position::new(5.0, 6.0, 0.0);
    let mut dying = walks(Held::default());
    dying.reads.state = ObjectState::Dying;
    h.set_point_walk(
        |move_to| HuntState::WalkSearch { move_to },
        &mut mover,
        &dying,
    )
    .unwrap();
    assert_eq!(
        (h.state, h.state.move_to().copied()),
        (HuntState::WalkSearch { move_to: None }, None)
    );
    h.set_point_walk(
        |move_to| HuntState::WalkSearch { move_to },
        &mut mover,
        &walks(Held::default()),
    )
    .unwrap();
    assert_eq!(
        h.state.move_to().copied().map(|m| m.state),
        Some(MoveTo::Start)
    );
}
