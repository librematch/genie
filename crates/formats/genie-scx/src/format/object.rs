//! An object placed in the scenario ([`ScenarioObject`]).

use crate::Result;
use crate::types::ScxVersion;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::UnitTypeId;
use std::io::{Read, Write};

// pub enum LostInformation {
//     DisabledTechs(i32, i32),
//     DisabledUnits(i32, i32),
//     DisabledBuildings(i32, i32),
//     MapType,
// }

/// An object placed in the scenario.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScenarioObject {
    /// Position (x, y, z) of this object.
    pub position: (f32, f32, f32),
    /// This object's unique ID.
    pub id: i32,
    /// The type ID of this object.
    pub object_type: UnitTypeId,
    /// State value.
    pub state: u8,
    /// Radian angle this unit is facing.
    pub angle: f32,
    /// Current animation frame.
    pub frame: i16,
    /// ID of the object this object is garrisoned in, or -1 when not
    /// garrisoned.
    pub garrisoned_in: Option<i32>,
}

impl ScenarioObject {
    /// Read a placed object in the layout of the given format version.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails.
    pub fn read_from(mut input: impl Read, version: ScxVersion) -> Result<Self> {
        let position = (
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
        );
        let id = input.read_i32::<LE>()?;
        let object_type = input.read_u16::<LE>()?.into();
        let state = input.read_u8()?;
        let angle = input.read_f32::<LE>()?;
        let frame = if version < ScxVersion(*b"1.15") {
            -1
        } else {
            input.read_i16::<LE>()?
        };
        let garrisoned_in = if version < ScxVersion(*b"1.13") {
            None
        } else {
            Some(input.read_i32::<LE>()?)
        }
        .and_then(|id| match id {
            -1 => None,
            id => Some(id),
        })
        .and_then(|id| match id {
            // 0 means -1 in "recent" versions
            0 if version > ScxVersion(*b"1.12") => None,
            id => Some(id),
        });

        Ok(Self {
            position,
            id,
            object_type,
            state,
            angle,
            frame,
            garrisoned_in,
        })
    }

    /// Write the placed object in the layout of the given format version.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails.
    pub fn write_to(&self, mut output: impl Write, version: ScxVersion) -> Result<()> {
        output.write_f32::<LE>(self.position.0)?;
        output.write_f32::<LE>(self.position.1)?;
        output.write_f32::<LE>(self.position.2)?;
        output.write_i32::<LE>(self.id)?;
        output.write_u16::<LE>(self.object_type.into())?;
        output.write_u8(self.state)?;
        output.write_f32::<LE>(self.angle)?;
        if version > ScxVersion(*b"1.14") {
            output.write_i16::<LE>(self.frame)?;
        }
        if version > ScxVersion(*b"1.12") {
            match self.garrisoned_in {
                Some(id) => output.write_i32::<LE>(id)?,
                None => output.write_i32::<LE>(-1)?,
            }
        }
        Ok(())
    }
}
