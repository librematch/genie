//! The tasks: attack ground, repair, ungarrison, flare, unit order, queue, gather point, the
//! market's buy and sell, the town bell and back to work.

use crate::actions::{Location2, ObjectsList};
use crate::{ObjectId, PlayerId, Result};
use crate::{fits, opt_fits};
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::{ReadSkipExt, UnitTypeId, f32_neq, read_opt_u32};
use std::io::{Read, Write};

/// Task an object to attack ground.
#[derive(Debug, Default, Clone)]
pub struct AttackGroundCommand {
    /// The target location of this command.
    pub location: Location2,
    /// The objects being tasked.
    pub objects: ObjectsList,
}

impl AttackGroundCommand {
    /// Read a `AttackGround` command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        let selected_count = i32::from(input.read_i8()?);
        input.skip(2)?;
        command.location = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
        command.objects = ObjectsList::read_from(input, selected_count)?;
        Ok(command)
    }

    /// Write this `AttackGround` command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than 127 objects, the most the format's `i8` count holds.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_i8(fits(self.objects.len())?)?;
        output.write_all(&[0, 0])?;
        output.write_f32::<LE>(self.location.0)?;
        output.write_f32::<LE>(self.location.1)?;
        self.objects.write_to(output)?;
        Ok(())
    }
}

/// Task units to repair an object.
#[derive(Debug, Default, Clone)]
pub struct RepairCommand {
    /// The target object of this order.
    pub target_id: Option<ObjectId>,
    /// The objects this command applies to.
    pub repairers: ObjectsList,
}

impl RepairCommand {
    /// Read a Repair command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        let selected_count = i32::from(input.read_u8()?);
        input.skip(2)?;
        command.target_id = read_opt_u32(&mut input)?;
        command.repairers = ObjectsList::read_from(input, selected_count)?;
        Ok(command)
    }

    /// Write a Repair command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than 255 repairers (the format's `u8` count), or if the target
    /// ID is above `i32::MAX`.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(fits(self.repairers.len())?)?;
        output.write_all(&[0, 0])?;
        output.write_i32::<LE>(opt_fits(self.target_id, -1)?)?;
        self.repairers.write_to(output)?;
        Ok(())
    }
}

/// Ungarrison objects from a given list of objects.
#[derive(Debug, Default, Clone)]
pub struct UngarrisonCommand {
    /// Which objects to ungarrison. aoc-mgz names the values 0 (all), 3 (selected), 4 (same
    /// type), 5 (not selected) and 6 (inverse type).
    pub ungarrison_type: i8,
    /// The unit type to ungarrison, for the kinds that select by type; read as an optional ID
    /// (-1 for none).
    pub unit_type_id: Option<ObjectId>,
    /// Where the ungarrisoned objects go, if a location was given (it is stored as -1, -1 if
    /// not).
    pub location: Option<Location2>,
    /// The objects to ungarrison from.
    pub objects: ObjectsList,
}

impl UngarrisonCommand {
    pub(super) fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        let selected_count = input.read_i8()?;
        let _padding = input.read_u16::<LE>()?;
        let x = input.read_f32::<LE>()?;
        let y = input.read_f32::<LE>()?;
        command.location = if f32_neq!(x, -1.0) && f32_neq!(y, -1.0) {
            Some((x, y))
        } else {
            None
        };
        command.ungarrison_type = input.read_i8()?;
        input.skip(3)?;
        command.unit_type_id = read_opt_u32(&mut input)?;
        command.objects = ObjectsList::read_from(input, i32::from(selected_count))?;
        Ok(command)
    }
}

/// Send a flare at the given location.
#[derive(Debug, Default, Clone, Copy)]
pub struct FlareCommand {
    /// The ID of the player sending the flare.
    pub player_id: PlayerId,
    /// The multiplayer ID of the player sending the flare.
    pub comm_player_id: PlayerId,
    /// Which players see the flare, indexed by player ID.
    pub recipients: [bool; 9],
    /// The location of the flare.
    pub location: Location2,
}

impl FlareCommand {
    /// Read a Flare command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if the unit ID stored before the recipients is not -1.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        input.skip(3)?;
        if input.read_i32::<LE>()? != -1 {
            return Err(crate::Error::InvalidData("a flare with a unit id"));
        }
        for receive in &mut command.recipients {
            *receive = input.read_u8()? != 0;
        }
        input.skip(3)?;
        command.location = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
        command.player_id = input.read_u8()?.into();
        command.comm_player_id = input.read_u8()?.into();
        input.skip(2)?;
        Ok(command)
    }
}

/// Give objects a unit order (command `0x75`), such as packing or unpacking a trebuchet.
#[derive(Debug, Default, Clone)]
pub struct UnitOrderCommand {
    /// The target object, if any.
    pub target_id: Option<ObjectId>,
    /// The order. aoc-mgz names the values 1 (pack a trebuchet), 2 (unpack a trebuchet) and 5
    /// (garrison).
    pub action: i8,
    /// A parameter of the order, if any (stored as -1 if not). aoc-mgz reads it as the index of
    /// the queue entry to cancel when a production queue is cancelled.
    pub param: Option<u8>,
    /// The target location, if one was given (it is stored as -1, -1 if not).
    pub location: Option<Location2>,
    /// An ID for the command (stored as -1 if absent); its use is not known yet.
    pub unique_id: Option<u32>,
    /// The objects receiving the order.
    pub objects: ObjectsList,
}

impl UnitOrderCommand {
    pub(super) fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        let selected_count = input.read_i8()?;
        let _padding = input.read_u16::<LE>()?;
        command.target_id = read_opt_u32(&mut input)?;
        command.action = input.read_i8()?;
        command.param = match input.read_i8()? {
            -1 => None,
            param => Some(param.cast_unsigned()),
        };
        let _padding = input.read_u16::<LE>()?;
        let x = input.read_f32::<LE>()?;
        let y = input.read_f32::<LE>()?;
        command.location = if f32_neq!(x, -1.0) && f32_neq!(y, -1.0) {
            Some((x, y))
        } else {
            None
        };
        command.unique_id = read_opt_u32(&mut input)?;
        command.objects = ObjectsList::read_from(input, i32::from(selected_count))?;
        Ok(command)
    }
}

/// Queue units of one type at a building (command `0x77`).
#[derive(Debug, Default, Clone, Copy)]
pub struct QueueCommand {
    /// The ID of the building where this unit is being queued.
    pub building_id: ObjectId,
    /// The ID of the unit type being queued.
    pub unit_type_id: UnitTypeId,
    /// The amount of units to queue.
    pub amount: u16,
}

impl QueueCommand {
    /// Read a Queue command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        input.skip(3)?;
        command.building_id = input.read_u32::<LE>()?.into();
        command.unit_type_id = input.read_u16::<LE>()?.into();
        command.amount = input.read_u16::<LE>()?;
        Ok(command)
    }

    /// Write this Queue command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_all(&[0, 0, 0])?;
        output.write_u32::<LE>(self.building_id.into())?;
        output.write_u16::<LE>(self.unit_type_id.into())?;
        output.write_u16::<LE>(self.amount)?;
        Ok(())
    }
}

/// Set the gather point of one or more buildings (command `0x78`).
#[derive(Debug, Default, Clone)]
pub struct SetGatherPointCommand {
    /// The IDs of the buildings whose gather points are being set.
    pub buildings: ObjectsList,
    /// The ID of the object being targeted, if the gather point is set to an object.
    pub target_id: Option<ObjectId>,
    /// The type ID of the unit being targeted, if the gather point is set to an object.
    pub target_type_id: Option<UnitTypeId>,
    /// The location of the new gather point, if the gather point is not set to an object.
    pub location: Option<Location2>,
}

impl SetGatherPointCommand {
    /// Read a Set Gather Point command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        let selected_count = i32::from(input.read_i8()?);
        input.skip(2)?;
        command.target_id = read_opt_u32(&mut input)?;
        command.target_type_id = match input.read_u16::<LE>()? {
            0xFFFF => None,
            id => Some(id.into()),
        };
        input.skip(2)?;
        command.location = Some((input.read_f32::<LE>()?, input.read_f32::<LE>()?));
        command.buildings = ObjectsList::read_from(input, selected_count)?;
        Ok(command)
    }

    /// Write this Set Gather Point command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than 255 buildings, the most the format's `u8` count holds.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(fits(self.buildings.len())?)?;
        output.write_all(&[0, 0])?;
        output.write_u32::<LE>(self.target_id.map_or(0xFFFF_FFFF, Into::into))?;
        output.write_u16::<LE>(self.target_type_id.map_or(0xFFFF, Into::into))?;
        output.write_all(&[0, 0])?;
        if let Some((x, y)) = self.location {
            output.write_f32::<LE>(x)?;
            output.write_f32::<LE>(y)?;
        } else {
            output.write_f32::<LE>(0.0)?;
            output.write_f32::<LE>(0.0)?;
        }
        self.buildings.write_to(output)?;
        Ok(())
    }
}

/// Read and write impl for market buying/selling commands, which are different commands but have
/// the same shape.
macro_rules! buy_sell_impl {
    ($name:ident) => {
        impl $name {
            /// Read this market command from an input stream.
            ///
            /// # Errors
            ///
            /// Returns an error if reading from `input` fails.
            pub fn read_from(mut input: impl Read) -> Result<Self> {
                Ok(Self {
                    player_id: input.read_u8()?.into(),
                    resource: input.read_u8()?,
                    amount: input.read_i8()?,
                    market_id: input.read_u32::<LE>()?.into(),
                })
            }

            /// Write this market command to an output stream.
            ///
            /// # Errors
            ///
            /// Returns an error if writing to `output` fails.
            pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
                output.write_u8(self.player_id.into())?;
                output.write_u8(self.resource)?;
                output.write_i8(self.amount)?;
                output.write_u32::<LE>(self.market_id.into())?;
                Ok(())
            }
        }
    };
}

/// Sell a resource at the market.
#[derive(Debug, Default, Clone, Copy)]
pub struct SellResourceCommand {
    /// The ID of the player issuing this command.
    pub player_id: PlayerId,
    /// The resource being sold.
    pub resource: u8,
    /// The amount being sold, in 100s. Typically this is 1 for selling 100 of a resource, or 5 for
    /// selling 500 (with Shift-click).
    pub amount: i8,
    /// The ID of the building where this resource is being bought.
    pub market_id: ObjectId,
}

buy_sell_impl!(SellResourceCommand);

/// Buy a resource at the market.
#[derive(Debug, Default, Clone, Copy)]
pub struct BuyResourceCommand {
    /// The ID of the player issuing this command.
    pub player_id: PlayerId,
    /// The resource being bought.
    pub resource: u8,
    /// The amount being bought, in 100s. Typically this is 1 for buying 100 of a resource, or 5 for
    /// buying 500 (with Shift-click).
    pub amount: i8,
    /// The ID of the building where this resource is being bought.
    pub market_id: ObjectId,
}

buy_sell_impl!(BuyResourceCommand);

/// Command `0x7f`, which aoc-mgz and the DE engine name the town bell: ring or silence the town
/// bell of a town center.
#[derive(Debug, Default, Clone, Copy)]
pub struct Unknown7fCommand {
    /// The town center whose bell is rung.
    pub object_id: ObjectId,
    /// Whether the bell is turned on (non-zero) or off, as aoc-mgz reads it.
    pub value: u32,
}

impl Unknown7fCommand {
    /// Read a `0x7f` (town bell) command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        input.skip(3)?;
        let object_id = input.read_u32::<LE>()?.into();
        let value = input.read_u32::<LE>()?;
        Ok(Self { object_id, value })
    }
}

/// Send villagers back to work after they've been garrisoned into the Town Center.
#[derive(Debug, Default, Clone, Copy)]
pub struct BackToWorkCommand {
    /// The building (typically a town center) the villagers leave.
    pub building_id: ObjectId,
}

impl BackToWorkCommand {
    /// Read a Back To Work command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        input.skip(3)?;
        let building_id = input.read_u32::<LE>()?.into();
        Ok(Self { building_id })
    }
}
