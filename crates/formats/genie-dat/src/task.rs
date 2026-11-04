use crate::conv::{fits, opt_fits};
use crate::sound::SoundId;
use crate::sprite::SpriteId;
use crate::unit_kind::UnitKindId;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_error::{GenieResult, invalid_input};
use genie_support::read_opt_u16;
use std::io::{Read, Write};
use std::ops::Deref;

/// The list of tasks a unit type can perform, in file order.
#[derive(Debug, Default, Clone)]
pub struct TaskList(Vec<Task>);

/// A task a unit type can perform (gather, build, attack, …): the action, the targets it applies
/// to, and the sprites and sounds the unit uses while doing it.
#[derive(Debug, Default, Clone, Copy)]
pub struct Task {
    id: u16,
    is_default: bool,
    action_kind: u16,
    object_class: i16,
    object_id: Option<UnitKindId>,
    terrain_id: i16,
    attribute_kinds: (i16, i16, i16, i16),
    work_values: (f32, f32),
    work_range: f32,
    auto_search_targets: bool,
    search_wait_time: f32,
    enable_targeting: bool,
    combat_level: u8,
    work_flags: (u16, u16),
    owner_kind: u8,
    holding_attribute: u8,
    state_building: u8,
    move_sprite: Option<SpriteId>,
    work_sprite: Option<SpriteId>,
    work_sprite2: Option<SpriteId>,
    carry_sprite: Option<SpriteId>,
    work_sound: Option<SoundId>,
    work_sound2: Option<SoundId>,
}

impl Deref for TaskList {
    type Target = Vec<Task>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TaskList {
    /// Read a task list from an input stream: a u16 count, then each task preceded by its type.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if a task's type is not 1.
    pub fn read_from(mut input: impl Read) -> GenieResult<Self> {
        let num_tasks = input.read_u16::<LE>()?;
        let mut tasks = vec![];
        for _ in 0..num_tasks {
            let task_kind = input.read_u16::<LE>()?;
            if task_kind != 1 {
                return Err(invalid_input("a task whose type is not 1"));
            }
            tasks.push(Task::read_from(&mut input)?);
        }

        Ok(Self(tasks))
    }

    /// Write this task list to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than `u16::MAX` tasks, or if a task's IDs do not fit the format
    /// (see [`Task::write_to`]).
    pub fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_u16::<LE>(fits(self.len())?)?;
        for task in self.iter() {
            output.write_u16::<LE>(1)?;
            task.write_to(output)?;
        }
        Ok(())
    }
}

impl Task {
    /// Read a task from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if an ID is out of range.
    pub fn read_from(mut input: impl Read) -> GenieResult<Self> {
        Ok(Self {
            id: input.read_u16::<LE>()?,
            is_default: input.read_u8()? != 0,
            action_kind: input.read_u16::<LE>()?,
            object_class: input.read_i16::<LE>()?,
            object_id: read_opt_u16(&mut input)?,
            terrain_id: input.read_i16::<LE>()?,
            attribute_kinds: (
                input.read_i16::<LE>()?,
                input.read_i16::<LE>()?,
                input.read_i16::<LE>()?,
                input.read_i16::<LE>()?,
            ),
            work_values: (input.read_f32::<LE>()?, input.read_f32::<LE>()?),
            work_range: input.read_f32::<LE>()?,
            auto_search_targets: input.read_u8()? != 0,
            search_wait_time: input.read_f32::<LE>()?,
            enable_targeting: input.read_u8()? != 0,
            combat_level: input.read_u8()?,
            work_flags: (input.read_u16::<LE>()?, input.read_u16::<LE>()?),
            owner_kind: input.read_u8()?,
            holding_attribute: input.read_u8()?,
            state_building: input.read_u8()?,
            move_sprite: read_opt_u16(&mut input)?,
            work_sprite: read_opt_u16(&mut input)?,
            work_sprite2: read_opt_u16(&mut input)?,
            carry_sprite: read_opt_u16(&mut input)?,
            work_sound: read_opt_u16(&mut input)?,
            work_sound2: read_opt_u16(&mut input)?,
        })
    }

    /// Write this task to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if the target unit type, a sprite or a sound ID does not fit the format's `i16`.
    pub fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_u16::<LE>(self.id)?;
        output.write_u8(u8::from(self.is_default))?;
        output.write_u16::<LE>(self.action_kind)?;
        output.write_i16::<LE>(self.object_class)?;
        output.write_i16::<LE>(opt_fits(self.object_id, -1)?)?;
        output.write_i16::<LE>(self.terrain_id)?;
        output.write_i16::<LE>(self.attribute_kinds.0)?;
        output.write_i16::<LE>(self.attribute_kinds.1)?;
        output.write_i16::<LE>(self.attribute_kinds.2)?;
        output.write_i16::<LE>(self.attribute_kinds.3)?;
        output.write_f32::<LE>(self.work_values.0)?;
        output.write_f32::<LE>(self.work_values.1)?;
        output.write_f32::<LE>(self.work_range)?;
        output.write_u8(u8::from(self.auto_search_targets))?;
        output.write_f32::<LE>(self.search_wait_time)?;
        output.write_u8(u8::from(self.enable_targeting))?;
        output.write_u8(self.combat_level)?;
        output.write_u16::<LE>(self.work_flags.0)?;
        output.write_u16::<LE>(self.work_flags.1)?;
        output.write_u8(self.owner_kind)?;
        output.write_u8(self.holding_attribute)?;
        output.write_u8(self.state_building)?;
        let ids = [
            opt_fits(self.move_sprite, -1),
            opt_fits(self.work_sprite, -1),
            opt_fits(self.work_sprite2, -1),
            opt_fits(self.carry_sprite, -1),
            opt_fits(self.work_sound, -1),
            opt_fits(self.work_sound2, -1),
        ];
        for id in ids {
            output.write_i16::<LE>(id?)?;
        }
        Ok(())
    }
}
