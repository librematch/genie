//! `UNIT_ORDER` (0x75): an order of the player's units by its action.

use crate::GenieResult;
use crate::commands::QueueId;
use crate::commands::Shift;
use crate::commands::UnitList;
use crate::garrison::GarrisonScope;
use crate::map::Point;
use crate::objects::ObjectId;
use crate::players::PlayerId;
use crate::production::UnitIndex;
use crate::world::World;

/// `UNIT_ORDER`'s action (`+0x1c`, the switch of `FUN_14161e370`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnitOrderAction {
    /// 1: an offboard trade (`FUN_140ad4ff0`), not modelled.
    OffboardTrade,
    /// 2: a unit transform to a point or a target (`FUN_140ae58a0`), not modelled.
    Transform,
    /// 3: a gather action (`FUN_140ac1b80`), not modelled.
    Gather,
    /// 4: a building's queue cancels an entry (`FUN_1411d66b0`, [`World::cancel_order`]).
    Cancel,
    /// 5: the units garrison in the target ([`World::garrison_order`]).
    Garrison,
    /// 6: the drop-off's search alone ([`World::unit_order`]).
    DropOffSearch,
    /// 7: the drop-off ([`World::unit_order`]).
    DropOff,
    /// 8: the drop-off, its work recorded for the town bell and the way back queued
    /// ([`World::unit_order`]).
    DropOffAndBack,
    /// Any other byte: the switch has no case for it, so it does nothing.
    Other(u8),
}

impl UnitOrderAction {
    /// The action of byte `+0x1c`.
    #[must_use]
    pub const fn of(byte: u8) -> Self {
        match byte {
            1 => Self::OffboardTrade,
            2 => Self::Transform,
            3 => Self::Gather,
            4 => Self::Cancel,
            5 => Self::Garrison,
            6 => Self::DropOffSearch,
            7 => Self::DropOff,
            8 => Self::DropOffAndBack,
            other => Self::Other(other),
        }
    }

    /// Its byte (`+0x1c`).
    #[must_use]
    pub const fn byte(self) -> u8 {
        match self {
            Self::OffboardTrade => 1,
            Self::Transform => 2,
            Self::Gather => 3,
            Self::Cancel => 4,
            Self::Garrison => 5,
            Self::DropOffSearch => 6,
            Self::DropOff => 7,
            Self::DropOffAndBack => 8,
            Self::Other(byte) => byte,
        }
    }
}

/// `UNIT_ORDER` (0x75, `FUN_140e7f200` -> `FUN_14161e370`): the units the packet lists (the count
/// at `+0x4`, the ids from `+0x21`) take the order `action` ([`World::unit_order`]) with the
/// object `target` (`tId`, `+8`, which the garrison reads) and the flags `+0x1d`, `+0x1e`;
/// `human` (`+0x20`) sets their last human order time first.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitOrderCommand {
    /// The player whose units take it.
    pub player: PlayerId,
    /// The units.
    pub units: UnitList,
    /// `tId`: the target object's id.
    pub target: Option<ObjectId>,
    /// `+0xc`, `+0x10`: the point, which the actions modelled do not read.
    pub point: Point,
    /// `+0x14`, the formatter's `qId`, which the actions modelled do not read.
    pub queue_id: QueueId,
    /// `+0x1d`: shift held, which a garrison reads as its dispatch (through the units' unit AIs,
    /// their slot 0x30) and a cancel as its batch (up to five of a kind).
    pub shift: Shift,
    /// `+0x1e`: the shelters a garrison takes (the target's line near it, or the target alone).
    pub scope: GarrisonScope,
    /// The order's action.
    pub action: UnitOrderAction,
    /// `+0x18`: a cancel's unit index in the queue.
    pub param: UnitIndex,
    /// `+0x20`: it sets their last human order time.
    pub human: bool,
}

impl UnitOrderCommand {
    /// `do_command`'s check refuses a count past the packet's ids; the handler reads the first
    /// of them ([`World::read_units`]). A human player's order takes the world time as its
    /// units' last human order time first ([`World::human_order`]); then the action: the
    /// garrison ([`World::garrison_order`]), a cancel ([`World::cancel_order`]) or the others
    /// ([`World::unit_order`]).
    pub(crate) fn run(&self, world: &mut World) -> GenieResult<()> {
        let Some(ids) = world.read_units(self.player, &self.units) else {
            return Ok(());
        };
        if self.human {
            world.human_order(ids);
        }
        match self.action {
            UnitOrderAction::Garrison => world.garrison_order(
                self.player,
                ids,
                self.target,
                self.shift.dispatch(),
                self.scope,
            ),
            UnitOrderAction::Cancel => world.cancel_order(ids, self.param, self.shift.batch()),
            action => world.unit_order(self.player, ids, action),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every byte maps to its action and back.
    #[test]
    fn an_action_keeps_its_byte() {
        for byte in 0..=u8::MAX {
            assert_eq!(UnitOrderAction::of(byte).byte(), byte);
        }
        assert_eq!(UnitOrderAction::of(5), UnitOrderAction::Garrison);
        assert_eq!(UnitOrderAction::of(9), UnitOrderAction::Other(9));
    }
}
