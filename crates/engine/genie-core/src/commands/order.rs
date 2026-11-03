//! The orders of units (INTERACT, WORK, MOVE, RETREAT, FOLLOW, GUARD): their units, the check,
//! the reset and the order's run.

use crate::GenieError;
use crate::commands::UnitList;
use crate::objects::ObjectId;
use crate::orders::{Interact, MoveOrder};
use crate::players::PlayerId;
use crate::world::World;
use crate::world::slots::PlayerSlots;

/// WORK's and RETREAT's runs (`FUN_140e6a9f0`, `FUN_140e69f00`).
const ORDER_UNMODELLED: GenieError = GenieError::unsupported("an order moves its units");

/// What an order command gives its units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Order {
    /// INTERACT: work on, attack or follow a target.
    Interact(Interact),
    /// MOVE: walk to a point.
    Move(MoveOrder),
    /// FOLLOW's target (`+0x8`).
    Follow(ObjectId),
    /// GUARD's target (`+0x8`).
    Guard(ObjectId),
    /// WORK: its units' work on a target (`FUN_140e6a9f0`), which is not modelled. Its packet
    /// has INTERACT's layout (the formatter prints the same fields).
    Work(Interact),
    /// RETREAT: its units only reset their idle timeouts here (`FUN_140e69f00`, not modelled
    /// beyond them). Its packet has INTERACT's layout.
    Retreat(Interact),
}

/// INTERACT (0x00), WORK (0x02), MOVE (0x03), RETREAT (0x23), FOLLOW or GUARD: before the order
/// runs, each listed unit's AI resets its idle timeout ([`UnitAi::reset_for_order`], WORK's
/// not), in list order, wherever the unit is. INTERACT's order then runs ([`Interact`]),
/// MOVE's ([`MoveOrder`]), FOLLOW's and GUARD's; WORK and RETREAT are not modelled.
///
/// [`UnitAi::reset_for_order`]: crate::unit_ai::UnitAi::reset_for_order
#[derive(Clone, Debug, PartialEq)]
pub struct OrderCommand {
    /// The player who orders.
    pub player: PlayerId,
    /// The units of the packet (the count at `+0x10` for INTERACT, WORK, MOVE and RETREAT, whose
    /// count `0xffff` names the last commanded units, `+0x4` for FOLLOW and GUARD).
    pub units: UnitList,
    /// The order they take.
    pub order: Order,
}

impl OrderCommand {
    /// Its units after the command's check ([`OrderCommand::units`]). FOLLOW (`FUN_140e66a60`)
    /// and GUARD (`FUN_140e67620`) of a target the world lacks do nothing. The units reset for
    /// the order (WORK's not), a human's take the world time as their last human order time
    /// (`+0x15`, `FUN_140e5e780`; FOLLOW's when a unit the world holds belongs to a player who
    /// is not a computer, `FUN_141659cf0`), then the order runs.
    pub(crate) fn run(&self, w: &mut World) -> Result<(), GenieError> {
        let (player, order) = (self.player, self.order);
        let Some(units) = self.units(w)? else {
            return Ok(());
        };
        if let Order::Work(_) = order {
            return Err(ORDER_UNMODELLED);
        }
        if let Order::Follow(target) | Order::Guard(target) = order
            && w.object(target).is_none()
        {
            return Ok(());
        }
        let human = match order {
            Order::Interact(o) => o.human,
            Order::Move(o) => o.human,
            Order::Follow(_) | Order::Guard(_) => Self::any_human_unit(w, &units)?,
            Order::Work(_) | Order::Retreat(_) => false,
        };
        w.reset_order_units(&units)?;
        if human {
            w.human_order(&units);
        }
        match order {
            Order::Interact(o) => w.interact(player, &units, &o),
            Order::Move(o) => w.move_order(player, &units, &o),
            Order::Follow(target) => w.follow(player, &units, target),
            Order::Guard(target) => w.guard(player, &units, target),
            Order::Work(_) | Order::Retreat(_) => Err(ORDER_UNMODELLED),
        }
    }

    /// Whether a unit of `units` the world holds belongs to a player who is not a computer.
    fn any_human_unit(w: &World, units: &[Option<ObjectId>]) -> Result<bool, GenieError> {
        for &id in units {
            if let Some((p, _)) = w.object(id)
                && !w.players.player(p)?.kind.is_computer()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Its units after the command's check. The checks of INTERACT (`FUN_140e55250`), WORK
    /// (`FUN_140e55eb0`), MOVE (`FUN_140e553f0`) and RETREAT (`FUN_140e55710`) refuse a count
    /// past the packet's ids but `0xffff`, then read the units ([`World::command_units`]) and
    /// make each one the world does not hold, or of another player without the setting
    /// `+0x1074`, none (the game's −1, [`World::checked_units`]); read from the last commanded
    /// units, they leave the nones there. A human player's command left without a unit, or a command of a player
    /// the world lacks, does nothing (None). Else the run reads its units again
    /// (`FUN_140e6b290` with 0): a list of the command's own becomes the last commanded units,
    /// nones and all. FOLLOW's and GUARD's checks (`FUN_140e54c50`, `FUN_140e550d0`) refuse a
    /// count past the packet's ids, the count `0xffff` too, and they take the first 60 units
    /// of their packet (500 for a computer, [`World::read_units`]); GUARD's check is BUILD's
    /// builders' ([`World::command_check`]), FOLLOW's runs in [`World::follow`].
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    fn units(&self, w: &mut World) -> Result<Option<Vec<Option<ObjectId>>>, GenieError> {
        let (source, order) = (&self.units, self.order);
        if let Order::Follow(_) | Order::Guard(_) = order {
            let Some(read) = w.read_units(self.player, source) else {
                return Ok(None);
            };
            return match order {
                Order::Guard(_) => w.command_check(self.player, read),
                _ => Ok(Some(read.to_vec())),
            };
        }
        let player = self.player;
        let last = source.names_last_commanded();
        if source.overruns() && !last {
            return Ok(None);
        }
        let Some(player_kind) = w.players.get(player.index()).map(|p| p.kind) else {
            return Ok(None);
        };
        let checked = w.checked_units(player, &w.command_units(player, source));
        if last {
            w.set_last_commanded(player, checked.clone());
        }
        if !player_kind.is_computer() && checked.iter().all(Option::is_none) {
            return Ok(None);
        }
        if !last {
            w.set_last_commanded(player, checked.clone());
        }
        Ok(Some(checked))
    }
}
