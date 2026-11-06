//! A saved object's top action as the slice models it ([`saved_actions`]).

use crate::combat::missile::ShooterId;
use crate::construction::build::{BuildAction, BuildTask};
use crate::garrison::ContainerId;
use crate::gathering::DropSiteId;
use crate::map::Distance;
use crate::map::Point;
use crate::map::{Offset, Position};
use crate::movement::fly::{FLY, Fly};
use crate::movement::move_to::MoveWait;
use crate::movement::move_to::{MOVE_TO, MoveTo, MoveToAction, TargetMoved};
use crate::objects::ObjectId;
use crate::objects::SpriteId;
use crate::objects::Voice;
use crate::objects::layers;
use crate::objects::masters::{ActionKind, Task};
use crate::objects::sprite::Graphic;
use crate::ownership::artifact::ARTIFACT_KIND;
use crate::world::Seconds;
use crate::world::load::object::OTHER_SUB_ACTION;
use crate::world::load::object::missile_part;
use crate::world::load::sub_actions::saved_attack;
use crate::{GenieError, GenieResult};
use genie_import::save::parts::combat_master;
use genie_save::generated::world::{Action, PlayerObjects, RgeAction, RgeStaticObject};

mod gathering;

use crate::combat::attack::ATTACK;
use crate::combat::missile::{
    HitMode, MISSILE, MissileAction, MissileState, MissileTargetId, Origin, StrikeResult,
};
use crate::construction::build::BuildState;
use crate::construction::repair::{REPAIR, RepairAction, RepairState, RepairTask};
use crate::construction::wait_solidify::{WAIT_SOLIDIFY, WaitSolidify, WaitSolidifyState};
use crate::construction::{FoundationId, RepairTargetId};
use crate::garrison::ENTER;
use crate::garrison::transport::{TRANSPORT, TransportAction, TransportMode, TransportState};
use crate::gathering::gather::GATHER;
use crate::gathering::hunt::HUNT;
use crate::gathering::make::MAKE;
use crate::movement::explore::{EXPLORE, ExploreAction};
use crate::movement::fly::FlyState;
use crate::movement::{Acceleration, Speed};
use crate::objects::Graphics;
use crate::ownership::artifact::{Artifact, ArtifactState, ArtifactSub};
use crate::world::load::top_actions::gathering::{import_gather, import_hunt, import_make};
use genie_save::generated::world::ActionList;

/// A task slot's task of a master (the master, the slot).
type TaskAt<'env> = &'env dyn Fn(i16, i16) -> GenieResult<Task>;

/// What `saved_actions` finds an object without actions: a state the caller rules out.
const NO_ACTIONS: GenieError =
    GenieError::unreachable("saved_actions called for an object without actions");

/// The top action of a saved object as the slice models it: the Artifact action (107), the
/// Build action (101) with its `MoveTo`, a `MoveTo` of its own (type 1), a bird's Fly action (10)
/// with its `MoveTo`, the Hunt (110) with its Attack or `MoveTo` (and in state 8 its second
/// target, the drop site) and a farm's Make (21). Any other saved action, and a second one
/// below the head ([`not_imported`]), stops the object's player.
#[allow(clippy::too_many_arguments)]
pub fn saved_actions(
    object: &genie_save::Object,
    rge_static_object: &RgeStaticObject,
    objects: &PlayerObjects,
    work: impl Fn(i16) -> GenieResult<BuildTask>,
    fly_task: impl Fn(i16) -> GenieResult<Task>,
    task_at: impl Fn(i16, i16) -> GenieResult<Task>,
    graphics: Option<&Graphics>,
    actions: &mut layers::Action,
) -> GenieResult<()> {
    let mut entries = object
        .actions()
        .map(ActionList::entries)
        .ok_or(NO_ACTIONS)?;
    let (kind, action) = entries.next().ok_or(NO_ACTIONS)?;
    let kind = ActionKind::try_from(kind)?;
    if let Some((below, _)) = entries.next() {
        return Err(not_imported().with("below the head", below));
    }
    let base = action
        .base()
        .ok_or(GenieError::unsupported("a saved action without its base"))?;
    if base.target2_id != -1 && !holds_second_target(kind) {
        return Err(GenieError::unsupported(
            "a saved action with a second target",
        ));
    }
    let saved = Saved {
        object,
        static_part: rge_static_object,
        objects,
        action,
        base,
        graphics,
    };
    match kind {
        ARTIFACT_KIND => import_artifact(&saved, &task_at, actions),
        MOVE_TO => import_move_to(&saved, actions),
        MISSILE => import_missile(&saved, actions),
        MAKE => import_make(&saved, &task_at, actions),
        FLY => import_fly(&saved, &fly_task, actions),
        ActionKind::BUILD => import_build(&saved, &work, actions),
        REPAIR => import_repair(&saved, &task_at, actions),
        HUNT => import_hunt(&saved, &task_at, actions),
        GATHER => import_gather(&saved, &task_at, actions),
        ENTER => import_enter(&saved, &task_at, actions),
        TRANSPORT => import_transport(&saved, &task_at, actions),
        EXPLORE => import_explore(&saved, &task_at, actions),
        ATTACK => import_attack(&saved, actions),
        WAIT_SOLIDIFY => import_wait_solidify(&saved, &task_at, actions),
        _ => Err(not_imported().with("action type", kind)),
    }
}

/// A saved action the slice does not import: one of a type it does not model, or one below the
/// head (the slice's lists hold one action: the restart of the action under a deleted head, its
/// slot 0x28, is not ported).
const fn not_imported() -> GenieError {
    GenieError::unsupported("a saved action the slice does not import")
}

/// The first action of a saved action list with its type (the save's `action_type`, parsed),
/// none for an empty list.
///
/// # Errors
///
/// A negative action type.
pub(crate) fn first_action(list: &ActionList) -> GenieResult<Option<(ActionKind, &Action)>> {
    list.entries()
        .next()
        .map(|(kind, action)| Ok((ActionKind::try_from(kind)?, action)))
        .transpose()
}

/// The second target (+0x34): only the Hunt, the Gather and the Build (their drop site), the
/// missile (its shooter) and the Attack hold one in the slice.
const fn holds_second_target(kind: ActionKind) -> bool {
    matches!(kind, HUNT | GATHER | MISSILE | ATTACK | ActionKind::BUILD)
}

/// What a saved top action's import reads: the object, its static part, its player's objects,
/// the action and its base, and the sprites' table.
struct Saved<'save> {
    object: &'save genie_save::Object,
    static_part: &'save RgeStaticObject,
    objects: &'save PlayerObjects,
    action: &'save Action,
    base: &'save RgeAction,
    graphics: Option<&'save Graphics>,
}

impl Saved<'_> {
    /// The task of the action's slot (`short_val`).
    fn task(&self, task_at: TaskAt<'_>) -> GenieResult<Task> {
        task_at(self.static_part.master_obj, self.base.short_val)
    }

    /// The action's first sub-action when it is a `MoveTo`, none without one; `other` for any
    /// other.
    fn move_to_sub(&self, other: GenieError) -> GenieResult<Option<&Action>> {
        match first_action(&self.base.sub_actions)? {
            None => Ok(None),
            Some((MOVE_TO, sub)) => Ok(Some(sub)),
            Some(_) => Err(other),
        }
    }

    /// The action's target point.
    const fn point(&self) -> Position {
        Position::new(self.base.target_x, self.base.target_y, self.base.target_z)
    }
}

/// A saved `MoveTo`'s state as the slice models it.
const fn move_to_state(state: u8) -> GenieResult<MoveTo> {
    match state {
        1 => Ok(MoveTo::Arrived),
        2 => Ok(MoveTo::Waiting),
        3 => Ok(MoveTo::Start),
        4 => Ok(MoveTo::Moving),
        13 => Ok(MoveTo::Failed),
        _ => Err(GenieError::unsupported(
            "a saved MoveTo in a state the slice does not model",
        )),
    }
}

/// A `MoveTo`'s range (`+0x68`).
fn sub_range(sub: &Action) -> GenieResult<f32> {
    sub.move_to_range()
        .ok_or(GenieError::unsupported("a saved MoveTo without its range"))
}

/// The point of an action without a target.
fn target_point(rge_action: &RgeAction) -> Option<Position> {
    (rge_action.target_id == -1).then_some(Position::new(
        rge_action.target_x,
        rge_action.target_y,
        rge_action.target_z,
    ))
}

/// The Artifact action (107): its state and timer, its `MoveTo` to its target and its task's
/// moving sprite.
fn import_artifact(
    saved: &Saved<'_>,
    task_at: TaskAt<'_>,
    actions: &mut layers::Action,
) -> GenieResult<()> {
    let base = saved.base;
    // its task (the base's `short_val`, −1 for none)
    let task = if base.short_val < 0 {
        None
    } else {
        Some(saved.task(task_at)?)
    };
    let sub = saved.move_to_sub(GenieError::unsupported(
        "a saved Artifact action with another sub-action",
    ))?;
    // a move to an object (set_state 5) holds the target and a quiet MoveTo (to the target,
    // or to a town centre's point); a move to a point (set_state 4) a MoveTo that is not quiet
    let move_to = sub
        .map(|action| saved_sub_move_to(action, saved.graphics))
        .transpose()?;
    actions.list.set_artifact(Some(Artifact {
        state: ArtifactState::of_byte(base.state)?,
        timer: Seconds::new(base.timer),
        gaia: saved.static_part.owner == 0,
        sub: move_to.map(|move_to| ArtifactSub::Walk { move_to }),
        goal: if base.target_id == -1 {
            move_to.and_then(|_| target_point(base))
        } else {
            // the target's place, `+0x38`
            (base.target_x != -1.0).then_some(saved.point())
        },
        target: ObjectId::of_field(base.target_id),
        task,
    }));
    Ok(())
}

/// A `MoveTo` of its own (type 1): its state, goal, range, sprite and wait; not quiet.
fn import_move_to(saved: &Saved<'_>, actions: &mut layers::Action) -> GenieResult<()> {
    let move_to = saved_move_to(saved.action, saved.graphics)?;
    actions.list.set_move_to(Some(move_to));
    Ok(())
}

/// A saved action's sub-action `MoveTo` ([`saved_move_to`]). The game makes each one to an
/// object quiet (`FUN_140aa0df0` with `+0x60` 1, or its setter `FUN_140a7a170` with 1), and
/// only a quiet one lets its target go without a release of its own
/// ([`MoveToAction::sub_update`]).
///
/// # Errors
///
/// [`saved_move_to`]'s, and a loud one to an object (`invalid_input`).
///
/// [`MoveToAction::sub_update`]: crate::movement::move_to::MoveToAction::sub_update
pub(crate) fn saved_sub_move_to(
    action: &Action,
    graphics: Option<&Graphics>,
) -> GenieResult<MoveToAction> {
    let move_to = saved_move_to(action, graphics)?;
    if move_to.voice.is_loud()
        && let Some(target) = move_to.target
    {
        return Err(GenieError::invalid_input(
            "a saved sub-action MoveTo to an object that is not quiet",
        )
        .with("target", target.get()));
    }
    Ok(move_to)
}

/// A saved `MoveTo` (type 1), of its own or an action's sub-action: its state, goal, target,
/// range, sprite, voice and wait.
pub(crate) fn saved_move_to(
    action: &Action,
    graphics: Option<&Graphics>,
) -> GenieResult<MoveToAction> {
    let base = action.base().ok_or(MOVE_TO_BASELESS)?;
    Ok(MoveToAction {
        state: move_to_state(base.state)?,
        // its point (`+0x38`..`+0x40`), for a target object the target's place it last took
        point: Position::new(base.target_x, base.target_y, base.target_z),
        target: ObjectId::of_field(base.target_id),
        range: Distance::new(sub_range(action)?),
        // its sprite (`+0x58`, the save's second `short_val`; −1 for none)
        sprite: saved_move_to_sprite(SpriteId::of_data(base.short_val_2), graphics)?,
        // `+0x60`, which the action's load reads (`FUN_140a7a2f0`)
        voice: Voice::of_byte(base.sub_action_value),
        // what it does when its target moves (`+0x61`)
        on_target_moved: TargetMoved::try_from(base.target_moved_state)?,
        // its wait (`+0x6c`, `+0x70`, `+0x74`)
        wait: saved_move_wait(action)?,
    })
}

/// A saved `MoveTo` without its base.
const MOVE_TO_BASELESS: GenieError = GenieError::unsupported("a saved MoveTo without its base");

/// A saved `MoveTo`'s sprite (`id`, none for −1) from the sprites' table.
fn saved_move_to_sprite(
    id: Option<SpriteId>,
    graphics: Option<&Graphics>,
) -> GenieResult<Option<Graphic>> {
    let Some(id) = id else {
        return Ok(None);
    };
    graphics
        .and_then(|graphics| graphics.get(id).copied())
        .map(Some)
        .ok_or_else(|| {
            GenieError::missing_data("a saved MoveTo's sprite outside the sprites' table")
                .with("sprite", id)
        })
}

/// A missile's action (8): its course and the missile object's own fields.
fn import_missile(saved: &Saved<'_>, actions: &mut layers::Action) -> GenieResult<()> {
    let (base, object) = (saved.base, saved.object);
    let ma = saved
        .action
        .rge_action_by_kind
        .as_deref()
        .and_then(|rge_action_by_kind| rge_action_by_kind.rge_action_missile.as_deref())
        .ok_or(GenieError::unsupported(
            "a saved missile action without its part",
        ))?;
    // the missile's own fields are its part (saved_unit's saved_missile)
    if missile_part(object).is_none() {
        return Err(GenieError::unsupported(
            "a missile action of an object that is not a missile",
        ));
    }
    if object.moving_part().is_none() {
        return Err(GenieError::unsupported("a missile without its moving part"));
    }
    // the hit mode copies the master's (`+0x80`, a miss's 1 is not saved)
    let hit_mode = saved
        .objects
        .master(saved.static_part.master_obj)
        .and_then(|master| master.rge_master.as_deref())
        .and_then(|rge_master| rge_master.rge_master_missile_object.as_deref())
        .map_or(HitMode::TARGET_ALONE, |rge_master_missile_object| {
            HitMode::of_data(rge_master_missile_object.missile_hit_info)
        });
    actions.list.set_missile(Some(MissileAction {
        state: MissileState::of_byte(base.state)?,
        target: MissileTargetId::of_field(base.target_id),
        fired_from: ShooterId::of_field(base.target2_id),
        point: saved.point(),
        timer: Distance::new(base.timer),
        direction: Offset::new(ma.velocity_x, ma.velocity_y, ma.velocity_z),
        ballistic_velocity: Speed::new(ma.ballistic_velocity),
        ballistic_acceleration: Acceleration::new(ma.ballistic_acceleration),
        distance: Distance::new(ma.distance),
        hit_mode,
        result: StrikeResult::try_from(ma.interacted_with_agility)?,
        origin: Origin::from_bytes(&ma.projectile_data),
        met: Vec::new(),
    }));
    Ok(())
}

/// A bird's Fly (10): its home, its `MoveTo` and its task.
fn import_fly(
    saved: &Saved<'_>,
    fly_task: &dyn Fn(i16) -> GenieResult<Task>,
    actions: &mut layers::Action,
) -> GenieResult<()> {
    let base = saved.base;
    // its MoveTo to the flight's point is not quiet
    let move_to = saved
        .move_to_sub(GenieError::unsupported(
            "a saved Fly action with another sub-action",
        ))?
        .map(|action| saved_sub_move_to(action, saved.graphics))
        .transpose()?;
    actions.list.set_fly(Some(Fly {
        state: FlyState::of_byte(base.state, move_to)?,
        timer: Seconds::new(base.timer),
        home: Point::new(base.target_x, base.target_y),
        task: fly_task(saved.static_part.master_obj)?,
    }));
    Ok(())
}

/// The Build (101): its task (`work`: with the builder's powerup), its `MoveTo` and its site.
fn import_build(
    saved: &Saved<'_>,
    work: &dyn Fn(i16) -> GenieResult<BuildTask>,
    actions: &mut layers::Action,
) -> GenieResult<()> {
    let base = saved.base;
    // its quiet MoveTo (FUN_140a7a170 with 1)
    let move_to = saved
        .move_to_sub(OTHER_SUB_ACTION)?
        .map(|action| saved_sub_move_to(action, saved.graphics))
        .transpose()?;
    let task = work(saved.static_part.master_obj)?;
    actions.list.set_build(Some(BuildAction {
        target: FoundationId::of_field(base.target_id),
        state: BuildState::of_byte(base.state, move_to)?,
        point: saved.point(),
        task,
        added: saved
            .action
            .tribe_action_build
            .as_deref()
            .is_some_and(|tribe_action_build| tribe_action_build.added_to_num_builders != 0),
        timer: Seconds::new(base.timer),
        site: DropSiteId::of_field(base.target2_id),
    }));
    Ok(())
}

/// The Repair (106): its task, its `MoveTo`, its range and whether it counts as a builder.
fn import_repair(
    saved: &Saved<'_>,
    task_at: TaskAt<'_>,
    actions: &mut layers::Action,
) -> GenieResult<()> {
    let base = saved.base;
    let part = saved
        .action
        .tribe_action_repair
        .as_deref()
        .ok_or(GenieError::missing_data("a saved Repair's own part"))?;
    // every MoveTo of the Repair is quiet: to the target, or to a point without one
    let move_to = saved
        .move_to_sub(OTHER_SUB_ACTION)?
        .map(|action| saved_sub_move_to(action, saved.graphics))
        .transpose()?;
    let task = saved.task(task_at)?;
    actions.list.set_repair(Some(RepairAction {
        target: RepairTargetId::of_field(base.target_id),
        state: RepairState::of_byte(base.state, move_to)?,
        point: saved.point(),
        timer: Seconds::new(base.timer),
        task: RepairTask::of(&task),
        range: saved
            .objects
            .master(saved.static_part.master_obj)
            .and_then(combat_master)
            .map_or(Distance::ZERO, |rge_master_combat_object| {
                Distance::new(rge_master_combat_object.weapon_range)
            }),
        counted: part.added_to_num_builders != 0,
        voice: Voice::of_byte(base.sub_action_value),
    }));
    Ok(())
}

/// The Enter (3): its task's range, class and moving sprite, its `MoveTo` and its retry.
fn import_enter(
    saved: &Saved<'_>,
    task_at: TaskAt<'_>,
    actions: &mut layers::Action,
) -> GenieResult<()> {
    use crate::garrison::EnterAction;
    use crate::garrison::EnterState;
    let base = saved.base;
    let first_time = saved
        .action
        .rge_action_enter
        .as_deref()
        .ok_or(GenieError::unsupported(
            "a saved Enter without its own part",
        ))?
        .first_time_enter;
    let sub = saved.move_to_sub(GenieError::unsupported(
        "a saved Enter with another sub-action",
    ))?;
    // its quiet MoveTo to the target (`FUN_140a87320` case 4: `FUN_140a7a170` with 1)
    let move_to = sub
        .map(|action| saved_sub_move_to(action, saved.graphics))
        .transpose()?;
    // the states the slice models: all but the walk to the meeting point (11)
    if !EnterState::of_byte(base.state, None)
        .is_ok_and(|enter_state| !matches!(enter_state, EnterState::WalkPoint { .. }))
    {
        return Err(GenieError::unsupported(
            "a saved Enter in a state the slice does not model",
        ));
    }
    let state = EnterState::of_byte(base.state, move_to)?;
    let task = saved.task(task_at)?;
    actions.list.set_enter(Some(EnterAction {
        target: ContainerId::of_field(base.target_id),
        state,
        point: saved.point(),
        timer: Seconds::new(base.timer),
        retry: first_time != 0,
        task_class: task.object_group,
        task,
    }));
    Ok(())
}

/// The Transport (12): the base alone is saved (`FUN_140aa3b20`): +0x34 is its second target's
/// word, its task the base's task slot (−1 for none: the base load's unsigned test,
/// `0x140a7b5f1`, gives no task), and +0x68 starts clear; `set_state` makes no sub-action but a
/// `MoveTo`.
fn import_transport(
    saved: &Saved<'_>,
    task_at: TaskAt<'_>,
    actions: &mut layers::Action,
) -> GenieResult<()> {
    let base = saved.base;
    // `set_state` makes no other sub-action: a save that holds one is no game's
    let sub = saved.move_to_sub(GenieError::invalid_input(
        "a saved Transport with another sub-action",
    ))?;
    actions.list.set_transport(Some(TransportAction {
        // its quiet MoveTo to the point (`0x140aa42a0`: `FUN_140a7a170` with 1)
        state: TransportState::of_byte(
            base.state,
            sub.map(|action| saved_sub_move_to(action, saved.graphics))
                .transpose()?,
        )?,
        mode: TransportMode::try_from(base.target2_id)?,
        point: saved.point(),
        retarget: false,
        task: match base.short_val {
            ..0 => None,
            _ => Some(saved.task(task_at)?),
        },
    }));
    Ok(())
}

/// The Explore (4): its state, its `MoveTo`, the wait and its task (the base's `short_val`, −1
/// for none).
fn import_explore(
    saved: &Saved<'_>,
    task_at: TaskAt<'_>,
    actions: &mut layers::Action,
) -> GenieResult<()> {
    use crate::movement::explore::ExploreState;
    let base = saved.base;
    let sub = saved.move_to_sub(GenieError::unsupported(
        "a saved Explore with another sub-action",
    ))?;
    // its quiet MoveTo to the point (`0x140a87e30`: `FUN_140a7a170` with 1)
    let move_to = sub
        .map(|action| saved_sub_move_to(action, saved.graphics))
        .transpose()?;
    let state = ExploreState::of_byte(base.state, move_to)?;
    if !matches!(state, ExploreState::Wait | ExploreState::Walk { .. }) {
        return Err(GenieError::unsupported(
            "a saved Explore in a state the slice does not model",
        ));
    }
    actions.list.set_explore(Some(ExploreAction {
        state,
        point: saved.point(),
        task: match base.short_val {
            ..0 => None,
            _ => Some(saved.task(task_at)?),
        },
    }));
    Ok(())
}

/// The Attack (9) of its own; it reads and changes the unit's sprite from here on.
fn import_attack(saved: &Saved<'_>, actions: &mut layers::Action) -> GenieResult<()> {
    actions
        .list
        .set_attack(Some(saved_attack(saved.action, saved.graphics)?));
    Ok(())
}

/// `TRIBE_Action_Wait_Solidify`'s load (`FUN_140ae8220`): the base action's state and task, and
/// the foundation it waits for (`mTargetBuildingId`, +0x68); it makes no sub-action.
fn import_wait_solidify(
    saved: &Saved<'_>,
    task_at: TaskAt<'_>,
    actions: &mut layers::Action,
) -> GenieResult<()> {
    if saved.base.sub_actions.entries().next().is_some() {
        return Err(GenieError::unsupported(
            "a saved Wait_Solidify with a sub-action",
        ));
    }
    let target = saved
        .action
        .tribe_action_wait_solidify
        .as_deref()
        .ok_or(GenieError::unsupported(
            "a saved Wait_Solidify without its own part",
        ))?
        .target_building_id;
    let target = FoundationId::new(ObjectId::try_from(target)?);
    let mut wait = WaitSolidify::new(saved.task(task_at)?, target);
    wait.set_state(WaitSolidifyState::of_byte(saved.base.state)?);
    actions.list.set_wait_solidify(Some(wait));
    Ok(())
}

/// A saved `MoveTo`'s wait (`+0x6c`, `+0x70`, `+0x74`); none for another sub-action.
///
/// # Errors
///
/// A saved wait state that names no state.
pub fn saved_move_wait(sub: &Action) -> GenieResult<MoveWait> {
    use crate::movement::move_to::MoveWaitState;
    sub.rge_action_by_kind
        .as_deref()
        .and_then(|rge_action_by_kind| rge_action_by_kind.rge_action_move_to.as_deref())
        .map_or_else(
            || Ok(MoveWait::default()),
            |rge_action_move_to| {
                Ok(MoveWait {
                    state: MoveWaitState::of_byte(rge_action_move_to.movement_wait_state)?,
                    timer: Seconds::new(rge_action_move_to.timer),
                    delay: Seconds::new(f32::from_bits(rge_action_move_to.default_wait_time)),
                })
            },
        )
}

#[cfg(test)]
mod tests;
