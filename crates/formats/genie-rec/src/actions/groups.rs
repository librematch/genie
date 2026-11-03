//! Resign and the group commands: waypoint, stance, guard, follow, patrol, formation and
//! `UserPatch`'s AI commands.

use crate::actions::{Location2, ObjectsList};
use crate::{ObjectId, PlayerId, Result};
use crate::{fits, opt_fits};
use arrayvec::ArrayVec;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::{ReadSkipExt, read_opt_u32};
use std::io::{Read, Write};

/// A player resigns or drops from the game.
#[derive(Debug, Default, Clone, Copy)]
pub struct ResignCommand {
    /// The ID of the player that is resigning.
    pub player_id: PlayerId,
    /// The multiplayer ID of the player that is resigning.
    pub comm_player_id: PlayerId,
    /// Is this "resignation" because the player dropped from the game?
    pub dropped: bool,
}

impl ResignCommand {
    /// Read a Resign command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let player_id = input.read_u8()?.into();
        let comm_player_id = input.read_u8()?.into();
        let dropped = input.read_u8()? != 0;
        Ok(Self {
            player_id,
            comm_player_id,
            dropped,
        })
    }

    /// Write this Resign command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(self.player_id.into())?;
        output.write_u8(self.comm_player_id.into())?;
        output.write_u8(u8::from(self.dropped))?;
        Ok(())
    }
}

/// Set a waypoint for a group of objects (command `0x10`).
#[derive(Debug, Default, Clone)]
pub struct GroupWaypointCommand {
    /// The ID of the player issuing this command.
    pub player_id: PlayerId,
    /// The waypoint's tile coordinates (x, y).
    pub location: (u8, u8),
    /// The objects receiving the waypoint.
    pub objects: ObjectsList,
}

impl GroupWaypointCommand {
    /// Read a group waypoint command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let player_id = input.read_u8()?.into();
        let num_units = input.read_u8()?;
        let x = input.read_u8()?;
        let y = input.read_u8()?;
        Ok(Self {
            player_id,
            location: (x, y),
            objects: ObjectsList::read_from(input, i32::from(num_units))?,
        })
    }

    /// Write this group waypoint command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than 255 objects, the most the format's `u8` count holds.
    pub fn write_to(&self, mut output: impl Write) -> Result<()> {
        output.write_u8(self.player_id.into())?;
        output.write_u8(fits(self.objects.len())?)?;
        output.write_u8(self.location.0)?;
        output.write_u8(self.location.1)?;
        self.objects.write_to(&mut output)?;
        Ok(())
    }
}

/// Set a group of objects's "AI State" (usually known as "stance").
#[derive(Debug, Default, Clone)]
pub struct UnitAiStateCommand {
    /// The new state. Aggressive/Defensive/No Attack/ etc.
    pub state: i8,
    /// The objects whose AI state is being changed.
    pub objects: ObjectsList,
}

impl UnitAiStateCommand {
    /// Read a `UnitAiState` command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let selected_count = input.read_u8()?;
        let state = input.read_i8()?;
        let objects = ObjectsList::read_from(input, i32::from(selected_count))?;
        Ok(Self { state, objects })
    }

    /// Write this `UnitAiState` command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than 255 objects, the most the format's `u8` count holds.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(fits(self.objects.len())?)?;
        output.write_i8(self.state)?;
        self.objects.write_to(output)?;
        Ok(())
    }
}

/// Task units to guard an object.
#[derive(Debug, Default, Clone)]
pub struct GuardCommand {
    /// The target object of this order.
    pub target_id: Option<ObjectId>,
    /// The objects this command applies to.
    pub objects: ObjectsList,
}

impl GuardCommand {
    /// Read a Guard command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        let selected_count = i32::from(input.read_u8()?);
        input.skip(2)?;
        command.target_id = read_opt_u32(&mut input)?;
        command.objects = ObjectsList::read_from(input, selected_count)?;
        Ok(command)
    }

    /// Write a Guard command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than 255 objects (the format's `u8` count), or if the target ID
    /// is above `i32::MAX`.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(fits(self.objects.len())?)?;
        output.write_all(&[0, 0])?;
        output.write_i32::<LE>(opt_fits(self.target_id, -1)?)?;
        self.objects.write_to(output)?;
        Ok(())
    }
}

/// Task units to follow an object.
#[derive(Debug, Default, Clone)]
pub struct FollowCommand {
    /// The target object of this order.
    pub target_id: Option<ObjectId>,
    /// The objects this command applies to.
    pub objects: ObjectsList,
}

impl FollowCommand {
    /// Read a Follow command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        let selected_count = i32::from(input.read_u8()?);
        input.skip(2)?;
        command.target_id = read_opt_u32(&mut input)?;
        command.objects = ObjectsList::read_from(input, selected_count)?;
        Ok(command)
    }

    /// Write a Follow command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than 255 objects (the format's `u8` count), or if the target ID
    /// is above `i32::MAX`.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(fits(self.objects.len())?)?;
        output.write_all(&[0, 0])?;
        output.write_i32::<LE>(opt_fits(self.target_id, -1)?)?;
        self.objects.write_to(output)?;
        Ok(())
    }
}

/// Task a group of objects to patrol along a given path.
#[derive(Debug, Default, Clone)]
pub struct PatrolCommand {
    /// The waypoints that this patrol should pass through.
    pub waypoints: ArrayVec<Location2, 10>,
    /// The objects to include in this formation.
    pub objects: ObjectsList,
}

impl PatrolCommand {
    /// Read a Patrol command from an input stream.
    ///
    /// The format always stores 10 waypoints: first their X coordinates, then their Y coordinates.
    /// Only the first `waypoint_count` of them are kept.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if the command says it has more than 10 waypoints.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        let selected_count = input.read_i8()?;
        let waypoint_count = input.read_u8()?;
        let _padding = input.read_u8()?;
        let mut raw_waypoints = [(0.0, 0.0); 10];
        for w in &mut raw_waypoints {
            w.0 = input.read_f32::<LE>()?;
        }
        for w in &mut raw_waypoints {
            w.1 = input.read_f32::<LE>()?;
        }
        command
            .waypoints
            .try_extend_from_slice(&raw_waypoints[0..usize::from(waypoint_count)])
            .map_err(|_| crate::Error::ValueOutOfRange)?;
        command.objects = ObjectsList::read_from(input, i32::from(selected_count))?;
        Ok(command)
    }

    /// Write this Patrol command to an output stream.
    ///
    /// Unused waypoint slots are written as (0.0, 0.0).
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than 127 objects, the most the format's `i8` count holds.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_i8(fits(self.objects.len())?)?;
        output.write_u8(fits(self.waypoints.len())?)?;
        output.write_u8(0)?;
        for i in 0..10 {
            output.write_f32::<LE>(self.waypoints.get(i).copied().unwrap_or_default().0)?;
        }
        for i in 0..10 {
            output.write_f32::<LE>(self.waypoints.get(i).copied().unwrap_or_default().1)?;
        }
        self.objects.write_to(output)?;
        Ok(())
    }
}

/// Task a group of objects to form a formation.
#[derive(Debug, Default, Clone)]
pub struct FormFormationCommand {
    /// The ID of the player issuing this command.
    pub player_id: PlayerId,
    /// The type of formation to form.
    pub formation_kind: i32,
    /// The objects to include in this formation.
    pub objects: ObjectsList,
}

impl FormFormationCommand {
    /// Read a Form Formation command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut command = Self::default();
        let selected_count = input.read_i8()?;
        command.player_id = input.read_u8()?.into();
        let _padding = input.read_u8()?;
        command.formation_kind = input.read_i32::<LE>()?;
        command.objects = ObjectsList::read_from(input, i32::from(selected_count))?;
        Ok(command)
    }

    /// Write this Form Formation command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than 127 objects, the most the format's `i8` count holds.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_i8(fits(self.objects.len())?)?;
        output.write_u8(self.player_id.into())?;
        output.write_u8(0)?;
        output.write_i32::<LE>(self.formation_kind)?;
        self.objects.write_to(output)?;
        Ok(())
    }
}

/// Meta-command for `UserPatch`'s new AI commands.
#[derive(Debug, Default, Clone)]
pub struct UserPatchAiCommand {
    /// The ID of the player issuing this command.
    pub player_id: PlayerId,
    /// 0: move to object
    /// 1: set unit ai state
    /// 2: ?
    /// 3: ?
    /// 4: stop unit group?
    /// 5: dropoff something?
    /// 6: dropoff something?
    /// 7: ?
    /// 8: set offensive target priority
    /// 9: reset offensive target priorities?
    /// 10: nothing?
    /// 11: stop unit group?
    /// 12: set gather point to garrison in self
    /// 13: set ai player name
    /// 14: unload object
    /// 15: nothing?
    pub ai_action: u8,
    /// The parameters of the AI action: up to 3 `u32` values, as many as the command's size
    /// leaves room for.
    pub params: ArrayVec<u32, 4>,
}

impl UserPatchAiCommand {
    /// Read a `UserPatch` AI command of `size` bytes from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if `size` is below 4, or if it leaves room for more than 3 parameters (20 or
    /// more).
    pub fn read_from(mut input: impl Read, size: u32) -> Result<Self> {
        let num_params = size.checked_sub(4).ok_or(crate::Error::ValueOutOfRange)? / 4;
        if 4 <= num_params {
            return Err(crate::Error::InvalidData(
                "a user patch AI command with more than 3 parameters",
            ));
        }
        let ai_action = input.read_u8()?;
        let player_id = input.read_u8()?.into();
        let _padding = input.read_u8()?;
        let mut params: ArrayVec<u32, 4> = ArrayVec::default();
        for _ in 0..num_params {
            params.push(input.read_u32::<LE>()?);
        }
        Ok(Self {
            player_id,
            ai_action,
            params,
        })
    }

    /// Write this `UserPatch` AI command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(self.ai_action)?;
        output.write_u8(self.player_id.into())?;
        output.write_u8(0)?;
        for p in &self.params {
            output.write_u32::<LE>(*p)?;
        }
        Ok(())
    }
}
