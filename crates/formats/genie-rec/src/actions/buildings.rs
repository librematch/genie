//! The buildings' commands: make, research, build, build wall and cancel build.

use crate::actions::{Location2, ObjectsList};
use crate::{ObjectId, PlayerId, Result};
use crate::{fits, opt_fits};
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::{ReadSkipExt, TechId, UnitKindId, read_le, read_opt_u32};
use std::io::{Read, Write};

/// Train a unit at a building (command `0x64`; aoc-mgz calls it the AI queue command).
#[derive(Debug, Default, Clone, Copy)]
pub struct MakeCommand {
    /// The ID of the player issuing this command.
    pub player_id: PlayerId,
    /// The building that trains the unit.
    pub building_id: ObjectId,
    /// The type of unit to train.
    pub unit_kind_id: UnitKindId,
    /// Not known yet; read as an optional object ID (-1 for none).
    pub target_id: Option<ObjectId>,
}

impl MakeCommand {
    /// Read a Make command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        input.skip(3)?;
        let building_id = input.read_u32::<LE>()?.into();
        let player_id = input.read_u8()?.into();
        let _padding = input.read_u8()?;
        let unit_kind_id = input.read_u16::<LE>()?.into();
        let target_id = read_opt_u32(&mut input)?;
        Ok(Self {
            player_id,
            building_id,
            unit_kind_id,
            target_id,
        })
    }

    /// Write this Make command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if the target ID is above `i32::MAX`.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_all(&[0, 0, 0])?;
        output.write_u32::<LE>(self.building_id.into())?;
        output.write_u8(self.player_id.into())?;
        output.write_u8(0)?;
        output.write_u16::<LE>(self.unit_kind_id.into())?;
        output.write_i32::<LE>(opt_fits(self.target_id, -1)?)?;
        Ok(())
    }
}

/// Start a research.
#[derive(Debug, Default, Clone, Copy)]
pub struct ResearchCommand {
    /// The ID of the player starting the research.
    pub player_id: PlayerId,
    /// The building where the research is taking place.
    pub building_id: ObjectId,
    /// The tech ID of the research.
    pub tech_id: TechId,
    /// Not known yet; read as an optional object ID (-1 for none).
    pub target_id: Option<ObjectId>,
}

impl ResearchCommand {
    /// Read a Research command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        input.skip(3)?;
        let building_id = input.read_u32::<LE>()?.into();
        let player_id = input.read_u8()?.into();
        let _padding = input.read_u8()?;
        let tech_id = input.read_u16::<LE>()?.into();
        let target_id = read_opt_u32(&mut input)?;
        Ok(Self {
            player_id,
            building_id,
            tech_id,
            target_id,
        })
    }

    /// Write this Research command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if the target ID is above `i32::MAX`.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_all(&[0, 0, 0])?;
        output.write_u32::<LE>(self.building_id.into())?;
        output.write_u8(self.player_id.into())?;
        output.write_u8(0)?;
        output.write_u16::<LE>(self.tech_id.into())?;
        output.write_i32::<LE>(opt_fits(self.target_id, -1)?)?;
        Ok(())
    }
}

/// Place a building foundation and task a group of villagers to start building.
#[derive(Debug, Default, Clone)]
pub struct BuildCommand {
    /// The ID of the player issuing this command.
    pub player_id: PlayerId,
    /// The type of building to place.
    pub unit_kind_id: UnitKindId,
    /// The location of the new building foundation.
    pub location: Location2,
    /// The index of the frame to use, for buildings with multiple graphics like houses.
    pub frame: u8,
    /// The IDs of the villagers that are tasked to build this building.
    pub builders: ObjectsList,
    /// Unique ID for the _command_ (not building)? Used by AIs?
    pub(super) unique_id: Option<u32>,
}

impl BuildCommand {
    /// Read a Build command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        let selected_count = input.read_i8()?;
        command.player_id = input.read_u8()?.into();
        let _padding = input.read_u8()?;
        let x = input.read_f32::<LE>()?;
        let y = input.read_f32::<LE>()?;
        command.location = (x, y);
        command.unit_kind_id = input.read_u16::<LE>()?.into();
        let _padding = input.read_u16::<LE>()?;
        command.unique_id = read_opt_u32(&mut input)?;
        command.frame = input.read_u8()?;
        input.skip(3)?;
        command.builders = ObjectsList::read_from(input, i32::from(selected_count))?;
        Ok(command)
    }
}

/// Task a group of villagers to build a wall from point A to point B.
#[derive(Debug, Default, Clone)]
pub struct BuildWallCommand {
    /// The ID of the player issuing this command.
    pub player_id: PlayerId,
    /// The tile (x, y) where the wall starts.
    pub start: (u8, u8),
    /// The tile (x, y) where the wall ends.
    pub end: (u8, u8),
    /// The type of wall to build.
    pub unit_kind_id: UnitKindId,
    /// The villagers that are tasked to build the wall.
    pub builders: ObjectsList,
}

impl BuildWallCommand {
    pub(super) fn read_from(mut input: impl Read) -> Result<Self> {
        let selected_count = input.read_i8()?;
        let player_id = input.read_u8()?.into();
        let start = (input.read_u8()?, input.read_u8()?);
        let end = (input.read_u8()?, input.read_u8()?);
        let _padding = input.read_u8()?;
        let unit_kind_id = input.read_u16::<LE>()?.into();
        let _padding = input.read_u16::<LE>()?;
        assert_eq!(
            input.read_u32::<LE>()?,
            0xFFFF_FFFF,
            "check out what this is for"
        );
        let builders = if selected_count == -1 {
            ObjectsList::SameAsLast
        } else {
            let mut list = read_le(&mut input, fits(selected_count)?, i32::from_le_bytes)?;
            if selected_count == 1 && list[0] == -1 {
                list.clear();
            }
            ObjectsList::List(list.into_iter().map(fits).collect::<Result<_>>()?)
        };
        Ok(Self {
            player_id,
            start,
            end,
            unit_kind_id,
            builders,
        })
    }
}

/// Delete a building or cancel a building that's not fully built yet.
#[derive(Debug, Default, Clone, Copy)]
pub struct CancelBuildCommand {
    /// The ID of the player issuing this command.
    pub player_id: PlayerId,
    /// The ID of the building to cancel.
    pub building_id: ObjectId,
}

impl CancelBuildCommand {
    /// Read a Cancel Build command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if the stored player ID is above 255.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        input.skip(3)?;
        let building_id = input.read_u32::<LE>()?.into();
        let player_id = fits(input.read_u32::<LE>()?)?;
        Ok(Self {
            player_id,
            building_id,
        })
    }

    /// Write this Cancel Build command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_all(&[0, 0, 0])?;
        output.write_u32::<LE>(self.building_id.into())?;
        output.write_u32::<LE>(u32::from(self.player_id))?;
        Ok(())
    }
}
