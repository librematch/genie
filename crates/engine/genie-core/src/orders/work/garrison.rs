//! The garrison finder's view of a unit: the task finder's actor and whether its own task list holds a garrison task on a target.

use crate::GenieResult;
use crate::garrison::ENTER;
use crate::map::Position;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::task_finder::Actor;
use crate::unit_ai::task_finder::world::TaskWorld;
use crate::unit_ai::task_finder::{TaskOn, find_task};
use crate::world::World;
use crate::world::slots::PlayerSlotsExt;

impl World {
    /// The unit at `idx` of player `player` as the task finder's actor.
    ///
    /// # Errors
    ///
    /// A unit or a master the world does not hold.
    fn garrison_actor(&self, player: PlayerId, idx: ListIndex) -> GenieResult<Actor<'_>> {
        let owner = self.players.player(player)?;
        let unit = owner.live(idx)?;
        Ok(Actor {
            id: unit.id,
            master: unit.master(self.master_tables(), player)?,
            player,
            held: unit.life.held(),
            attribute_95: owner.attributes.get(95).copied().unwrap_or(Amount::ZERO),
            civ: owner.civ,
        })
    }

    /// The unit's own task list's finder with filter 3 (slot 2 of its master's `+0x788`,
    /// `FUN_1412784d0`: the garrison's, [`crate::unit_ai::task_finder::find_task`]) on `target` at
    /// the gather `point`: whether it finds a task, as a gather point on a building asks
    /// (`FUN_140ace9c0`).
    pub(crate) fn garrison_task_found(
        &self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        point: Position,
    ) -> GenieResult<bool> {
        let actor = self.garrison_actor(player, idx)?;
        let master = actor.master;
        let world = TaskWorld {
            players: &self.players,
            treaty: self.treaty.as_ref(),
            drop_sites: &self.model.drop_sites,
        };
        let task_target = self.task_target(target, 1)?;
        let general = self.task_target(target, 2)?;
        Ok(find_task(
            master,
            &actor,
            &TaskOn {
                target: Some(&task_target),
                general: Some(&general),
                terrain: self.task_terrain(point),
            },
            Some(ENTER),
            &world,
            self.model.slots,
            &self.model.roles,
        )?
        .is_some())
    }
}
