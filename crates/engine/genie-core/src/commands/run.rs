//! A command's run in the command phase ([`World::run`]), a human player's order and the
//! order units' reset.

use crate::GenieError;
use crate::commands::Command;
use crate::math::rng::Stream;
use crate::objects::ObjectId;
use crate::world::World;
use crate::world::slots::PlayerSlots;

impl World {
    /// Run a command in the command phase (`TRIBE_Command::do_command`, `0x140e77b00`): its own
    /// run ([`Command::run`]). Where the slice does not model what the command does, the lane of
    /// the player it stops ([`Command::stopped_player`]) stops.
    pub(crate) fn run(&mut self, command: &Command) {
        if let Err(e) = command.run(self) {
            let player = command.stopped_player(self.players.len());
            self.stop_player(player, e);
        }
    }

    /// `FUN_140e5e780`: each unit of `ids` with a unit AI takes the world time as its last human
    /// order time (`+0x650`).
    pub(crate) fn human_order(&mut self, ids: &[Option<ObjectId>]) {
        for &id in ids.iter().flatten() {
            let unit = self
                .players
                .iter_mut()
                .flat_map(|p| p.objects.iter_mut())
                .find(|o| o.id == id)
                .filter(|o| o.combat.ai.is_some());
            if let Some(u) = unit {
                u.last_human_order = self.world_time;
                self.spatial.set_gait(u.id, u.gait());
            }
        }
    }
}

impl World {
    /// `FUN_140e6cfc0`: each of an order's units (INTERACT, MOVE, RETREAT) the world holds,
    /// sleeping ones too (`FUN_1415417a0`), with a unit AI resets it for the order
    /// ([`UnitAi::reset_for_order`]), in list order.
    ///
    /// [`UnitAi::reset_for_order`]: crate::unit_ai::UnitAi::reset_for_order
    pub(crate) fn reset_order_units(
        &mut self,
        units: &[Option<ObjectId>],
    ) -> Result<(), GenieError> {
        for &id in units {
            let Some(place) = self.place(id) else {
                continue;
            };
            let (p, i) = match place {
                crate::orders::Place::Live(p, i) | crate::orders::Place::Sleeping(p, i) => (p, i),
            };
            let player = self.players.player_mut(p)?;
            let unit = match place {
                crate::orders::Place::Live(..) => player.objects.checked_mut(i)?,
                crate::orders::Place::Sleeping(..) => player.sleeping.checked_mut(i)?,
            };
            if let Some(ai) = unit.combat.ai.as_mut() {
                ai.reset_for_order(self.streams.get(Stream::Ai));
            }
        }
        Ok(())
    }
}
