//! Each saved top action the slice imports, and the saved actions it refuses.

use crate::combat::attack::ATTACK;
use crate::combat::missile::ShooterId;
use crate::combat::missile::{HitMode, MISSILE, MissileState};
use crate::construction::FoundationId;
use crate::construction::build::BuildState;
use crate::construction::build::BuildTask;
use crate::garrison::transport::TransportMode;
use crate::gathering::DropSiteId;
use crate::gathering::FarmStops;
use crate::map::Distance;
use crate::map::Position;
use crate::movement::explore::EXPLORE;
use crate::movement::move_to::MoveWaitState;
use crate::movement::move_to::{Goal, MOVE_TO, MoveTo};
use crate::objects::FacetCount;
use crate::objects::Frame;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::Voice;
use crate::objects::WorkRate;
use crate::objects::layers;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::SoundEvents;
use crate::objects::masters::TargetOwner;
use crate::objects::masters::Task;
use crate::objects::masters::TaskFlags;
use crate::objects::masters::TaskId;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::TaskTerrain;
use crate::objects::masters::WorkValue;
use crate::objects::sprite::Graphic;
use crate::objects::{Graphics, SpeedMultiplier};
use crate::ownership::artifact::{ARTIFACT_KIND, ArtifactState};
use crate::test_ids::action_kind;
use crate::test_ids::class_id;
use crate::test_ids::object_id;
use crate::testing::attribute;
use crate::world::Seconds;
use crate::world::load::fixtures::{
    action_list, combat_master, combat_mut, combat_object, missile_tuning, with_masters,
};
use crate::world::load::object::OTHER_SUB_ACTION;
use crate::world::load::object::saved_missile;
use crate::world::load::top_actions::saved_actions;
use crate::world::load::top_actions::{saved_move_to, saved_sub_move_to};
use crate::{GenieError, GenieResult};
use genie_save::generated::world::{
    Action, PlayerObjects, RgeAction, RgeActionAttack, RgeActionByKind, RgeActionEnter,
    RgeActionGather, RgeActionMake, RgeActionMissile, RgeActionMoveTo, RgeMissileObject, RgeObject,
    TribeActionArtifact, TribeActionBuild, TribeActionHunt, TribeActionRepair,
    TribeActionWaitSolidify,
};

/// The task of slot `id`, its work range `range`.
fn task(id: i16, range: f32) -> Task {
    Task {
        id: TaskId::of_task(id),
        action_kind: action_kind(0),
        object_group: Some(class_id(3)),
        object_id: None,
        combat_level: 0,
        combat_level_flag: TaskFlags::NONE,
        terrain: TaskTerrain::NONE,
        owner_kind: TargetOwner::ANY,
        holding_attr: 0,
        attribute_kind: Some(attribute::WOOD),
        attribute_kind2: Some(attribute::STONE),
        attribute_kind3: Some(attribute::GOLD),
        attribute_kind4: Some(attribute::HEADROOM),
        wait_time: Seconds::ZERO,
        work_val1: WorkValue::new(0.5),
        work_val2: WorkValue::ZERO,
        work_range: Distance::new(range),
        work_flag: 0,
        enablement: Some(attribute::FOOD),
        sprites: TaskSprites::NONE,
        sound_events: SoundEvents::new(-1, -1),
    }
}

/// An action's base in state `state`: no targets, its point (1, 2, 0), its timer 1.5.
fn base(state: u8) -> RgeAction {
    RgeAction {
        state,
        target_id: -1,
        target2_id: -1,
        target_x: 1.0,
        target_y: 2.0,
        timer: 1.5,
        ..RgeAction::default()
    }
}

/// An action of the base factory's classes, its part set by `set`.
fn by_kind(set: impl FnOnce(&mut RgeActionByKind)) -> Action {
    let mut parts = RgeActionByKind::default();
    set(&mut parts);
    Action {
        rge_action_by_kind: Some(Box::new(parts)),
        ..Action::default()
    }
}

/// An Explore (its base alone) of `b`.
fn explore(b: RgeAction) -> Action {
    by_kind(|t| t.rge_action_explore = Some(Box::new(b)))
}

/// A `MoveTo` in state `state` to its base's point, its range `range`, asked again after its
/// wait (wait state 2, timer 0.25, delay 0.5).
fn move_to(state: u8, range: f32) -> Action {
    by_kind(|t| {
        t.rge_action_move_to = Some(Box::new(RgeActionMoveTo {
            // no sprite (`+0x58` null)
            rge_action: RgeAction {
                short_val_2: -1,
                ..base(state)
            },
            range,
            movement_wait_state: 2,
            timer: 0.25,
            default_wait_time: 0.5_f32.to_bits(),
        }));
    })
}

/// `a`, a [`move_to`], quiet (`+0x60` 1): the sub-action `MoveTo` an action makes.
fn quiet(mut a: Action) -> Action {
    if let Some(m) = a
        .rge_action_by_kind
        .as_mut()
        .and_then(|t| t.rge_action_move_to.as_mut())
    {
        m.rge_action.sub_action_value = 1;
    }
    a
}

/// `b` with the sub-action `sub` of type `kind`.
fn with_sub(mut b: RgeAction, kind: ActionKind, sub: Action) -> RgeAction {
    b.sub_actions = action_list(vec![(kind, sub)]);
    b
}

/// Unit 5 (master 83) acting `action` of type `kind`.
fn acting(kind: ActionKind, action: Action) -> genie_save::Object {
    let mut object = combat_object(5, 83);
    combat_mut(&mut object)
        .rge_combat_object
        .rge_action_object
        .actions = action_list(vec![(kind, action)]);
    object
}

/// `object`'s top action imported into an action layer: each task slot's task of range 2, the
/// fly task 8, the build task 7 (range 1, work rate 2, moving sprite 9), no sprites' table.
fn import(object: &genie_save::Object, objects: &PlayerObjects) -> GenieResult<layers::Action> {
    import_drawn(object, objects, None)
}

/// [`import`] with the sprites' table `graphics`.
fn import_drawn(
    object: &genie_save::Object,
    objects: &PlayerObjects,
    graphics: Option<&Graphics>,
) -> GenieResult<layers::Action> {
    let mut actions = layers::Action::default();
    let s = object.static_part().expect("a test object's static part");
    saved_actions(
        object,
        s,
        objects,
        |_| {
            Ok(BuildTask {
                range: Distance::new(1.0),
                work_value: WorkValue::new(0.5),
                work_rate: WorkRate::new(2.0),
                resource: None,
                id: TaskId::of_task(7),
                moving: Some(SpriteId::named(9)),
            })
        },
        |_| Ok(task(8, 3.0)),
        |_, slot| Ok(task(slot, 2.0)),
        graphics,
        &mut actions,
    )?;
    Ok(actions)
}

/// Unit 5's top action `action` of type `kind` imported, its player without masters.
fn imported(kind: ActionKind, action: Action) -> GenieResult<layers::Action> {
    import(&acting(kind, action), &PlayerObjects::default())
}

fn bits3(v: Position) -> [u32; 3] {
    v.get().map(f32::to_bits)
}

#[test]
fn an_action_without_its_base_is_unsupported() {
    assert_eq!(
        imported(EXPLORE, Action::default()).unwrap_err(),
        GenieError::unsupported("a saved action without its base")
    );
}

/// The second target only where the slice models one; an action the slice does not import is
/// refused.
#[test]
fn second_target_or_an_unknown_action_is_unsupported() {
    let mut b = base(3);
    b.target2_id = 9;
    assert_eq!(
        imported(EXPLORE, explore(b)).unwrap_err(),
        GenieError::unsupported("a saved action with a second target")
    );
    assert_eq!(
        imported(action_kind(99), explore(base(0))).unwrap_err(),
        GenieError::unsupported("a saved action the slice does not import")
    );
}

/// A saved list of two actions: the slice's lists hold one (the restart of the action under a
/// deleted head, slot 0x28, is not ported), so the import refuses the action below the head
/// rather than dropping it, and names it.
#[test]
fn second_saved_action_is_refused() {
    let mut object = acting(MOVE_TO, move_to(2, 0.5));
    combat_mut(&mut object)
        .rge_combat_object
        .rge_action_object
        .actions = action_list(vec![
        (MOVE_TO, move_to(2, 0.5)),
        (EXPLORE, explore(base(0))),
    ]);
    let err = import(&object, &PlayerObjects::default()).unwrap_err();
    assert_eq!(
        err,
        GenieError::unsupported("a saved action the slice does not import")
    );
    assert_eq!(
        err.context().collect::<Vec<_>>(),
        [("below the head", EXPLORE.to_string().as_str())]
    );
}

#[test]
fn move_to_takes_its_state_point_range_and_wait() {
    let m = imported(MOVE_TO, move_to(4, 0.5))
        .unwrap()
        .list
        .move_to()
        .copied()
        .unwrap();
    assert_eq!(m.state, MoveTo::Moving);
    assert_eq!(
        (m.target, bits3(m.point)),
        (None, bits3(Position::new(1.0, 2.0, 0.0)))
    );
    assert_eq!(
        (m.target, m.range.get().to_bits(), m.sprite),
        (None, 0.5_f32.to_bits(), None)
    );
    assert_eq!(
        (
            m.wait.state,
            m.wait.timer.get().to_bits(),
            m.wait.delay.get().to_bits()
        ),
        (
            MoveWaitState::AskedAgain,
            0.25_f32.to_bits(),
            0.5_f32.to_bits()
        )
    );
    for (state, saved) in [
        (1, MoveTo::Arrived),
        (2, MoveTo::Waiting),
        (3, MoveTo::Start),
        (13, MoveTo::Failed),
    ] {
        let m = imported(MOVE_TO, move_to(state, 0.5))
            .unwrap()
            .list
            .move_to()
            .copied()
            .unwrap();
        assert_eq!(m.state, saved);
    }
    assert_eq!(
        imported(MOVE_TO, move_to(7, 0.5)).unwrap_err(),
        GenieError::unsupported("a saved MoveTo in a state the slice does not model")
    );
    assert_eq!(
        imported(MOVE_TO, explore(base(4))).unwrap_err(),
        GenieError::unsupported("a saved MoveTo without its range")
    );
}

/// A `MoveTo`'s wait state (`+0x6c`) is 0, 1 or 2; a saved 3 fails the read.
/// A saved `MoveTo`'s voice is the save's `+0x60` (`FUN_140a7a2f0`): 0 is loud, any other byte
/// quiet. An action's sub-action `MoveTo` to an object that is not quiet, which the game never
/// makes, is refused; one to a point may be loud (the Fly's, the Artifact's walk to a point).
#[test]
fn saved_move_to_takes_its_voice_from_the_save() {
    let voice = |a: &Action| saved_move_to(a, None).map(|m| m.voice);
    assert_eq!(voice(&move_to(4, 0.5)), Ok(Voice::Loud));
    assert_eq!(voice(&quiet(move_to(4, 0.5))), Ok(Voice::Quiet));
    let to_object = |a: Action| {
        let mut a = a;
        if let Some(m) = a
            .rge_action_by_kind
            .as_mut()
            .and_then(|t| t.rge_action_move_to.as_mut())
        {
            m.rge_action.target_id = 9;
        }
        a
    };
    let loud = saved_sub_move_to(&to_object(move_to(4, 0.5)), None).unwrap_err();
    assert_eq!(
        (loud.kind(), loud.reason()),
        (
            crate::ErrorKind::InvalidInput,
            "a saved sub-action MoveTo to an object that is not quiet"
        )
    );
    let held = saved_sub_move_to(&to_object(quiet(move_to(4, 0.5))), None).unwrap();
    assert_eq!(
        (held.target, held.voice),
        (Some(object_id(9)), Voice::Quiet)
    );
    let to_point = saved_sub_move_to(&move_to(4, 0.5), None).unwrap();
    assert_eq!((to_point.target, to_point.voice), (None, Voice::Loud));
}

#[test]
fn move_to_wait_state_of_none_fails() {
    let mut walk = move_to(4, 0.5);
    if let Some(m) = walk
        .rge_action_by_kind
        .as_deref_mut()
        .and_then(|t| t.rge_action_move_to.as_deref_mut())
    {
        m.movement_wait_state = 3;
    }
    assert!(imported(MOVE_TO, walk).is_err());
}

/// A `MoveTo`'s sprite (`+0x58`, the save's second `short_val`) comes from the sprites'
/// table; −1 is none, and an id the table lacks (or no table) is missing data.
#[test]
fn move_to_takes_its_sprite_from_the_sprites_table() {
    let walking = Graphic {
        id: SpriteId::named(5),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(16),
        speed_multiplier: SpeedMultiplier::of_data(1.5),
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let graphics = Graphics::from([(walking.id, walking)]);
    let mut action = move_to(4, 0.5);
    let mut shown = |sprite: i16, graphics| {
        let parts = action.rge_action_by_kind.as_deref_mut().unwrap();
        parts
            .rge_action_move_to
            .as_deref_mut()
            .unwrap()
            .rge_action
            .short_val_2 = sprite;
        let object = acting(MOVE_TO, action.clone());
        import_drawn(&object, &PlayerObjects::default(), graphics)
            .map(|a| a.list.move_to().copied().unwrap().sprite)
    };
    assert_eq!(shown(5, Some(&graphics)), Ok(Some(walking)));
    assert_eq!(shown(-1, None), Ok(None));
    let lacking = shown(4, Some(&graphics)).unwrap_err();
    assert_eq!(
        (lacking.kind(), lacking.reason()),
        (
            crate::ErrorKind::MissingData,
            "a saved MoveTo's sprite outside the sprites' table"
        )
    );
    assert!(shown(5, None).is_err());
}

fn artifact(b: RgeAction) -> Action {
    Action {
        tribe_action_artifact: Some(Box::new(TribeActionArtifact {
            rge_action: b,
            automove_livestock: 0,
        })),
        ..Action::default()
    }
}

/// An Artifact waits, or walks to its target: its `MoveTo`'s goal, a point or an object, with
/// its range, quiet toward an object; another sub-action is refused. A walking Artifact checks
/// (state 6): its `set_state`'s walks (4 and 5, `FUN_140aace30`) add the `MoveTo` and pass on
/// to 6.
#[test]
fn an_artifact_waits_or_walks_to_its_target() {
    let a = imported(ARTIFACT_KIND, artifact(base(ArtifactState::Wait.get())))
        .unwrap()
        .list
        .artifact()
        .copied()
        .unwrap();
    assert_eq!(
        (a.state, a.target, a.move_to().copied(), a.goal),
        (ArtifactState::Wait, None, None, None)
    );
    assert!(a.gaia);
    let mut b = base(ArtifactState::Check.get());
    b.target_id = 9;
    b.target_x = 4.0;
    let a = imported(
        ARTIFACT_KIND,
        artifact(with_sub(b.clone(), MOVE_TO, quiet(move_to(4, 0.5)))),
    )
    .unwrap()
    .list
    .artifact()
    .copied()
    .unwrap();
    let walk = a.move_to().copied().unwrap();
    assert_eq!((walk.state, walk.voice), (MoveTo::Moving, Voice::Quiet));
    assert_eq!(a.goal.map(bits3), Some(bits3(Position::new(4.0, 2.0, 0.0))));
    assert!(
        matches!(walk.walks_to(), Goal::Point(p) if bits3(p) == bits3(Position::new(1.0, 2.0, 0.0)))
    );
    assert_eq!(walk.range.get().to_bits(), 0.5_f32.to_bits());
    let mut to_object = quiet(move_to(4, 0.5));
    to_object
        .rge_action_by_kind
        .as_deref_mut()
        .unwrap()
        .rge_action_move_to
        .as_deref_mut()
        .unwrap()
        .rge_action
        .target_id = 9;
    let a = imported(
        ARTIFACT_KIND,
        artifact(with_sub(b.clone(), MOVE_TO, to_object)),
    )
    .unwrap()
    .list
    .artifact()
    .copied()
    .unwrap();
    assert_eq!(
        a.move_to().copied().map(|m| m.walks_to()),
        Some(Goal::Target(object_id(9)))
    );
    // the target's place is its goal however near the origin; an unset place (-1) is none
    let mut near = base(ArtifactState::Check.get());
    near.target_id = 9;
    let a = imported(ARTIFACT_KIND, artifact(near.clone()))
        .unwrap()
        .list
        .artifact()
        .copied()
        .unwrap();
    assert_eq!(a.goal.map(bits3), Some(bits3(Position::new(1.0, 2.0, 0.0))));
    near.target_x = -1.0;
    let a = imported(ARTIFACT_KIND, artifact(near))
        .unwrap()
        .list
        .artifact()
        .copied()
        .unwrap();
    assert_eq!(a.goal, None);
    // without a target the goal is the MoveTo's point, the action's own
    let a = imported(
        ARTIFACT_KIND,
        artifact(with_sub(
            base(ArtifactState::Check.get()),
            MOVE_TO,
            move_to(4, 0.5),
        )),
    )
    .unwrap()
    .list
    .artifact()
    .copied()
    .unwrap();
    assert_eq!(a.goal.map(bits3), Some(bits3(Position::new(1.0, 2.0, 0.0))));
    // a move to the point: its MoveTo is not quiet, its AI hears the end
    assert_eq!(a.move_to().copied().map(|m| m.voice), Some(Voice::Loud));
    assert_eq!(
        imported(
            ARTIFACT_KIND,
            artifact(with_sub(b.clone(), MOVE_TO, explore(base(4))))
        )
        .unwrap_err(),
        GenieError::unsupported("a saved MoveTo without its range")
    );
    assert_eq!(
        imported(
            ARTIFACT_KIND,
            artifact(with_sub(b, EXPLORE, explore(base(3))))
        )
        .unwrap_err(),
        GenieError::unsupported("a saved Artifact action with another sub-action")
    );
}

/// A missile of unit 5's place, flying 5 from its shooter 900 (its second target).
fn missile() -> genie_save::Object {
    missile_of(5.0, 0)
}

/// [`missile`] `distance` from its point, a charge projectile for `charge` 1.
fn missile_of(distance: f32, charge: u8) -> genie_save::Object {
    let mut b = base(MissileState::Flying.get());
    b.target2_id = 900;
    let action = by_kind(|t| {
        t.rge_action_missile = Some(Box::new(RgeActionMissile {
            rge_action: b,
            distance,
            ballistic_velocity: 3.0,
            ..RgeActionMissile::default()
        }));
    });
    let mut flying = RgeMissileObject {
        max_range: 7.0,
        fired_from_id: 900,
        is_charge_projectile: charge,
        ..RgeMissileObject::default()
    };
    flying.rge_combat_object.rge_action_object.actions = action_list(vec![(MISSILE, action)]);
    flying
        .rge_combat_object
        .rge_action_object
        .rge_moving_object
        .rge_animated_object
        .rge_static_object
        .id = 6;
    genie_save::Object {
        rge_object: Some(Box::new(RgeObject {
            rge_missile_object: Some(Box::new(flying)),
            ..RgeObject::default()
        })),
        ..genie_save::Object::default()
    }
}

#[test]
fn missile_flies_with_its_actions_course() {
    assert!(import(&missile(), &PlayerObjects::default()).is_ok());
    let tuning = missile_tuning();
    let m = saved_missile(&missile(), &tuning).unwrap();
    assert_eq!(
        (m.fired_from, m.max_range.get().to_bits()),
        (Some(ShooterId::new(object_id(900))), 7.0_f32.to_bits())
    );
    assert!(m.flying && !m.charge);
    // a charge projectile; one at the landing distance has landed
    let landing = tuning.landing;
    let m = saved_missile(&missile_of(landing, 1), &tuning).unwrap();
    assert!(m.charge && !m.flying);
    let action = import(&missile(), &PlayerObjects::default())
        .unwrap()
        .list
        .missile()
        .cloned()
        .unwrap();
    assert_eq!(
        (action.state, action.fired_from, action.hit_mode),
        (
            MissileState::Flying,
            Some(ShooterId::new(object_id(900))),
            HitMode::TARGET_ALONE
        )
    );
    assert_eq!(action.ballistic_velocity.get().to_bits(), 3.0_f32.to_bits());
    assert_eq!(
        imported(MISSILE, explore(base(2))).unwrap_err(),
        GenieError::unsupported("a saved missile action without its part")
    );
    let not_missile = by_kind(|t| t.rge_action_missile = Some(Box::default()));
    assert_eq!(
        imported(MISSILE, not_missile).unwrap_err(),
        GenieError::unsupported("a missile action of an object that is not a missile")
    );
}

#[test]
fn farms_make_takes_its_timers_and_task() {
    use crate::gathering::make::{MAKE, MakeState};
    let make = |b: RgeAction| {
        by_kind(|t| {
            t.rge_action_make = Some(Box::new(RgeActionMake {
                rge_action: b,
                work_timer: 9.0,
            }));
        })
    };
    let mut b = base(1);
    b.short_val = 3;
    let m = imported(MAKE, make(b))
        .unwrap()
        .list
        .make()
        .copied()
        .unwrap();
    assert_eq!((m.state, m.task.id), (MakeState::Done, TaskId::of_task(3)));
    assert_eq!(
        (m.remaining.get().to_bits(), m.total.get().to_bits()),
        (1.5_f32.to_bits(), 9.0_f32.to_bits())
    );
    assert_eq!(
        imported(
            MAKE,
            make(with_sub(base(1), MOVE_TO, quiet(move_to(4, 0.5))))
        )
        .unwrap_err(),
        GenieError::unsupported("a saved Make with a sub-action")
    );
    assert_eq!(
        imported(MAKE, explore(base(1))).unwrap_err(),
        GenieError::unsupported("a saved Make without its part")
    );
}

#[test]
fn birds_fly_takes_its_home_and_move_to() {
    use crate::movement::fly::{FLY, FlyState};
    let bird = |b: RgeAction| by_kind(|t| t.rge_action_bird = Some(Box::new(b)));
    let f = imported(FLY, bird(base(3)))
        .unwrap()
        .list
        .fly()
        .copied()
        .unwrap();
    assert_eq!(
        (f.state, f.task.id),
        (FlyState::Choosing, TaskId::of_task(8))
    );
    assert_eq!(
        f.home.get().map(f32::to_bits),
        [1.0_f32, 2.0].map(f32::to_bits)
    );
    let f = imported(FLY, bird(with_sub(base(4), MOVE_TO, move_to(4, 0.5))))
        .unwrap()
        .list
        .fly()
        .copied()
        .unwrap();
    let flight = f.state.move_to().copied().unwrap();
    assert_eq!((flight.state, flight.voice), (MoveTo::Moving, Voice::Loud));
    assert_eq!(
        (flight.target, bits3(flight.point)),
        (None, bits3(Position::new(1.0, 2.0, 0.0)))
    );
    assert_eq!(
        imported(FLY, bird(with_sub(base(3), EXPLORE, explore(base(3))))).unwrap_err(),
        GenieError::unsupported("a saved Fly action with another sub-action")
    );
    // state 3 drops the MoveTo and makes the flight's only as it flies (4)
    assert_eq!(
        imported(FLY, bird(with_sub(base(3), MOVE_TO, move_to(4, 0.5))))
            .unwrap_err()
            .reason(),
        "a MoveTo of a Fly that does not fly"
    );
}

fn build(b: RgeAction) -> Action {
    Action {
        tribe_action_build: Some(Box::new(TribeActionBuild {
            rge_action: b,
            added_to_num_builders: 1,
        })),
        ..Action::default()
    }
}

/// A Build takes its task as the import's task gives it (the builder's powerup is the
/// object's import's, its combat layer's), its quiet `MoveTo` (its range, point, wait and
/// sprite) and its site.
#[test]
fn build_takes_its_task_move_to_and_site() {
    let mut b = base(4);
    b.target_id = 30;
    b.target2_id = 31;
    let objects = PlayerObjects::default();
    let walking = with_sub(b.clone(), MOVE_TO, quiet(move_to(4, 0.75)));
    let a = import(&acting(ActionKind::BUILD, build(walking)), &objects)
        .unwrap()
        .list
        .build()
        .copied()
        .unwrap();
    let walk = a.state.move_to().copied().unwrap();
    assert_eq!(
        (a.target, a.site, walk.target, walk.state, walk.voice),
        (
            Some(FoundationId::new(object_id(30))),
            Some(DropSiteId::new(object_id(31))),
            None,
            MoveTo::Moving,
            Voice::Quiet
        )
    );
    assert!(a.added);
    assert_eq!(a.task.work_rate.get().to_bits(), 2.0_f32.to_bits());
    assert_eq!(walk.range.get().to_bits(), 0.75_f32.to_bits());
    assert_eq!(bits3(walk.point), bits3(Position::new(1.0, 2.0, 0.0)));
    assert_eq!(walk.wait.state, MoveWaitState::AskedAgain);
    // a MoveTo without a sprite (−1) keeps none
    assert_eq!(walk.sprite, None);
    let mut dropping = b.clone();
    dropping.state = BuildState::Drop { move_to: None }.get();
    // the drop-off's MoveTo with the carrying sprite 12 (its `+0x58`)
    let mut carrying = quiet(move_to(4, 0.75));
    carrying
        .rge_action_by_kind
        .as_mut()
        .and_then(|t| t.rge_action_move_to.as_mut())
        .expect("a MoveTo")
        .rge_action
        .short_val_2 = 12;
    let dropping = with_sub(dropping, MOVE_TO, carrying);
    let carried = Graphic {
        id: SpriteId::named(12),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(16),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let graphics = Graphics::from([(carried.id, carried)]);
    let a = import_drawn(
        &acting(ActionKind::BUILD, build(dropping)),
        &objects,
        Some(&graphics),
    )
    .unwrap()
    .list
    .build()
    .copied()
    .unwrap();
    let walk = a.state.move_to().copied().unwrap();
    assert_eq!(walk.range.get().to_bits(), 0.75_f32.to_bits());
    assert_eq!(walk.sprite, Some(carried));
    let a = imported(ActionKind::BUILD, build(b.clone()))
        .unwrap()
        .list
        .build()
        .copied()
        .unwrap();
    assert_eq!(a.state.move_to().copied(), None);
    // without a MoveTo, the task's moving sprite for the next
    assert_eq!(a.task.moving, Some(SpriteId::named(9)));
    assert_eq!(
        imported(
            ActionKind::BUILD,
            build(with_sub(b, EXPLORE, explore(base(3))))
        )
        .unwrap_err(),
        OTHER_SUB_ACTION
    );
}

fn repair(b: RgeAction) -> Action {
    Action {
        tribe_action_repair: Some(Box::new(TribeActionRepair {
            rge_action: b,
            save_target_command_flag: 0,
            added_to_num_builders: 1,
        })),
        ..Action::default()
    }
}

#[test]
fn repair_takes_its_task_and_counts_its_builder() {
    use crate::construction::repair::REPAIR;
    let mut b = base(4);
    b.short_val = 2;
    b.sub_action_value = 1;
    let r = imported(
        REPAIR,
        repair(with_sub(b.clone(), MOVE_TO, quiet(move_to(4, 0.5)))),
    )
    .unwrap()
    .list
    .repair()
    .copied()
    .unwrap();
    let walk = r.state.move_to().copied().unwrap();
    assert_eq!(
        (r.task.id, walk.state, walk.voice, r.range.get().to_bits()),
        (TaskId::of_task(2), MoveTo::Moving, Voice::Quiet, 0)
    );
    assert!(r.counted && r.voice == Voice::Quiet);
    // a MoveTo without a target walks to its point, within its own range
    assert!(
        matches!(walk.walks_to(), Goal::Point(p) if bits3(p) == bits3(Position::new(1.0, 2.0, 0.0)))
    );
    assert_eq!(walk.range.get().to_bits(), 0.5_f32.to_bits());
    let r = imported(REPAIR, repair(b.clone()))
        .unwrap()
        .list
        .repair()
        .copied()
        .unwrap();
    assert_eq!(r.state.move_to().copied(), None);
    assert_eq!(
        imported(REPAIR, repair(with_sub(b, EXPLORE, explore(base(3))))).unwrap_err(),
        OTHER_SUB_ACTION
    );
    assert_eq!(
        imported(REPAIR, explore(base(4))).unwrap_err(),
        GenieError::missing_data("a saved Repair's own part")
    );
}

fn hunt(b: RgeAction) -> Action {
    Action {
        tribe_action_hunt: Some(Box::new(TribeActionHunt {
            rge_action: b,
            target_kind: 4,
            moved_sheep: 1,
            sheep_still_time: 3,
            sheep_start_x: 6.0_f32.to_bits(),
            sheep_start_y: 7.0_f32.to_bits(),
            sheep_drop_site_id: 40,
        })),
        ..Action::default()
    }
}

/// A Hunt takes its task, drop site and lead, its quiet `MoveTo` with its range and wait; its Attack needs
/// the sprites' table; a state or sub-action the slice does not model is refused.
#[test]
fn hunt_takes_its_task_lead_and_move_to() {
    use crate::gathering::hunt::{HUNT, HuntState};
    let h = imported(HUNT, hunt(base(HuntState::Idle.get())))
        .unwrap()
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (h.target_class, h.drop_site, h.task.id),
        (
            Some(ClassId::named(4)),
            Some(DropSiteId::new(object_id(40))),
            TaskId::of_task(0)
        )
    );
    assert!(
        h.lead.moved && h.state.attack().copied().is_none() && h.state.move_to().copied().is_none()
    );
    assert_eq!(
        h.lead.start.get().map(f32::to_bits),
        [6.0_f32, 7.0].map(f32::to_bits)
    );
    let h = imported(
        HUNT,
        hunt(with_sub(
            base(HuntState::Walk { move_to: None }.get()),
            MOVE_TO,
            quiet(move_to(4, 0.75)),
        )),
    )
    .unwrap()
    .list
    .hunt()
    .copied()
    .unwrap();
    // every MoveTo of the Hunt is quiet
    let walk = h.state.move_to().copied().unwrap();
    assert_eq!(
        (
            walk.state,
            walk.voice,
            walk.range.get().to_bits(),
            walk.wait.state
        ),
        (
            MoveTo::Moving,
            Voice::Quiet,
            0.75_f32.to_bits(),
            MoveWaitState::AskedAgain
        )
    );
    let attack = by_kind(|t| {
        t.rge_action_attack = Some(Box::new(RgeActionAttack {
            rge_action: base(4),
            ..RgeActionAttack::default()
        }));
    });
    assert_eq!(
        imported(
            HUNT,
            hunt(with_sub(
                base(HuntState::Attack { attack: None }.get()),
                ATTACK,
                attack
            ))
        )
        .unwrap_err(),
        GenieError::unsupported("a saved Attack without the sprites' table")
    );
    assert_eq!(
        imported(
            HUNT,
            hunt(with_sub(
                base(HuntState::Walk { move_to: None }.get()),
                EXPLORE,
                explore(base(3))
            ))
        )
        .unwrap_err(),
        GenieError::unsupported("a saved Hunt with another sub-action")
    );
    assert_eq!(
        imported(HUNT, hunt(base(99))).unwrap_err(),
        GenieError::unsupported("a saved Hunt in a state the slice does not model")
    );
    // a MoveTo belongs to a walk: in any other state it is no state of the game
    let idle = with_sub(base(HuntState::Idle.get()), MOVE_TO, quiet(move_to(4, 0.5)));
    let stray = imported(HUNT, hunt(idle)).unwrap_err();
    assert_eq!(
        (stray.kind(), stray.reason()),
        (
            crate::ErrorKind::InvalidInput,
            "a MoveTo of a Hunt that does not walk"
        )
    );
    assert_eq!(
        imported(HUNT, explore(base(HuntState::Idle.get()))).unwrap_err(),
        GenieError::unsupported("a saved Hunt without its own part")
    );
}

fn gather(b: RgeAction) -> Action {
    by_kind(|t| {
        t.rge_action_gather = Some(Box::new(RgeActionGather {
            rge_action: b,
            target_kind: 5,
            number_stops: 2,
            farm_x: 3.0,
            ..RgeActionGather::default()
        }));
    })
}

/// A Gather takes its stops and task; a `MoveTo` without a target walks to its point.
#[test]
fn gather_takes_its_stops_and_move_to() {
    use crate::gathering::gather::{GATHER, GatherState};
    let g = imported(GATHER, gather(base(GatherState::Work.get())))
        .unwrap()
        .list
        .gather()
        .copied()
        .unwrap();
    assert_eq!(
        (g.target_class, g.stops, g.task.id),
        (
            Some(ClassId::try_from(5).unwrap()),
            FarmStops::of_data(2),
            TaskId::of_task(0)
        )
    );
    assert!(g.worked && g.state.move_to().copied().is_none());
    assert_eq!(g.farm.x().to_bits(), 3.0_f32.to_bits());
    let g = imported(
        GATHER,
        gather(with_sub(
            base(GatherState::Walk { move_to: None }.get()),
            MOVE_TO,
            quiet(move_to(4, 0.75)),
        )),
    )
    .unwrap()
    .list
    .gather()
    .copied()
    .unwrap();
    assert!(!g.worked);
    // every MoveTo of the Gather is quiet; one without a target walks to its point
    let walk = g.state.move_to().copied().unwrap();
    assert_eq!((walk.state, walk.voice), (MoveTo::Moving, Voice::Quiet));
    assert!(
        matches!(walk.walks_to(), Goal::Point(p) if bits3(p) == bits3(Position::new(1.0, 2.0, 0.0)))
    );
    assert_eq!(
        (walk.range.get().to_bits(), walk.wait.state),
        (0.75_f32.to_bits(), MoveWaitState::AskedAgain)
    );
    assert_eq!(
        imported(
            GATHER,
            gather(with_sub(
                base(GatherState::Walk { move_to: None }.get()),
                EXPLORE,
                explore(base(3))
            ))
        )
        .unwrap_err(),
        GenieError::unsupported("a saved Gather with another sub-action")
    );
    assert_eq!(
        imported(GATHER, gather(base(99))).unwrap_err(),
        GenieError::unsupported("a saved Gather in a state the slice does not model")
    );
    // a MoveTo belongs to a walk: in any other state it is no state of the game
    let working = with_sub(
        base(GatherState::Work.get()),
        MOVE_TO,
        quiet(move_to(4, 0.5)),
    );
    let stray = imported(GATHER, gather(working)).unwrap_err();
    assert_eq!(
        (stray.kind(), stray.reason()),
        (
            crate::ErrorKind::InvalidInput,
            "a MoveTo of a Gather that does not walk"
        )
    );
    assert_eq!(
        imported(GATHER, explore(base(GatherState::Work.get()))).unwrap_err(),
        GenieError::unsupported("a saved Gather without its own part")
    );
}

fn enter(b: RgeAction) -> Action {
    Action {
        rge_action_enter: Some(Box::new(RgeActionEnter {
            rge_action: b,
            first_time_enter: 1,
        })),
        ..Action::default()
    }
}

#[test]
fn an_enter_takes_its_task_and_retry() {
    use crate::garrison::{ENTER, EnterState};
    let walking = EnterState::Walk { move_to: None };
    let mut b = base(walking.get());
    b.short_val = 1;
    let e = imported(
        ENTER,
        enter(with_sub(b.clone(), MOVE_TO, quiet(move_to(4, 0.5)))),
    )
    .unwrap()
    .list
    .enter()
    .copied()
    .unwrap();
    let m = e.state.move_to().copied().unwrap();
    assert_eq!(
        (e.state.bare(), m.state, m.voice, e.task_class),
        (walking, MoveTo::Moving, Voice::Quiet, Some(class_id(3)))
    );
    assert!(e.retry);
    assert_eq!(
        (e.range.get().to_bits(), m.wait.state),
        (2.0_f32.to_bits(), MoveWaitState::AskedAgain)
    );
    assert_eq!(
        imported(ENTER, enter(with_sub(b, EXPLORE, explore(base(3))))).unwrap_err(),
        GenieError::unsupported("a saved Enter with another sub-action")
    );
    assert_eq!(
        imported(ENTER, enter(base(7))).unwrap_err(),
        GenieError::unsupported("a saved Enter in a state the slice does not model")
    );
    // a MoveTo belongs to a walk: in any other state it is no state of the game
    let done = with_sub(
        base(EnterState::Done.get()),
        MOVE_TO,
        quiet(move_to(4, 0.5)),
    );
    let stray = imported(ENTER, enter(done)).unwrap_err();
    assert_eq!(
        (stray.kind(), stray.reason()),
        (
            crate::ErrorKind::InvalidInput,
            "a MoveTo of an Enter that does not walk"
        )
    );
    assert_eq!(
        imported(ENTER, explore(base(EnterState::Done.get()))).unwrap_err(),
        GenieError::unsupported("a saved Enter without its own part")
    );
}

#[test]
fn transport_takes_its_base_alone() {
    use crate::garrison::transport::{TRANSPORT, TransportState};
    let transport = |b: RgeAction| Action {
        rge_action_transport: Some(Box::new(b)),
        ..Action::default()
    };
    let sailing = TransportState::Sail { move_to: None };
    let t = imported(
        TRANSPORT,
        transport(with_sub(
            base(sailing.get()),
            MOVE_TO,
            quiet(move_to(4, 0.5)),
        )),
    )
    .unwrap()
    .list
    .transport()
    .copied()
    .unwrap();
    let m = t.state.move_to().copied().unwrap();
    assert_eq!(
        (t.state.bare(), t.mode, m.state, m.voice),
        (sailing, TransportMode::New, MoveTo::Moving, Voice::Quiet)
    );
    assert!(!t.retarget);
    assert_eq!(
        imported(
            TRANSPORT,
            transport(with_sub(base(2), EXPLORE, explore(base(3))))
        )
        .unwrap_err(),
        GenieError::invalid_input("a saved Transport with another sub-action")
    );
    // a MoveTo belongs to a sail: an idle Transport's is no state of the game
    let idle = imported(
        TRANSPORT,
        transport(with_sub(base(2), MOVE_TO, quiet(move_to(4, 0.5)))),
    )
    .unwrap_err();
    assert_eq!(
        (idle.kind(), idle.reason()),
        (
            crate::ErrorKind::InvalidInput,
            "a MoveTo of a Transport that does not sail"
        )
    );
}

#[test]
fn an_explore_takes_its_quiet_move_to_with_its_wait() {
    use crate::movement::explore::{EXPLORE, ExploreState};
    let e = imported(EXPLORE, explore(base(ExploreState::Wait.get())))
        .unwrap()
        .list
        .explore()
        .copied()
        .unwrap();
    assert_eq!(e.state, ExploreState::Wait);
    let e = imported(
        EXPLORE,
        explore(with_sub(
            base(ExploreState::Walk { move_to: None }.get()),
            MOVE_TO,
            quiet(move_to(4, 0.5)),
        )),
    )
    .unwrap()
    .list
    .explore()
    .copied()
    .unwrap();
    // a quiet MoveTo (its maker's `+0x60`, which the save does not hold) with the saved wait
    let m = e.state.move_to().copied().unwrap();
    assert_eq!(
        (m.state, m.voice, m.wait.state),
        (MoveTo::Moving, Voice::Quiet, MoveWaitState::AskedAgain)
    );
    assert_eq!(
        imported(
            EXPLORE,
            explore(with_sub(
                base(ExploreState::Walk { move_to: None }.get()),
                EXPLORE,
                explore(base(3))
            ))
        )
        .unwrap_err(),
        GenieError::unsupported("a saved Explore with another sub-action")
    );
    assert_eq!(
        imported(EXPLORE, explore(base(1))).unwrap_err(),
        GenieError::unsupported("a saved Explore in a state the slice does not model")
    );
    // every set_state drops the sub-actions and only the walk makes one
    assert_eq!(
        imported(
            EXPLORE,
            explore(with_sub(
                base(ExploreState::Wait.get()),
                MOVE_TO,
                quiet(move_to(4, 0.5))
            ))
        )
        .unwrap_err()
        .reason(),
        "a MoveTo of an Explore that does not walk"
    );
}

#[test]
fn an_attack_needs_the_sprites_table() {
    let attack = by_kind(|t| {
        t.rge_action_attack = Some(Box::new(RgeActionAttack {
            rge_action: base(4),
            ..RgeActionAttack::default()
        }));
    });
    assert_eq!(
        imported(ATTACK, attack).unwrap_err(),
        GenieError::unsupported("a saved Attack without the sprites' table")
    );
}

#[test]
fn wait_solidify_waits_for_its_foundation() {
    use crate::construction::wait_solidify::{WAIT_SOLIDIFY, WaitSolidifyState};
    let wait = |b: RgeAction| Action {
        tribe_action_wait_solidify: Some(Box::new(TribeActionWaitSolidify {
            rge_action: b,
            target_building_id: 40,
        })),
        ..Action::default()
    };
    let mut b = base(2);
    b.short_val = 1;
    let w = imported(WAIT_SOLIDIFY, wait(b))
        .unwrap()
        .list
        .wait_solidify()
        .copied()
        .unwrap();
    assert_eq!(
        (w.state, w.target.id(), w.task.id),
        (
            WaitSolidifyState::Stopped,
            object_id(40),
            TaskId::of_task(1)
        )
    );
    assert_eq!(
        imported(
            WAIT_SOLIDIFY,
            wait(with_sub(base(2), MOVE_TO, quiet(move_to(4, 0.5))))
        )
        .unwrap_err(),
        GenieError::unsupported("a saved Wait_Solidify with a sub-action")
    );
    assert_eq!(
        imported(WAIT_SOLIDIFY, explore(base(2))).unwrap_err(),
        GenieError::unsupported("a saved Wait_Solidify without its own part")
    );
}

/// The sprites' table of the Attack tests: an attack sprite 21 of ten frames of 0.1 s.
fn swing_sprites() -> Graphics {
    let swing = Graphic {
        id: SpriteId::named(21),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    Graphics::from([(swing.id, swing)])
}

/// An Attack in state `state` that walks with sprite `walk` (`short_val`) and swings sprite 21
/// (`short_val_2`), with a hit to come (`need_to_attack`) for `pending`.
fn attack(state: u8, walk: u16, pending: bool) -> Action {
    by_kind(|t| {
        t.rge_action_attack = Some(Box::new(RgeActionAttack {
            rge_action: base(state),
            short_val: walk,
            short_val_2: 21,
            need_to_attack: u8::from(pending),
            ..RgeActionAttack::default()
        }));
    })
}

/// A Hunt's Attack in its swing past the hit (state 7, no hit to come) rests the unit's
/// reload (its master's 2.5 s) less the attack sprite's length (1 s); one with its hit to
/// come, or in another state, keeps no rest. A swing longer than the reload rests 0. An Attack
/// in a state that does not attack is the import's error.
#[test]
fn hunts_swing_past_its_hit_rests_the_reload_less_the_swing() {
    use crate::combat::attack::{ATTACK, AttackState};
    use crate::gathering::hunt::{HUNT, HuntState};
    let master = |reload: f32| {
        let mut m = combat_master(83);
        m.tribe_master_combat_object
            .as_deref_mut()
            .unwrap()
            .rge_master_combat_object
            .speed_of_attack = reload;
        with_masters(vec![m])
    };
    let sprites = swing_sprites();
    let swing = |state: AttackState, pending: bool, objects: &PlayerObjects| {
        let a = import_drawn(
            &acting(
                HUNT,
                hunt(with_sub(
                    base(HuntState::Attack { attack: None }.get()),
                    ATTACK,
                    attack(state.get(), 4, pending),
                )),
            ),
            objects,
            Some(&sprites),
        )
        .unwrap()
        .list
        .hunt()
        .copied()
        .unwrap()
        .state
        .attack()
        .copied()
        .unwrap();
        (a.swing.get().to_bits(), a.rest.get().to_bits())
    };
    let rest = 2.5_f32 - 10.0 * 0.1_f32;
    assert_eq!(
        swing(AttackState::Swing, false, &master(2.5)),
        (rest.to_bits(), rest.to_bits())
    );
    assert_eq!(swing(AttackState::Swing, true, &master(2.5)), (0, 0));
    assert_eq!(
        swing(AttackState::Move { move_to: None }, false, &master(2.5)),
        (0, 0)
    );
    let short = 0.5_f32 - 10.0 * 0.1_f32;
    assert_eq!(
        swing(AttackState::Swing, false, &master(0.5)),
        (short.to_bits(), 0)
    );
    // an Attack belongs to the attack: in a walk it is no state of the game
    let stray = import_drawn(
        &acting(
            HUNT,
            hunt(with_sub(
                base(HuntState::Walk { move_to: None }.get()),
                ATTACK,
                attack(AttackState::Move { move_to: None }.get(), 4, false),
            )),
        ),
        &master(2.5),
        Some(&sprites),
    )
    .unwrap_err();
    assert_eq!(
        (stray.kind(), stray.reason()),
        (
            crate::ErrorKind::InvalidInput,
            "an Attack of a Hunt that does not attack"
        )
    );
}
