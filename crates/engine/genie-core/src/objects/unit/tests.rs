use crate::GenieError;
use crate::combat::attack::AttackState;
use crate::combat::attack::tests::own_attack;
use crate::construction::FoundationId;
use crate::construction::build::tests::started;
use crate::construction::build::{BuildState, BuildTask};
use crate::construction::wait_solidify::WaitSolidifyState;
use crate::construction::wait_solidify::{WAIT_SOLIDIFY, WaitSolidify};
use crate::garrison::ContainerId;
use crate::garrison::tests::enter_task;
use crate::garrison::transport::{TransportAction, TransportState};
use crate::garrison::{ENTER, EnterAction};
use crate::gathering::GatherTargetId;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::tests::{hunting, walker};
use crate::map::Distance;
use crate::map::Position;
use crate::math::rng::Well512;
use crate::movement::Speed;
use crate::movement::TopSpeed;
use crate::movement::explore::{ExploreAction, bare_walk};
use crate::movement::fly::{Fly, FlyState};
use crate::movement::move_to::TargetMoved;
use crate::movement::move_to::{MoveTo, MoveToAction, MoveWait};
use crate::movement::{Me, PathData};
use crate::objects::ActionState;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectState;
use crate::objects::SleepCheck;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::Voice;
use crate::objects::WorkRate;
use crate::objects::actions::ActionList;
use crate::objects::facing::SpriteFacets;
use crate::objects::layers::{Action, Action as LayersAction, DefaultTask};
use crate::objects::layers::{Animated, Motion};
use crate::objects::layers::{IdleCount, IdleTime, Speedup, SpeedupValue};
use crate::objects::lifecycle::{Class, Lifecycle};
use crate::objects::masters::ActionKind;
use crate::objects::masters::TaskId;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::{Task, WorkValue};
use crate::objects::sprite::{ActiveSprite, Graphic};
use crate::objects::unit::Unit;
use crate::objects::{FacetCount, Frame};
use crate::ownership::artifact::{Artifact, ArtifactSub};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::test_ids::group_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::task_key;
use crate::testing::class::NO_CLASS;
use crate::testing::roles;
use crate::testing::slots;
use crate::testing::tuning;
use crate::unit_ai::UnitAi;
use crate::world::Seconds;
use crate::world::World;
use crate::world::objects_ref::ObjectsRef;
use crate::world::others::OthersRef;
use crate::world::parts::Part;
use crate::world::tests::{forage_world, in_update};
use crate::world::{Millis, OBJECT_MASTER, Unread, WorldTime};
use std::iter::once;
use std::sync::Arc;

/// A graphic of sixteen facets, its sequence flag bit 1 set (`turns`).
const WALKING: Graphic = Graphic {
    id: SpriteId::named(300),
    frames: Frame::of_data(10),
    facets: FacetCount::of_data(16),
    speed_multiplier: SpeedMultiplier::ONE,
    frame_duration: Seconds::new(0.1),
    replay_delay: Seconds::ZERO,
    flags: SpriteFlags::ANIMATED.union(SpriteFlags::TURNS),
};

/// `RGE_Static_Object::update`'s sleep: a static object flagged to sleep drops the flag and its
/// update ends; one not flagged goes on; one above the static type is not modelled.
#[test]
fn static_object_flagged_to_sleep_ends_its_update() {
    let flagged = |class| Unit {
        life: Lifecycle {
            class,
            goto_sleep: true,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut unit = flagged(Class::Static);
    assert_eq!(unit.go_to_sleep(None, None), Ok(SleepCheck::FallsAsleep));
    assert!(!unit.life.goto_sleep);
    assert_eq!(unit.go_to_sleep(None, None), Ok(SleepCheck::StaysAwake));
    let mut animated = flagged(Class::Animated);
    assert_eq!(
        animated.go_to_sleep(None, None).unwrap_err().reason(),
        "an object above the static type goes to sleep"
    );
    // a static object flagged with movement state or a unit AI is not modelled either
    let (walker, unit_ai) = (walker(), UnitAi::default());
    for (mover, ai) in [(Some(&walker), None), (None, Some(&unit_ai))] {
        assert!(flagged(Class::Static).go_to_sleep(mover, ai).is_err());
    }
}

/// The movement state takes the current sprite's facets and its sequence flag bit 1; without a
/// sprite it keeps what it had.
#[test]
fn the_movement_state_takes_the_sprite_facets() {
    let mut unit = Unit::default();
    let mut mover = walker();
    unit.take_sprite_facets(Some(&mut mover));
    assert_eq!(mover.sprite_facets, None);
    unit.sprite = Some(ActiveSprite::start(
        WALKING,
        &mut Well512::default(),
        tuning().sprite,
    ));
    unit.take_sprite_facets(Some(&mut mover));
    assert_eq!(
        mover.sprite_facets,
        Some(SpriteFacets {
            count: FacetCount::of_data(16),
            turns: true,
        })
    );
}

/// The animated object's update: a sprite with an animation speed runs its clock by the
/// update's dt; one without a speed stands still.
#[test]
fn an_animated_object_sprite_runs_by_the_update_dt() {
    let animated = |speed: Option<f32>| {
        let mut world = forage_world();
        let players = &mut world.players[1].objects[ListIndex::new(0)];
        players.sprite = Some(ActiveSprite::start(
            WALKING,
            &mut Well512::default(),
            tuning().sprite,
        ));
        // a static object has no animation speed, so no clock
        let (id, before) = (players.id, players.sprite);
        let motion =
            speed.map(|speed| Box::new(Motion::Animated(Animated::new(Speed::new(speed)))));
        world.set_part(id, Part::Motion(motion)).unwrap();
        in_update(
            &mut world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, _, step| {
                unit.animate(step);
                Ok(())
            },
        )
        .unwrap();
        (before, world.players[1].objects[ListIndex::new(0)].sprite)
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
    let mut world = forage_world();
    let players = &mut world.players[1].objects[ListIndex::new(0)];
    players.life.state = ObjectState::Dying;
    world
        .parts
        .actions
        .of_mut(players.id)
        .unwrap()
        .list
        .set_move_to(Some(MoveToAction {
            sprite: None,
            state: MoveTo::Moving,
            point: Position::new(40.0, 80.0, 0.0),
            target: None,
            range: Distance::ZERO,
            voice: Voice::Loud,
            on_target_moved: TargetMoved::Follows,
            wait: MoveWait::default(),
        }));
    in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        Unit::actions_past_life,
    )
    .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world.parts.actions_of(unit.id).list.move_to().copied(),
        None
    );
    assert_eq!(
        world.parts.mover_of(unit.id).unwrap().animated.speed,
        Speed::new(0.0)
    );
    let mut world = forage_world();
    world.set_unread(
        world.players[1].objects[ListIndex::new(0)].id,
        GenieError::unsupported("an action"),
    );
    let in_update = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        Unit::actions_past_life,
    );
    assert_eq!(in_update.unwrap_err().reason(), "an action");
}

/// After the actions the units the unit's walk met take their pauses at once (slot 0x5f0: the
/// walked level's and the gait): Gaia's 600 with a path level; 601 without a mover and an id
/// the world does not hold take none. The unit's place is its own, which the world's index
/// follows (FD-045-D15): nothing copies the index's place back. A walk that met others in its
/// way is not modelled.
#[test]
fn after_its_actions_the_units_walk_met_take_their_pauses() {
    let mut world = forage_world();
    let at = world.players[1].objects[ListIndex::new(0)].position();
    let mut met = walker();
    met.paths = vec![PathData::without_path()];
    world
        .parts
        .put_mover(world.players[0].objects[ListIndex::new(0)].id, Some(met));
    let mover = world
        .parts
        .mover_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    mover.pauses = vec![
        (object_id(600), Seconds::new(0.5)),
        (object_id(601), Seconds::new(0.5)),
        (object_id(4242), Seconds::new(0.5)),
    ];
    let paused = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, _, step| {
            unit.after_actions(step)?;
            let (_, bush) = OthersRef::of(&step.world.players, &step.world.index, step.player)
                .unwrap()
                .find(object_id(600))
                .unwrap();
            let level = step.world.parts.mover_of(bush.id).unwrap().paths[0].pause_time;
            Ok((
                level,
                ObjectsRef::new(
                    &step.world.players,
                    &step.world.index,
                    &step.world.model.masters,
                    &step.world.parts,
                    &step.world.model.roles,
                    &step.world.model.tuning,
                    step.world.treaty.as_ref(),
                )
                .gait(object_id(600))
                .and_then(|gait| gait.pause),
            ))
        },
    )
    .unwrap();
    assert_eq!(paused, (Seconds::new(0.5), Some(Seconds::new(0.5))));
    let players = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(players.position(), at);
    assert_eq!(
        world
            .spatial
            .get(object_id(9175))
            .map(|placed| placed.position),
        Some(at)
    );
    assert_eq!(world.parts.mover_of(players.id).unwrap().pauses, []);
    world
        .parts
        .mover_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .in_the_way = vec![object_id(600)];
    let in_update = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, _, step| unit.after_actions(step),
    );
    assert_eq!(
        in_update.unwrap_err().reason(),
        "a walk's unit in the way (notify 0x215) the action does not pass on"
    );
}

/// The update up to the actions: a forager goes on to its actions with its life step's answer;
/// a static object flagged to sleep ends there (None).
#[test]
fn the_update_up_to_the_actions_ends_for_sleeper() {
    let mut world = forage_world();
    let step = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, action, step| Unit::update_to_actions(unit, action, step, &roles()),
    )
    .unwrap();
    assert!(step.is_some());
    let bush = &mut world.players[0].objects[ListIndex::new(0)];
    bush.life.class = Class::Static;
    bush.life.goto_sleep = true;
    let step = in_update(
        &mut world,
        (PlayerId::new(0), ListIndex::new(0)),
        50,
        |unit, action, step| Unit::update_to_actions(unit, action, step, &roles()),
    )
    .unwrap();
    assert_eq!(step, None);
    assert!(!world.players[0].objects[ListIndex::new(0)].life.goto_sleep);
}

/// The actions' update: an idle forager runs none and captures nothing; a wait for a
/// foundation the world does not hold ends; a Gather walks on to its bush.
#[test]
fn the_actions_update_runs_the_action_held() {
    let mut world = forage_world();
    let captured = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, action, step| Unit::update_actions(unit, action, step, &roles(), slots()),
    )
    .unwrap();
    assert!(captured.is_none());
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_wait_solidify(Some(gone_foundation()));
    let captured = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, action, step| Unit::update_actions(unit, action, step, &roles(), slots()),
    )
    .unwrap();
    assert!(captured.is_none());
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .wait_solidify(),
        None
    );
    world
        .gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    let before = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .gather()
        .copied();
    let captured = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, action, step| Unit::update_actions(unit, action, step, &roles(), slots()),
    )
    .unwrap();
    assert!(captured.is_none());
    let after = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .gather()
        .copied();
    assert_eq!(
        after.map(|gather_action| gather_action.target),
        Some(Some(GatherTargetId::new(object_id(600))))
    );
    assert_ne!(before, after);
}

/// A wait for a foundation (state 0) on one the world does not hold.
fn gone_foundation() -> WaitSolidify {
    WaitSolidify {
        state: WaitSolidifyState::Waiting,
        task: Task::default(),
        target: FoundationId::new(object_id(99_999)),
    }
}

/// The list's update (`FUN_140a992b0`) runs its head's update, then the idle update
/// (`FUN_140a78470`) of each action below it, which is not ported: a wait for a gone foundation
/// on top of a Gather stops there, before its deletion.
#[test]
fn the_list_update_stops_at_an_action_below_its_head() {
    let mut world = forage_world();
    world
        .gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    world
        .parts
        .actions
        .of_mut(unit.id)
        .unwrap()
        .list
        .set_wait_solidify(Some(gone_foundation()));
    assert_eq!(
        world.parts.actions_of(unit.id).top_action_kind(),
        Some(WAIT_SOLIDIFY)
    );
    let err = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, action, step| Unit::update_actions(unit, action, step, &roles(), slots()),
    )
    .unwrap_err();
    assert_eq!(err.kind(), crate::ErrorKind::Unreachable);
    assert_eq!(
        err.reason(),
        "an action below the head of the list (its idle update, FUN_140a78470)"
    );
    assert!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .wait_solidify()
            .is_some()
    );
}

/// An object inside another runs its inside update (`FUN_140a98790`) in place of its actions':
/// its Gather goes, with no walk; an empty list in state 2 starts the master's default task,
/// which waits for the next update.
#[test]
fn an_object_inside_another_runs_its_inside_update() {
    let mut world = forage_world();
    world
        .gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    let at = world.players[1].objects[ListIndex::new(0)].position();
    world.players[1].objects[ListIndex::new(0)].life.inside =
        Some(ContainerId::new(object_id(700)));
    assert_eq!(
        in_update(
            &mut world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, action, step| Unit::update_actions(unit, action, step, &roles(), slots())
        ),
        Ok(None)
    );
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert!(world.parts.actions_of(unit.id).list.is_empty());
    assert_eq!(unit.position(), at);
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    unit.life.inside = None;
    world.parts.actions.of_mut(unit.id).unwrap().default_task = DefaultTask::Artifact;
    assert_eq!(
        in_update(
            &mut world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, action, step| Unit::update_actions(unit, action, step, &roles(), slots())
        ),
        Ok(None)
    );
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .artifact(),
        Some(&Artifact::new(PlayerId::new(1), None)),
        "the Artifact the default task made has not run yet"
    );
}

/// Slot 0x190 (`FUN_14106d090`, `FUN_140a78450`): the object the top action targets, none for an
/// action without one (a wait for a foundation) or without an action, and an error for an action the slice does not hold.
#[test]
fn the_top_action_object_is_its_target_unless_it_has_none() {
    use crate::construction::build::BuildTask;
    let task = BuildTask {
        range: Distance::new(0.1),
        work_value: WorkValue::new(1.0),
        work_rate: WorkRate::new(1.0),
        resource: None,
        id: TaskId::of_task(-1),
        key: task_key(ActionKind::BUILD),
        sprites: TaskSprites::NONE,
    };
    let building = Action {
        list: ActionList::default().with_build(Some(started(
            object_id(800),
            Position::new(1.0, 2.0, 0.0),
            Distance::ZERO,
            task,
        ))),
        ..Default::default()
    };
    assert_eq!(building.top_action_object(None), Ok(Some(object_id(800))));
    // a wait for a foundation keeps it at +0x68, its +0x30 is −1
    let waiting = Action {
        list: ActionList::default().with_wait_solidify(Some(WaitSolidify::new(
            Task::default(),
            FoundationId::new(object_id(900)),
        ))),
        ..Default::default()
    };
    assert_eq!(waiting.top_action_object(None), Ok(None));
    assert_eq!(Action::default().top_action_object(None), Ok(None));
    // an object the load could not read whole: its top action is unknown
    let unread = Unread::new(GenieError::unsupported("an action"));
    assert!(building.top_action_object(Some(&unread)).is_err());
}

/// The top action's sub-action: a Hunt's `MoveTo` with its state number, else its Attack; a
/// Build's `MoveTo`; none for a `MoveTo` of its own or without an action.
#[test]
fn the_top_action_sub_action_is_its_move_to_or_its_attack() {
    use crate::movement::move_to::{MOVE_TO, MoveTo, MoveToAction};
    use crate::world::checkpoint::TopAction;
    let walking = |state| TopAction {
        kind: MOVE_TO,
        state,
    };
    let mut hunt = hunting(7);
    hunt.state = HuntState::Walk {
        move_to: Some(MoveToAction::sub_in(MoveTo::Arrived)),
    };
    let hunter = Action {
        list: ActionList::default().with_hunt(Some(hunt)),
        ..Default::default()
    };
    assert_eq!(hunter.top_sub_action(), Some(walking(ActionState::new(1))));
    let mut idle = hunt;
    idle.state = idle.state.bare();
    let resting = Action {
        list: ActionList::default().with_hunt(Some(idle)),
        ..Default::default()
    };
    assert_eq!(resting.top_sub_action(), None);
    let mut build = started(
        object_id(800),
        Position::new(1.0, 2.0, 0.0),
        Distance::ZERO,
        BuildTask {
            range: Distance::new(0.1),
            work_value: WorkValue::new(1.0),
            work_rate: WorkRate::new(1.0),
            resource: None,
            id: TaskId::of_task(-1),
            key: task_key(ActionKind::BUILD),
            sprites: TaskSprites::NONE,
        },
    );
    build.state = BuildState::Move {
        move_to: Some(MoveToAction::sub_in(MoveTo::Moving)),
    };
    let builder = Action {
        list: ActionList::default().with_build(Some(build)),
        ..Default::default()
    };
    assert_eq!(builder.top_sub_action(), Some(walking(ActionState::new(4))));
    assert_eq!(Action::default().top_sub_action(), None);
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
        let mut layer = LayersAction::default();
        layer.list.put(action);
        layer.top_sub_action()
    };
    let mut artifact = Artifact::new(PlayerId::new(1), None);
    artifact.sub = Some(ArtifactSub::Walk {
        move_to: MoveToAction::sub_in(MoveTo::Moving),
    });
    assert_eq!(of(Action::Artifact(artifact)), walking);
    let mut enter = EnterAction::new(
        ContainerId::new(object_id(7)),
        Position::new(1.0, 2.0, 0.0),
        &enter_task(Distance::new(0.5), NO_CLASS),
    );
    artifact.sub = None;
    artifact.sub = Some(ArtifactSub::Enter { enter });
    assert_eq!(
        of(Action::Artifact(artifact)),
        Some(TopAction {
            kind: ENTER,
            state: ActionState::new(enter.state.get()),
        })
    );
    enter = enter.walking(MoveTo::Moving);
    assert_eq!(of(Action::Enter(enter)), walking);
    let mut fly = Fly::start(
        Task::default(),
        Position::new(1.0, 2.0, 0.0),
        &mut Well512::default(),
    );
    fly.state = FlyState::Flying {
        move_to: Some(MoveToAction::sub_in(MoveTo::Moving)),
    };
    assert_eq!(of(Action::Fly(fly)), walking);
    let mut attack = own_attack(7, AttackState::Move { move_to: None }, Distance::new(0.5));
    *attack.state.walk_mut().unwrap() = Some(MoveToAction::sub_in(MoveTo::Moving));
    assert_eq!(of(Action::Attack(attack)), walking);
    let (mut explore, _) = ExploreAction::new(
        Position::new(1.0, 2.0, 0.0),
        &bare_walk(),
        &tuning().move_to,
    )
    .unwrap();
    if let Some(move_to_action) = explore.state.move_to_mut() {
        move_to_action.state = MoveTo::Moving;
    }
    assert_eq!(of(Action::Explore(explore)), walking);
    let mut transport = TransportAction::new(Position::new(1.0, 2.0, 0.0));
    transport.state = TransportState::Sail {
        move_to: Some(MoveToAction::sub_in(MoveTo::Moving)),
    };
    assert_eq!(of(Action::Transport(transport)), walking);
    let wait = WaitSolidify {
        state: WaitSolidifyState::Waiting,
        task: Task::default(),
        target: FoundationId::new(object_id(7)),
    };
    assert_eq!(of(Action::WaitSolidify(wait)), None);
}

/// A unit's master: its own when it has one (a captured unit's copy), else its player's of its
/// id; an error with neither.
#[test]
fn unit_reads_its_own_master_before_its_player_s() {
    use crate::objects::masters::{Master, MasterTables, Masters};
    use crate::world::parts::PartTable;
    let shared = Master {
        id: master_id(4),
        hit_points: MasterHitPoints::of_data(25),
        ..Master::default()
    };
    let every = vec![
        Arc::new(Masters::default()),
        Arc::new(Masters::new([shared])),
    ];
    let unit = Unit {
        id: object_id(7),
        life: Lifecycle {
            master_id: master_id(4),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let none = PartTable::default();
    let tables = MasterTables::new(&every, &none);
    assert_eq!(
        unit.master(tables, PlayerId::new(1))
            .map(|master| master.hit_points),
        Ok(MasterHitPoints::of_data(25))
    );
    let own: PartTable<Master> = once((
        object_id(7),
        Master {
            id: master_id(4),
            hit_points: MasterHitPoints::of_data(40),
            ..Master::default()
        },
    ))
    .collect();
    let with_own = MasterTables::new(&every, &own);
    assert_eq!(
        unit.master(with_own, PlayerId::new(1))
            .map(|master| master.hit_points),
        Ok(MasterHitPoints::of_data(40))
    );
    assert_eq!(
        unit.master(with_own, PlayerId::new(0))
            .map(|master| master.hit_points),
        Ok(MasterHitPoints::of_data(40))
    );
    let missing = unit
        .master(tables, PlayerId::new(0))
        .expect_err("player 0 has no master 4");
    assert_eq!(missing, OBJECT_MASTER);
    assert!(
        unit.master(tables, PlayerId::new(2)).is_err(),
        "no table for player 2"
    );
}

/// A reader of only the object's spatial record ([`MasterTables::of_placed`]): the object's
/// own master first (a captured target's copy of its old owner's), else the record's player's
/// master of the record's id; an error with neither.
#[test]
fn placed_object_reads_its_own_master_before_its_record_s() {
    use crate::map::Footprint;
    use crate::map::spatial::Placed;
    use crate::objects::masters::BuildableOn;
    use crate::objects::masters::ObstructionKind;
    use crate::objects::masters::{Master, MasterTables, Masters};
    use crate::world::parts::PartTable;
    let shared = |hit_points| Master {
        id: master_id(4),
        hit_points: MasterHitPoints::of_data(hit_points),
        ..Master::default()
    };
    let every = vec![
        Arc::new(Masters::new([shared(10)])),
        Arc::new(Masters::new([shared(25)])),
    ];
    let placed = Placed {
        position: Position::new(1.0, 2.0, 0.0),
        radius: Footprint::new(0.5, 0.5),
        class: NO_CLASS,
        player: PlayerId::new(1),
        master: master_id(4),
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: false,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    let none = PartTable::default();
    let hit_points = |tables: MasterTables<'_>, placed: &Placed| {
        tables
            .of_placed(object_id(7), placed)
            .map(|master| master.hit_points)
    };
    assert_eq!(
        hit_points(MasterTables::new(&every, &none), &placed),
        Ok(MasterHitPoints::of_data(25))
    );
    let own: PartTable<Master> = once((object_id(7), shared(40))).collect();
    assert_eq!(
        hit_points(MasterTables::new(&every, &own), &placed),
        Ok(MasterHitPoints::of_data(40))
    );
    let elsewhere = Placed {
        player: PlayerId::new(2),
        ..placed
    };
    assert_eq!(
        hit_points(MasterTables::new(&every, &none), &elsewhere)
            .map_err(|error| error == OBJECT_MASTER),
        Err(true)
    );
}

/// The walk's view of the unit itself ([`Unit::me`]): its id, its top action's state (none
/// without one), its group and its last human order's time, which ranks it against a met unit.
#[test]
fn unit_own_view_for_its_walk() {
    let mut unit = Unit {
        id: object_id(5),
        last_human_order: WorldTime::from_millis(2001),
        ..Unit::default()
    };
    unit.group.id = Some(group_id(4));
    assert_eq!(
        unit.me(&Action::default(), TopSpeed::Own),
        Me {
            id: object_id(5),
            action_state: ActionState::NO_ACTION,
            group: Some(group_id(4)),
            human_order: WorldTime::from_millis(2001),
            top_speed: TopSpeed::Own,
        }
    );
}

/// The idle statistics of `RGE_Action_Object::update` (from `0x14106b11d`): a villager that may
/// be interrupted in state 2 begins an idle once (its count one more, its start the world's
/// time) and keeps it; past state 2 the idle ends into its milliseconds, its owner's
/// attribute 250 takes the seconds and 249 the count, and the fields go to 0; the update after
/// adds nothing. Another class keeps none.
#[test]
fn villager_idle_time_goes_to_its_owner_past_its_life() {
    let mut world = forage_world();
    world.players[1].attributes.resize(300, Amount::ZERO);
    let me = (PlayerId::new(1), ListIndex::new(0));
    let id = world.players[1].objects[ListIndex::new(0)].id;
    let step = |world: &mut World, at: u32| {
        world.world_time = WorldTime::from_millis(at);
        in_update(world, me, 50, |unit, action, step| {
            unit.action_update_end(action, step)
        })
        .unwrap();
    };
    let begun = IdleTime {
        since: WorldTime::from_millis(1000),
        idle: Millis::ZERO,
        count: IdleCount::ZERO.next(),
    };
    step(&mut world, 1000);
    assert_eq!(world.parts.actions_of(id).idle, begun);
    step(&mut world, 1400);
    assert_eq!(world.parts.actions_of(id).idle, begun);
    let statistics = |world: &World| {
        let attributes = &world.players[1].attributes;
        (attributes[249].get(), attributes[250].get())
    };
    let (count, seconds) = statistics(&world);
    world.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Dying;
    step(&mut world, 1800);
    assert_eq!(world.parts.actions_of(id).idle, IdleTime::default());
    assert_eq!(statistics(&world), (count + 1.0, seconds + 0.8));
    step(&mut world, 2000);
    assert_eq!(statistics(&world), (count + 1.0, seconds + 0.8));
    let mut world = forage_world();
    let master = world.players[1].objects[ListIndex::new(0)].life.master_id;
    for masters in &mut world.model.masters {
        if let Some(master) = Arc::make_mut(masters).get_mut(master) {
            master.class = NO_CLASS;
        }
    }
    step(&mut world, 1000);
    assert_eq!(world.parts.actions_of(id).idle, IdleTime::default());
}

/// The speed-up's check of `RGE_Action_Object::update` (`0x14106b34f`): a building the world
/// holds whose squared footprint gap is below the square of the owner's attribute 256 stays;
/// out of range, or gone, the building is none and the value 0.
#[test]
fn speedup_building_stays_only_within_its_range() {
    let mut world = forage_world();
    world.players[1].attributes.resize(300, Amount::ZERO);
    let me = (PlayerId::new(1), ListIndex::new(0));
    let id = world.players[1].objects[ListIndex::new(0)].id;
    let held = Speedup {
        building: Some(object_id(600)),
        value: SpeedupValue::of_data(1.5),
    };
    let check = |world: &mut World, speedup: Speedup, range: f32| {
        world.players[1].attributes[256] = Amount::new(range);
        world.parts.actions.of_mut(id).unwrap().speedup = speedup;
        in_update(world, me, 50, |unit, action, step| {
            unit.action_update_end(action, step)
        })
        .unwrap();
        world.parts.actions_of(id).speedup
    };
    assert_eq!(check(&mut world, held, 1000.0), held);
    assert_eq!(check(&mut world, held, 0.0), Speedup::NONE);
    let gone = Speedup {
        building: Some(object_id(9999)),
        ..held
    };
    assert_eq!(check(&mut world, gone, 1000.0), Speedup::NONE);
}
