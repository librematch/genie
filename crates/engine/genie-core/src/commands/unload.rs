//! UNLOAD (0x6f): units let out of the player's buildings and transports.

use crate::GenieError;
use crate::commands::UnitList;
use crate::map::Point;
use crate::objects::ObjectId;
use crate::players::PlayerId;
use crate::world::World;

/// UNLOAD (0x6f, `FUN_14161f410`): the units the packet lists (the count at `+0x4`, the ids
/// from `+0x19`; a building, the units inside it; a unit inside one, itself) let units out
/// ([`World::unload`]) toward `point` (−1, −1 for none), those `pick` names
/// (`FUN_14118a610`); `sound` (`+0x18`) plays the building's.
#[derive(Clone, Debug, PartialEq)]
pub struct UnloadCommand {
    /// The player.
    pub player: PlayerId,
    /// Its objects.
    pub units: UnitList,
    /// Where they go.
    pub point: Point,
    /// Which units leave: the packet's flag (`+0x14`) and the value it reads (`+0x10`).
    pub pick: UnloadPick,
    /// `+0x18`.
    pub sound: bool,
}

/// Which units an UNLOAD lets out (`FUN_14118a610`'s switch on the packet's flag, `+0x14`): the
/// flag decides what its value (`+0x10`) names, a player, a group of classes or an object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnloadPick {
    /// Flag 0: every unit but class 42, with the value it does not read (the packet's, kept).
    All(i32),
    /// Flag 1: the units of the player of this number (the value the game compares with a
    /// player's index).
    Player(i32),
    /// Flag 2: the units of a group of classes (`unload_group_takes`).
    Group(i32),
    /// Flag 3: the object itself.
    Object(Option<ObjectId>),
    /// Flag 4: the units of the object's master (any villager for a villager).
    SameMaster(Option<ObjectId>),
    /// Flag 5: every unit but the object and class 42.
    AllBut(Option<ObjectId>),
    /// Flag 6: the units not of the object's master.
    OtherMasters(Option<ObjectId>),
    /// Another flag, which picks none, with its value.
    Other(u8, i32),
}

impl UnloadPick {
    /// The pick of the packet's `flag` and `value`.
    #[must_use]
    pub const fn of_packet(flag: u8, value: i32) -> Self {
        let object = ObjectId::of_field(value);
        match flag {
            0 => Self::All(value),
            1 => Self::Player(value),
            2 => Self::Group(value),
            3 => Self::Object(object),
            4 => Self::SameMaster(object),
            5 => Self::AllBut(object),
            6 => Self::OtherMasters(object),
            other => Self::Other(other, value),
        }
    }

    /// The packet's flag and value (an encoder's).
    #[must_use]
    pub const fn packet(self) -> (u8, i32) {
        match self {
            Self::All(v) => (0, v),
            Self::Player(v) => (1, v),
            Self::Group(v) => (2, v),
            Self::Object(o) => (3, ObjectId::field(o)),
            Self::SameMaster(o) => (4, ObjectId::field(o)),
            Self::AllBut(o) => (5, ObjectId::field(o)),
            Self::OtherMasters(o) => (6, ObjectId::field(o)),
            Self::Other(flag, v) => (flag, v),
        }
    }

    /// The object flags 3 to 6 name (`FUN_1415417a0` reads it); none for the other flags.
    #[must_use]
    pub const fn object(self) -> Option<ObjectId> {
        match self {
            Self::Object(o) | Self::SameMaster(o) | Self::AllBut(o) | Self::OtherMasters(o) => o,
            Self::All(_) | Self::Player(_) | Self::Group(_) | Self::Other(..) => None,
        }
    }
}

impl UnloadCommand {
    /// The check (`FUN_140e6f620`, [`World::unload_check`]) refuses a count past the packet's
    /// ids; the handler reads the first of them ([`World::read_units`], [`World::unload`]).
    pub(crate) fn run(&self, w: &mut World) -> Result<(), GenieError> {
        let Some(ids) = w.read_units(self.player, &self.units) else {
            return Ok(());
        };
        w.unload(self, ids)
    }
}
