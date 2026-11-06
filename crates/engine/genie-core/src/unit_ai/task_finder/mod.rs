//! The task finder (`TRIBE_Task_List` slot 2, `FUN_1412784d0`, with the `RGE_Task_List` finder
//! `FUN_1411a8ba0` and its test of a task on a target, slot 4 `FUN_1411a8620`): which task of a
//! master's task list a unit does on a target object. The work command (object slot 37,
//! `FUN_14106ff70`) asks it of the unit's own master and, through the task swap group, of the
//! player's other masters ([`crate::objects::masters::work_master`]).
//!
//! The finder tries the special tasks first, each for the targets it is about (dropping off what
//! the unit carries, building its player's foundation, garrisoning, repairing, converting, healing
//! and the like), then the general search: a task for the target's combat level, else for its
//! master, else for its class, each the first in list order the task's own test passes, else a
//! task for the terrain under the target's point. The masters' classes and ids it compares against
//! are engine constants; the game stores them encoded (a class as its value − 0xeac, a master id as
//! its id + 0x609c), the port compares the decoded values and names the encoded ones.

use crate::combat::attack::ATTACK;
use crate::construction::repair::REPAIR;
use crate::garrison::GarrisonSwitch;
use crate::garrison::{ENTER, garrison_kind_takes, is_garrison_task};
use crate::gathering::PastureId;
use crate::gathering::hunt::HUNT;
use crate::map::TerrainId;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::lifecycle::Held;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::TargetOwner;
use crate::objects::masters::{Master, Task};
use crate::players::AllyTest;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::players::{CivId, Relations};
use crate::unit_ai::task_finder::target_steps::on_target;
use crate::{GenieError, GenieResult};

use genie_model::unit_ai::task_finder::drop_site;
mod target_steps;
pub(crate) mod world;

pub use self::drop_site::DropSite;
pub use self::drop_site::DropTarget;
use crate::world::layer::AttributeRoles;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::Slots;

/// The task types the general search leaves out (`FUN_14109f8c0`: 151, 152, 154, 155, 157, 160).
const fn special_kind(action_kind: ActionKind) -> bool {
    let action_kind = action_kind.get().wrapping_sub(0x97) as u16;
    action_kind < 10 && (0x25bu32 >> action_kind) & 1 != 0
}

/// What the finder reads of the unit that would do the task.
#[derive(Clone, Copy, Debug)]
pub struct Actor<'master> {
    /// The unit.
    pub id: ObjectId,
    /// Its own master (the finder's `param_3`, also when another master's list is searched).
    pub master: &'master Master,
    /// The owner's player id (`+0x718`).
    pub player: PlayerId,
    /// What it carries (`+0x308`, `+0x310`).
    pub held: Held,
    /// The owner's attribute 95 (`+0x2f8`), which some monk tasks read.
    pub attribute_95: Amount,
    /// The owner's civ (`FUN_140e879e0` of `player+0x738`), which drop sites read.
    pub civ: CivId,
}

/// What the finder reads of a target object.
#[derive(Clone, Copy, Debug)]
pub struct TaskTarget<'master> {
    /// The target.
    pub id: ObjectId,
    /// Its master.
    pub master: &'master Master,
    /// Its player.
    pub player: PlayerId,
    /// The object state (`+0x640`).
    pub state: ObjectState,
    /// What it holds (`+0x308`, `+0x310`): a resource's kind and amount.
    pub held: Held,
    /// Its hit points and their most (slots 0xc0 and 200).
    pub hp: HitPoints,
    /// Its most hit points (slot 200).
    pub max_hp: HitPoints,
    /// `+0x68a`, `unit_attackable_flag` (`FUN_141188b10`).
    pub attackable: bool,
    /// `+0x790`, `parent_pasture_id`: the pasture it is in, if any.
    pub pasture: Option<PastureId>,
    /// The type of the first action in its action list (`+0x880`), if any.
    pub top_action: Option<ActionKind>,
}

/// What the work command's finder searches on: its target (none for a point), the target its
/// general search reads (the main building's) and the terrain under the command's point
/// (`FUN_1411a8ba0`, [`crate::map::terrain::Terrain::at_task_point`]; none off the map).
#[derive(Clone, Copy, Debug)]
pub struct TaskOn<'target, 'master> {
    /// The target, none for a point.
    pub target: Option<&'target TaskTarget<'master>>,
    /// The target the general search reads.
    pub general: Option<&'target TaskTarget<'master>>,
    /// The terrain under the point.
    pub terrain: Option<TerrainId>,
}

/// What the finder reads of the world: the players' relations (`FUN_141663dc0`: whether `from`
/// counts `to` as its own or allied by the game's `with_ally` flag) and the drop sites.
/// `FUN_141663dc0` for a player of diplomacy `relations` (`player+0x260`): one's own player
/// counts; Gaia never does; else relation 0 does, and 4 by `test`; relation 1 counts only
/// under allied victory (`FUN_1415e8e10`), which the slice does not model.
#[must_use]
pub fn allied(relations: &Relations, from: PlayerId, to: PlayerId, test: AllyTest) -> bool {
    if from == to {
        return true;
    }
    if to == PlayerId::GAIA {
        return false;
    }
    relations
        .get(to)
        .is_some_and(|relation| relation.allied(test))
}

/// The players' relations and the drop sites, as the task finder reads them.
pub trait Diplomacy {
    /// Player `from` counts `to` as its own or allied, by `test`.
    fn allied(&self, from: PlayerId, to: PlayerId, test: AllyTest) -> bool;
    /// `FUN_140fdd190`: a building of master `building` takes what `actor` carries.
    fn drops_at(&self, building: MasterId, actor: &Actor<'_>) -> bool;
}

/// `FUN_140fdd8a0`: the table's first entry for building `building`, the worker class `class`
/// and civ `civ` (an entry with civs is for those alone).
#[must_use]
pub fn drop_site(
    table: &[DropSite],
    building: MasterId,
    class: ClassId,
    civ: CivId,
) -> Option<&DropSite> {
    table.iter().find(|drop_site| {
        (drop_site.civs.is_empty() || drop_site.civs.contains(&civ))
            && drop_site.building == building
            && drop_site.worker_class == Some(class)
    })
}

/// `FUN_140fdd190` over the table: the first entry for the actor's civ and kind of worker whose
/// building is `building` decides, by whether it takes the kind the actor carries.
#[must_use]
pub fn drops_at(table: &[DropSite], building: MasterId, actor: &Actor<'_>) -> bool {
    for drop_site in table {
        if !drop_site.civs.is_empty() && !drop_site.civs.contains(&actor.civ) {
            continue;
        }
        let worker = if drop_site.workers.is_empty() {
            drop_site
                .worker_class
                .is_none_or(|class_id| class_id == actor.master.class)
        } else {
            drop_site.workers.contains(&actor.master.id)
        };
        if worker && drop_site.building == building {
            return drop_site
                .targets
                .iter()
                .any(|drop_target| drop_target.attribute == actor.held.kind);
        }
    }
    false
}

/// `FUN_140fdd950` with no buildings left out: the buildings of the drop-site table's entries
/// for workers of class `class` (any for −1; an entry's own worker list is not read) that list
/// resource types (`+0x40`), for `civ` where the entry names civs, that take `kind`: a basic
/// resource (below 4, compared unsigned, so −1 is not one) among their resource types, any other
/// among their targets' attribute types; in table order.
#[must_use]
pub fn table_drop_buildings(
    table: &[DropSite],
    kind: Option<AttributeId>,
    class: Option<ClassId>,
    civ: CivId,
) -> Vec<MasterId> {
    table
        .iter()
        .filter(|class_drop_site| {
            class.is_none_or(|class_id| class_drop_site.worker_class == Some(class_id))
        })
        .filter(|drop_site| !drop_site.resource_kinds.is_empty())
        .filter(|civ_drop_site| civ_drop_site.civs.is_empty() || civ_drop_site.civs.contains(&civ))
        .filter(|drop_site| match kind {
            Some(basic) if basic.is_resource() => drop_site.resource_kinds.contains(&basic),
            _ => drop_site
                .targets
                .iter()
                .any(|drop_target| drop_target.attribute == kind),
        })
        .map(|drop_site| drop_site.building)
        .collect()
}

/// A task's requirement (`FUN_1410622c0`): no enabling resource (−1), or the player's attribute of
/// that index above 0.
#[must_use]
pub fn enabled(attributes: &[Amount], resource: Option<AttributeId>) -> bool {
    resource.is_none_or(|attribute_id| {
        attributes
            .get(attribute_id.index())
            .is_some_and(|&amount| Amount::ZERO < amount)
    })
}

/// One search over the list: the first task the predicate takes, in list order.
fn first(list: &Master, pred: impl Fn(&Task) -> bool) -> Option<&Task> {
    list.action_part().tasks.iter().find(|task| pred(task))
}

/// The filter: any task (−1), or one of this type.
fn wanted(filter: Option<ActionKind>, task: &Task) -> bool {
    filter.is_none_or(|action_kind| action_kind == task.action_kind)
}

/// The task's own test on a target (`FUN_1411a8620`): a task for a holder (`holding_attr` 1)
/// wants the target to hold a resource of its type (or any, for a task without one); garrisoning
/// (3) and type 106 never apply; attacks (7, 9) want an attackable target; the task's owner type
/// says whose target (1 own, 2 an enemy player's, 3 Gaia's, 4 own, mutually allied or Gaia's, 5
/// an enemy's or, for a unit whose master has a combat level, Gaia's, 6 another's); never the unit
/// itself, but for type 4.
fn applies(task: &Task, actor: &Actor<'_>, target: &TaskTarget<'_>, dip: &impl Diplomacy) -> bool {
    if task.holding_attr == 1 {
        let Held { kind, amount } = target.held;
        if kind.is_none() || amount <= Amount::ZERO {
            return false;
        }
        if task.attribute_kind != kind
            && task.attribute_kind3 != kind
            && (task.attribute_kind.is_some() || task.attribute_kind3.is_some())
        {
            return false;
        }
    }
    match task.action_kind {
        ENTER | REPAIR => return false,
        ActionKind::COMBAT | ATTACK if !target.attackable => return false,
        _ => {}
    }
    let (me, them) = (actor.player, target.player);
    let ok = match task.owner_kind {
        TargetOwner::OWN => them == me,
        TargetOwner::ENEMY => {
            them != me && them != PlayerId::GAIA && !dip.allied(me, them, AllyTest::WithRelation4)
        }
        TargetOwner::GAIA => them == PlayerId::GAIA,
        // type 4 answers at once, the unit itself included
        TargetOwner::FRIENDLY => {
            return them == me
                || them == PlayerId::GAIA
                || (dip.allied(me, them, AllyTest::WithRelation4)
                    && dip.allied(them, me, AllyTest::WithRelation4));
        }
        // of Gaia's targets only for a unit whose own master has a combat level
        TargetOwner::ENEMY_OR_GAIA => {
            if them == me
                || (them != PlayerId::GAIA && dip.allied(me, them, AllyTest::WithRelation4))
            {
                false
            } else {
                them != PlayerId::GAIA || actor.master.combat_level.is_some()
            }
        }
        TargetOwner::OTHER => them != me,
        _ => true,
    };
    ok && actor.id != target.id
}

/// `FUN_1411a8ba0`, the general search: with a target, a task for its combat level (a target in
/// state below 3 whose master has one; tasks that have one and the special types left out), else
/// for its master, else for its class, each the first whose test passes and that the filter
/// takes; else, at a point on the map, a task for the terrain there.
fn general<'master>(
    list: &'master Master,
    actor: &Actor<'_>,
    target: Option<&TaskTarget<'_>>,
    terrain: Option<TerrainId>,
    filter: Option<ActionKind>,
    dip: &impl Diplomacy,
) -> Option<&'master Task> {
    if list.action_part().tasks.is_empty() {
        return None;
    }
    if let Some(task_target) = target {
        let usable = |task: &Task| {
            !special_kind(task.action_kind)
                && applies(task, actor, task_target, dip)
                && wanted(filter, task)
        };
        if task_target.state < ObjectState::Dying
            && task_target.master.combat_level.is_some()
            && let Some(task) = first(list, |task| task.combat_level != 0 && usable(task))
        {
            return Some(task);
        }
        if let Some(task) = first(list, |task| {
            task.object_id == Some(task_target.master.id) && usable(task)
        }) {
            return Some(task);
        }
        if let Some(task) = first(list, |task| {
            task.for_class(task_target.master.class) && usable(task)
        }) {
            return Some(task);
        }
    }
    let kind = terrain?;
    first(list, |task| {
        // the task's short against the terrain's byte
        task.terrain.is(kind) && wanted(filter, task)
    })
}

/// The task finder with filter 3 (`FUN_1412784d0`'s first part, which mode 1 runs too): on its
/// player's or a mutually allied player's building in state 2 that garrisons, for a unit master
/// of type 70 or master 1808, the first garrison task on buildings
/// ([`crate::garrison::is_garrison_task`]) when the building's `garrison_type` takes the master
/// ([`crate::garrison::garrison_kind_takes`], class 42 left out); none on a building whose top
/// action is `0x7c` or `0x7d` (`FUN_1411e8630`; a computer player's production comes first,
/// `FUN_1411d5520`), which is not modelled. Else none: the caller goes on to the rest of the
/// finder for a target whose master has object flag 0 (a transport, [`find_task`]).
fn garrison_task<'master>(
    list: &'master Master,
    actor: &Actor<'_>,
    target: Option<&TaskTarget<'_>>,
    dip: &impl Diplomacy,
    roles: &Roles,
) -> GenieResult<Option<&'master Task>> {
    let Some(task_target) = target else {
        return Ok(None);
    };
    let mutual = dip.allied(actor.player, task_target.player, AllyTest::WithRelation4)
        && dip.allied(task_target.player, actor.player, AllyTest::WithRelation4);
    if (task_target.player == actor.player || mutual)
        && task_target.player != PlayerId::GAIA
        && task_target.state == ObjectState::Alive
        // A building's master type (`'P'`).
        && task_target.master.kind == MasterKind::BUILDING
        && (list.kind == MasterKind::COMBAT || roles.of_master(list.id).contains(MasterRoles::MULE_CART))
        && task_target.master.garrison.holds()
    {
        let garrison_kind = task_target
            .master
            .building
            .map_or(GarrisonKinds::NONE, |building_master| {
                building_master.garrison_kind
            });
        let takes = garrison_kind_takes(
            garrison_kind,
            list.class,
            list.id,
            GarrisonSwitch::TaskFinder,
            roles,
        );
        if let Some(task) = first(list, |task| is_garrison_task(task, roles)).filter(|_| takes) {
            if matches!(
                task_target.top_action,
                Some(ActionKind::PACK | ActionKind::UNPACK)
            ) {
                return Err(GenieError::unsupported(
                    "a garrison task on a building at work 0x7c or 0x7d (FUN_1411e8630)",
                ));
            }
            return Ok(Some(task));
        }
    }
    Ok(None)
}

/// The task of `list` (a master's task list) for `actor` on `on` (`FUN_1412784d0` with the
/// work command's arguments: the target's point, the filter (−1 any, else an action type) and mode
/// 0). The target is an annex's main building already (the finder's first step, `+0xbb8`); the
/// general one the target the general search sees, which repeats that step (an annex of an annex:
/// its main building's main building). The terrain: the terrain under the point when the map's
/// terrain table counts it, which only a task for a terrain reads.
pub fn find_task<'master>(
    list: &'master Master,
    actor: &Actor<'_>,
    on: &TaskOn<'_, '_>,
    filter: Option<ActionKind>,
    dip: &impl Diplomacy,
    slots: Slots,
    roles: &Roles,
) -> GenieResult<Option<&'master Task>> {
    if let Finding::Answer(task) = finder_head(list, actor, on.target, filter, dip, roles)? {
        return Ok(task);
    }
    if let Some(task_target) = on.target
        && let Finding::Answer(task) = on_target(list, actor, task_target, filter, dip, roles)?
    {
        return Ok(task);
    }
    finder_tail(list, actor, on, filter, dip, slots, roles)
}

/// A step of the task finder: it answers (a task, or none), or the finder goes on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Finding<'master> {
    /// The finder's answer.
    Answer(Option<&'master Task>),
    /// The next step decides.
    Next,
}

/// The task finder's first steps: the garrison filter (3, [`garrison_task`]; with none, a
/// target that is no transport has no task); no task of a master with object flag 0x10 on prey
/// that stores something; the relic carrier's task 0x87 at its player's monastery (master
/// 109). A monk's conversion under attribute 95 is not modelled.
fn finder_head<'master>(
    list: &'master Master,
    actor: &Actor<'_>,
    target: Option<&TaskTarget<'_>>,
    filter: Option<ActionKind>,
    dip: &impl Diplomacy,
    roles: &Roles,
) -> GenieResult<Finding<'master>> {
    let own = actor.master;
    // the garrison filter (3): a building's garrison task; with none, a target whose master has
    // object flag 0 (a transport) goes on through the finder to its garrison into the target
    // (`LAB_1412796b4`), any other has no task
    if filter == Some(ENTER) {
        if let Some(task) = garrison_task(list, actor, target, dip, roles)? {
            return Ok(Finding::Answer(Some(task)));
        }
        if target.is_none_or(|task_target| {
            !task_target
                .master
                .object_flags
                .contains(ObjectFlags::GARRISON)
        }) {
            return Ok(Finding::Answer(None));
        }
    }
    // a master with object flag 0x10 has no task on prey whose master stores something
    // (`+0x610`, its first storage amount, above 0)
    if let Some(task_target) = target
        && own.object_flags.contains(ObjectFlags::SCOUT)
        && roles
            .of_class(task_target.master.class)
            .contains(ClassRoles::PREY)
        && Amount::ZERO
            < task_target
                .master
                .storage
                .first()
                .map_or(Amount::ZERO, |&(_, amount, _)| amount)
    {
        return Ok(Finding::Answer(None));
    }
    // LAB_141278e9f: the monk tasks for carried relics and conversions of villagers
    if roles
        .of_some_attribute(actor.held.kind)
        .contains(AttributeRoles::RELIC)
        && target.is_some_and(|task_target| {
            task_target.state == ObjectState::Alive
                && task_target.player == actor.player
                && roles
                    .of_master(task_target.master.id)
                    .contains(MasterRoles::TOWN_CENTRE)
        })
        && let Some(task) = first(list, |task| task.action_kind == ActionKind::ACTION_135)
    {
        return Ok(Finding::Answer(Some(task)));
    }
    let monk_master = roles.of_master(own.id).contains(MasterRoles::SCOUT_CAVALRY)
        || roles.of_master(own.id).contains(MasterRoles::LIGHT_CAVALRY);
    if actor.attribute_95 == Amount::new(2.0)
        && monk_master
        && target.is_some_and(|task_target| {
            roles
                .of_class(task_target.master.class)
                .contains(ClassRoles::VILLAGER)
        })
    {
        return Err(GenieError::unsupported(
            "the task finder's monk conversion of villagers (attribute 95)",
        ));
    }
    if actor.attribute_95 == Amount::new(3.0)
        && monk_master
        && target.is_some_and(|task_target| {
            roles
                .of_class(task_target.master.class)
                .contains(ClassRoles::BUILDING)
        })
    {
        return Err(GenieError::unsupported(
            "the task finder's monk conversion of buildings (attribute 95)",
        ));
    }
    Ok(Finding::Next)
}

/// The task finder's tail (`LAB_14127ba9e`, `LAB_14127bb9f`): task 13 for an own, allied or
/// artifact target below state 3 (under that filter alone); a villager on livestock, or on
/// what is inside something, takes task 110; anything else the general search
/// ([`general`]). Conversions, task 0x84, healing, task 0x88, class 53's conversion and master
/// 1105 are not modelled.
fn finder_tail<'master>(
    list: &'master Master,
    actor: &Actor<'_>,
    on: &TaskOn<'_, '_>,
    filter: Option<ActionKind>,
    dip: &impl Diplomacy,
    slots: Slots,
    roles: &Roles,
) -> GenieResult<Option<&'master Task>> {
    let target = on.target;
    let own = actor.master;
    // the tail: conversions, task 0x84, healing, task 0x88, the siege and class-53 tasks
    let monk = roles.of_class(own.class).contains(ClassRoles::MONK);
    let converts = list
        .action_part()
        .tasks
        .iter()
        .any(|task| task.action_kind == ActionKind::CONVERT);
    if target.is_some() && (monk || (converts && filter == Some(ActionKind::CONVERT))) {
        return Err(GenieError::unsupported(
            "the task finder's conversion (0x68)",
        ));
    }
    if list
        .action_part()
        .tasks
        .iter()
        .any(|task| task.action_kind == ActionKind::PICK_UP)
        && target.is_some_and(|task_target| {
            roles
                .of_class(task_target.master.class)
                .contains(ClassRoles::RELIC)
        })
    {
        return Err(GenieError::unsupported("the task finder's task 0x84"));
    }
    if list
        .action_part()
        .tasks
        .iter()
        .any(|task| task.action_kind == ActionKind::HEAL)
        && target.is_some()
    {
        return Err(GenieError::unsupported("the task finder's healing (0x69)"));
    }
    if roles
        .of_class(own.class)
        .contains(ClassRoles::MONK_WITH_RELIC)
        && target.is_some()
    {
        return Err(GenieError::unsupported("the task finder's task 0x88"));
    }
    if roles
        .of_class(own.class)
        .contains(ClassRoles::BOARDING_SHIP)
        && target.is_some()
    {
        return Err(GenieError::unsupported(
            "the task finder's class-53 conversion",
        ));
    }
    let mut inside = false;
    if let Some(task_target) = target {
        if roles.of_class(own.class).contains(ClassRoles::SIEGE)
            && roles.of_master(own.id).contains(MasterRoles::SIEGE_TOWER)
        {
            return Err(GenieError::unsupported("the task finder for master 1105"));
        }
        // LAB_14127ba9e: a list with task 13 takes it, under that filter alone, for an own,
        // allied or artifact target in a state below 3
        if task_target.state < ObjectState::Dying
            && (task_target.player == actor.player
                || dip.allied(actor.player, task_target.player, AllyTest::WithRelation4)
                || roles
                    .of_class(task_target.master.class)
                    .contains(ClassRoles::ARTIFACT))
            && let Some(task) = first(list, |task| task.action_kind == ActionKind::GUARD)
        {
            return Ok((filter == Some(ActionKind::GUARD)).then_some(task));
        }
        inside = task_target.pasture.is_some();
    }
    // LAB_14127bb9f: a villager on livestock (or on what is inside something) takes task 110
    // for class 58; anything else the general search
    if let Some(task_target) = target
        && (roles
            .of_class(task_target.master.class)
            .contains(ClassRoles::LIVESTOCK)
            || inside)
        && roles.of_class(own.class).contains(ClassRoles::VILLAGER)
    {
        return Ok(first(list, |task| {
            task.action_kind == HUNT
                && (task.for_class(slots.class.livestock) || inside)
                && wanted(filter, task)
        }));
    }
    Ok(general(list, actor, on.general, on.terrain, None, dip))
}

#[cfg(test)]
mod tests;
