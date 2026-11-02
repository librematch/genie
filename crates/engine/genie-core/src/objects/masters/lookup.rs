//! A player's masters by id ([`Masters`]) and the work command's choice of master and task.

use crate::GenieError;
use crate::construction::repair::REPAIR;
use crate::objects::masters::ActionType;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::action_type;
use crate::objects::masters::ids;
use crate::objects::masters::{Master, MasterId, Masters, Task};
use crate::players::PlayerId;
use crate::unit_ai::task_finder::{Actor, Diplomacy, TaskOn, find_task};
use crate::world::parts::PartTable;

/// Player `player`'s master `id` in `every`, the world's masters: one table per player, as the
/// game's player holds its masters' pointers (`player+0x28`); none where the player has no
/// table or its table no such master (a null pointer there). The one lookup of a master by its
/// player and id: an object's master goes through [`crate::objects::unit::Unit::master`], an
/// invariant one through [`crate::world::World::master`].
#[must_use]
pub fn player_master(every: &[Masters], player: PlayerId, id: MasterId) -> Option<&Master> {
    every.of(player)?.get(id)
}

/// The masters an object's master pointer (`+0xa0`) reads: its own copy by its id
/// ([`crate::world::parts::Parts::own_master`]) when it has one, else its player's table of
/// `players`. [`crate::world::World::master_tables`] gives the world's.
#[derive(Clone, Copy, Debug)]
pub struct MasterTables<'a> {
    /// The world's masters, one table per player.
    pub players: &'a [Masters],
    /// The objects' own masters by id.
    pub own: &'a PartTable<Master>,
}

impl<'a> MasterTables<'a> {
    /// The world's players' tables `players` and the objects' own masters `own`.
    #[must_use]
    pub const fn new(players: &'a [Masters], own: &'a PartTable<Master>) -> Self {
        Self { players, own }
    }

    /// Player `player`'s objects' masters ([`OwnerMasters`]).
    #[must_use]
    pub fn of(self, player: PlayerId) -> OwnerMasters<'a> {
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
pub struct OwnerMasters<'a> {
    /// The objects' own masters by id.
    pub own: &'a PartTable<Master>,
    /// The owner's table.
    pub table: Option<&'a Masters>,
}

impl<'a> OwnerMasters<'a> {
    /// The own masters `own` and the owner's table `table`.
    #[must_use]
    pub const fn new(own: &'a PartTable<Master>, table: Option<&'a Masters>) -> Self {
        Self { own, table }
    }
}

/// The world's masters, one table per player (the game's player holds its masters' pointers,
/// `player+0x28`), looked up by the player.
pub trait PlayersMasters {
    /// Player `player`'s masters, none where the world holds no table for it.
    fn of(&self, player: PlayerId) -> Option<&Masters>;

    /// Player `player`'s masters, to change.
    fn of_mut(&mut self, player: PlayerId) -> Option<&mut Masters>;
}

impl PlayersMasters for [Masters] {
    fn of(&self, player: PlayerId) -> Option<&Masters> {
        self.get(player.index())
    }

    fn of_mut(&mut self, player: PlayerId) -> Option<&mut Masters> {
        self.get_mut(player.index())
    }
}

impl Masters {
    /// The masters, by their ids.
    pub fn new(masters: impl IntoIterator<Item = Master>) -> Self {
        let mut by_id: Vec<Option<Master>> = Vec::new();
        for m in masters {
            let i = m.id.index();
            if let Some(slot) = by_id.get_mut(i) {
                *slot = Some(m);
            } else {
                by_id.resize(i, None);
                by_id.push(Some(m));
            }
        }
        Self { by_id }
    }

    /// The master `id`, if the player has it.
    #[must_use]
    pub fn get(&self, id: MasterId) -> Option<&Master> {
        self.by_id.get(id.index())?.as_ref()
    }

    /// Every master, by id.
    pub fn iter(&self) -> impl Iterator<Item = &Master> {
        self.by_id.iter().flatten()
    }

    /// Every master, by id, to change (a technology's effect).
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Master> {
        self.by_id.iter_mut().flatten()
    }

    /// The master `id` to change, if the player has it.
    pub fn get_mut(&mut self, id: MasterId) -> Option<&mut Master> {
        self.by_id.get_mut(id.index())?.as_mut()
    }

    /// The work command's choice of master and task (`FUN_14106ff70` up to the task's
    /// requirement) on `on` (its target, none for a point, and the terrain under the point): the
    /// unit's own master's task ([`find_task`]) unless it has none or only an attack (7); then,
    /// with a task swap group, the first master by id of the unit's class, type and group with a
    /// task of another type (its master changes, slot 0x90), except that a gather task (106) for
    /// a farm, a pasture or the other objects the work command lists, with any filter, prefers
    /// the first such master with a task for the target's class and a work value, else the first
    /// with a task for any target. `Ok(None)`: no task.
    pub fn work_master<'m>(
        &'m self,
        actor: &Actor<'_>,
        on: &TaskOn<'_, '_>,
        filter: Option<ActionType>,
        dip: &impl Diplomacy,
    ) -> Result<Option<(MasterId, &'m Task)>, GenieError> {
        let own = actor.master;
        let own_list = self.get(own.id).ok_or(GenieError::unsupported(
            "a unit of a master the player does not have",
        ))?;
        let own_task = find_task(
            own_list, actor, on.target, on.general, on.terrain, filter, dip,
        )?;
        if own_task.is_none_or(|t| t.action_type == action_type::COMBAT)
            && own.task_by_group != TaskGroup::NONE
        {
            let (mut by_class, mut generic) = (None, None);
            for candidate in self.by_id.iter().flatten() {
                if candidate.class != own.class
                    || candidate.kind != own.kind
                    || candidate.task_by_group != own.task_by_group
                {
                    continue;
                }
                let found = find_task(
                    candidate, actor, on.target, on.general, on.terrain, filter, dip,
                )?;
                let Some(task) = found else { continue };
                if own_task.is_some_and(|t| t.action_type == task.action_type) {
                    continue;
                }
                // a gather task for a point has no target: the game reads none there, as the
                // finder gives a point only a task for its terrain, never a gather one
                let listed = on.target.filter(|t| {
                    matches!(
                        t.master.id,
                        ids::FARM
                            | ids::ID_357
                            | ids::ID_1187
                            | ids::ID_1188
                            | ids::PASTURE_MANGROVE
                            | ids::ID_1894
                            | ids::PASTURE
                            | ids::ID_1898
                    )
                });
                let Some(target) = listed.filter(|t| {
                    task.action_type == REPAIR
                        && (filter.is_none() || filter == Some(REPAIR))
                        && task.object_id != Some(t.master.id)
                }) else {
                    return Ok(Some((candidate.id, task)));
                };
                if by_class.is_none()
                    && task.for_class(target.master.class)
                    && task.work_val1.positive()
                {
                    by_class = Some((candidate.id, task));
                } else if generic.is_none()
                    && task.object_group.is_none()
                    && task.object_id.is_none()
                {
                    generic = Some((candidate.id, task));
                }
            }
            if let Some(pick) = by_class.or(generic) {
                return Ok(Some(pick));
            }
        }
        Ok(own_task.map(|t| (own.id, t)))
    }
}
