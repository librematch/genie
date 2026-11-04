//! A player's masters by id ([`Masters`]) and the work command's choice of master and task.

use crate::construction::repair::REPAIR;
use crate::objects::masters::ActionKind;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::{Master, MasterId, Masters, Task};
use crate::players::PlayerId;
use crate::unit_ai::task_finder::{Actor, Diplomacy, TaskOn, find_task};
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::parts::PartTable;
use crate::{GenieError, GenieResult};
use std::sync::Arc;

/// Player `player`'s master `id` in `every`, the world's masters: one table per player, as the
/// game's player holds its masters' pointers (`player+0x28`); none where the player has no
/// table or its table no such master (a null pointer there). The one lookup of a master by its
/// player and id: an object's master goes through [`crate::objects::unit::Unit::master`], an
/// invariant one through [`crate::world::World::master`].
#[must_use]
pub fn player_master(every: &[Arc<Masters>], player: PlayerId, id: MasterId) -> Option<&Master> {
    every.of(player)?.get(id)
}

/// The masters an object's master pointer (`+0xa0`) reads: its own copy by its id
/// ([`crate::world::parts::Parts::own_master`]) when it has one, else its player's table of
/// `players`. [`crate::world::World::master_tables`] gives the world's.
#[derive(Clone, Copy, Debug)]
pub struct MasterTables<'masters> {
    /// The world's masters, one table per player.
    pub(crate) players: &'masters [Arc<Masters>],
    /// The objects' own masters by id.
    pub(crate) own: &'masters PartTable<Master>,
}

impl<'masters> MasterTables<'masters> {
    /// The world's players' tables `players` and the objects' own masters `own`.
    #[must_use]
    pub const fn new(players: &'masters [Arc<Masters>], own: &'masters PartTable<Master>) -> Self {
        Self { players, own }
    }

    /// Player `player`'s objects' masters ([`OwnerMasters`]).
    #[must_use]
    pub fn of(self, player: PlayerId) -> OwnerMasters<'masters> {
        OwnerMasters {
            own: self.own,
            table: self.players.of(player),
        }
    }
}

/// The masters one owner's objects read through their master pointers (`+0xa0`): an object's
/// own copy by its id first ([`crate::world::parts::Parts::own_master`]), else the owner's
/// table (none for an owner without one).
#[derive(Clone, Copy, Debug)]
pub struct OwnerMasters<'masters> {
    /// The objects' own masters by id.
    pub(crate) own: &'masters PartTable<Master>,
    /// The owner's table.
    pub(crate) table: Option<&'masters Masters>,
}

impl<'masters> OwnerMasters<'masters> {
    /// The own masters `own` and the owner's table `table`.
    #[must_use]
    pub const fn new(own: &'masters PartTable<Master>, table: Option<&'masters Masters>) -> Self {
        Self { own, table }
    }
}

/// The world's masters, one table per player (the game's player holds its masters' pointers,
/// `player+0x28`), looked up by the player.
pub trait PlayersMastersExt {
    /// Player `player`'s masters, none where the world holds no table for it.
    fn of(&self, player: PlayerId) -> Option<&Masters>;

    /// Player `player`'s masters, to change.
    fn of_mut(&mut self, player: PlayerId) -> Option<&mut Masters>;
}

impl PlayersMastersExt for [Arc<Masters>] {
    fn of(&self, player: PlayerId) -> Option<&Masters> {
        self.get(player.index()).map(|masters| &**masters)
    }

    fn of_mut(&mut self, player: PlayerId) -> Option<&mut Masters> {
        self.get_mut(player.index()).map(Arc::make_mut)
    }
}

/// The work command's choice of master and task (`FUN_14106ff70` up to the task's
/// requirement) on `on` (its target, none for a point, and the terrain under the point): the
/// unit's own master's task ([`find_task`]) unless it has none or only an attack (7); then,
/// with a task swap group, the first master by id of the unit's class, type and group with a
/// task of another type (its master changes, slot 0x90), except that a gather task (106) for
/// a farm, a pasture or the other objects the work command lists, with any filter, prefers
/// the first such master with a task for the target's class and a work value, else the first
/// with a task for any target: the master of the player's list (`masters`) and its task.
/// `Ok(None)`: no task.
pub fn work_master<'masters>(
    masters: &'masters Masters,
    actor: &Actor<'_>,
    on: &TaskOn<'_, '_>,
    filter: Option<ActionKind>,
    dip: &impl Diplomacy,
    slots: Slots,
    roles: &Roles,
) -> GenieResult<Option<(&'masters Master, &'masters Task)>> {
    let own = actor.master;
    let own_list = masters.get(own.id).ok_or(GenieError::unsupported(
        "a unit of a master the player does not have",
    ))?;
    let own_task = find_task(own_list, actor, on, filter, dip, slots, roles)?;
    if own_task.is_none_or(|task| task.action_kind == ActionKind::COMBAT)
        && own.action_part().task_by_group != TaskGroup::NONE
    {
        let (mut by_class, mut generic) = (None, None);
        for candidate in masters {
            if candidate.class != own.class
                || candidate.kind != own.kind
                || candidate.action_part().task_by_group != own.action_part().task_by_group
            {
                continue;
            }
            let found = find_task(candidate, actor, on, filter, dip, slots, roles)?;
            let Some(task) = found else { continue };
            if own_task.is_some_and(|own| own.action_kind == task.action_kind) {
                continue;
            }
            // a gather task for a point has no target: the game reads none there, as the
            // finder gives a point only a task for its terrain, never a gather one
            let listed = on.target.filter(|task_target| {
                roles.of_master(task_target.master.id).intersects(
                    MasterRoles::FARM
                        | MasterRoles::DEAD_FARM
                        | MasterRoles::RICE_FARM
                        | MasterRoles::DEAD_RICE_FARM
                        | MasterRoles::PASTURE_MANGROVE
                        | MasterRoles::DEAD_PASTURE_MANGROVE
                        | MasterRoles::PASTURE
                        | MasterRoles::DEAD_PASTURE,
                )
            });
            let Some(target) = listed.filter(|task_target| {
                task.action_kind == REPAIR
                    && (filter.is_none() || filter == Some(REPAIR))
                    && task.object_id != Some(task_target.master.id)
            }) else {
                return Ok(Some((candidate, task)));
            };
            if by_class.is_none()
                && task.for_class(target.master.class)
                && task.work_val1.positive()
            {
                by_class = Some((candidate, task));
            } else if generic.is_none() && task.object_group.is_none() && task.object_id.is_none() {
                generic = Some((candidate, task));
            }
        }
        if let Some(pick) = by_class.or(generic) {
            return Ok(Some(pick));
        }
    }
    Ok(own_task.map(|task| (own_list, task)))
}
