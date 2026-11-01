//! The task finder's steps on a target: the drop-off, its player's foundation, the garrison into
//! it and its repair.

use crate::GenieError;
use crate::gathering::gather::GATHER;
use crate::gathering::hunt::HUNT;
use crate::objects::ObjectState;
use crate::objects::lifecycle::Held;
use crate::objects::masters::ActionType;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::action_type;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::{Master, Task};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::task_finder::Actor;
use crate::unit_ai::task_finder::BUILDING;
use crate::unit_ai::task_finder::Diplomacy;
use crate::unit_ai::task_finder::Finding;
use crate::unit_ai::task_finder::TaskTarget;
use crate::unit_ai::task_finder::first;
use crate::unit_ai::task_finder::wanted;

/// The task finder's steps on a `target` (`LAB_1412795f5` to `LAB_1412796b4`): the drop-off
/// ([`drop_off_finding`]), its player's foundation ([`build_task`]), the garrison into it
/// ([`garrison_into`]), the repair ([`repair_finding`]), the trade ([`trade_task`]); a garrison
/// into it without a task ends the finder there. Master 199 is not modelled.
pub(super) fn on_target<'m>(
    list: &'m Master,
    actor: &Actor<'_>,
    t: &TaskTarget<'_>,
    filter: Option<ActionType>,
    dip: &impl Diplomacy,
) -> Result<Finding<'m>, GenieError> {
    if let Finding::Answer(task) = drop_off_finding(list, actor, t, filter, dip) {
        return Ok(Finding::Answer(task));
    }
    // LAB_1412795f5: building its player's foundation
    if t.player == actor.player && t.master.kind == BUILDING && t.state == ObjectState::Foundation {
        return Ok(Finding::Answer(build_task(list, t, filter)));
    }
    if t.master.id == ids::FISH_TRAP {
        return Err(GenieError::unsupported("the task finder on master 199"));
    }
    // LAB_141279cfa -> LAB_1412796b4: garrisoning, repairing, task 0x6f
    let (garrison, no_task) = garrison_into(list, actor, t, filter, dip);
    if let Finding::Answer(task) = garrison {
        return Ok(Finding::Answer(task));
    }
    if let Finding::Answer(task) = repair_finding(list, actor, t, filter, dip) {
        return Ok(Finding::Answer(task));
    }
    if let Some(task) = trade_task(list, t, filter) {
        return Ok(Finding::Answer(Some(task)));
    }
    // a garrison into the target without a task for it ends the finder before its tail, after
    // the repair and task 0x6f (`LAB_1412796b4`'s flag, with any filter)
    Ok(if no_task {
        Finding::Answer(None)
    } else {
        Finding::Next
    })
}

/// The trade task (0x6f) on a dock (45) or market (84) in a state below 3 (`LAB_1412796b4`'s
/// last loop): the first task of type 0x6f for the target's master or its class that the
/// filter takes; none goes on to the finder's tail.
fn trade_task<'m>(
    list: &'m Master,
    t: &TaskTarget<'_>,
    filter: Option<ActionType>,
) -> Option<&'m Task> {
    let trades =
        t.state < ObjectState::Dying && (t.master.id == ids::ID_45 || t.master.id == ids::MARKET);
    trades
        .then(|| {
            first(list, |task| {
                // The trade task's action type (`0x6f`).
                task.action_type == action_type::TRADE
                    && (task.object_id == Some(t.master.id) || task.for_class(t.master.class))
                    && wanted(filter, task)
            })
        })
        .flatten()
}

/// Dropping off what the unit carries at its player's finished drop site: the first gathering
/// task (5, 110, 111) for what it carries the filter takes (not a gathering one on a farm at
/// work, `0x7d`); without one and any filter, no task unless the site is a building at work
/// `0x7d` or idle.
pub(super) fn drop_off_finding<'m>(
    list: &'m Master,
    actor: &Actor<'_>,
    t: &TaskTarget<'_>,
    filter: Option<ActionType>,
    dip: &impl Diplomacy,
) -> Finding<'m> {
    let Held {
        kind: held_kind,
        amount: held_amount,
    } = actor.held;
    // dropping off what the unit carries at its player's finished drop site: the tasks of
    // gathering (5, 110, 111) for what it carries
    if held_kind.is_some()
        && Amount::ZERO < held_amount
        && t.state == ObjectState::Alive
        && t.player == actor.player
        && dip.drops_at(t.master.id, actor)
    {
        let farm_at_work = t.master.kind == BUILDING && t.top_action == Some(action_type::UNPACK);
        if let Some(task) = first(list, |task| {
            let gathers = matches!(task.action_type, GATHER | HUNT);
            !(farm_at_work && gathers)
                && task.attribute_type == held_kind
                && matches!(task.action_type, GATHER | HUNT | action_type::TRADE)
                && wanted(filter, task)
        }) {
            return Finding::Answer(Some(task));
        }
        if filter.is_none()
            && (t.master.kind != BUILDING || t.top_action.is_some_and(|a| a != action_type::UNPACK))
        {
            return Finding::Answer(None);
        }
    }
    Finding::Next
}

/// The garrison into the target (a transport): its player's or a mutual ally's in a state below
/// 3 that takes units, for a transport, a unit that is no villager or the garrison filter: the
/// first task of action 3 on its class the filter takes; with none, the garrison filter has no
/// task. The flag: no task after the repair and task 0x6f (any filter).
pub(super) fn garrison_into<'m>(
    list: &'m Master,
    actor: &Actor<'_>,
    t: &TaskTarget<'_>,
    filter: Option<ActionType>,
    dip: &impl Diplomacy,
) -> (Finding<'m>, bool) {
    let own = actor.master;
    let allied = dip.allied(actor.player, t.player, false);
    let own_or_allied = t.player == actor.player
        || (dip.allied(actor.player, t.player, true)
            && dip.allied(t.player, actor.player, true)
            && allied);
    // the garrison into the target (a transport): the first task of action 3 on its class
    // (the whole shorts) the filter takes (any, or 3); with none, the garrison filter has
    // no task, and any filter ends the finder after the repair and task 0x6f below
    if own_or_allied
        && t.state < ObjectState::Dying
        && t.master.object_flags.contains(ObjectFlags::GARRISON)
        && (t.master.class == class::TRANSPORT
            || own.class != class::VILLAGER
            || filter == Some(crate::garrison::ENTER))
    {
        if let Some(task) = first(list, |task| {
            task.action_type == crate::orders::GARRISON
                && task.for_class(t.master.class)
                && wanted(filter, task)
        }) {
            return (Finding::Answer(Some(task)), false);
        }
        if filter == Some(crate::garrison::ENTER) {
            return (Finding::Answer(None), false);
        }
        return (Finding::Next, filter.is_none());
    }
    (Finding::Next, false)
}

/// The repair of a damaged building or ship of the unit's player, Gaia or an ally, in a state
/// below 3: of the repair tasks the filter takes, the first for its master (which ends the
/// scan), else the last for its class with a work value, else the last for any target; the
/// repair filter without one has no task.
pub(super) fn repair_finding<'m>(
    list: &'m Master,
    actor: &Actor<'_>,
    t: &TaskTarget<'_>,
    filter: Option<ActionType>,
    dip: &impl Diplomacy,
) -> Finding<'m> {
    let repairable = t.player == actor.player
        || t.player == PlayerId::GAIA
        || dip.allied(actor.player, t.player, true);
    let repair_class = t.master.kind == BUILDING
        || [
            class::TRANSPORT,
            class::FISHING_SHIP,
            class::WARSHIP,
            class::TRADE_BOAT,
            class::CLASS_53,
            class::SIEGE,
            class::CLASS_51,
            class::CLASS_54,
            class::CLASS_55,
        ]
        .contains(&t.master.class);
    if repairable && repair_class && t.hp < t.max_hp && t.state < ObjectState::Dying {
        // the repair tasks the filter takes: the last for any target, the last for the
        // target's class, else the first for its master, which ends the scan; the class's
        // needs a work value above 0, the any one only without a class's
        let mut any = None;
        let mut by_class = None;
        for task in &list.tasks {
            if task.action_type != crate::construction::repair::REPAIR || !wanted(filter, task) {
                continue;
            }
            if task.object_group.is_none() && task.object_id.is_none() {
                any = Some(task);
            }
            if task.for_class(t.master.class) {
                by_class = Some(task);
            }
            if task.object_id == Some(t.master.id) {
                return Finding::Answer(Some(task));
            }
        }
        match by_class {
            None if any.is_some() => return Finding::Answer(any),
            Some(task) if task.work_val1.positive() => return Finding::Answer(Some(task)),
            _ => {}
        }
        if filter == Some(crate::construction::repair::REPAIR) {
            return Finding::Answer(None);
        }
    }
    Finding::Next
}

/// Building a foundation (`0x1412795f5`; the building's action check `FUN_1411e8630` holds for a
/// foundation without actions): of the Build tasks (101) the filter takes, the one for the
/// foundation's master, else the last for its class, each only with a work value, else the last
/// for any target.
fn build_task<'m>(
    list: &'m Master,
    t: &TaskTarget<'_>,
    filter: Option<ActionType>,
) -> Option<&'m Task> {
    let (mut by_class, mut generic) = (None, None);
    for task in list
        .tasks
        .iter()
        .filter(|t| t.action_type == action_type::BUILD && wanted(filter, t))
    {
        if task.object_id == Some(t.master.id) {
            return (task.work_val1.positive()).then_some(task);
        }
        if task.for_class(t.master.class) {
            by_class = Some(task);
        }
        if task.object_group.is_none() && task.object_id.is_none() {
            generic = Some(task);
        }
    }
    by_class.map_or(generic, |task| (task.work_val1.positive()).then_some(task))
}
