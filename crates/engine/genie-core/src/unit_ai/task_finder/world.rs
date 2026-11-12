//! What the task finder reads of the world: the relations, the drop sites, a wait's progress,
//! a target's state and the search of slot 0x820.

use crate::GenieResult;
use crate::combat::damage::most_hp;
use crate::construction::FoundationId;
use crate::map::Position;
use crate::map::TerrainId;
use crate::map::terrain::Terrain;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::Building;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterId;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::{Master, Masters};
use crate::objects::masters::{MasterTables, Task};
use crate::objects::unit::Unit;
use crate::objects::unit::UnitFlags;
use crate::players::AllyTest;
use crate::players::PlayerId;
use crate::players::treaty::TreatyMonitor;
use crate::players::{NO_RELATIONS, Relations};
use crate::unit_ai::task_finder::{Actor, Diplomacy, DropSite, TaskOn, TaskTarget};
use crate::unit_ai::task_finder::{
    allied as task_finder_allied, drops_at as task_finder_drops_at, find_task,
};
use crate::unit_ai::world_view::Foreign;
use crate::world::Seconds;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::others::OthersRef;
use crate::world::parts::PartTable;
use crate::world::parts::Parts;
use crate::world::player::Player;
use crate::world::slots::PlayerSlotsExt;

/// The terrain the task finder's general search reads at `point` (`FUN_1411a8ba0`,
/// [`Terrain::at_task_point`]); none on a world without its map's terrain.
pub(crate) fn point_terrain(map: Option<&Terrain>, point: Position) -> Option<TerrainId> {
    map.and_then(|terrain| terrain.at_task_point(point.ground()))
}

/// The players' relations, the world's treaty monitor and the drop sites, as the task finder
/// reads them.
pub(crate) struct TaskWorld<'world> {
    pub(crate) players: &'world [Player],
    /// The world's treaty monitor (`+0x820`), which the ally test reads.
    pub(crate) treaty: Option<&'world TreatyMonitor>,
    pub(crate) drop_sites: &'world [DropSite],
}

impl Diplomacy for TaskWorld<'_> {
    /// [`crate::unit_ai::task_finder::allied`] with `from`'s diplomacy.
    fn allied(&self, from: PlayerId, to: PlayerId, test: AllyTest) -> bool {
        let relations = self
            .players
            .player(from)
            .map_or(&NO_RELATIONS, |player| &player.relations);
        task_finder_allied(relations, self.treaty, from, to, test)
    }

    fn drops_at(&self, building: MasterId, actor: &Actor<'_>) -> bool {
        task_finder_drops_at(self.drop_sites, building, actor)
    }
}

/// The players' relations, the world's treaty monitor and the drop sites as the task finder
/// reads them while a unit's owner's list is out for its update
/// ([`UnitView`](crate::unit_ai::world_view::UnitView)): the owner's relations, the others'.
pub(crate) struct SideWorld<'world> {
    pub(crate) player: PlayerId,
    pub(crate) relations: &'world Relations,
    pub(crate) foreign: Foreign<'world>,
    /// The world's treaty monitor (`+0x820`), which the ally test reads.
    pub(crate) treaty: Option<&'world TreatyMonitor>,
    pub(crate) drop_sites: &'world [DropSite],
}

impl Diplomacy for SideWorld<'_> {
    /// [`crate::unit_ai::task_finder::allied`] with `from`'s diplomacy.
    fn allied(&self, from: PlayerId, to: PlayerId, test: AllyTest) -> bool {
        let relations = if from == self.player {
            self.relations
        } else {
            self.foreign
                .player(from)
                .map_or(&NO_RELATIONS, |player| &player.relations)
        };
        task_finder_allied(relations, self.treaty, from, to, test)
    }

    fn drops_at(&self, building: MasterId, actor: &Actor<'_>) -> bool {
        task_finder_drops_at(self.drop_sites, building, actor)
    }
}

/// The build points (`+0xc50`) a wait for a foundation reads of its foundation
/// ([`crate::construction::wait_solidify::WaitSolidify::update`]) among its owner's other objects and the
/// other players' lists: None when the world does not hold it. Its target is the building that
/// sent the unit off (the unit AI's notify 0x216), whose build points are its construction's;
/// the world never gives its id to another object, so an object without a construction is not
/// met. `buildings` holds the objects' building layers.
pub(crate) fn wait_progress(
    foundation: FoundationId,
    others: OthersRef<'_>,
    buildings: &PartTable<Building>,
) -> Option<Seconds> {
    others.get(foundation.id()).and_then(|unit| {
        buildings
            .get(unit.id)?
            .construction
            .map(|construction| construction.progress)
    })
}

/// The state (`+0x640`) of object `id` among its owner's other objects and the other players'
/// lists: None when they do not hold it (the unit itself, or one of its owner's sleeping objects:
/// static objects only, never a drop site, which the callers ask for).
pub(crate) fn state_of(id: Option<ObjectId>, others: OthersRef<'_>) -> Option<ObjectState> {
    others.get(id).map(|unit| unit.life.state)
}

/// What the task finder reads of object `id`, as `find` holds the objects (with their owners)
/// and `every` the players' masters, `parts` the objects' parts; an annex stands for its main
/// building (`+0xbb8` of its building layer), `steps` times (`World::task_target`'s reading).
/// None when `find` does not hold it.
pub(crate) fn task_target_with<'world>(
    id: ObjectId,
    steps: usize,
    find: &impl Fn(ObjectId) -> Option<(PlayerId, &'world Unit)>,
    parts: &Parts,
    every: MasterTables<'world>,
) -> GenieResult<Option<TaskTarget<'world>>> {
    let mut id = id;
    for _ in 0..steps {
        match find(id).and_then(|(_, unit)| parts.building_of(unit.id).annex_of) {
            Some(main) => id = main.id(),
            None => break,
        }
    }
    let Some((player, unit)) = find(id) else {
        return Ok(None);
    };
    let master = unit.master(every, player)?;
    Ok(Some(TaskTarget {
        id: unit.id,
        master,
        player,
        state: unit.life.state,
        held: unit.life.held(),
        hp: unit.life.hp + parts.combat_of(unit.id).hp_powerup.room(),
        max_hp: {
            let combat = parts.combat_of(unit.id);
            most_hp(combat.hp_stinger, master.hit_points, combat.hp_powerup)
        },
        attackable: unit.flags.contains(UnitFlags::ATTACKABLE),
        pasture: unit.pasture,
        top_action: parts.actions_of(unit.id).top_action_kind(),
    }))
}

/// The search of a unit's slot 0x820 (`FUN_14106c6a0`) on `on` (the target, the target the
/// general search reads, its main building's, and the terrain) with `filter`: the list of its own master
/// (`actor.master`) gives the task ([`crate::unit_ai::task_finder::find_task`] in mode 0); without one, a
/// master with a task swap group (`+0x7b8`) tries its player's masters (`masters`) in id order
/// of its class, type and group, its own too, with any filter, and the first with a task gives
/// it. The task, with the master that gave it when the swap group did: with its `param_7` the
/// routine switches the unit to that master (slot 0x90 with 0), its own too, which is the
/// caller's. None: no task. The terrain: the terrain under the point the caller passes
/// ([`point_terrain`]; none for the Build's drop-off's −1), which only a task for a terrain
/// reads. `World::find_work_task` in `crate::orders` runs it with the switch.
pub(crate) fn task_search<'masters>(
    masters: &'masters Masters,
    actor: &Actor<'_>,
    on: &TaskOn<'_, '_>,
    filter: Option<ActionKind>,
    dip: &impl Diplomacy,
    slots: Slots,
    roles: &Roles,
) -> GenieResult<Option<(Task, Option<&'masters Master>)>> {
    let own = actor.master;
    let find = |list: &Master, filter: Option<ActionKind>| {
        find_task(list, actor, on, filter, dip, slots, roles).map(Option::<&Task>::copied)
    };
    if let Some(task) = find(own, filter)? {
        return Ok(Some((task, None)));
    }
    if own.action_part().task_by_group == TaskGroup::NONE {
        return Ok(None);
    }
    for candidate in masters {
        if candidate.class != own.class
            || candidate.kind != own.kind
            || candidate.action_part().task_by_group != own.action_part().task_by_group
        {
            continue;
        }
        if let Some(task) = find(candidate, None)? {
            return Ok(Some((task, Some(candidate))));
        }
    }
    Ok(None)
}
