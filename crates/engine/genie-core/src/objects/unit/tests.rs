use crate::GenieError;
use crate::combat::attack::AttackState;
use crate::construction::wait_solidify::WaitSolidifyState;
use crate::map::Distance;
use crate::map::Position;
use crate::movement::Speed;
use crate::movement::move_to::{MoveTo, MoveToAction, MoveWait};
use crate::objects::ActionState;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::WorkRate;
use crate::objects::actions::ActionList;
use crate::objects::layers::{Animated, Body};
use crate::objects::lifecycle::{Class, Lifecycle};
use crate::objects::masters::TaskId;
use crate::objects::sprite::{ActiveSprite, Graphic};
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::world::Seconds;
use crate::world::others::OthersRef;
use crate::world::tests::{forage_world, in_update};

/// A graphic of sixteen facets, its sequence flag bit 1 set (`turns`).
const WALKING: Graphic = Graphic {
    id: SpriteId::named(300),
    frames: crate::objects::Frame::of_data(10),
    facets: crate::objects::FacetCount::of_data(16),
    speed_multiplier: SpeedMultiplier::ONE,
    frame_duration: Seconds::new(0.1),
    replay_delay: Seconds::ZERO,
    flags: SpriteFlags::ANIMATED.union(SpriteFlags::TURNS),
};

/// `RGE_Static_Object::update`'s sleep: a static object flagged to sleep drops the flag and its
/// update ends; one not flagged goes on; one above the static type is not modelled.
#[test]
fn a_static_object_flagged_to_sleep_ends_its_update() {
    let flagged = |class| Unit {
        life: Lifecycle {
            class,
            goto_sleep: true,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut u = flagged(Class::Static);
    assert_eq!(u.go_to_sleep(), Ok(true));
    assert!(!u.life.goto_sleep);
    assert_eq!(u.go_to_sleep(), Ok(false));
    let mut animated = flagged(Class::Animated);
    assert_eq!(
        animated.go_to_sleep().unwrap_err().reason(),
        "an object above the static type goes to sleep"
    );
}

/// The movement state takes the current sprite's facets and its sequence flag bit 1; without a
/// sprite it keeps what it had.
#[test]
fn the_movement_state_takes_the_sprite_s_facets() {
    let mut u = Unit {
        body: Body::Moving(crate::gathering::hunt::tests::walker()),
        ..Unit::default()
    };
    u.take_sprite_facets();
    assert_eq!(u.body.mover().unwrap().sprite_facets, None);
    u.sprite = Some(ActiveSprite::start(
        WALKING,
        &mut crate::math::rng::Well512::default(),
    ));
    u.take_sprite_facets();
    assert_eq!(
        u.body.mover().unwrap().sprite_facets,
        Some(crate::objects::facing::SpriteFacets {
            count: crate::objects::FacetCount::of_data(16),
            turns: true,
        })
    );
}

/// The animated object's update: a sprite with an animation speed runs its clock by the
/// update's dt; one without a speed stands still.
#[test]
fn an_animated_object_s_sprite_runs_by_the_update_s_dt() {
    let animated = |speed: Option<f32>| {
        let mut t = forage_world();
        let u = &mut t.players[1].objects[ListIndex::new(0)];
        u.sprite = Some(ActiveSprite::start(
            WALKING,
            &mut crate::math::rng::Well512::default(),
        ));
        // a static object has no animation speed, so no clock
        let at = u.position();
        u.body = speed.map_or_else(
            || Body::at(at),
            |speed| Body::new(at, Some(Animated::new(Speed::new(speed))), None),
        );
        let before = u.sprite;
        in_update(&mut t, (PlayerId::new(1), ListIndex::new(0)), 50, |u, c| {
            u.animate(c);
            Ok(())
        })
        .unwrap();
        (before, t.players[1].objects[ListIndex::new(0)].sprite)
    };
    let (before, after) = animated(Some(1.0));
    assert_ne!(before, after);
    let (before, after) = animated(None);
    assert_eq!(before, after);
}

/// `RGE_Action_Object::update` after the moving update: an object past state 2 loses its action
/// list (a `MoveTo`'s teardown stops the unit); an alive object with an unmodelled action fails.
#[test]
fn an_object_past_state_2_loses_its_actions() {
    let mut t = forage_world();
    let u = &mut t.players[1].objects[ListIndex::new(0)];
    u.life.state = ObjectState::Dying;
    u.actions.set_move_to(Some(MoveToAction {
        sprite: None,
        state: MoveTo::Moving,
        point: Position::new(40.0, 80.0, 0.0),
        target: None,
        range: Distance::ZERO,
        quiet: false,
        on_target_moved: crate::movement::move_to::TargetMoved::Follows,
        wait: MoveWait::default(),
    }));
    in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        Unit::actions_past_life,
    )
    .unwrap();
    let u = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(u.actions.move_to().copied(), None);
    assert_eq!(u.body.mover().unwrap().animated.speed, Speed::new(0.0));
    let mut t = forage_world();
    t.players[1].objects[ListIndex::new(0)].unmodelled_action =
        Some(GenieError::unsupported("an action"));
    let r = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        Unit::actions_past_life,
    );
    assert_eq!(r.unwrap_err().reason(), "an action");
}

/// After the actions the units the unit's walk met take their pauses at once (slot 0x5f0: the
/// walked level's and the gait): Gaia's 600 with a path level; 601 without a mover and an id
/// the world does not hold take none. The unit's place is its own, which the world's index
/// follows (FD-045-D15): nothing copies the index's place back. A walk that met others in its
/// way is not modelled.
#[test]
fn after_its_actions_the_units_a_walk_met_take_their_pauses() {
    let mut t = forage_world();
    let at = t.players[1].objects[ListIndex::new(0)].position();
    let mut met = crate::gathering::hunt::tests::walker();
    met.paths = vec![crate::movement::PathData::without_path()];
    t.players[0].objects[ListIndex::new(0)]
        .body
        .put_mover(Some(met));
    let mover = t.players[1].objects[ListIndex::new(0)]
        .body
        .mover_mut()
        .unwrap();
    mover.pauses = vec![
        (object_id(600), Seconds::new(0.5)),
        (object_id(601), Seconds::new(0.5)),
        (object_id(4242), Seconds::new(0.5)),
    ];
    let paused = in_update(&mut t, (PlayerId::new(1), ListIndex::new(0)), 50, |u, c| {
        u.after_actions(c)?;
        let (_, bush) = OthersRef::of(&c.w.players, c.player)
            .unwrap()
            .find(object_id(600))
            .unwrap();
        let level = bush.body.mover().unwrap().paths[0].pause_time;
        Ok((
            level,
            c.w.spatial.gait(object_id(600)).and_then(|g| g.pause),
        ))
    })
    .unwrap();
    assert_eq!(paused, (Seconds::new(0.5), Some(Seconds::new(0.5))));
    let u = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(u.position(), at);
    assert_eq!(t.spatial.get(object_id(9175)).map(|p| p.position), Some(at));
    assert_eq!(u.body.mover().unwrap().pauses, []);
    t.players[1].objects[ListIndex::new(0)]
        .body
        .mover_mut()
        .unwrap()
        .in_the_way = vec![object_id(600)];
    let r = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        Unit::after_actions,
    );
    assert_eq!(
        r.unwrap_err().reason(),
        "a walk's unit in the way (notify 0x215) the action does not pass on"
    );
}

/// The update up to the actions: a forager goes on to its actions with its life step's answer;
/// a static object flagged to sleep ends there (None).
#[test]
fn the_update_up_to_the_actions_ends_for_a_sleeper() {
    let mut t = forage_world();
    let step = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        Unit::update_to_actions,
    )
    .unwrap();
    assert!(step.is_some());
    let bush = &mut t.players[0].objects[ListIndex::new(0)];
    bush.life.class = Class::Static;
    bush.life.goto_sleep = true;
    let step = in_update(
        &mut t,
        (PlayerId::new(0), ListIndex::new(0)),
        50,
        Unit::update_to_actions,
    )
    .unwrap();
    assert_eq!(step, None);
    assert!(!t.players[0].objects[ListIndex::new(0)].life.goto_sleep);
}

/// The actions' update: an idle forager runs none and captures nothing; a wait for a
/// foundation the world does not hold ends; a Gather walks on to its bush.
#[test]
fn the_actions_update_runs_the_action_held() {
    let mut t = forage_world();
    let captured = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        Unit::update_actions,
    )
    .unwrap();
    assert!(captured.is_none());
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .set_wait_solidify(Some(gone_foundation()));
    let captured = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        Unit::update_actions,
    )
    .unwrap();
    assert!(captured.is_none());
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .wait_solidify(),
        None
    );
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    let before = t.players[1].objects[ListIndex::new(0)]
        .actions
        .gather()
        .copied();
    let captured = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        Unit::update_actions,
    )
    .unwrap();
    assert!(captured.is_none());
    let after = t.players[1].objects[ListIndex::new(0)]
        .actions
        .gather()
        .copied();
    assert_eq!(after.map(|g| g.target), Some(Some(object_id(600))));
    assert_ne!(before, after);
}

/// A wait for a foundation (state 0) on one the world does not hold.
fn gone_foundation() -> crate::construction::wait_solidify::WaitSolidify {
    crate::construction::wait_solidify::WaitSolidify {
        state: WaitSolidifyState::Waiting,
        task: crate::objects::masters::Task::default(),
        target: object_id(99_999),
    }
}

/// The list's update (`FUN_140a992b0`) runs its head's update, then the idle update
/// (`FUN_140a78470`) of each action below it, which is not ported: a wait for a gone foundation
/// on top of a Gather stops there, before its deletion.
#[test]
fn the_list_s_update_stops_at_an_action_below_its_head() {
    let mut t = forage_world();
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    let unit = &mut t.players[1].objects[ListIndex::new(0)];
    unit.actions.set_wait_solidify(Some(gone_foundation()));
    assert_eq!(
        unit.top_action_kind(),
        Some(crate::construction::wait_solidify::WAIT_SOLIDIFY)
    );
    let err = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        Unit::update_actions,
    )
    .unwrap_err();
    assert_eq!(err.kind(), crate::ErrorKind::Unreachable);
    assert_eq!(
        err.reason(),
        "an action below the head of the list (its idle update, FUN_140a78470)"
    );
    assert!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .wait_solidify()
            .is_some()
    );
}

/// An object inside another runs its inside update (`FUN_140a98790`) in place of its actions':
/// its Gather goes, with no walk; an empty list in state 2 starts the master's default task,
/// which waits for the next update.
#[test]
fn an_object_inside_another_runs_its_inside_update() {
    let mut t = forage_world();
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    let at = t.players[1].objects[ListIndex::new(0)].position();
    t.players[1].objects[ListIndex::new(0)].life.inside = Some(object_id(700));
    assert_eq!(
        in_update(
            &mut t,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            Unit::update_actions
        ),
        Ok(None)
    );
    let unit = &t.players[1].objects[ListIndex::new(0)];
    assert!(unit.actions.is_empty());
    assert_eq!(unit.position(), at);
    let unit = &mut t.players[1].objects[ListIndex::new(0)];
    unit.life.inside = None;
    unit.default_artifact = true;
    assert_eq!(
        in_update(
            &mut t,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            Unit::update_actions
        ),
        Ok(None)
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].actions.artifact(),
        Some(&crate::ownership::artifact::Artifact::new(false, None)),
        "the Artifact the default task made has not run yet"
    );
}

/// Slot 0x190 (`FUN_14106d090`, `FUN_140a78450`): the object the top action targets, none for an
/// action without one (a wait for a foundation) or without an action, and an error for an action the slice does not hold.
#[test]
fn the_top_action_s_object_is_its_target_unless_it_has_none() {
    use crate::construction::build::BuildTask;
    let task = BuildTask {
        range: Distance::new(0.1),
        work_value: crate::objects::masters::WorkValue::new(1.0),
        work_rate: WorkRate::new(1.0),
        resource: None,
        id: TaskId::of_task(-1),
        moving: None,
    };
    let building = Unit {
        actions: ActionList::default().with_build(Some(
            crate::construction::build::tests::started(
                object_id(800),
                Position::new(1.0, 2.0, 0.0),
                Distance::ZERO,
                task,
            ),
        )),
        ..Unit::default()
    };
    assert_eq!(building.top_action_object(), Ok(Some(object_id(800))));
    // a wait for a foundation keeps it at +0x68, its +0x30 is −1
    let waiting = Unit {
        actions: ActionList::default().with_wait_solidify(Some(
            crate::construction::wait_solidify::WaitSolidify::new(
                crate::objects::masters::Task::default(),
                object_id(900),
            ),
        )),
        ..Unit::default()
    };
    assert_eq!(waiting.top_action_object(), Ok(None));
    assert_eq!(Unit::default().top_action_object(), Ok(None));
    let unmodelled = Unit {
        unmodelled_action: Some(GenieError::unsupported("an action")),
        ..building
    };
    assert!(unmodelled.top_action_object().is_err());
}

/// The top action's sub-action: a Hunt's `MoveTo` with its state number, else its Attack; a
/// Build's `MoveTo`; none for a `MoveTo` of its own or without an action.
#[test]
fn the_top_action_s_sub_action_is_its_move_to_or_its_attack() {
    use crate::movement::move_to::{MOVE_TO, MoveTo, MoveToAction};
    use crate::world::checkpoint::TopAction;
    let walking = |state| TopAction {
        kind: MOVE_TO,
        state,
    };
    let mut hunt = crate::gathering::hunt::tests::hunting(7);
    hunt.move_to = Some(MoveToAction::sub_in(MoveTo::Arrived));
    let hunter = Unit {
        actions: ActionList::default().with_hunt(Some(hunt)),
        ..Unit::default()
    };
    assert_eq!(hunter.top_sub_action(), Some(walking(ActionState::new(1))));
    let mut idle = hunt;
    idle.move_to = None;
    let resting = Unit {
        actions: ActionList::default().with_hunt(Some(idle)),
        ..Unit::default()
    };
    assert_eq!(resting.top_sub_action(), None);
    let mut build = crate::construction::build::tests::started(
        object_id(800),
        Position::new(1.0, 2.0, 0.0),
        Distance::ZERO,
        crate::construction::build::BuildTask {
            range: Distance::new(0.1),
            work_value: crate::objects::masters::WorkValue::new(1.0),
            work_rate: WorkRate::new(1.0),
            resource: None,
            id: TaskId::of_task(-1),
            moving: None,
        },
    );
    build.move_to = Some(MoveToAction::sub_in(MoveTo::Moving));
    let builder = Unit {
        actions: ActionList::default().with_build(Some(build)),
        ..Unit::default()
    };
    assert_eq!(builder.top_sub_action(), Some(walking(ActionState::new(4))));
    assert_eq!(Unit::default().top_sub_action(), None);
}

/// The sub-action of each kind of top action: the walk of an Artifact, a Fly, an Attack, an
/// Explore, an Enter and a Transport, an Artifact's Enter when it does not walk, and none for an
/// own `MoveTo`, a Make or a wait for a foundation.
#[test]
fn every_kind_of_top_action_shows_its_sub_action() {
    use crate::movement::move_to::{MOVE_TO, MoveTo};
    use crate::objects::actions::Action;
    use crate::world::checkpoint::TopAction;
    let walking = Some(TopAction {
        kind: MOVE_TO,
        state: ActionState::new(MoveTo::Moving.number()),
    });
    let of = |action: Action| {
        let mut u = Unit::default();
        u.actions.put(action);
        u.top_sub_action()
    };
    let mut artifact = crate::ownership::artifact::Artifact::new(false, None);
    artifact.move_to = Some(MoveToAction::sub_in(MoveTo::Moving));
    assert_eq!(of(Action::Artifact(artifact)), walking);
    let mut enter = crate::garrison::EnterAction::new(
        object_id(7),
        Position::new(1.0, 2.0, 0.0),
        &crate::garrison::tests::enter_task(
            Distance::new(0.5),
            crate::objects::masters::class::NO_CLASS,
        ),
    );
    artifact.move_to = None;
    artifact.enter = Some(enter);
    assert_eq!(
        of(Action::Artifact(artifact)),
        Some(TopAction {
            kind: crate::garrison::ENTER,
            state: ActionState::new(enter.state.get()),
        })
    );
    enter = enter.walking(MoveTo::Moving);
    assert_eq!(of(Action::Enter(enter)), walking);
    let mut fly = crate::movement::fly::Fly::start(
        crate::objects::masters::Task::default(),
        Position::new(1.0, 2.0, 0.0),
        &mut crate::math::rng::Well512::default(),
    );
    fly.move_to = Some(MoveToAction::sub_in(MoveTo::Moving));
    assert_eq!(of(Action::Fly(fly)), walking);
    let mut attack =
        crate::combat::attack::tests::own_attack(7, AttackState::Move, Distance::new(0.5));
    attack.move_to = Some(MoveToAction::sub_in(MoveTo::Moving));
    assert_eq!(of(Action::Attack(attack)), walking);
    let (mut explore, _) = crate::movement::explore::ExploreAction::new(
        Position::new(1.0, 2.0, 0.0),
        &crate::movement::explore::bare_walk(),
    )
    .unwrap();
    if let Some(m) = explore.move_to.as_mut() {
        m.state = MoveTo::Moving;
    }
    assert_eq!(of(Action::Explore(explore)), walking);
    let mut transport =
        crate::garrison::transport::TransportAction::new(Position::new(1.0, 2.0, 0.0));
    transport.move_to = Some(MoveToAction::sub_in(MoveTo::Moving));
    assert_eq!(of(Action::Transport(transport)), walking);
    let wait = crate::construction::wait_solidify::WaitSolidify {
        state: WaitSolidifyState::Waiting,
        task: crate::objects::masters::Task::default(),
        target: object_id(7),
    };
    assert_eq!(of(Action::WaitSolidify(wait)), None);
}

/// A unit's master: its own when it has one (a captured unit's copy), else its player's of its
/// id; an error with neither.
#[test]
fn a_unit_reads_its_own_master_before_its_player_s() {
    use crate::objects::masters::{Master, Masters};
    let shared = Master {
        id: master_id(4),
        hit_points: MasterHitPoints::of_data(25),
        ..Master::default()
    };
    let every = vec![Masters::default(), Masters::new([shared])];
    let mut u = Unit {
        id: object_id(7),
        life: Lifecycle {
            master_id: master_id(4),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    assert_eq!(
        u.master(&every, PlayerId::new(1)).map(|m| m.hit_points),
        Ok(MasterHitPoints::of_data(25))
    );
    u.own_master = Some(Box::new(Master {
        id: master_id(4),
        hit_points: MasterHitPoints::of_data(40),
        ..Master::default()
    }));
    assert_eq!(
        u.master(&every, PlayerId::new(1)).map(|m| m.hit_points),
        Ok(MasterHitPoints::of_data(40))
    );
    assert_eq!(
        u.master(&every, PlayerId::new(0)).map(|m| m.hit_points),
        Ok(MasterHitPoints::of_data(40))
    );
    u.own_master = None;
    let missing = u
        .master(&every, PlayerId::new(0))
        .expect_err("player 0 has no master 4");
    assert_eq!(missing, crate::world::OBJECT_MASTER);
    assert!(
        u.master(&every, PlayerId::new(2)).is_err(),
        "no table for player 2"
    );
}
