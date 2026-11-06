//! An action's slot 0x50 across a task change: the action's task and the new master's task that
//! takes its place, whose sprites an Attack and a `MoveTo` swap ([`TaskChange`]), and the task
//! change of an object's actions (`FUN_140a97970` -> `FUN_140a78150`, [`take_tasks`]).

use crate::construction::build::BuildTask;
use crate::construction::repair::RepairTask;
use crate::gathering::hunt::HuntTask;
use crate::math::rng::Well512;
use crate::movement::MoveStats;
use crate::movement::move_to::MoveTo;
use crate::movement::move_to::MoveToAction;
use crate::objects::Graphics;
use crate::objects::SpriteId;
use crate::objects::actions::{Action, ActionList, SubActionMut};
use crate::objects::masters::Master;
use crate::objects::masters::Task;
use crate::objects::masters::TaskKey;
use crate::objects::masters::TaskSprites;
use crate::objects::sprite::Graphic;
use crate::objects::sprite::{ActiveSprite, new_sprite};
use crate::world::layer::generated::SpriteTuning;

/// The tasks of a task change (`FUN_140a78150`, an upgrade's or a changed sprite's): the sprites
/// of the action's task (`old`) and of the new master's task that takes its place (`next`). An
/// action's slot 0x50 takes them as its third and fourth arguments; slot 0x90 with 1 and an
/// action without a task pass none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TaskChange {
    /// The action's task's sprites.
    pub(crate) old: TaskSprites,
    /// The sprites of the new master's task that takes its place.
    pub(crate) next: TaskSprites,
}

/// A task's sprite that a slot 0x50 compares with the one an action holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskSprite {
    /// `+0x40`, the sprite while it walks to its target.
    Moving,
    /// `+0x48`, the sprite while it comes on.
    Proceeding,
    /// `+0x50`, the sprite while it works.
    Working,
}

impl TaskSprite {
    /// This sprite of a task's `sprites`.
    const fn of(self, sprites: TaskSprites) -> Option<SpriteId> {
        match self {
            Self::Moving => sprites.moving,
            Self::Proceeding => sprites.proceeding,
            Self::Working => sprites.working,
        }
    }
}

/// What a slot 0x50 holds in place of the sprite `shown`: with `tasks`, the new task's sprite of
/// the first of `slots` whose old task's sprite `shown` is (none where the new task has none or
/// `graphics` lacks it, a null pointer); else `otherwise`. The game compares the pointers, so a
/// sprite `graphics` lacks matches nothing.
pub(crate) fn swapped(
    shown: Graphic,
    tasks: Option<TaskChange>,
    slots: &[TaskSprite],
    otherwise: Option<Graphic>,
    graphics: &Graphics,
) -> Option<Graphic> {
    let Some(change) = tasks else {
        return otherwise;
    };
    slots
        .iter()
        .find(|slot| graphics.resolve(slot.of(change.old)) == Some(shown))
        .map_or(otherwise, |slot| graphics.resolve(slot.of(change.next)))
}

/// What a task change reads of an action's task (`+0x48`): its key, which the new master's task
/// it takes matches, and its sprites, which the action's slot 0x50 swaps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeldTask {
    /// Its action type, target class and target master.
    pub(crate) key: TaskKey,
    /// Its four sprites.
    pub(crate) sprites: TaskSprites,
}

impl HeldTask {
    /// What a task change reads of `task`.
    const fn of(task: &Task) -> Self {
        Self {
            key: task.key(),
            sprites: task.sprites,
        }
    }
}

impl Action {
    /// Its task (`+0x48`) as a task change reads it, none for an action without one: the task
    /// factory gives an Attack none (`FUN_140a97e00` case 7), a `MoveTo` and a Missile hold none,
    /// nor do a Transport of the unload command and an Explore of the explore command. The
    /// Attack's constructors (`FUN_140a7c210`, `FUN_140a7bf50`) and the Missile's
    /// (`FUN_140a9a040`) store no task.
    pub(crate) fn held_task(&self) -> Option<HeldTask> {
        match self {
            Self::Hunt(hunt_action) => Some(HeldTask {
                key: hunt_action.task.key,
                sprites: hunt_action.task.sprites,
            }),
            Self::Gather(gather_action) => Some(HeldTask {
                key: gather_action.task.key,
                sprites: gather_action.task.sprites,
            }),
            Self::Repair(repair_action) => Some(HeldTask {
                key: repair_action.task.key,
                sprites: repair_action.task.sprites,
            }),
            Self::Build(build_action) => Some(HeldTask {
                key: build_action.task.key,
                sprites: build_action.task.sprites,
            }),
            Self::Transport(transport_action) => transport_action.task.as_ref().map(HeldTask::of),
            Self::Make(make_action) => Some(HeldTask::of(&make_action.task)),
            Self::Fly(fly) => Some(HeldTask::of(&fly.task)),
            Self::WaitSolidify(wait_solidify) => Some(HeldTask::of(&wait_solidify.task)),
            Self::Enter(enter_action) => Some(HeldTask::of(&enter_action.task)),
            Self::Explore(explore_action) => explore_action.task.as_ref().map(HeldTask::of),
            Self::Artifact(artifact) => artifact.task.as_ref().map(HeldTask::of),
            Self::Attack(_) | Self::MoveTo(_) | Self::Missile(_) => None,
        }
    }

    /// The end of a task change (`FUN_140a78150`): it holds `task` (`+0x48`), and so reads its
    /// values from then on; a Build keeps its builder's work rate. An action that holds no task
    /// ([`Action::held_task`]) is not changed: no task change reaches it.
    pub(crate) const fn hold_task(&mut self, task: &Task) {
        match self {
            Self::Hunt(hunt_action) => hunt_action.task = HuntTask::of(task),
            Self::Gather(gather_action) => gather_action.task = HuntTask::of(task),
            Self::Repair(repair_action) => repair_action.task = RepairTask::of(task),
            Self::Build(build_action) => {
                build_action.task = BuildTask::of(task, build_action.task.work_rate);
            }
            Self::Transport(transport_action) => transport_action.task = Some(*task),
            Self::Make(make_action) => make_action.task = *task,
            Self::Fly(fly) => fly.task = *task,
            Self::WaitSolidify(wait_solidify) => wait_solidify.task = *task,
            Self::Enter(enter_action) => enter_action.task = *task,
            Self::Explore(explore_action) => explore_action.task = Some(*task),
            Self::Artifact(artifact) => artifact.task = Some(*task),
            Self::Attack(_) | Self::MoveTo(_) | Self::Missile(_) => {}
        }
    }
}

/// What an action's slot 0x60 and a unit's slot 0x68 show on in a task change or a master
/// change: the unit's sprite, its movement (whose top speed follows the sprite) and the Sprite
/// stream.
pub(crate) struct Looks<'unit> {
    /// The unit's sprite.
    pub(crate) sprite: &'unit mut Option<ActiveSprite>,
    /// Its movement, none for a unit that does not move.
    pub(crate) stats: Option<&'unit mut MoveStats>,
    /// The Sprite stream.
    pub(crate) rng: &'unit mut Well512,
    /// The sprites' tuning.
    pub(crate) sprite_tuning: SpriteTuning,
}

impl Looks<'_> {
    /// Slot 0x68 in a task or master change (`FUN_141217000`): the unit shows `graphic`
    /// ([`new_sprite`]). The slice holds sprites from an attack on: a unit whose sprite it does not
    /// hold keeps none, but the new instance still draws from the Sprite stream.
    pub(crate) fn show(&mut self, graphic: Graphic) {
        if self.sprite.is_some() {
            new_sprite(
                self.sprite,
                self.stats.as_deref_mut(),
                graphic,
                self.rng,
                self.sprite_tuning,
            );
        } else {
            let _ = ActiveSprite::start(graphic, self.rng, self.sprite_tuning);
        }
    }
}

/// `FUN_140a97970` -> `FUN_140a78150` on each action of `list`, the head first, as its unit's
/// master `old` gives way to `new` (the unit still of `old`), with the sprites' table
/// `graphics`:
/// - an action without a task runs its slot 0x50 and its sub-action's (`FUN_140a979b0`) with
///   none ([`action_take_master`], [`sub_action_take_master`]); a new master without a task
///   list (`+0x788` null) would take that path too, which no master of the `.dat` has (its load
///   makes the list, `FUN_1410a14e0`);
/// - one with a task takes the last of `new`'s tasks with its task's key ([`TaskKey`]); none,
///   an empty list too (`JS`), changes nothing;
/// - else its slot 0x50 and its sub-action's run with both tasks' sprites ([`TaskChange`]), and
///   it holds the new task ([`Action::hold_task`]). Before them the unit's slot 0x68 runs with
///   its sprite when that is one of the new task's four; every class's slot 0x68
///   (`FUN_141217000`, `FUN_141158620`) returns at once for the sprite the unit shows, so it
///   changes nothing.
pub(crate) fn take_tasks(
    list: &mut ActionList,
    old: &Master,
    new: &Master,
    graphics: &Graphics,
    looks: &mut Looks<'_>,
) {
    let tasks = &new.action_part().tasks;
    for action in list {
        let Some(held) = action.held_task() else {
            action_take_master(action, old, new, None, graphics, looks);
            sub_action_take_master(action, old, new, None, graphics, looks);
            continue;
        };
        let Some(found) = tasks.iter().rev().find(|task| task.key() == held.key) else {
            continue;
        };
        let change = Some(TaskChange {
            old: held.sprites,
            next: found.sprites,
        });
        action_take_master(action, old, new, change, graphics, looks);
        sub_action_take_master(action, old, new, change, graphics, looks);
        action.hold_task(found);
    }
}

/// An action's slot 0x50 with the new master `next` and the tasks of a task change (`tasks`), its
/// unit still of `old`: an Attack's ([`Arms::take_master`], `FUN_140a7f800`) and a `MoveTo`'s
/// ([`move_to_take_master`]); every other action's is the base's, empty (`0x140a782d0`).
///
/// [`Arms::take_master`]: crate::combat::attack::Arms::take_master
pub(crate) fn action_take_master(
    action: &mut Action,
    old: &Master,
    next: &Master,
    tasks: Option<TaskChange>,
    graphics: &Graphics,
    looks: &mut Looks<'_>,
) {
    match action {
        Action::Attack(attack) => attack.arms.take_master(next, tasks, graphics),
        Action::MoveTo(move_to) => move_to_take_master(move_to, old, next, tasks, graphics, looks),
        Action::Artifact(_)
        | Action::Build(_)
        | Action::Repair(_)
        | Action::Fly(_)
        | Action::Hunt(_)
        | Action::Gather(_)
        | Action::Explore(_)
        | Action::Enter(_)
        | Action::Transport(_)
        | Action::Missile(_)
        | Action::Make(_)
        | Action::WaitSolidify(_) => {}
    }
}

/// `FUN_140a979b0` on an action's sub-actions (`+0x50`) with the new master `next` and the tasks
/// `tasks`, its unit still of `old`: its sub-action's slot 0x50 ([`Action::sub_action_mut`]: a
/// walk's `MoveTo`, [`move_to_take_master`]; a Hunt's Attack, [`Arms::take_master`]). The
/// sub-action's own sub-actions do not take part.
///
/// [`Arms::take_master`]: crate::combat::attack::Arms::take_master
pub(crate) fn sub_action_take_master(
    action: &mut Action,
    old: &Master,
    next: &Master,
    tasks: Option<TaskChange>,
    graphics: &Graphics,
    looks: &mut Looks<'_>,
) {
    match action.sub_action_mut() {
        Some(SubActionMut::MoveTo(move_to)) => {
            move_to_take_master(move_to, old, next, tasks, graphics, looks);
        }
        Some(SubActionMut::Attack(attack)) => attack.arms.take_master(next, tasks, graphics),
        None => {}
    }
}

/// A `MoveTo`'s slot 0x50 ([`MoveToAction::take_master`], `FUN_140aa1fb0`), which ends in its
/// slot 0x60 (`0x140aa1f90`): in state 4 the unit's slot 0x68 with its sprite ([`Looks::show`];
/// a null sprite shows `old`'s standing one, `FUN_141158620`).
fn move_to_take_master(
    move_to: &mut MoveToAction,
    old: &Master,
    next: &Master,
    tasks: Option<TaskChange>,
    graphics: &Graphics,
    looks: &mut Looks<'_>,
) {
    move_to.take_master(old, next, tasks, graphics);
    if move_to.state == MoveTo::Moving
        && let Some(graphic) = move_to
            .sprite
            .or_else(|| graphics.resolve(old.standing_sprite))
    {
        looks.show(graphic);
    }
}

#[cfg(test)]
mod tests;
