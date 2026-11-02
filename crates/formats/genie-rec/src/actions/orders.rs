//! The orders: order, stop, work, move, create, add resource and the AI's order (`0x0a`).

use crate::actions::{Location2, Location3, ObjectsList};
use crate::{ObjectId, PlayerId, Result};
use crate::{fits, opt_fits};
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::{ReadSkipExt, UnitTypeId, read_opt_u32};
use std::io::{Read, Write};

/// Task an object to a target object or a target location.
#[derive(Debug, Default, Clone)]
pub struct OrderCommand {
    /// The ID of the player executing this command.
    pub player_id: PlayerId,
    /// The target object of this order.
    pub target_id: Option<ObjectId>,
    /// The target location of this order.
    pub location: Location2,
    /// The objects this command applies to.
    pub objects: ObjectsList,
}

impl OrderCommand {
    /// Read an Order command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self {
            player_id: input.read_u8()?.into(),
            ..Default::default()
        };
        input.skip(2)?;
        command.target_id = read_opt_u32(&mut input)?;
        let selected_count = input.read_i32::<LE>()?;
        command.location = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
        command.objects = ObjectsList::read_from(input, selected_count)?;
        Ok(command)
    }

    /// Write an Order command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if the target ID is above `i32::MAX`, or if there are more than `u32::MAX` objects.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(self.player_id.into())?;
        output.write_all(&[0, 0])?;
        output.write_i32::<LE>(opt_fits(self.target_id, -1)?)?;
        output.write_u32::<LE>(fits(self.objects.len())?)?;
        output.write_f32::<LE>(self.location.0)?;
        output.write_f32::<LE>(self.location.1)?;
        self.objects.write_to(output)?;
        Ok(())
    }
}

/// Task objects to stop.
#[derive(Debug, Default, Clone)]
pub struct StopCommand {
    /// The objects to stop.
    pub objects: ObjectsList,
}

impl StopCommand {
    /// Read a Stop command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        let selected_count = input.read_i8()?;
        command.objects = ObjectsList::read_from(input, i32::from(selected_count))?;
        Ok(command)
    }

    /// Write this Stop command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than 127 objects, the most the format's `i8` count holds.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_i8(fits(self.objects.len())?)?;
        self.objects.write_to(output)?;
        Ok(())
    }
}

/// Task an object to work.
#[derive(Debug, Default, Clone)]
pub struct WorkCommand {
    /// The target object of this command.
    pub target_id: Option<ObjectId>,
    /// The target location of this command.
    pub location: Location2,
    /// The objects being tasked.
    pub objects: ObjectsList,
}

impl WorkCommand {
    /// Read a Work command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        input.skip(3)?;
        command.target_id = read_opt_u32(&mut input)?;
        let selected_count = input.read_i8()?;
        input.skip(3)?;
        command.location = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
        command.objects = ObjectsList::read_from(input, i32::from(selected_count))?;
        Ok(command)
    }

    /// Write this Work command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than 127 objects, the most the format's `i8` count holds.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_all(&[0, 0, 0])?;
        output.write_i32::<LE>(self.target_id.map_or(-1, |u| u32::from(u).cast_signed()))?;
        output.write_i8(fits(self.objects.len())?)?;
        output.write_all(&[0, 0, 0])?;
        output.write_f32::<LE>(self.location.0)?;
        output.write_f32::<LE>(self.location.1)?;
        self.objects.write_to(output)?;
        Ok(())
    }
}

/// Task an object to move.
#[derive(Debug, Default, Clone)]
pub struct MoveCommand {
    /// The ID of the player issuing this command.
    pub player_id: PlayerId,
    /// The target object of this command.
    pub target_id: Option<ObjectId>,
    /// The target location of this command.
    pub location: Location2,
    /// The objects being tasked.
    pub objects: ObjectsList,
}

impl MoveCommand {
    /// Read a Move command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self {
            player_id: input.read_u8()?.into(),
            ..Default::default()
        };
        input.skip(2)?;
        command.target_id = read_opt_u32(&mut input)?;
        let selected_count = input.read_i8()?;
        input.skip(3)?;
        command.location = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
        command.objects = ObjectsList::read_from(input, i32::from(selected_count))?;
        Ok(command)
    }

    /// Write this Move command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than 127 objects, the most the format's `i8` count holds.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(self.player_id.into())?;
        output.write_all(&[0, 0])?;
        output.write_i32::<LE>(self.target_id.map_or(-1, |u| u32::from(u).cast_signed()))?;
        output.write_i8(fits(self.objects.len())?)?;
        output.write_all(&[0, 0, 0])?;
        output.write_f32::<LE>(self.location.0)?;
        output.write_f32::<LE>(self.location.1)?;
        self.objects.write_to(output)?;
        Ok(())
    }
}
/// A command that instantly places a unit type at a given location.
///
/// Typically used for cheats and the like.
#[derive(Debug, Default, Clone, Copy)]
pub struct CreateCommand {
    /// The ID of the player issuing this command.
    pub player_id: PlayerId,
    /// The type of unit to create.
    pub unit_type_id: UnitTypeId,
    /// The location.
    pub location: Location3,
}

impl CreateCommand {
    /// Read a Create command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        let _padding = input.read_u8()?;
        command.unit_type_id = input.read_u16::<LE>()?.into();
        command.player_id = input.read_u8()?.into();
        let _padding = input.read_u8()?;
        command.location = (
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
        );
        Ok(command)
    }

    /// Write this Create command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(0)?;
        output.write_u16::<LE>(self.unit_type_id.into())?;
        output.write_u8(self.player_id.into())?;
        output.write_u8(0)?;
        output.write_f32::<LE>(self.location.0)?;
        output.write_f32::<LE>(self.location.1)?;
        output.write_f32::<LE>(self.location.2)?;
        Ok(())
    }
}

/// Add resources to a player's stockpile.
///
/// Typically used for cheats.
#[derive(Debug, Default, Clone, Copy)]
pub struct AddResourceCommand {
    /// The player this command applies to.
    pub player_id: PlayerId,
    /// The resource to add.
    pub resource: u8,
    /// The amount to add to this resource. May be negative for subtracting.
    pub amount: f32,
}

impl AddResourceCommand {
    /// Read an `AddResource` command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let player_id = input.read_u8()?.into();
        let resource = input.read_u8()?;
        let _padding = input.read_u8()?;
        let amount = input.read_f32::<LE>()?;
        Ok(Self {
            player_id,
            resource,
            amount,
        })
    }

    /// Write this `AddResource` command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(self.player_id.into())?;
        output.write_u8(self.resource)?;
        output.write_u8(0)?;
        output.write_f32::<LE>(self.amount)?;
        Ok(())
    }
}

/// An order that an AI player gives to its objects (command `0x0a`).
#[derive(Debug, Default, Clone)]
pub struct AiOrderCommand {
    /// The ID of the player the command belongs to (the first player byte).
    pub player_id: PlayerId,
    /// The second player byte, read as the player issuing the order; not verified.
    pub issuer: PlayerId,
    /// The objects receiving the order. A single object is stored in the fixed part of the
    /// command, more than one in a list after it.
    pub objects: ObjectsList,
    /// The kind of order.
    pub order_type: u16,
    /// The priority of the order.
    pub order_priority: i8,
    /// The target object, if any.
    pub target_id: Option<ObjectId>,
    /// The owner of the target, if any.
    pub target_player_id: Option<PlayerId>,
    /// The target location.
    pub target_location: Location3,
    /// The range of the order.
    pub range: f32,
    /// Whether the order is carried out immediately.
    pub immediate: bool,
    /// Whether the order goes to the front of the objects' order queue instead of the back.
    pub add_to_front: bool,
}

impl AiOrderCommand {
    /// Read an AI order command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if the target's owner is stored as a negative number other than -1.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        let selected_count = i32::from(input.read_i8()?);
        command.player_id = input.read_u8()?.into();
        command.issuer = input.read_u8()?.into();
        let object_id = input.read_u32::<LE>()?;
        command.order_type = input.read_u16::<LE>()?;
        command.order_priority = input.read_i8()?;
        let _padding = input.read_u8()?;
        command.target_id = read_opt_u32(&mut input)?;
        command.target_player_id = match input.read_i8()? {
            -1 => None,
            id => Some(fits(id)?),
        };
        input.skip(3)?;
        command.target_location = (
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
        );
        command.range = input.read_f32::<LE>()?;
        command.immediate = input.read_u8()? != 0;
        command.add_to_front = input.read_u8()? != 0;
        let _padding = input.read_u16::<LE>()?;
        command.objects = if selected_count == 1 {
            ObjectsList::List(vec![object_id.into()])
        } else {
            ObjectsList::read_from(input, selected_count)?
        };
        Ok(command)
    }

    /// Write this AI order command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than 127 objects (the format's `i8` count), or if the target ID
    /// is above `i32::MAX`.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_i8(fits(self.objects.len())?)?;
        output.write_u8(self.player_id.into())?;
        output.write_u8(self.issuer.into())?;
        match &self.objects {
            ObjectsList::List(list) if list.len() == 1 => {
                output.write_u32::<LE>(list[0].into())?;
            }
            _ => output.write_i32::<LE>(-1)?,
        }
        output.write_u16::<LE>(self.order_type)?;
        output.write_i8(self.order_priority)?;
        output.write_u8(0)?;
        output.write_i32::<LE>(opt_fits(self.target_id, -1)?)?;
        output.write_i8(
            self.target_player_id
                .map_or(-1, |id| u8::from(id).cast_signed()),
        )?;
        output.write_all(&[0, 0, 0])?;
        output.write_f32::<LE>(self.target_location.0)?;
        output.write_f32::<LE>(self.target_location.1)?;
        output.write_f32::<LE>(self.target_location.2)?;
        output.write_f32::<LE>(self.range)?;
        output.write_u8(u8::from(self.immediate))?;
        output.write_u8(u8::from(self.add_to_front))?;
        output.write_all(&[0, 0])?;
        if self.objects.len() > 1 {
            self.objects.write_to(output)?;
        }
        Ok(())
    }
}
