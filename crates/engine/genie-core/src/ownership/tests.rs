use crate::GenieResult;
use crate::map::Distance;
use crate::map::Position;
use crate::movement::Speed;
use crate::movement::move_to::{MoveTo, MoveToAction, MoveWait};
use crate::objects::ListIndex;
use crate::objects::Voice;
use crate::objects::masters::ClassId;
use crate::ownership::artifact::Artifact;
use crate::players::PlayerId;
use crate::test_ids::object_id;
use crate::testing::class;

use crate::movement::move_to::TargetMoved;
use crate::objects::WorkerCount;
use crate::ownership::artifact::{ArtifactState, ArtifactSub};
use crate::testing::roles;
use crate::unit_ai::AiAction;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAi;
use crate::world::World;
use crate::world::others::OthersRef;
use crate::world::parts::Object;
use crate::world::tests::{forage_world, in_update};

/// [`forage_world`]'s forager walking to (40, 80) on the bush 600, its unit AI on a gather
/// order there.
fn walking_forager() -> World {
    let mut world = forage_world();
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    world
        .parts
        .actions
        .of_mut(unit.id)
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
    let ai = world.parts.unit_ai.get_mut(unit.id).unwrap();
    (ai.current_order, ai.current_action, ai.current_target) = (
        Some(OrderKind::Move),
        Some(AiAction::Move),
        Some(object_id(600)),
    );
    world
}

/// Object 0 of player 1's capture stop as master class `class`, then the object (its row and
/// parts) and Gaia's bushes the stop let go in the update (each held by one worker before it,
/// none after).
fn stopped(mut world: World, class: ClassId) -> GenieResult<(Object, Vec<i32>)> {
    for unit in &mut world.players[0].objects {
        unit.workers = WorkerCount::of_data(1);
    }
    let released = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |own_unit, actions, step| {
            own_unit.capture_stop(actions, step, class, &roles())?;
            let gone = OthersRef::of(&step.world.players, &step.world.index, step.player)
                .unwrap()
                .foreign_iter()
                .filter(|(_, unit)| unit.workers == WorkerCount::of_data(0));
            Ok(gone.map(|(_, unit)| unit.id.get()).collect())
        },
    )?;
    Ok((
        world
            .swap_remove_live(PlayerId::new(1), ListIndex::new(0))
            .unwrap(),
        released,
    ))
}

/// `FUN_14108c050` without an Artifact: the action list goes (the `MoveTo`'s teardown stops the
/// unit), then the unit AI of any class but livestock ends its order and forgets its target.
#[test]
fn capture_stop_tears_the_actions_down_and_stops_the_unit_ai() {
    let (object, released) = stopped(walking_forager(), class::VILLAGER).unwrap();
    assert_eq!(object.parts.actions.list.move_to().copied(), None);
    assert_eq!(released, [0_i32; 0]);
    let mover = object.parts.mover().unwrap();
    assert_eq!(mover.animated.speed, Speed::new(0.0));
    assert_eq!(mover.velocity.unwrap().velocity.y(), 0.0);
    let ai = object.parts.unit_ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (None, None, None)
    );
    assert_eq!(
        (ai.last_order, ai.last_action),
        (Some(OrderKind::Move), Some(AiAction::Move))
    );
    // livestock's unit AI, and one on action 0x2c2, keep their order and target
    let (object, _) = stopped(walking_forager(), class::LIVESTOCK).unwrap();
    assert_eq!(object.parts.actions.list.move_to().copied(), None);
    let kept = |ai: &UnitAi| (ai.current_order, ai.current_target);
    assert_eq!(
        kept(object.parts.unit_ai.as_ref().unwrap()),
        (Some(OrderKind::Move), Some(object_id(600)))
    );
    let mut world = walking_forager();
    world
        .parts
        .unit_ai
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .current_action = Some(AiAction::HeroPower);
    let (object, _) = stopped(world, class::VILLAGER).unwrap();
    assert_eq!(
        kept(object.parts.unit_ai.as_ref().unwrap()),
        (Some(OrderKind::Move), Some(object_id(600)))
    );
}

/// `FUN_14108c050` with an Artifact on top: its slot 0x38 keeps the action, drops its walking
/// `MoveTo` (which stops the unit) and lets its target go; an animal walking without movement
/// state is not modelled.
#[test]
fn capture_stop_keeps_the_artifact_and_lets_its_target_go() {
    let artifact = || Artifact {
        sub: Some(ArtifactSub::Walk {
            move_to: MoveToAction::sub_in(MoveTo::Moving),
        }),
        goal: Some(Position::new(32.5, 89.5, 0.0)),
        target: Some(object_id(600)),
        ..Artifact::new(PlayerId::new(1), None)
    };
    let mut world = forage_world();
    world
        .parts
        .actions_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_artifact(Some(artifact()));
    let (object, released) = stopped(world, class::LIVESTOCK).unwrap();
    let held_artifact = object.parts.actions.list.artifact().copied().unwrap();
    assert_eq!(
        (
            held_artifact.move_to().copied(),
            held_artifact.goal,
            held_artifact.target,
            held_artifact.state
        ),
        (None, None, None, ArtifactState::Check)
    );
    assert_eq!(released, [600]);
    assert_eq!(
        object.parts.mover().unwrap().animated.speed,
        Speed::new(0.0)
    );
    let mut world = forage_world();
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    world
        .parts
        .actions_mut(unit.id)
        .unwrap()
        .list
        .set_artifact(Some(artifact()));
    world.parts.put_mover(unit.id, None);
    assert_eq!(
        stopped(world, class::LIVESTOCK).unwrap_err().reason(),
        "a walking animal without movement state"
    );
}
