//! A saved object's top action as the slice models it ([`saved_actions`]).

use crate::{OTHER_SUB_ACTION, combat_master, saved_attack, saved_move_wait, saved_sprite};
use genie_core::GenieError;
use genie_core::construction::build::{BuildAction, BuildTask};
use genie_core::map::Distance;
use genie_core::map::Point;
use genie_core::map::{Offset, Position};
use genie_core::movement::fly::{FLY, Fly};
use genie_core::movement::move_to::{MOVE_TO, MoveTo, MoveToAction, TargetMoved};
use genie_core::objects::ObjectId;
use genie_core::objects::SpriteId;
use genie_core::objects::masters::action_type::BUILD;
use genie_core::objects::masters::{ActionType, Task};
use genie_core::objects::sprite::Graphic;
use genie_core::objects::unit::Unit;
use genie_core::ownership::artifact::ARTIFACT_TYPE;
use genie_core::world::Seconds;
use genie_save::generated::world::{Action, PlayerObjects, RgeAction, RgeStaticObject};
use std::collections::BTreeMap;

mod gathering;

use crate::top_actions::gathering::{import_gather, import_hunt, import_make};

/// A task slot's task of a master (the master, the slot).
type TaskAt<'a> = &'a dyn Fn(i16, i16) -> Result<Task, GenieError>;

/// What `saved_actions` finds an object without actions: a state the caller rules out.
const NO_ACTIONS: GenieError =
    GenieError::unreachable("saved_actions called for an object without actions");

/// The top action of a saved object as the slice models it: the Artifact action (107), the
/// Build action (101) with its `MoveTo`, a `MoveTo` of its own (type 1), a bird's Fly action (10)
/// with its `MoveTo`, the Hunt (110) with its Attack or `MoveTo` (and in state 8 its second
/// target, the drop site) and a farm's Make (21). Any other saved action, and a second one
/// below the head ([`not_imported`]), stops the object's player.
#[allow(clippy::too_many_arguments)]
pub(super) fn saved_actions(
    object: &genie_save::Object,
    s: &RgeStaticObject,
    objects: &PlayerObjects,
    work: impl Fn(i16) -> Result<BuildTask, GenieError>,
    fly_task: impl Fn(i16) -> Result<Task, GenieError>,
    task_at: impl Fn(i16, i16) -> Result<Task, GenieError>,
    graphics: Option<&BTreeMap<SpriteId, Graphic>>,
    unit: &mut Unit,
) -> Result<(), GenieError> {
    let mut entries = object
        .actions()
        .map(genie_save::generated::world::ActionList::entries)
        .ok_or(NO_ACTIONS)?;
    let (kind, action) = entries.next().ok_or(NO_ACTIONS)?;
    let kind = ActionType::try_from(kind)?;
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
        s,
        objects,
        action,
        base,
        graphics,
    };
    match kind {
        ARTIFACT_TYPE => import_artifact(&saved, &task_at, unit),
        MOVE_TO => import_move_to(&saved, unit),
        genie_core::combat::missile::MISSILE => import_missile(&saved, unit),
        genie_core::gathering::make::MAKE => import_make(&saved, &task_at, unit),
        FLY => import_fly(&saved, &fly_task, unit),
        BUILD => import_build(&saved, &work, unit),
        genie_core::construction::repair::REPAIR => import_repair(&saved, &task_at, unit),
        genie_core::gathering::hunt::HUNT => import_hunt(&saved, &task_at, unit),
        genie_core::gathering::gather::GATHER => import_gather(&saved, &task_at, unit),
        genie_core::garrison::ENTER => import_enter(&saved, &task_at, unit),
        genie_core::garrison::transport::TRANSPORT => import_transport(&saved, unit),
        genie_core::movement::explore::EXPLORE => import_explore(&saved, unit),
        genie_core::combat::attack::ATTACK => import_attack(&saved, unit),
        genie_core::construction::wait_solidify::WAIT_SOLIDIFY => {
            import_wait_solidify(&saved, &task_at, unit)
        }
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
pub(crate) fn first_action(
    list: &genie_save::generated::world::ActionList,
) -> Result<Option<(ActionType, &Action)>, GenieError> {
    list.entries()
        .next()
        .map(|(kind, action)| Ok((ActionType::try_from(kind)?, action)))
        .transpose()
}

/// The second target (+0x34): only the Hunt, the Gather and the Build (their drop site), the
/// missile (its shooter) and the Attack hold one in the slice.
const fn holds_second_target(kind: ActionType) -> bool {
    matches!(
        kind,
        genie_core::gathering::hunt::HUNT
            | genie_core::gathering::gather::GATHER
            | genie_core::combat::missile::MISSILE
            | genie_core::combat::attack::ATTACK
            | BUILD
    )
}

/// What a saved top action's import reads: the object, its static part, its player's objects,
/// the action and its base, and the sprites' table.
struct Saved<'a> {
    object: &'a genie_save::Object,
    s: &'a RgeStaticObject,
    objects: &'a PlayerObjects,
    action: &'a Action,
    base: &'a RgeAction,
    graphics: Option<&'a BTreeMap<SpriteId, Graphic>>,
}

impl Saved<'_> {
    /// The task of the action's slot (`short_val`).
    fn task(&self, task_at: TaskAt<'_>) -> Result<Task, GenieError> {
        task_at(self.s.master_obj, self.base.short_val)
    }

    /// The action's first sub-action when it is a `MoveTo`, none without one; `other` for any
    /// other.
    fn move_to_sub(&self, other: GenieError) -> Result<Option<&Action>, GenieError> {
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
const fn move_to_state(state: u8) -> Result<MoveTo, GenieError> {
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
fn sub_range(sub: &Action) -> Result<f32, GenieError> {
    sub.move_to_range()
        .ok_or(GenieError::unsupported("a saved MoveTo without its range"))
}

/// The point of an action without a target.
fn target_point(b: &RgeAction) -> Option<Position> {
    (b.target_id == -1).then_some(Position::new(b.target_x, b.target_y, b.target_z))
}

/// The Artifact action (107): its state and timer, its `MoveTo` to its target and its task's
/// moving sprite.
fn import_artifact(
    saved: &Saved<'_>,
    task_at: TaskAt<'_>,
    unit: &mut Unit,
) -> Result<(), GenieError> {
    let base = saved.base;
    // its task (the base's `short_val`, −1 for none)
    let moving = if base.short_val < 0 {
        None
    } else {
        saved.task(task_at)?.sprites.moving
    };
    let sub = saved.move_to_sub(GenieError::unsupported(
        "a saved Artifact action with another sub-action",
    ))?;
    // a move to an object (set_state 5) holds the target and a quiet MoveTo (to the target,
    // or to a town centre's point); a move to a point (set_state 4) a MoveTo that is not quiet
    let quiet = base.target_id != -1;
    let move_to = sub
        .map(|s| saved_move_to(s, saved.graphics, quiet))
        .transpose()?;
    unit.actions
        .set_artifact(Some(genie_core::ownership::artifact::Artifact {
            state: genie_core::ownership::artifact::ArtifactState::of_byte(base.state)?,
            timer: Seconds::new(base.timer),
            gaia: saved.s.owner == 0,
            move_to,
            goal: if base.target_id == -1 {
                move_to.and_then(|_| target_point(base))
            } else {
                // the target's place, `+0x38`
                (base.target_x != -1.0).then_some(saved.point())
            },
            target: ObjectId::of_field(base.target_id),
            enter: None,
            moving,
        }));
    Ok(())
}

/// A `MoveTo` of its own (type 1): its state, goal, range, sprite and wait; not quiet.
fn import_move_to(saved: &Saved<'_>, unit: &mut Unit) -> Result<(), GenieError> {
    let move_to = saved_move_to(saved.action, saved.graphics, false)?;
    unit.actions.set_move_to(Some(move_to));
    Ok(())
}

/// A saved `MoveTo` (type 1), of its own or an action's sub-action: its state, goal, target,
/// range, sprite and wait. The save does not hold `+0x60`: `quiet` is its maker's.
pub(crate) fn saved_move_to(
    action: &Action,
    graphics: Option<&BTreeMap<SpriteId, Graphic>>,
    quiet: bool,
) -> Result<MoveToAction, GenieError> {
    let base = action.base().ok_or(MOVE_TO_BASELESS)?;
    Ok(MoveToAction {
        state: move_to_state(base.state)?,
        // its point (`+0x38`..`+0x40`), for a target object the target's place it last took
        point: Position::new(base.target_x, base.target_y, base.target_z),
        target: ObjectId::of_field(base.target_id),
        range: Distance::new(sub_range(action)?),
        // its sprite (`+0x58`, the save's second `short_val`; −1 for none)
        sprite: saved_move_to_sprite(SpriteId::of_data(base.short_val_2), graphics)?,
        quiet,
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
    graphics: Option<&BTreeMap<SpriteId, Graphic>>,
) -> Result<Option<Graphic>, GenieError> {
    let Some(id) = id else {
        return Ok(None);
    };
    graphics
        .and_then(|g| g.get(&id).copied())
        .map(Some)
        .ok_or_else(|| {
            GenieError::missing_data("a saved MoveTo's sprite outside the sprites' table")
                .with("sprite", id)
        })
}

/// A missile's action (8): its course and the missile object's own fields.
fn import_missile(saved: &Saved<'_>, unit: &mut Unit) -> Result<(), GenieError> {
    let (base, object) = (saved.base, saved.object);
    let ma = saved
        .action
        .rge_action_by_type
        .as_deref()
        .and_then(|t| t.rge_action_missile.as_deref())
        .ok_or(GenieError::unsupported(
            "a saved missile action without its part",
        ))?;
    let mo = object
        .rge_object
        .as_deref()
        .and_then(|r| r.rge_missile_object.as_deref())
        .ok_or(GenieError::unsupported(
            "a missile action of an object that is not a missile",
        ))?;
    let moving = object
        .moving_part()
        .ok_or(GenieError::unsupported("a missile without its moving part"))?;
    // the hit mode copies the master's (`+0x80`, a miss's 1 is not saved)
    let hit_mode = saved
        .objects
        .master(saved.s.master_obj)
        .and_then(|m| m.rge_master.as_deref())
        .and_then(|r| r.rge_master_missile_object.as_deref())
        .map_or(genie_core::combat::missile::HitMode::TARGET_ALONE, |m| {
            genie_core::combat::missile::HitMode::of_data(m.missile_hit_info)
        });
    unit.missile = Some(genie_core::combat::missile::Missile {
        max_range: Distance::new(mo.max_range),
        fired_from: ObjectId::of_field(mo.fired_from_id),
        charge: mo.is_charge_projectile != 0,
        flying: genie_core::combat::missile::LANDING < ma.distance,
        step: Offset::new(moving.velocity_x, moving.velocity_y, moving.velocity_z),
    });
    unit.actions
        .set_missile(Some(genie_core::combat::missile::MissileAction {
            state: genie_core::combat::missile::MissileState::of_byte(base.state)?,
            target: ObjectId::of_field(base.target_id),
            fired_from: ObjectId::of_field(base.target2_id),
            point: saved.point(),
            timer: Distance::new(base.timer),
            direction: Offset::new(ma.velocity_x, ma.velocity_y, ma.velocity_z),
            ballistic_velocity: genie_core::movement::Speed::new(ma.ballistic_velocity),
            ballistic_acceleration: genie_core::movement::Acceleration::new(
                ma.ballistic_acceleration,
            ),
            distance: Distance::new(ma.distance),
            hit_mode,
            result: genie_core::combat::missile::StrikeResult::of_data(ma.interacted_with_agility),
            origin: genie_core::combat::missile::Origin::from_bytes(&ma.projectile_data),
            met: Vec::new(),
        }));
    Ok(())
}

/// A bird's Fly (10): its home, its `MoveTo` and its task.
fn import_fly(
    saved: &Saved<'_>,
    fly_task: &dyn Fn(i16) -> Result<Task, GenieError>,
    unit: &mut Unit,
) -> Result<(), GenieError> {
    let base = saved.base;
    // its MoveTo to the flight's point is not quiet
    let move_to = saved
        .move_to_sub(GenieError::unsupported(
            "a saved Fly action with another sub-action",
        ))?
        .map(|s| saved_move_to(s, saved.graphics, false))
        .transpose()?;
    unit.actions.set_fly(Some(Fly {
        state: genie_core::movement::fly::FlyState::of_byte(base.state)?,
        timer: Seconds::new(base.timer),
        move_to,
        home: Point::new(base.target_x, base.target_y),
        task: fly_task(saved.s.master_obj)?,
    }));
    Ok(())
}

/// The Build (101): its task with the builder's powerup, its `MoveTo` and its site.
fn import_build(
    saved: &Saved<'_>,
    work: &dyn Fn(i16) -> Result<BuildTask, GenieError>,
    unit: &mut Unit,
) -> Result<(), GenieError> {
    let base = saved.base;
    // its quiet MoveTo (FUN_140a7a170 with 1)
    let move_to = saved
        .move_to_sub(OTHER_SUB_ACTION)?
        .map(|s| saved_move_to(s, saved.graphics, true))
        .transpose()?;
    let mut task = work(saved.s.master_obj)?;
    task.work_rate += unit.work_rate_powerup;
    unit.actions.set_build(Some(BuildAction {
        target: ObjectId::of_field(base.target_id),
        state: genie_core::construction::build::BuildState::of_byte(base.state)?,
        move_to,
        point: saved.point(),
        task,
        added: saved
            .action
            .tribe_action_build
            .as_deref()
            .is_some_and(|b| b.added_to_num_builders != 0),
        timer: Seconds::new(base.timer),
        site: ObjectId::of_field(base.target2_id),
    }));
    Ok(())
}

/// The Repair (106): its task, its `MoveTo`, its range and whether it counts as a builder.
fn import_repair(
    saved: &Saved<'_>,
    task_at: TaskAt<'_>,
    unit: &mut Unit,
) -> Result<(), GenieError> {
    let base = saved.base;
    let part = saved
        .action
        .tribe_action_repair
        .as_deref()
        .ok_or(GenieError::missing_data("a saved Repair's own part"))?;
    // every MoveTo of the Repair is quiet: to the target, or to a point without one
    let move_to = saved
        .move_to_sub(OTHER_SUB_ACTION)?
        .map(|s| saved_move_to(s, saved.graphics, true))
        .transpose()?;
    let t = saved.task(task_at)?;
    unit.actions
        .set_repair(Some(genie_core::construction::repair::RepairAction {
            target: ObjectId::of_field(base.target_id),
            state: genie_core::construction::repair::RepairState::of_byte(base.state)?,
            move_to,
            point: saved.point(),
            timer: Seconds::new(base.timer),
            task: genie_core::construction::repair::RepairTask::of(&t),
            range: saved
                .objects
                .master(saved.s.master_obj)
                .and_then(combat_master)
                .map_or(Distance::ZERO, |c| Distance::new(c.weapon_range)),
            counted: part.added_to_num_builders != 0,
            quiet: base.sub_action_value != 0,
        }));
    Ok(())
}

/// The Enter (3): its task's range, class and moving sprite, its `MoveTo` and its retry.
fn import_enter(saved: &Saved<'_>, task_at: TaskAt<'_>, unit: &mut Unit) -> Result<(), GenieError> {
    use genie_core::garrison::EnterAction;
    use genie_core::garrison::EnterState;
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
        .map(|s| saved_move_to(s, saved.graphics, true))
        .transpose()?;
    let state = EnterState::of_byte(base.state)
        .ok()
        .filter(|s| {
            matches!(
                s,
                EnterState::Done | EnterState::Wait | EnterState::Walk | EnterState::Failed
            )
        })
        .ok_or(GenieError::unsupported(
            "a saved Enter in a state the slice does not model",
        ))?;
    let task = saved.task(task_at)?;
    unit.actions.set_enter(Some(EnterAction {
        target: ObjectId::of_field(base.target_id),
        state,
        move_to,
        point: saved.point(),
        range: task.work_range,
        timer: Seconds::new(base.timer),
        retry: first_time != 0,
        task_class: task.object_group,
        moving: task.sprites.moving,
    }));
    Ok(())
}

/// The Transport (12): the base alone is saved (`FUN_140aa3b20`): +0x34 is its second target's
/// word, its task the base's task id (−1 for none), and +0x68 starts clear; `set_state` makes no
/// sub-action but a `MoveTo`.
fn import_transport(saved: &Saved<'_>, unit: &mut Unit) -> Result<(), GenieError> {
    let base = saved.base;
    let sub = saved.move_to_sub(GenieError::unreachable(
        "a saved Transport with another sub-action",
    ))?;
    unit.actions
        .set_transport(Some(genie_core::garrison::transport::TransportAction {
            state: genie_core::garrison::transport::TransportState::of_byte(base.state)?,
            mode: base.target2_id,
            point: saved.point(),
            // its quiet MoveTo to the point (`0x140aa42a0`: `FUN_140a7a170` with 1)
            move_to: sub
                .map(|s| saved_move_to(s, saved.graphics, true))
                .transpose()?,
            retarget: false,
            // the save writes the task's id, −1 for none
            task: genie_core::objects::masters::TaskId::of_field(base.short_val)?,
        }));
    Ok(())
}

/// The Explore (4): its state, its `MoveTo` and the wait.
fn import_explore(saved: &Saved<'_>, unit: &mut Unit) -> Result<(), GenieError> {
    use genie_core::movement::explore::ExploreState;
    let base = saved.base;
    let sub = saved.move_to_sub(GenieError::unsupported(
        "a saved Explore with another sub-action",
    ))?;
    // its quiet MoveTo to the point (`0x140a87e30`: `FUN_140a7a170` with 1)
    let move_to = sub
        .map(|s| saved_move_to(s, saved.graphics, true))
        .transpose()?;
    let state = ExploreState::of_byte(base.state)
        .ok()
        .filter(|s| matches!(s, ExploreState::Wait | ExploreState::Walk))
        .ok_or(GenieError::unsupported(
            "a saved Explore in a state the slice does not model",
        ))?;
    unit.actions
        .set_explore(Some(genie_core::movement::explore::ExploreAction {
            state,
            point: saved.point(),
            move_to,
        }));
    Ok(())
}

/// The Attack (9) of its own; it reads and changes the unit's sprite from here on.
fn import_attack(saved: &Saved<'_>, unit: &mut Unit) -> Result<(), GenieError> {
    unit.actions
        .set_attack(Some(saved_attack(saved.action, saved.graphics)?));
    unit.sprite = saved.graphics.and_then(|g| saved_sprite(saved.s, g));
    Ok(())
}

/// `TRIBE_Action_Wait_Solidify`'s load (`FUN_140ae8220`): the base action's state and task, and
/// the foundation it waits for (`mTargetBuildingId`, +0x68); it makes no sub-action.
fn import_wait_solidify(
    saved: &Saved<'_>,
    task_at: TaskAt<'_>,
    unit: &mut Unit,
) -> Result<(), GenieError> {
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
    let target = ObjectId::try_from(target)?;
    let mut wait =
        genie_core::construction::wait_solidify::WaitSolidify::new(saved.task(task_at)?, target);
    wait.set_state(
        genie_core::construction::wait_solidify::WaitSolidifyState::of_byte(saved.base.state)?,
    );
    unit.actions.set_wait_solidify(Some(wait));
    Ok(())
}

#[cfg(test)]
mod tests;
