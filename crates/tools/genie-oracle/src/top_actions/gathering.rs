//! The gathering actions of a saved object's top action: the Hunt, the Gather and a farm's
//! Make ([`crate::top_actions::saved_actions`]).

use crate::top_actions::{Saved, TaskAt, first_action, saved_move_to};
use crate::{saved_attack, saved_reload};
use genie_core::GenieError;
use genie_core::gathering::DropSiteId;
use genie_core::map::Point;
use genie_core::movement::move_to::MOVE_TO;
use genie_core::objects::Voice;
use genie_core::objects::layers;
use genie_core::objects::masters::{ClassId, Task};
use genie_core::players::Amount;
use genie_core::world::Seconds;
use genie_save::generated::world::Action;

/// A farm's Make (21): its timers and task; it makes no sub-action.
pub(super) fn import_make(
    saved: &Saved<'_>,
    task_at: TaskAt<'_>,
    actions: &mut layers::Action,
) -> Result<(), GenieError> {
    let base = saved.base;
    let make = saved
        .action
        .rge_action_by_type
        .as_deref()
        .and_then(|t| t.rge_action_make.as_deref())
        .ok_or(GenieError::unsupported("a saved Make without its part"))?;
    if base.sub_actions.entries().next().is_some() {
        return Err(GenieError::unsupported("a saved Make with a sub-action"));
    }
    actions
        .list
        .set_make(Some(genie_core::gathering::make::MakeAction {
            state: genie_core::gathering::make::MakeState::of_byte(base.state)?,
            remaining: Amount::new(base.timer),
            total: Amount::new(make.work_timer),
            task: saved.task(task_at)?,
        }));
    Ok(())
}

/// A Hunt's or a Gather's task.
const fn hunt_task(t: &Task) -> genie_core::gathering::hunt::HuntTask {
    genie_core::gathering::hunt::HuntTask {
        id: t.id,
        range: t.work_range,
        resource: t.attribute_type,
        sprites: t.sprites,
        work_value: t.work_val1,
        productivity: t.attribute_type2,
        kind4: t.attribute_type4,
    }
}

/// The Hunt (110): its task, drop site and lead, with its Attack or `MoveTo`. Its Attack reads
/// and changes the unit's sprite from here on.
pub(super) fn import_hunt(
    saved: &Saved<'_>,
    task_at: TaskAt<'_>,
    actions: &mut layers::Action,
) -> Result<(), GenieError> {
    use genie_core::gathering::hunt::{HuntAction, HuntState};
    let base = saved.base;
    let part = saved
        .action
        .tribe_action_hunt
        .as_deref()
        .ok_or(GenieError::unsupported("a saved Hunt without its own part"))?;
    // the states the slice models
    if !HuntState::of_byte(base.state, None, None).is_ok_and(|s| {
        matches!(
            s,
            HuntState::Idle
                | HuntState::Search
                | HuntState::Walk { .. }
                | HuntState::Face
                | HuntState::Gather
                | HuntState::Drop { .. }
                | HuntState::Lead
                | HuntState::Attack { .. }
                | HuntState::WalkSearch { .. }
                | HuntState::WalkPoint { .. }
        )
    }) {
        return Err(GenieError::unsupported(
            "a saved Hunt in a state the slice does not model",
        ));
    }
    let task = hunt_task(&saved.task(task_at)?);
    let target_class = ClassId::of_param(part.target_type)?;
    let (walk, attack) = match first_action(&base.sub_actions)? {
        None => (None, None),
        Some((genie_core::combat::attack::ATTACK, sub)) => (None, Some(hunt_attack(saved, sub)?)),
        // every MoveTo of the Hunt is quiet (`FUN_140a7a170` with 1)
        Some((MOVE_TO, sub)) => (
            Some(saved_move_to(sub, saved.graphics, Voice::Quiet)?),
            None,
        ),
        Some(_) => {
            return Err(GenieError::unsupported(
                "a saved Hunt with another sub-action",
            ));
        }
    };
    let hunt = HuntAction {
        target: genie_core::gathering::HuntTargetId::of_field(base.target_id),
        state: HuntState::of_byte(base.state, walk, attack)?,
        point: saved.point(),
        target_class,
        task,
        drop_site: DropSiteId::of_field(part.sheep_drop_site_id as i32),
        carry_to: DropSiteId::of_field(base.target2_id),
        timer: Seconds::new(base.timer),
        lead: genie_core::gathering::hunt::Lead {
            moved: part.moved_sheep != 0,
            still_time: part.sheep_still_time as i16,
            start: Point::new(
                f32::from_bits(part.sheep_start_x),
                f32::from_bits(part.sheep_start_y),
            ),
        },
    };
    actions.list.set_hunt(Some(hunt));
    Ok(())
}

/// A Hunt's Attack. A swing past its hit: the swing's times (`+0xb0`, `+0xb4`, unsaved) are
/// what the hit left, the reload less the attack sprite's length.
fn hunt_attack(
    saved: &Saved<'_>,
    sub: &Action,
) -> Result<genie_core::combat::attack::Attack, GenieError> {
    let mut attack = saved_attack(sub, saved.graphics)?;
    if attack.state == genie_core::combat::attack::AttackState::Swing && !attack.pending {
        let reload = saved_reload(saved.object, saved.s, saved.objects).ok_or(
            GenieError::unsupported("a saved swing without its unit's reload"),
        )?;
        let g = attack.arms.sprite.ok_or(GenieError::unsupported(
            "a saved swing without its attack sprite",
        ))?;
        let rest = reload - g.frames.as_f32() * g.frame_duration;
        attack.swing = Seconds::new(rest);
        attack.rest = Seconds::new(rest.max(0.0));
    }
    Ok(attack)
}

/// The Gather (5): its stops and task, with its `MoveTo`.
pub(super) fn import_gather(
    saved: &Saved<'_>,
    task_at: TaskAt<'_>,
    actions: &mut layers::Action,
) -> Result<(), GenieError> {
    use genie_core::gathering::gather::{GatherAction, GatherState};
    let base = saved.base;
    let part = saved
        .action
        .rge_action_by_type
        .as_deref()
        .and_then(|b| b.rge_action_gather.as_deref())
        .ok_or(GenieError::unsupported(
            "a saved Gather without its own part",
        ))?;
    // the states the slice models
    if !GatherState::of_byte(base.state, None).is_ok_and(|s| {
        matches!(
            s,
            GatherState::Walk { .. }
                | GatherState::Face
                | GatherState::Work
                | GatherState::Drop { .. }
                | GatherState::Done
                | GatherState::Idle
                | GatherState::Moved { .. }
                | GatherState::Search
                | GatherState::WalkBack { .. }
        )
    }) {
        return Err(GenieError::unsupported(
            "a saved Gather in a state the slice does not model",
        ));
    }
    // every MoveTo of the Gather is quiet (`FUN_140a7a170` with 1): to the target, the drop
    // site or a point
    let walk = saved
        .move_to_sub(GenieError::unsupported(
            "a saved Gather with another sub-action",
        ))?
        .map(|sub| saved_move_to(sub, saved.graphics, Voice::Quiet))
        .transpose()?;
    let state = GatherState::of_byte(base.state, walk)?;
    let task = hunt_task(&saved.task(task_at)?);
    let gather = GatherAction {
        target: genie_core::gathering::GatherTargetId::of_field(base.target_id),
        carry_to: DropSiteId::of_field(base.target2_id),
        state,
        point: saved.point(),
        timer: Seconds::new(base.timer),
        task,
        target_class: ClassId::of_param(part.target_type)?,
        stops: genie_core::gathering::FarmStops::of_data(part.number_stops),
        current_stop: genie_core::gathering::StopIndex::of_data(part.current_stop),
        time_per_stop: Seconds::new(part.time_per_stop),
        time_at_stop: Seconds::new(part.time_at_current_stop),
        last_stop: Point::new(part.last_stop_x, part.last_stop_y),
        // `+0x80` is not saved: set_state(7) set it
        worked: state == GatherState::Work,
        farm: Point::new(part.farm_x, part.farm_y),
        gather_at: Point::new(part.gather_x, part.gather_y),
    };
    actions.list.set_gather(Some(gather));
    Ok(())
}
