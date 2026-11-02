use crate::GenieError;
use crate::map::Distance;
use crate::map::Position;
use crate::movement::Speed;
use crate::movement::move_to::{MoveTo, MoveToAction, MoveWait};
use crate::objects::ListIndex;
use crate::objects::masters::ClassId;
use crate::objects::masters::class;
use crate::ownership::artifact::Artifact;
use crate::players::PlayerId;
use crate::test_ids::object_id;

use crate::unit_ai::ActionKind;
use crate::unit_ai::OrderKind;
use crate::world::World;
use crate::world::others::OthersRef;
use crate::world::parts::Object;
use crate::world::tests::{forage_world, in_update};

/// [`forage_world`]'s forager walking to (40, 80) on the bush 600, its unit AI on a gather
/// order there.
fn walking_forager() -> World {
    let mut t = forage_world();
    let u = &mut t.players[1].objects[ListIndex::new(0)];
    t.parts
        .actions
        .of_mut(u.id)
        .unwrap()
        .list
        .set_move_to(Some(MoveToAction {
            sprite: None,
            state: MoveTo::Moving,
            point: Position::new(40.0, 80.0, 0.0),
            target: None,
            range: Distance::ZERO,
            quiet: false,
            on_target_moved: crate::movement::move_to::TargetMoved::Follows,
            wait: MoveWait::default(),
        }));
    let ai = t.parts.unit_ai.get_mut(u.id).unwrap();
    (ai.current_order, ai.current_action, ai.current_target) = (
        Some(OrderKind::Move),
        Some(ActionKind::Move),
        Some(object_id(600)),
    );
    t
}

/// Object 0 of player 1's capture stop as master class `class`, then the object (its row and
/// parts) and Gaia's bushes the stop let go in the update (each held by one worker before it,
/// none after).
fn stopped(mut t: World, class: ClassId) -> Result<(Object, Vec<i32>), GenieError> {
    for b in &mut t.players[0].objects {
        b.workers = crate::objects::WorkerCount::of_data(1);
    }
    let released = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |u, actions, c| {
            u.capture_stop(actions, c, class)?;
            let gone = OthersRef::of(&c.w.players, &c.w.index, c.player)
                .unwrap()
                .foreign_iter()
                .filter(|(_, b)| b.workers == crate::objects::WorkerCount::of_data(0));
            Ok(gone.map(|(_, b)| b.id.get()).collect())
        },
    )?;
    Ok((
        t.swap_remove_live(PlayerId::new(1), ListIndex::new(0))
            .unwrap(),
        released,
    ))
}

/// `FUN_14108c050` without an Artifact: the action list goes (the `MoveTo`'s teardown stops the
/// unit), then the unit AI of any class but livestock ends its order and forgets its target.
#[test]
fn a_capture_stop_tears_the_actions_down_and_stops_the_unit_ai() {
    let (o, released) = stopped(walking_forager(), class::VILLAGER).unwrap();
    assert_eq!(o.parts.actions.list.move_to().copied(), None);
    assert_eq!(released, []);
    let m = o.parts.mover().unwrap();
    assert_eq!(m.animated.speed, Speed::new(0.0));
    assert_eq!(m.velocity.unwrap().velocity.y(), 0.0);
    let ai = o.parts.unit_ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (None, None, None)
    );
    assert_eq!(
        (ai.last_order, ai.last_action),
        (Some(OrderKind::Move), Some(ActionKind::Move))
    );
    // livestock's unit AI, and one on action 0x2c2, keep their order and target
    let (o, _) = stopped(walking_forager(), class::LIVESTOCK).unwrap();
    assert_eq!(o.parts.actions.list.move_to().copied(), None);
    let kept = |ai: &crate::unit_ai::UnitAi| (ai.current_order, ai.current_target);
    assert_eq!(
        kept(o.parts.unit_ai.as_ref().unwrap()),
        (Some(OrderKind::Move), Some(object_id(600)))
    );
    let mut t = walking_forager();
    t.parts
        .unit_ai
        .get_mut(t.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .current_action = Some(ActionKind::HeroPower);
    let (o, _) = stopped(t, class::VILLAGER).unwrap();
    assert_eq!(
        kept(o.parts.unit_ai.as_ref().unwrap()),
        (Some(OrderKind::Move), Some(object_id(600)))
    );
}

/// `FUN_14108c050` with an Artifact on top: its slot 0x38 keeps the action, drops its walking
/// `MoveTo` (which stops the unit) and lets its target go; an animal walking without movement
/// state is not modelled.
#[test]
fn a_capture_stop_keeps_the_artifact_and_lets_its_target_go() {
    let artifact = || Artifact {
        move_to: Some(MoveToAction::sub_in(MoveTo::Moving)),
        goal: Some(Position::new(32.5, 89.5, 0.0)),
        target: Some(object_id(600)),
        ..Artifact::new(false, None)
    };
    let mut t = forage_world();
    t.parts
        .actions_mut(t.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_artifact(Some(artifact()));
    let (o, released) = stopped(t, class::LIVESTOCK).unwrap();
    let a = o.parts.actions.list.artifact().copied().unwrap();
    assert_eq!(
        (a.move_to, a.goal, a.target, a.state),
        (
            None,
            None,
            None,
            crate::ownership::artifact::ArtifactState::Check
        )
    );
    assert_eq!(released, [600]);
    assert_eq!(o.parts.mover().unwrap().animated.speed, Speed::new(0.0));
    let mut t = forage_world();
    let u = &mut t.players[1].objects[ListIndex::new(0)];
    t.parts
        .actions_mut(u.id)
        .unwrap()
        .list
        .set_artifact(Some(artifact()));
    t.parts.put_mover(u.id, None);
    assert_eq!(
        stopped(t, class::LIVESTOCK).unwrap_err().reason(),
        "a walking animal without movement state"
    );
}
