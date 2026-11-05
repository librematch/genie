//! A slot 0x50's swap of an action's sprite across a task change.

use crate::construction::build::tests::{TASK as BUILD_TASK, started};
use crate::construction::build::{BuildAction, BuildTask};
use crate::construction::wait_solidify::WaitSolidify;
use crate::garrison::EnterAction;
use crate::garrison::transport::TransportAction;
use crate::gathering::gather::GATHER;
use crate::gathering::hunt::tests::hunting;
use crate::gathering::hunt::{HUNT, HuntAction, HuntState, HuntTask};
use crate::map::{Distance, Position};
use crate::math::rng::Well512;
use crate::movement::Speed;
use crate::movement::explore::{ExploreAction, ExploreState};
use crate::movement::move_to::{MoveTo, MoveToAction};
use crate::objects::Graphics;
use crate::objects::actions::{Action, ActionList, SubActionMut};
use crate::objects::masters::{ActionPart, ClassId, Master, Task, TaskSprites};
use crate::objects::role::RoleId;
use crate::objects::sprite::{ActiveSprite, Graphic};
use crate::objects::task_change::{
    HeldTask, Looks, TaskChange, TaskSprite, action_take_master, sub_action_take_master, swapped,
    take_tasks,
};
use crate::objects::{FacetCount, Frame, SpeedMultiplier, SpriteFlags, SpriteId};
use crate::ownership::artifact::{Artifact, ArtifactSub};
use crate::players::PlayerId;
use crate::test_ids::{object_id, task_id};
use crate::testing::tuning;
use crate::world::Seconds;

/// The graphic of sprite `id`.
const fn graphic(id: i16) -> Graphic {
    Graphic {
        id: SpriteId::named(id),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    }
}

/// The graphics of sprites 1 to 4 and 11 to 13.
fn graphics() -> Graphics {
    Graphics::from([1, 2, 3, 4, 11, 12, 13].map(|id| (SpriteId::named(id), graphic(id))))
}

/// A task change from the sprites (1, 2, 3) to (11, 12, 13), the carrying ones 4 and 14.
const CHANGE: TaskChange = TaskChange {
    old: TaskSprites::of_data([1, 2, 3, 4]),
    next: TaskSprites::of_data([11, 12, 13, 14]),
};

/// Without tasks the sprite takes the other one; with them a sprite of the old task's slots
/// takes the new task's of the same slot, the first slot that holds it first.
#[test]
fn sprite_of_old_task_takes_the_new_task_sprite_of_its_slot() {
    let graphics = graphics();
    let both = [TaskSprite::Proceeding, TaskSprite::Working];
    let otherwise = Some(graphic(4));
    assert_eq!(
        [
            swapped(
                graphic(1),
                None,
                &[TaskSprite::Moving],
                otherwise,
                &graphics
            ),
            swapped(
                graphic(1),
                Some(CHANGE),
                &[TaskSprite::Moving],
                otherwise,
                &graphics
            ),
            swapped(graphic(2), Some(CHANGE), &both, otherwise, &graphics),
            swapped(graphic(3), Some(CHANGE), &both, otherwise, &graphics),
        ],
        [
            otherwise,
            Some(graphic(11)),
            Some(graphic(12)),
            Some(graphic(13))
        ]
    );
    let same = TaskChange {
        old: TaskSprites::of_data([1, 2, 2, 4]),
        ..CHANGE
    };
    assert_eq!(
        swapped(graphic(2), Some(same), &both, otherwise, &graphics),
        Some(graphic(12)),
        "the first slot"
    );
}

/// A sprite none of the slots holds takes the other one (the carrying sprite is none of them);
/// a new task's sprite the table lacks is none, and an old task's sprite the table lacks (a null
/// pointer) matches nothing.
#[test]
fn sprite_not_of_old_task_takes_other_one() {
    let graphics = graphics();
    let otherwise = Some(graphic(4));
    let all = [
        TaskSprite::Moving,
        TaskSprite::Proceeding,
        TaskSprite::Working,
    ];
    assert_eq!(
        swapped(graphic(4), Some(CHANGE), &all, None, &graphics),
        None,
        "the carrying sprite"
    );
    let unlinked_next = TaskChange {
        next: TaskSprites::of_data([21, 12, 13, 14]),
        ..CHANGE
    };
    assert_eq!(
        swapped(graphic(1), Some(unlinked_next), &all, otherwise, &graphics),
        None
    );
    let unlinked_old = TaskChange {
        old: TaskSprites::of_data([5, 2, 3, 4]),
        ..CHANGE
    };
    assert_eq!(
        swapped(graphic(5), Some(unlinked_old), &all, otherwise, &graphics),
        otherwise
    );
}

/// A Hunt task (110) of id `id` for targets of class `class` with the sprites `sprites`.
fn hunt_task(id: i16, class: Option<i16>, sprites: [i16; 4]) -> Task {
    Task {
        id: task_id(id),
        action_kind: HUNT,
        object_group: class.map(ClassId::named),
        sprites: TaskSprites::of_data(sprites),
        ..Task::default()
    }
}

/// A master whose task list is `tasks`, standing with sprite `standing`.
fn tasked(tasks: Vec<Task>, standing: i16) -> Master {
    Master {
        standing_sprite: Some(SpriteId::named(standing)),
        action: Some(ActionPart {
            tasks,
            ..ActionPart::default()
        }),
        ..Master::default()
    }
}

/// A `MoveTo` to (5, 5) in `state` showing sprite `sprite`.
fn walk(state: MoveTo, sprite: i16) -> MoveToAction {
    MoveToAction {
        state,
        ..MoveToAction::to_point(
            Position::new(5.0, 5.0, 0.0),
            Distance::ZERO,
            Some(graphic(sprite)),
            Speed::new(1.0),
        )
    }
}

/// A Hunt on `task` walking with `move_to`.
fn hunt_walking(task: &Task, move_to: MoveToAction) -> Action {
    Action::Hunt(HuntAction {
        task: HuntTask::of(task),
        state: HuntState::Walk {
            move_to: Some(move_to),
        },
        ..hunting(7)
    })
}

/// The unit's sprite, its Sprite stream and the task change's run on them.
struct Shown {
    sprite: Option<ActiveSprite>,
    rng: Well512,
}

impl Shown {
    /// A unit showing sprite `sprite`.
    fn showing(sprite: i16) -> Self {
        let mut rng = Well512::default();
        let sprite = Some(ActiveSprite::start(
            graphic(sprite),
            &mut rng,
            tuning().sprite,
        ));
        Self { sprite, rng }
    }

    /// The view of [`take_tasks`] and the slots 0x50.
    fn looks(&mut self) -> Looks<'_> {
        Looks {
            sprite: &mut self.sprite,
            stats: None,
            rng: &mut self.rng,
            sprite_tuning: tuning().sprite,
        }
    }

    /// The id of the sprite it shows.
    fn id(&self) -> Option<i16> {
        self.sprite
            .map(|active_sprite| active_sprite.graphic.id.get())
    }
}

/// The graphics of sprites 1 to 5, 11 to 15 and 21 to 25.
fn task_graphics() -> Graphics {
    Graphics::from(
        [1, 2, 3, 4, 5, 11, 12, 13, 14, 15, 21, 22, 23, 24, 25]
            .map(|id| (SpriteId::named(id), graphic(id))),
    )
}

/// `FUN_140a78150`: an action takes the last of the new master's tasks with its task's action
/// type, class and target, its sub-action's slot 0x50 swapping the old task's sprites for that
/// task's; the moving `MoveTo`'s slot 0x60 shows its new sprite.
#[test]
fn an_action_takes_the_last_task_of_the_new_master_with_its_task_key() {
    let graphics = task_graphics();
    let held = hunt_task(0, None, [1, 2, 3, 4]);
    let old = tasked(vec![held], 5);
    let first = hunt_task(0, None, [11, 12, 13, 14]);
    let other_class = hunt_task(1, Some(3), [21, 22, 23, 24]);
    let last = hunt_task(2, None, [21, 22, 23, 24]);
    let new = tasked(vec![first, other_class, last], 15);
    let mut list = ActionList::default();
    list.put(hunt_walking(&held, walk(MoveTo::Moving, 1)));
    let mut shown = Shown::showing(1);
    take_tasks(&mut list, &old, &new, &graphics, &mut shown.looks());
    let hunt = list.hunt().expect("the Hunt");
    assert_eq!(hunt.task, HuntTask::of(&last));
    assert_eq!(
        hunt.state.move_to().and_then(|move_to| move_to.sprite),
        Some(graphic(21))
    );
    assert_eq!(shown.id(), Some(21), "slot 0x60 shows the new sprite");
}

/// An action whose task the new master has no task of the same key for changes nothing, an
/// empty list too (`JS`): no slot 0x50 runs, no sprite shows, the task stays.
#[test]
fn an_action_whose_task_the_new_master_lacks_changes_nothing() {
    let graphics = task_graphics();
    let held = hunt_task(0, None, [1, 2, 3, 4]);
    let old = tasked(vec![held], 5);
    for new in [
        tasked(vec![hunt_task(0, Some(3), [11, 12, 13, 14])], 15),
        tasked(Vec::new(), 15),
    ] {
        let mut list = ActionList::default();
        list.put(hunt_walking(&held, walk(MoveTo::Moving, 5)));
        let before = list.clone();
        let mut shown = Shown::showing(5);
        let rng = shown.rng.clone();
        take_tasks(&mut list, &old, &new, &graphics, &mut shown.looks());
        assert_eq!(list, before);
        assert_eq!((shown.id(), shown.rng == rng), (Some(5), true));
    }
}

/// An action without a task runs its slot 0x50 with none: a top `MoveTo` that shows the old
/// master's standing sprite takes the new master's and, moving, shows it; one that is not
/// moving shows nothing.
#[test]
fn an_action_without_task_takes_the_new_master_with_no_tasks() {
    let graphics = task_graphics();
    let old = tasked(Vec::new(), 5);
    let new = tasked(vec![hunt_task(0, None, [11, 12, 13, 14])], 15);
    for (state, shows) in [(MoveTo::Moving, Some(15)), (MoveTo::Start, Some(5))] {
        let mut list = ActionList::default().with_move_to(Some(walk(state, 5)));
        let mut shown = Shown::showing(5);
        take_tasks(&mut list, &old, &new, &graphics, &mut shown.looks());
        assert_eq!(
            list.move_to().and_then(|move_to| move_to.sprite),
            Some(graphic(15))
        );
        assert_eq!(shown.id(), shows);
    }
}

/// The task each action holds (`+0x48`): a Hunt's, a Build's, a Transport's on a task, a Make's
/// and a wait's whole task; none for a `MoveTo`, a Transport of the unload command, an Explore,
/// an Enter and an Artifact (genie holds none of the last three).
#[test]
fn the_held_task_is_the_one_each_action_holds() {
    let task = hunt_task(4, Some(3), [1, 2, 3, 4]);
    let of = |task: &Task| {
        Some(HeldTask {
            key: task.key(),
            sprites: task.sprites,
        })
    };
    let point = Position::new(5.0, 5.0, 0.0);
    let build_action = started(object_id(30), point, Distance::ZERO, BUILD_TASK);
    assert_eq!(
        hunt_walking(&task, walk(MoveTo::Start, 1)).held_task(),
        of(&task)
    );
    assert_eq!(
        Action::Build(build_action).held_task(),
        Some(HeldTask {
            key: BUILD_TASK.key,
            sprites: BUILD_TASK.sprites,
        })
    );
    assert_eq!(
        Action::Transport(TransportAction::on_task(Some(task), point)).held_task(),
        of(&task)
    );
    let foundation = RoleId::new(object_id(30));
    assert_eq!(
        Action::WaitSolidify(WaitSolidify::new(task, foundation)).held_task(),
        of(&task)
    );
    for none in [
        Action::MoveTo(walk(MoveTo::Start, 1)),
        Action::Transport(TransportAction::new(point)),
        Action::Explore(ExploreAction {
            state: ExploreState::Wait,
            point,
        }),
        Action::Enter(EnterAction::new(RoleId::new(object_id(30)), point, &task)),
        Action::Artifact(Artifact::new(PlayerId::new(1), None)),
    ] {
        assert_eq!(none.held_task(), None, "{none:?}");
    }
}

/// The end of a task change: an action holds the new task and reads its values; a Build keeps
/// its builder's work rate; an action that holds no task stays as it was.
#[test]
fn an_action_holds_the_new_task_and_reads_its_values() {
    let point = Position::new(5.0, 5.0, 0.0);
    let new = Task {
        action_kind: GATHER,
        work_range: Distance::new(2.0),
        ..hunt_task(9, Some(3), [11, 12, 13, 14])
    };
    let mut hunt = hunt_walking(&hunt_task(0, None, [1, 2, 3, 4]), walk(MoveTo::Start, 1));
    hunt.hold_task(&new);
    assert_eq!(hunt.held_task().map(|held| held.key), Some(new.key()));
    let mut build_action = Action::Build(started(object_id(30), point, Distance::ZERO, BUILD_TASK));
    build_action.hold_task(&new);
    assert_eq!(
        build_action,
        Action::Build(BuildAction {
            task: BuildTask::of(&new, BUILD_TASK.work_rate),
            ..started(object_id(30), point, Distance::ZERO, BUILD_TASK)
        })
    );
    let mut transport = Action::Transport(TransportAction::new(point));
    transport.hold_task(&new);
    assert_eq!(
        transport,
        Action::Transport(TransportAction::on_task(Some(new), point))
    );
    let mut move_to = Action::MoveTo(walk(MoveTo::Start, 1));
    move_to.hold_task(&new);
    assert_eq!(move_to, Action::MoveTo(walk(MoveTo::Start, 1)));
}

/// An action's sub-action (`+0x50`): a walk's `MoveTo`, a Hunt's Attack in its state 12, an
/// Artifact's walk; none for an Artifact's Enter, a top `MoveTo`, a state that does not walk.
#[test]
fn the_sub_action_is_the_walk_or_the_hunt_attack() {
    let point = Position::new(5.0, 5.0, 0.0);
    let task = hunt_task(0, None, [1, 2, 3, 4]);
    let mut hunt = hunt_walking(&task, walk(MoveTo::Start, 1));
    assert_eq!(
        hunt.sub_action_mut(),
        Some(SubActionMut::MoveTo(&mut walk(MoveTo::Start, 1)))
    );
    let mut gathering = Action::Hunt(hunting(7));
    assert_eq!(gathering.sub_action_mut(), None);
    let mut artifact_walk = Action::Artifact(Artifact {
        sub: Some(ArtifactSub::Walk {
            move_to: walk(MoveTo::Start, 2),
        }),
        ..Artifact::new(PlayerId::new(1), None)
    });
    assert_eq!(
        artifact_walk.sub_action_mut(),
        Some(SubActionMut::MoveTo(&mut walk(MoveTo::Start, 2)))
    );
    let mut artifact_enter = Action::Artifact(Artifact {
        sub: Some(ArtifactSub::Enter {
            enter: EnterAction::new(RoleId::new(object_id(30)), point, &task),
        }),
        ..Artifact::new(PlayerId::new(1), None)
    });
    assert_eq!(artifact_enter.sub_action_mut(), None);
    let mut explore = Action::Explore(ExploreAction {
        state: ExploreState::Walk {
            move_to: Some(walk(MoveTo::Start, 3)),
        },
        point,
    });
    assert_eq!(
        explore.sub_action_mut(),
        Some(SubActionMut::MoveTo(&mut walk(MoveTo::Start, 3)))
    );
    let mut build_action = Action::Build(started(object_id(30), point, Distance::ZERO, BUILD_TASK));
    assert!(matches!(
        build_action.sub_action_mut(),
        Some(SubActionMut::MoveTo(_))
    ));
    assert_eq!(
        Action::MoveTo(walk(MoveTo::Start, 1)).sub_action_mut(),
        None
    );
    assert_eq!(
        Action::Transport(TransportAction::new(point)).sub_action_mut(),
        None
    );
}

/// The slot 0x50 of an action and of its sub-action with no tasks: a top `MoveTo` takes the new
/// master's standing sprite for the old one's; a Hunt's walk alike; a Build's own slot 0x50 is
/// empty.
#[test]
fn the_slots_0x50_take_the_new_master_sprites() {
    let graphics = task_graphics();
    let old = tasked(Vec::new(), 5);
    let new = tasked(Vec::new(), 15);
    let mut shown = Shown::showing(5);
    let mut move_to = Action::MoveTo(walk(MoveTo::Start, 5));
    action_take_master(
        &mut move_to,
        &old,
        &new,
        None,
        &graphics,
        &mut shown.looks(),
    );
    assert_eq!(move_to, Action::MoveTo(walk(MoveTo::Start, 15)));
    let task = hunt_task(0, None, [1, 2, 3, 4]);
    let mut hunt = hunt_walking(&task, walk(MoveTo::Start, 5));
    action_take_master(&mut hunt, &old, &new, None, &graphics, &mut shown.looks());
    assert_eq!(hunt, hunt_walking(&task, walk(MoveTo::Start, 5)));
    sub_action_take_master(&mut hunt, &old, &new, None, &graphics, &mut shown.looks());
    assert_eq!(hunt, hunt_walking(&task, walk(MoveTo::Start, 15)));
    assert_eq!(shown.id(), Some(5));
}

/// The unit's slot 0x68 with its own sprite when that is one of the new task's four changes
/// nothing (`FUN_141217000` returns for the sprite shown): the instance stays and the Sprite
/// stream draws nothing; the action still takes the new task.
#[test]
fn the_unit_showing_one_sprite_of_the_new_task_keeps_its_instance() {
    let graphics = task_graphics();
    let held = hunt_task(0, None, [1, 2, 3, 4]);
    let old = tasked(vec![held], 5);
    let found = hunt_task(1, None, [11, 12, 13, 14]);
    let new = tasked(vec![found], 15);
    let mut list = ActionList::default();
    list.put(Action::Hunt(HuntAction {
        task: HuntTask::of(&held),
        ..hunting(7)
    }));
    let mut shown = Shown::showing(12);
    let before = (shown.sprite, shown.rng.clone());
    take_tasks(&mut list, &old, &new, &graphics, &mut shown.looks());
    assert_eq!((shown.sprite, shown.rng), before);
    assert_eq!(
        list.hunt().map(|hunt| hunt.task),
        Some(HuntTask::of(&found))
    );
}
