//! The idle update of an action the list update does not run (`FUN_140a784a0`), kind by kind,
//! and of a unit's list from its step: below the head, and from the head in the inside update.

use super::*;
use crate::GenieError;
use crate::combat::AttackTargetId;
use crate::combat::attack::AttackState;
use crate::combat::attack::tests::own_attack;
use crate::combat::missile::{
    HitMode, MissileAction, MissileState, MissileTargetId, Origin, ShooterId, StrikeResult,
};
use crate::commands::QueueId;
use crate::construction::build::tests::started;
use crate::construction::build::{BuildAction, BuildTask};
use crate::construction::pack::{PackAction, PackState, PackStatus};
use crate::construction::repair::{RepairAction, RepairState, RepairTask};
use crate::construction::unpack::{UnpackAction, UnpackState, UnpackStatus};
use crate::construction::wait_solidify::WaitSolidify;
use crate::construction::{FoundationId, RepairTargetId};
use crate::garrison::EnterState;
use crate::garrison::transport::{TransportAction, TransportState};
use crate::gathering::DropSiteId;
use crate::gathering::gather::GatherAction;
use crate::gathering::hunt::tests::hunting;
use crate::gathering::hunt::{HuntAction, HuntState, Lead};
use crate::gathering::make::{MakeAction, MakeState};
use crate::map::Point;
use crate::map::{Distance, Offset, Position};
use crate::movement::explore::{ExploreAction, ExploreState};
use crate::movement::fly::{Fly, FlyState};
use crate::movement::move_to::{MoveTo, MoveToAction};
use crate::movement::{Acceleration, Speed};
use crate::objects::ListIndex;
use crate::objects::Voice;
use crate::objects::WorkRate;
use crate::objects::actions::ActionList;
use crate::objects::masters::ActionKind;
use crate::objects::masters::TaskId;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::{Task, WorkValue};
use crate::objects::teardown::HeadCode;
use crate::objects::teardown::Refund;
use crate::ownership::artifact::Artifact;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::test_ids::object_id;
use crate::test_ids::task_key;
use crate::unit_ai::{Answer, OrderForce};
use crate::world::Seconds;
use crate::world::World;
use crate::world::tests::{enter_town_centre, forage_world, town_workers_and_holding};
use crate::world::tests::{villager_holding, wait_over, walk_to_town_centre};

/// The idle update of `action` with every object it holds in `state`.
fn idled_in(action: &mut Action, state: ObjectState) -> Idled {
    idled_by(action, |_| state)
}

/// The idle update of `action` with the object of each id in the state `state_of` gives.
fn idled_by(action: &mut Action, state_of: impl Fn(ObjectId) -> ObjectState) -> Idled {
    action
        .idle_update(&IdleReads {
            state_of: &|id| Ok(state_of(id)),
        })
        .unwrap()
}

/// What an idle update that let `released` go in this order and left no building gives.
fn releasing(released: &[i32]) -> Idled {
    Idled {
        clears_count: false,
        left: TornDown {
            refund: Refund::Nothing,
            releases: released.iter().copied().map(object_id).collect(),
            builder_leaves: Vec::new(),
            pack: None,
            unpack: None,
        },
    }
}

/// The target of `action` and of each sub-action under it, the action's first: one entry per
/// level that [`IdledAction::into_sub_action`] reaches.
fn held_targets(action: &mut Action) -> Vec<Option<ObjectId>> {
    let mut held = Vec::new();
    let mut level = Some(IdledAction::Listed(action));
    while let Some(idled_action) = level {
        held.push(idled_action.target());
        level = idled_action.into_sub_action();
    }
    held
}

/// `count` levels of actions that each hold `id`.
fn holding(id: i32, count: usize) -> Vec<Option<ObjectId>> {
    vec![Some(object_id(id)); count]
}

/// A list's `MoveTo` that holds `target`, not quiet; it walks to the target, its point the
/// origin.
fn move_to_holding(target: i32) -> MoveToAction {
    MoveToAction {
        target: Some(object_id(target)),
        voice: Voice::Loud,
        ..MoveToAction::sub_in(MoveTo::Moving)
    }
}

/// A walk's `MoveTo` that holds `target`, its point the origin: quiet, as every owner makes one
/// (`FUN_140aa0df0` with `+0x60` 1, the quiet setter `FUN_140a7a170` with 1).
fn walk_holding(target: i32) -> MoveToAction {
    MoveToAction {
        target: Some(object_id(target)),
        ..MoveToAction::sub_in(MoveTo::Moving)
    }
}

/// The forager 9175's Gather of bush 600 (its quiet walk holding the bush), carrying to the town
/// centre 700.
fn gather_of_bush() -> GatherAction {
    let mut world = forage_world();
    assert_eq!(
        world.gather_command(
            PlayerId::new(1),
            ListIndex::FIRST,
            object_id(600),
            OrderForce::Forced,
        ),
        Ok(Answer::Taken)
    );
    let gather = *world
        .parts
        .actions_of(object_id(9175))
        .list
        .gather()
        .unwrap();
    GatherAction {
        carry_to: Some(DropSiteId::new(object_id(700))),
        ..gather
    }
}

/// A Build of foundation 10028 (its builder `added` to its count), its drop site 10050.
fn build_on_site(added: bool) -> BuildAction {
    let task = BuildTask {
        range: Distance::ZERO,
        work_value: WorkValue::new(1.0),
        work_rate: WorkRate::new(1.0),
        resource: None,
        id: TaskId::of_task(-1),
        key: task_key(ActionKind::BUILD),
        sprites: TaskSprites::NONE,
    };
    BuildAction {
        added,
        site: Some(DropSiteId::new(object_id(10050))),
        ..started(
            object_id(10028),
            Position::new(30.0, 88.0, 0.0),
            Distance::ZERO,
            task,
        )
    }
}

/// `build_action` after its target went: uncounted, and its quiet walk's `MoveTo` holding the
/// foundation no longer either.
fn dropped_build(build_action: BuildAction) -> BuildAction {
    let mut dropped = BuildAction {
        target: None,
        added: false,
        ..build_action
    };
    if let Some(Some(walk)) = dropped.state.walk_mut() {
        walk.target = None;
    }
    dropped
}

/// A Repair of object 20 (its repairer `counted` among the target's repairers), with `voice`.
fn repair_of(counted: bool, voice: Voice) -> RepairAction {
    RepairAction {
        target: Some(RepairTargetId::new(object_id(20))),
        state: RepairState::Work,
        point: Position::ORIGIN,
        timer: Seconds::ZERO,
        task: RepairTask::of(&Task::default()),
        range: Distance::ZERO,
        counted,
        voice,
    }
}

/// A missile on object 9150, fired from object 9149.
fn missile_on() -> MissileAction {
    MissileAction {
        state: MissileState::Flying,
        target: Some(MissileTargetId::new(object_id(9150))),
        fired_from: Some(ShooterId::new(object_id(9149))),
        point: Position::ORIGIN,
        timer: Distance::ZERO,
        direction: Offset::ZERO,
        ballistic_velocity: Speed::ZERO,
        ballistic_acceleration: Acceleration::ZERO,
        distance: Distance::ZERO,
        hit_mode: HitMode::TARGET_ALONE,
        result: StrikeResult::None,
        origin: Origin::NONE,
        met: Vec::new(),
    }
}

/// The unit's Attack on 9149 walking to it (its quiet walk, `FUN_140aa0df0` through
/// `FUN_140a7a170` with 1).
fn attack_walking() -> Attack {
    own_attack(
        9149,
        AttackState::Move {
            move_to: Some(walk_holding(9149)),
        },
        Distance::ZERO,
    )
}

/// The Enter of town centre 9150 walking to it (its quiet walk, `garrison/walk_in.rs`).
fn enter_walking() -> EnterAction {
    EnterAction {
        state: EnterState::Walk {
            move_to: Some(walk_holding(9150)),
        },
        ..enter_town_centre()
    }
}

/// The Unpack walking to 9149 (its quiet walk, `FUN_140aa0df0` through `FUN_140a7a170` with 1),
/// not at work yet.
fn unpack_walking() -> UnpackAction {
    UnpackAction {
        state: UnpackState::Moving {
            move_to: Some(walk_holding(9149)),
        },
        target: Some(object_id(9149)),
        point: Point::ORIGIN,
        timer: Seconds::ZERO,
        unique_id: QueueId::of_data(-1),
        status: UnpackStatus::Idle,
        range: Distance::new(15.9),
    }
}

/// The Hunt of animal 1 in its state 12, carrying to drop site 2: its quiet Attack on the animal
/// (`FUN_140a7c210` on the Hunt's `+0x20`) walking to it.
fn hunt_attacking() -> HuntAction {
    let attack = Attack {
        voice: Voice::Quiet,
        ..own_attack(
            1,
            AttackState::Move {
                move_to: Some(walk_holding(1)),
            },
            Distance::ZERO,
        )
    };
    HuntAction {
        carry_to: Some(DropSiteId::new(object_id(2))),
        state: HuntState::Attack {
            attack: Some(attack),
        },
        ..hunting(1)
    }
}

/// `FUN_140a784a0` tests `+0x640` against 7 (`JC`): a target in state 7 or 8 goes, one in state
/// 6 stays, for each kind's slot 0x68 with none; the base's lets it go. The action holds no
/// target after, and nothing else changes.
#[test]
fn target_past_state_6_goes_and_is_let_go() {
    let artifact = Artifact {
        target: Some(object_id(9150)),
        ..Artifact::new(PlayerId::new(1), None)
    };
    let enter = enter_town_centre();
    let actions = [
        (Action::Artifact(artifact), 9150),
        (Action::MoveTo(move_to_holding(9149)), 9149),
        (Action::Enter(enter), 9150),
        (Action::Hunt(hunting(9149)), 9149),
        (
            Action::Missile(MissileAction {
                fired_from: None,
                ..missile_on()
            }),
            9150,
        ),
        (Action::Repair(repair_of(false, Voice::Loud)), 20),
        (
            Action::Pack(PackAction {
                state: PackState::Working,
                target: Some(object_id(9150)),
                timer: Seconds::new(3.0),
                status: PackStatus::Packing,
                unique_id: QueueId::of_data(-1),
            }),
            9150,
        ),
        (
            Action::Unpack(UnpackAction {
                state: UnpackState::Working,
                target: Some(object_id(9150)),
                point: Point::new(4.0, 5.0),
                timer: Seconds::new(3.0),
                unique_id: QueueId::of_data(-1),
                status: UnpackStatus::Unpacking,
                range: Distance::new(15.9),
            }),
            9150,
        ),
    ];
    for (action, target) in actions {
        let mut kept = action.clone();
        assert_eq!(
            idled_in(&mut kept, ObjectState::Undead),
            Idled::default(),
            "{action:?}"
        );
        assert_eq!(kept, action);
        for gone in [ObjectState::Removed, ObjectState::Gone] {
            let mut dropped = action.clone();
            assert_eq!(idled_in(&mut dropped, gone), releasing(&[target]));
            assert_eq!(dropped.target(), None, "{action:?}");
        }
    }
    let mut artifact_held = Action::Artifact(artifact);
    let _ = idled_in(&mut artifact_held, ObjectState::Removed);
    assert_eq!(
        artifact_held,
        Action::Artifact(Artifact {
            target: None,
            ..artifact
        })
    );
}

/// The Gather's slot 0x68 is the base's: its target goes and is let go; then its drop site (the
/// second target) past state 6 goes and is let go too, its place and the rest as they were.
#[test]
fn gather_lets_its_target_then_its_drop_site_go() {
    let gather = gather_of_bush();
    let mut action = Action::Gather(gather);
    assert_eq!(
        idled_in(&mut action, ObjectState::Removed),
        releasing(&[600, 700])
    );
    let mut dropped = GatherAction {
        target: None,
        carry_to: None,
        ..gather
    };
    // its quiet walk's MoveTo holds the bush no longer either, without a release
    if let Some(Some(walk)) = dropped.state.walk_mut() {
        walk.target = None;
    }
    assert_eq!(action, Action::Gather(dropped));
}

/// The Attack's slot 0x68 (`FUN_140a82a10`) clears the unit's attack count on a target it held,
/// then the base lets it go: the unit's own Attack is loud.
#[test]
fn attack_clears_the_count_and_lets_its_target_go() {
    let attack = own_attack(9149, AttackState::Ready, Distance::ZERO);
    let mut action = Action::Attack(attack);
    assert_eq!(
        idled_in(&mut action, ObjectState::Removed),
        Idled {
            clears_count: true,
            ..releasing(&[9149])
        }
    );
    assert_eq!(
        action,
        Action::Attack(Attack {
            target: None,
            ..attack
        })
    );
}

/// The Hunt's slot 0x68 (`FUN_140ac7980`) clears its lead after the base, its drop site past
/// state 6 goes too.
#[test]
fn hunt_clears_its_lead_with_its_target() {
    let leading = HuntAction {
        carry_to: Some(DropSiteId::new(object_id(700))),
        lead: Lead {
            moved: true,
            still_time: 4,
            start: Point::new(3.0, 4.0),
        },
        ..hunting(9149)
    };
    let mut action = Action::Hunt(leading);
    assert_eq!(
        idled_in(&mut action, ObjectState::Removed),
        releasing(&[9149, 700])
    );
    assert_eq!(
        action,
        Action::Hunt(HuntAction {
            target: None,
            carry_to: None,
            lead: Lead::NEW,
            ..leading
        })
    );
}

/// The Build's (`FUN_140ab0300`) and the Repair's (`FUN_140ada470`) slot 0x68 first take a
/// counted builder or repairer out of its building's count, then the base lets the target go
/// unless the action is quiet (a Repair's `+0x60`); an uncounted one only lets go. The Build's
/// drop site goes after.
#[test]
fn counted_builder_and_repairer_leave_their_building_first() {
    for added in [true, false] {
        let mut action = Action::Build(build_on_site(added));
        let idled = idled_in(&mut action, ObjectState::Removed);
        let leaves = if added {
            vec![object_id(10028)]
        } else {
            vec![]
        };
        assert_eq!(
            (idled.left.builder_leaves, idled.left.releases),
            (leaves, vec![object_id(10028), object_id(10050)])
        );
        assert_eq!(
            action,
            Action::Build(dropped_build(BuildAction {
                site: None,
                ..build_on_site(added)
            }))
        );
    }
    for (counted, voice, released) in [
        (true, Voice::Loud, vec![object_id(20)]),
        (true, Voice::Quiet, vec![]),
        (false, Voice::Loud, vec![object_id(20)]),
    ] {
        let mut action = Action::Repair(repair_of(counted, voice));
        let idled = idled_in(&mut action, ObjectState::Gone);
        let leaves = if counted { vec![object_id(20)] } else { vec![] };
        assert_eq!(
            (idled.left.builder_leaves, idled.left.releases),
            (leaves, released)
        );
        assert_eq!(
            action,
            Action::Repair(RepairAction {
                target: None,
                counted: false,
                ..repair_of(counted, voice)
            })
        );
    }
}

/// A second target (`+0x28`) goes by its own state, whatever the target's: one past state 6 is
/// let go and goes, one in state 6 stays, for each kind that holds one (the Gather's, the Hunt's
/// and the Build's drop site, a missile's shooter, the attack command's second target). The
/// release tests no `+0x60` (`0x140a784cc`), and no quiet action holds a second target.
#[test]
fn second_target_goes_by_its_own_state() {
    let actions = [
        (Action::Gather(gather_of_bush()), 600, 700),
        (
            Action::Hunt(HuntAction {
                carry_to: Some(DropSiteId::new(object_id(700))),
                ..hunting(9149)
            }),
            9149,
            700,
        ),
        (Action::Build(build_on_site(false)), 10028, 10050),
        (Action::Missile(missile_on()), 9150, 9149),
        (
            Action::Attack(Attack {
                target2: Some(AttackTargetId::new(object_id(9150))),
                ..own_attack(9149, AttackState::Ready, Distance::ZERO)
            }),
            9149,
            9150,
        ),
    ];
    for (action, target, second) in actions {
        let is_second = |id: ObjectId| id == object_id(second);
        let mut second_gone = action.clone();
        let idled = idled_by(&mut second_gone, |id| {
            if is_second(id) {
                ObjectState::Removed
            } else {
                ObjectState::Undead
            }
        });
        assert_eq!(idled, releasing(&[second]), "{action:?}");
        assert_eq!(
            (second_gone.target(), second_gone.second_target()),
            (Some(object_id(target)), None)
        );
        let mut target_gone = action.clone();
        let idled = idled_by(&mut target_gone, |id| {
            if is_second(id) {
                ObjectState::Undead
            } else {
                ObjectState::Removed
            }
        });
        assert_eq!(idled.left.releases, vec![object_id(target)], "{action:?}");
        assert_eq!(
            (target_gone.target(), target_gone.second_target()),
            (None, Some(object_id(second)))
        );
    }
}

/// `FUN_140a98bf0` runs `FUN_140a784a0` on each sub-action: a walk's `MoveTo` (quiet, an
/// Unpack's too), the Hunt's Attack (quiet) and the Artifact's Enter. A target past state 6 goes
/// at every level and only the loud ones let it go: the Hunt's Attack clears the unit's attack
/// count. A target in state 6 stays held at every level, and nothing changes.
#[test]
fn sub_actions_drop_their_targets_past_state_6_and_keep_them_in_state_6() {
    let entering = |target: Option<ObjectId>| Artifact {
        target,
        sub: Some(ArtifactSub::Enter {
            enter: enter_walking(),
        }),
        ..Artifact::new(PlayerId::new(1), None)
    };
    let to_object = Artifact {
        target: Some(object_id(9149)),
        sub: Some(ArtifactSub::Walk {
            move_to: walk_holding(9149),
        }),
        ..Artifact::new(PlayerId::new(1), None)
    };
    let cases = [
        (
            Action::Attack(attack_walking()),
            holding(9149, 2),
            true,
            vec![9149],
        ),
        (
            Action::Enter(enter_walking()),
            holding(9150, 2),
            false,
            vec![9150],
        ),
        (
            Action::Artifact(to_object),
            holding(9149, 2),
            false,
            vec![9149],
        ),
        (
            Action::Artifact(entering(None)),
            vec![None, Some(object_id(9150)), Some(object_id(9150))],
            false,
            vec![9150],
        ),
        (
            Action::Hunt(hunt_attacking()),
            holding(1, 3),
            true,
            vec![1, 2],
        ),
        (
            Action::Unpack(unpack_walking()),
            holding(9149, 2),
            false,
            vec![9149],
        ),
    ];
    for (action, held, clears_count, released) in cases {
        let mut kept = action.clone();
        assert_eq!(held_targets(&mut kept), held, "{action:?}");
        assert_eq!(idled_in(&mut kept, ObjectState::Undead), Idled::default());
        assert_eq!(kept, action);
        let mut dropped = action.clone();
        assert_eq!(
            idled_in(&mut dropped, ObjectState::Removed),
            Idled {
                clears_count,
                ..releasing(&released)
            },
            "{action:?}"
        );
        assert_eq!(held_targets(&mut dropped), vec![None; held.len()]);
    }
}

/// The sub-actions take the idle update after the action's own targets: the Artifact lets its
/// own target go before its Enter lets the town centre go; the Hunt its animal before its drop
/// site, then its Attack and the Attack's walk hold the animal no longer.
#[test]
fn sub_actions_follow_the_targets_in_order() {
    let entering = Artifact {
        target: Some(object_id(9149)),
        sub: Some(ArtifactSub::Enter {
            enter: enter_walking(),
        }),
        ..Artifact::new(PlayerId::new(1), None)
    };
    let mut action = Action::Artifact(entering);
    assert_eq!(
        idled_in(&mut action, ObjectState::Removed),
        releasing(&[9149, 9150])
    );
    let hunt = hunt_attacking();
    let mut action = Action::Hunt(hunt);
    let _ = idled_in(&mut action, ObjectState::Removed);
    let HuntState::Attack {
        attack: Some(attack),
    } = hunt.state
    else {
        panic!("the Hunt attacks: {hunt:?}");
    };
    let gone = Attack {
        target: None,
        state: AttackState::Move {
            move_to: Some(MoveToAction {
                target: None,
                ..walk_holding(1)
            }),
        },
        ..attack
    };
    assert_eq!(
        action,
        Action::Hunt(HuntAction {
            target: None,
            carry_to: None,
            state: HuntState::Attack { attack: Some(gone) },
            lead: Lead::NEW,
            ..hunt
        })
    );
}

/// An action without a target (`+0x20` null) reads no object: a wait for a foundation keeps its
/// foundation at `+0x68`, a Make holds none, and the walks of a Fly (loud, to the flight's
/// point), of an Explore and of a sailing Transport (quiet, each to its point) hold none either;
/// a state read would fail.
#[test]
fn action_without_target_reads_no_object() {
    let fails = |id: ObjectId| Err(GenieError::unreachable("read").with("object", id));
    let to_point = MoveToAction::sub_to_point(Position::new(10.0, 10.0, 0.0), Distance::ZERO, None);
    let fly = Fly {
        state: FlyState::Flying {
            move_to: Some(MoveToAction {
                voice: Voice::Loud,
                ..to_point
            }),
        },
        timer: Seconds::new(1.0),
        home: Point::new(10.0, 10.0),
        task: Task::default(),
    };
    let exploring = ExploreAction {
        state: ExploreState::Walk {
            move_to: Some(to_point),
        },
        ..ExploreAction::new(Position::new(10.0, 10.0, 0.0))
    };
    let sailing = TransportAction {
        state: TransportState::Sail {
            move_to: Some(to_point),
        },
        ..TransportAction::new(Position::new(10.0, 10.0, 0.0))
    };
    let making = MakeAction {
        state: MakeState::Making,
        remaining: Amount::new(175.0),
        total: Amount::new(175.0),
        task: Task::default(),
    };
    let actions = [
        Action::WaitSolidify(WaitSolidify::new(
            Task::default(),
            FoundationId::new(object_id(10028)),
        )),
        Action::Explore(ExploreAction::new(Position::ORIGIN)),
        Action::Explore(exploring),
        Action::Fly(fly),
        Action::Make(making),
        Action::Transport(TransportAction::new(Position::ORIGIN)),
        Action::Transport(sailing),
    ];
    for action in actions {
        let mut idle = action.clone();
        assert_eq!(
            idle.idle_update(&IdleReads { state_of: &fails }),
            Ok(Idled::default())
        );
        assert_eq!(idle, action);
    }
    let mut held = Action::MoveTo(move_to_holding(9149));
    assert_eq!(
        held.idle_update(&IdleReads { state_of: &fails })
            .unwrap_err()
            .reason(),
        "read"
    );
}

/// The villager's step with `from` as the idle update's start, and its code `code` after.
fn end_update(world: &mut World, code: HeadCode, from: IdleFrom) -> GenieResult<()> {
    world.run_on_step(
        PlayerId::new(1),
        ListIndex::new(2),
        |unit, actions, step| unit.end_head_update(actions, step, code, from),
    )
}

/// The workers of town centres 9149 and 9150.
fn town_workers(world: &World) -> (i32, i32) {
    let (first, second, ..) = town_workers_and_holding(world);
    (first, second)
}

/// `world`'s object `id` in `state`.
fn put_in_state(world: &mut World, id: i32, state: ObjectState) {
    world.object_mut(object_id(id)).unwrap().1.life.state = state;
}

/// The list update (`FUN_140a992b0`) runs the idle update (`FUN_140a78470`) of each action below
/// the head, in the list's order: the walk and the Artifact let their removed town centres go at
/// once (one worker less each) and hold them no longer; the head keeps its target. A town centre
/// in state 6 stays held.
#[test]
fn list_update_idles_each_action_below_the_head() {
    let artifact = Artifact {
        target: Some(object_id(9150)),
        ..Artifact::new(PlayerId::new(1), None)
    };
    let below = ActionList::default()
        .with_artifact(Some(artifact))
        .with_move_to(Some(walk_to_town_centre()));
    let mut world = villager_holding(wait_over(below.clone()), ObjectState::Alive);
    for id in [9149, 9150] {
        put_in_state(&mut world, id, ObjectState::Undead);
    }
    end_update(&mut world, HeadCode::Keeps, IdleFrom::BelowHead).unwrap();
    assert_eq!(world.parts.actions_of(object_id(40)).list, wait_over(below));
    assert_eq!(town_workers(&world), (1, 1));
    for id in [9149, 9150] {
        put_in_state(&mut world, id, ObjectState::Removed);
    }
    end_update(&mut world, HeadCode::Keeps, IdleFrom::BelowHead).unwrap();
    assert_eq!(
        world.parts.actions_of(object_id(40)).list,
        wait_over(
            ActionList::default()
                .with_artifact(Some(Artifact {
                    target: None,
                    ..artifact
                }))
                .with_move_to(Some(MoveToAction {
                    target: None,
                    ..walk_to_town_centre()
                }))
        )
    );
    assert_eq!(town_workers(&world), (0, 0));
}

/// The inside update (`FUN_140a98790`) idles the head too (`FUN_140a78480`) before its
/// deletion; the list update leaves the head to its own update.
#[test]
fn inside_update_idles_the_head_too() {
    let head = ActionList::default().with_move_to(Some(walk_to_town_centre()));
    for (from, workers, target) in [
        (IdleFrom::BelowHead, 1, Some(object_id(9149))),
        (IdleFrom::Head, 0, None),
    ] {
        let mut world = villager_holding(head.clone(), ObjectState::Alive);
        put_in_state(&mut world, 9149, ObjectState::Removed);
        end_update(&mut world, HeadCode::Keeps, from).unwrap();
        assert_eq!(town_workers(&world).0, workers, "{from:?}");
        assert_eq!(
            world
                .parts
                .actions_of(object_id(40))
                .list
                .move_to()
                .map(|move_to_action| move_to_action.target),
            Some(target)
        );
    }
}

/// An Attack below the head whose target went clears the unit's attack count (`+0x900`); one on
/// a target in state 6 keeps it.
#[test]
fn idle_attack_on_removed_target_clears_the_attack_count() {
    let attack = own_attack(9149, AttackState::Ready, Distance::ZERO);
    for (state, count) in [
        (ObjectState::Undead, AttackCount::of_data(3)),
        (ObjectState::Removed, AttackCount::ZERO),
    ] {
        let mut world = villager_holding(
            wait_over(ActionList::default().with_attack(Some(attack))),
            ObjectState::Alive,
        );
        world.parts.combat_mut(object_id(40)).unwrap().attack_count = AttackCount::of_data(3);
        put_in_state(&mut world, 9149, state);
        end_update(&mut world, HeadCode::Keeps, IdleFrom::BelowHead).unwrap();
        assert_eq!(
            world.parts.combat_of(object_id(40)).attack_count,
            count,
            "{state:?}"
        );
    }
}

/// A counted builder's Build below the head on a removed foundation leaves its builders and lets
/// it go (`FUN_140ab0300`, then `FUN_140a7a230`).
#[test]
fn counted_build_below_the_head_leaves_the_removed_foundation() {
    let mut build_action = build_on_site(true);
    build_action.site = None;
    let mut world = villager_holding(
        wait_over(ActionList::default().with_build(Some(build_action))),
        ObjectState::Alive,
    );
    put_in_state(&mut world, 10028, ObjectState::Removed);
    end_update(&mut world, HeadCode::Keeps, IdleFrom::BelowHead).unwrap();
    let foundation = &world.players[2].objects[ListIndex::FIRST];
    assert_eq!(
        (
            foundation.workers.get(),
            world
                .parts
                .building_of(foundation.id)
                .construction
                .map(|construction| construction.builders.get())
        ),
        (0, Some(0))
    );
    assert_eq!(
        world.parts.actions_of(object_id(40)).list.build().copied(),
        Some(dropped_build(build_action))
    );
}

/// An action that holds the updating unit itself reads the unit's own state, which the world's
/// table does not find while it updates; one that holds an object the world lacks fails, the
/// actions after it untouched.
#[test]
fn own_target_reads_the_unit_and_missing_one_fails() {
    let own = Artifact {
        target: Some(object_id(40)),
        ..Artifact::new(PlayerId::new(1), None)
    };
    let mut world = villager_holding(
        wait_over(ActionList::default().with_artifact(Some(own))),
        ObjectState::Alive,
    );
    assert_eq!(
        end_update(&mut world, HeadCode::Keeps, IdleFrom::BelowHead),
        Ok(())
    );
    let lost = ActionList::default()
        .with_move_to(Some(walk_to_town_centre()))
        .with_artifact(Some(Artifact {
            target: Some(object_id(99_999)),
            ..own
        }));
    let mut world = villager_holding(wait_over(lost.clone()), ObjectState::Alive);
    put_in_state(&mut world, 9149, ObjectState::Removed);
    assert_eq!(
        end_update(&mut world, HeadCode::Keeps, IdleFrom::BelowHead)
            .unwrap_err()
            .reason(),
        object_gone(object_id(99_999)).reason()
    );
    assert_eq!(world.parts.actions_of(object_id(40)).list, wait_over(lost));
    assert_eq!(town_workers(&world), (1, 1));
}

/// A code of 1 or 3 deletes the head after the idle update of the actions below it: the Enter
/// under the walk has let its removed town centre go before the walk's deletion lets go of its
/// own.
#[test]
fn deletion_follows_the_idle_update() {
    let list = ActionList::default()
        .with_enter(Some(enter_town_centre()))
        .with_move_to(Some(walk_to_town_centre()));
    let mut world = villager_holding(list, ObjectState::Dying);
    put_in_state(&mut world, 9150, ObjectState::Removed);
    end_update(&mut world, HeadCode::Deletes, IdleFrom::BelowHead).unwrap();
    assert_eq!(
        world.parts.actions_of(object_id(40)).list,
        ActionList::default().with_enter(Some(EnterAction {
            target: None,
            ..enter_town_centre()
        }))
    );
    assert_eq!(town_workers(&world), (0, 0));
}
