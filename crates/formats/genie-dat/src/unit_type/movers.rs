//! The animated, moving and action parts of a unit type.

use crate::conv::opt_fits;
use crate::sound::SoundId;
use crate::sprite::SpriteId;
use crate::task::TaskList;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::{UnitTypeId, read_opt_u16};
use std::io::{Read, Result, Write};

/// The attribute of animated unit types.
#[derive(Debug, Default, Clone, Copy)]
pub struct AnimatedUnitTypeAttributes {
    /// Movement speed.
    pub speed: f32,
}

impl AnimatedUnitTypeAttributes {
    /// Read the animated attributes from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read, _version: f32) -> Result<Self> {
        Ok(Self {
            speed: input.read_f32::<LE>()?,
        })
    }

    /// Write this unit type to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to(&self, mut output: impl Write, _version: f32) -> Result<()> {
        output.write_f32::<LE>(self.speed)?;
        Ok(())
    }
}

/// The attributes of moving unit types.
#[derive(Debug, Default, Clone, Copy)]
pub struct MovingUnitTypeAttributes {
    /// Sprite shown while walking, or `None`.
    pub move_sprite: Option<SpriteId>,
    /// Sprite shown while running, or `None`.
    pub run_sprite: Option<SpriteId>,
    /// Rotation speed.
    pub turn_speed: f32,
    /// Old size class (genieutils' `OldSizeClass`).
    pub size_class: u8,
    /// Unit type left behind as a trail while moving (e.g. footprints; genieutils' `TrackingUnit`),
    /// or `None`.
    pub trailing_unit: Option<UnitTypeId>,
    /// When the trailing unit is left (genieutils' `TrackingUnitMode`).
    pub trailing_options: u8,
    /// Spacing of the trailing units (genieutils' `TrackingUnitDensity`).
    pub trailing_spacing: f32,
    /// Old movement algorithm (genieutils' `OldMoveAlgorithm`).
    pub move_algorithm: u8,
    /// Turning radius.
    pub turn_radius: f32,
    /// genieutils' `TurnRadiusSpeed`: the speed that goes with the turning radius.
    pub turn_radius_speed: f32,
    /// The most the unit turns per second while moving.
    pub maximum_yaw_per_second_moving: f32,
    /// Time for a full turn while standing.
    pub stationary_yaw_revolution_time: f32,
    /// The most the unit turns per second while standing.
    pub maximum_yaw_per_second_stationary: f32,
}

impl MovingUnitTypeAttributes {
    /// Read the moving attributes from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if an ID is out of range.
    pub fn read_from(mut input: impl Read, _version: f32) -> Result<Self> {
        Ok(Self {
            move_sprite: read_opt_u16(&mut input)?,
            run_sprite: read_opt_u16(&mut input)?,
            turn_speed: input.read_f32::<LE>()?,
            size_class: input.read_u8()?,
            trailing_unit: read_opt_u16(&mut input)?,
            trailing_options: input.read_u8()?,
            trailing_spacing: input.read_f32::<LE>()?,
            move_algorithm: input.read_u8()?,
            turn_radius: input.read_f32::<LE>()?,
            turn_radius_speed: input.read_f32::<LE>()?,
            maximum_yaw_per_second_moving: input.read_f32::<LE>()?,
            stationary_yaw_revolution_time: input.read_f32::<LE>()?,
            maximum_yaw_per_second_stationary: input.read_f32::<LE>()?,
        })
    }

    /// Write this unit type to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if a sprite or unit ID does not fit the format's `i16`.
    pub fn write_to(&self, mut output: impl Write, _version: f32) -> Result<()> {
        output.write_i16::<LE>(opt_fits(self.move_sprite, -1)?)?;
        output.write_i16::<LE>(opt_fits(self.run_sprite, -1)?)?;
        output.write_f32::<LE>(self.turn_speed)?;
        output.write_u8(self.size_class)?;
        output.write_i16::<LE>(opt_fits(self.trailing_unit, -1)?)?;
        output.write_u8(self.trailing_options)?;
        output.write_f32::<LE>(self.trailing_spacing)?;
        output.write_u8(self.move_algorithm)?;
        output.write_f32::<LE>(self.turn_radius)?;
        output.write_f32::<LE>(self.turn_radius_speed)?;
        output.write_f32::<LE>(self.maximum_yaw_per_second_moving)?;
        output.write_f32::<LE>(self.stationary_yaw_revolution_time)?;
        output.write_f32::<LE>(self.maximum_yaw_per_second_stationary)?;
        Ok(())
    }
}

/// The attributes of unit types that can be given tasks.
#[derive(Debug, Default, Clone)]
pub struct ActionUnitTypeAttributes {
    /// Index of the task the unit does by default, or `None`.
    pub default_task: Option<u16>,
    /// How far the unit searches for targets.
    pub search_radius: f32,
    /// Base rate of the unit's work (gathering, building, …).
    pub work_rate: f32,
    /// Unit type at which the unit drops off resources, or `None`.
    pub drop_site: Option<UnitTypeId>,
    /// A second drop site, or `None`.
    pub backup_drop_site: Option<UnitTypeId>,
    /// Task swap group (genieutils' `TaskSwapGroup`): units of one group turn into each other with
    /// the task, like the kinds of villager.
    pub task_by_group: u8,
    /// Sound played when the unit is given a command (genieutils' `AttackSound`), or `None`.
    pub command_sound: Option<SoundId>,
    /// Sound played when the unit is ordered to move, or `None`.
    pub move_sound: Option<SoundId>,
    /// Task list for older versions; newer game versions store the task list at the root of the
    /// dat file, and use `unit_type.copy_id` to refer to one of those task lists.
    pub tasks: Option<TaskList>,
    /// genieutils' `RunPattern`; its use is not known yet.
    pub run_pattern: u8,
}

impl ActionUnitTypeAttributes {
    /// Read the action attributes from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if an ID is out of range.
    pub fn read_from(mut input: impl Read, _version: f32) -> Result<Self> {
        Ok(Self {
            default_task: read_opt_u16(&mut input)?,
            search_radius: input.read_f32::<LE>()?,
            work_rate: input.read_f32::<LE>()?,
            drop_site: read_opt_u16(&mut input)?,
            backup_drop_site: read_opt_u16(&mut input)?,
            task_by_group: input.read_u8()?,
            command_sound: read_opt_u16(&mut input)?,
            move_sound: read_opt_u16(&mut input)?,
            run_pattern: input.read_u8()?,
            ..Default::default()
        })
    }

    /// Write this unit type to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if the default task, a unit or a sound ID does not fit the format's `i16`.
    pub fn write_to(&self, mut output: impl Write, _version: f32) -> Result<()> {
        output.write_i16::<LE>(opt_fits(self.default_task, -1)?)?;
        output.write_f32::<LE>(self.search_radius)?;
        output.write_f32::<LE>(self.work_rate)?;
        output.write_i16::<LE>(opt_fits(self.drop_site, -1)?)?;
        output.write_i16::<LE>(opt_fits(self.backup_drop_site, -1)?)?;
        output.write_u8(self.task_by_group)?;
        output.write_i16::<LE>(opt_fits(self.command_sound, -1)?)?;
        output.write_i16::<LE>(opt_fits(self.move_sound, -1)?)?;
        output.write_u8(self.run_pattern)?;
        Ok(())
    }
}
