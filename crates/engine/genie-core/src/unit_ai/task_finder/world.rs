//! What the task finder reads of the world: the relations, the drop sites, a wait's progress,
//! a target's state and the search of slot 0x820.

use crate::GenieError;
use crate::map::Position;
use crate::map::TerrainId;
use crate::map::terrain::Terrain;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::layers::Building;
use crate::objects::masters::ActionType;
use crate::objects::masters::MasterId;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::{Master, Masters};
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::task_finder::{Actor, Diplomacy, DropSite, TaskTarget};
use crate::unit_ai::world_view::Foreign;
use crate::world::Seconds;
use crate::world::others::OthersRef;
use crate::world::parts::PartTable;
use crate::world::player::Player;
use crate::world::slots::PlayerSlots;

/// The terrain the task finder's general search reads at `point` (`FUN_1411a8ba0`,
/// [`Terrain::at_task_point`]); none on a world without its map's terrain.
pub(crate) fn point_terrain(map: Option<&Terrain>, point: Position) -> Option<TerrainId> {
    map.and_then(|t| t.at_task_point(point.ground()))
}

/// The players' relations and the drop sites, as the task finder reads them.
pub(crate) struct TaskWorld<'a> {
    pub(crate) players: &'a [Player],
    pub(crate) drop_sites: &'a [DropSite],
}

impl Diplomacy for TaskWorld<'_> {
    /// [`crate::unit_ai::task_finder::allied`] with `from`'s diplomacy.
    fn allied(&self, from: PlayerId, to: PlayerId, with_ally: bool) -> bool {
        let relations = self
            .players
            .player(from)
            .map_or(&crate::players::NO_RELATIONS, |p| &p.relations);
        crate::unit_ai::task_finder::allied(relations, from, to, with_ally)
    }

    fn drops_at(&self, building: MasterId, actor: &Actor<'_>) -> bool {
        crate::unit_ai::task_finder::drops_at(self.drop_sites, building, actor)
    }
}

/// The players' relations and the drop sites as the task finder reads them while a unit's
/// owner's list is out for its update ([`UnitView`]): the owner's relations, the others'.
pub(crate) struct SideWorld<'a> {
    pub(crate) player: PlayerId,
    pub(crate) relations: &'a crate::players::Relations,
    pub(crate) foreign: Foreign<'a>,
    pub(crate) drop_sites: &'a [DropSite],
}

impl Diplomacy for SideWorld<'_> {
    /// [`crate::unit_ai::task_finder::allied`] with `from`'s diplomacy.
    fn allied(&self, from: PlayerId, to: PlayerId, with_ally: bool) -> bool {
        let relations = if from == self.player {
            self.relations
        } else {
            self.foreign
                .player(from)
                .map_or(&crate::players::NO_RELATIONS, |p| &p.relations)
        };
        crate::unit_ai::task_finder::allied(relations, from, to, with_ally)
    }

    fn drops_at(&self, building: MasterId, actor: &Actor<'_>) -> bool {
        crate::unit_ai::task_finder::drops_at(self.drop_sites, building, actor)
    }
}

/// The build points (`+0xc50`) a wait for a foundation reads of object `id`
/// ([`crate::construction::wait_solidify::WaitSolidify::update`]) among its owner's other objects and the
/// other players' lists: None when the world does not hold it. Its target is the building that
/// sent the unit off (the unit AI's notify 0x216), whose build points are its construction's;
/// the world never gives its id to another object, so an object without a construction is not
/// met. `buildings` holds the objects' building layers.
pub(crate) fn wait_progress(
    id: ObjectId,
    others: OthersRef<'_>,
    buildings: &PartTable<Building>,
) -> Option<Seconds> {
    others
        .get(id)
        .and_then(|u| buildings.get(u.id)?.construction.map(|k| k.progress))
}

/// The state (`+0x640`) of object `id` among its owner's other objects and the other players'
/// lists: None when they do not hold it (the unit itself, or one of its owner's sleeping objects:
/// static objects only, never a drop site, which the callers ask for).
pub(crate) fn state_of(
    id: Option<ObjectId>,
    others: OthersRef<'_>,
) -> Option<crate::objects::ObjectState> {
    others.get(id).map(|u| u.life.state)
}

/// What the task finder reads of object `id`, as `find` holds the objects (with their owners)
/// and `every` the players' masters; an annex stands for its main building (`+0xbb8` of its
/// layer in `buildings`), `steps` times (`World::task_target`'s reading). None when `find` does
/// not hold it.
pub(crate) fn task_target_with<'a>(
    id: ObjectId,
    steps: usize,
    find: &impl Fn(ObjectId) -> Option<(PlayerId, &'a Unit)>,
    buildings: &PartTable<Building>,
    every: crate::objects::masters::MasterTables<'a>,
) -> Result<Option<TaskTarget<'a>>, GenieError> {
    let mut id = id;
    for _ in 0..steps {
        match find(id).and_then(|(_, u)| buildings.get(u.id)?.annex_of) {
            Some(main) => id = main.id(),
            None => break,
        }
    }
    let Some((player, u)) = find(id) else {
        return Ok(None);
    };
    let master = u.master(every, player)?;
    Ok(Some(TaskTarget {
        id: u.id,
        master,
        player,
        state: u.life.state,
        held: u.life.held(),
        hp: u.life.hp + u.combat.hp_powerup.room(),
        max_hp: HitPoints::of_master(master.hit_points) + u.combat.hp_powerup.max,
        attackable: u.attackable,
        pasture: u.pasture,
        top_action: u.top_action_kind(),
    }))
}

/// The search of a unit's slot 0x820 (`FUN_14106c6a0`) on target `t` (`general` the target the
/// general search reads, its main building's) with `filter`: the list of its own master
/// (`actor.master`) gives the task ([`crate::unit_ai::task_finder::find_task`] in mode 0); without one, a
/// master with a task swap group (`+0x7b8`) tries its player's masters (`masters`) in id order
/// of its class, type and group, its own too, with any filter, and the first with a task gives
/// it. The task, with the master that gave it when the swap group did: with its `param_7` the
/// routine switches the unit to that master (slot 0x90 with 0), its own too, which is the
/// caller's. None: no task. `terrain`: the terrain under the point the caller passes, the
/// target's place ([`point_terrain`]), which only a task for a terrain reads (as
/// `World::find_work_task`, the same routine's port in `crate::orders`, which this module cannot
/// call).
pub(crate) fn task_search<'m>(
    masters: &'m Masters,
    actor: &Actor<'_>,
    t: &TaskTarget<'_>,
    general: &TaskTarget<'_>,
    terrain: Option<TerrainId>,
    filter: Option<ActionType>,
    dip: &impl Diplomacy,
) -> Result<Option<(crate::objects::masters::Task, Option<&'m Master>)>, GenieError> {
    let own = actor.master;
    let find = |list: &Master, filter: Option<ActionType>| {
        crate::unit_ai::task_finder::find_task(
            list,
            actor,
            Some(t),
            Some(general),
            terrain,
            filter,
            dip,
        )
        .map(Option::<&crate::objects::masters::Task>::copied)
    };
    if let Some(task) = find(own, filter)? {
        return Ok(Some((task, None)));
    }
    if own.task_by_group == TaskGroup::NONE {
        return Ok(None);
    }
    for candidate in masters.iter() {
        if candidate.class != own.class
            || candidate.kind != own.kind
            || candidate.task_by_group != own.task_by_group
        {
            continue;
        }
        if let Some(task) = find(candidate, None)? {
            return Ok(Some((task, Some(candidate))));
        }
    }
    Ok(None)
}
