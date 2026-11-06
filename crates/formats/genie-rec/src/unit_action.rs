use crate::ObjectId;
use crate::element::{ReadHeader, WriteHeader};
use crate::errors::unknown_unit_action;
use crate::header_reader::RecordingHeaderReader;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
pub use genie_dat::sprite::SpriteId;
use genie_error::GenieResult;
pub use genie_support::UnitKindId;
use genie_support::{read_opt_u16, read_opt_u32};
use std::io::{Read, Write};

/// A unit's action (a task it carries out), with the sub-actions it runs.
#[derive(Debug, Clone)]
pub struct UnitAction {
    /// The action's state (its step in the action's state machine). Stored as a `u8` up to save
    /// version 11.76.
    pub state: u32,
    /// The object the action targets, if any.
    pub target_object_id: Option<ObjectId>,
    /// A second target object, if any.
    pub target_object_id_2: Option<ObjectId>,
    /// The target position (x, y, z).
    pub target_position: (f32, f32, f32),
    /// The action's timer.
    pub timer: f32,
    /// Whether and how the target has moved; the values are not known yet.
    pub target_moved_state: u8,
    /// The task (of the unit type's task list) the action carries out, if any.
    pub task_id: Option<u16>,
    /// Not known yet.
    pub sub_action_value: u8,
    /// The sub-actions this action runs.
    pub sub_actions: Vec<Self>,
    /// The sprite the unit shows during the action, if any.
    pub sprite_id: Option<SpriteId>,
    /// The data specific to the action type.
    pub params: ActionKind,
}

impl ReadHeader for UnitAction {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let action_kind = input.read_u16::<LE>()?;
        Self::read_from_inner(input, action_kind)
    }
}

impl UnitAction {
    fn read_from_inner<R: Read>(
        input: &mut RecordingHeaderReader<R>,
        action_kind: u16,
    ) -> GenieResult<Self> {
        // TODO this is different between AoC 1.0 and AoC 1.0c. This version check is a guess
        // and may not actually be when it changed. May have to become more specific in the
        // future!
        let state = if input.version() <= 11.76 {
            u32::from(input.read_u8()?)
        } else {
            input.read_u32::<LE>()?
        };
        let _target_object_pointer = input.read_u32::<LE>()?;
        let _target_object_pointer_2 = input.read_u32::<LE>()?;
        let target_object_id = read_opt_u32(input)?;
        let target_object_id_2 = read_opt_u32(input)?;
        let target_position = (
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
        );
        let timer = input.read_f32::<LE>()?;
        let target_moved_state = input.read_u8()?;
        let task_id = read_opt_u16(input)?;
        let sub_action_value = input.read_u8()?;
        let sub_actions = Self::read_list_from(input)?;
        let sprite_id = read_opt_u16(input)?;
        let params = ActionKind::read_from(input, action_kind)?;

        Ok(Self {
            state,
            target_object_id,
            target_object_id_2,
            target_position,
            timer,
            target_moved_state,
            task_id,
            sub_action_value,
            sub_actions,
            sprite_id,
            params,
        })
    }

    /// Read a list of actions, each one prefixed with its type, up to a type of 0.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails.
    ///
    /// Also fails on an action type the reader does not know yet (see [`ActionKind::read_from`]).
    pub fn read_list_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Vec<Self>> {
        let mut list = vec![];
        loop {
            let action_kind = input.read_u16::<LE>()?;
            if action_kind == 0 {
                return Ok(list);
            }
            let action = Self::read_from_inner(input, action_kind)?;
            list.push(action);
        }
    }
}

/// The action types the reader knows, with the data specific to each.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum ActionKind {
    /// Move to a position or object (type 1).
    MoveTo(ActionMoveTo),
    /// Enter (garrison in) an object (type 3).
    Enter(ActionEnter),
    /// Explore the map (type 4).
    Explore,
    /// Attack (type 9).
    Attack(ActionAttack),
    /// Fly around, as birds do (type 10).
    Bird,
    /// Transport units (type 12).
    Transport,
    /// Guard (type 13).
    Guard,
    /// Make (train or research) something (type 21).
    Make(ActionMake),
    /// An artifact's (relic's) behaviour (type 107).
    Artifact,
}

impl ActionKind {
    /// Read the data of an action of type `action_type`.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails.
    ///
    /// Also fails on an action type the reader does not know yet
    /// (invalid input).
    pub fn read_from<R: Read>(
        input: &mut RecordingHeaderReader<R>,
        action_kind: u16,
    ) -> GenieResult<Self> {
        let data = match action_kind {
            1 => Self::MoveTo(ActionMoveTo::read_from(input)?),
            3 => Self::Enter(ActionEnter::read_from(input)?),
            4 => Self::Explore,
            9 => Self::Attack(ActionAttack::read_from(input)?),
            10 => Self::Bird,
            12 => Self::Transport,
            13 => Self::Guard,
            21 => Self::Make(ActionMake::read_from(input)?),
            107 => Self::Artifact,
            _ => return Err(unknown_unit_action(action_kind)),
        };
        Ok(data)
    }
}

/// The data of a move action.
#[derive(Debug, Default, Clone, Copy)]
pub struct ActionMoveTo {
    /// How close the unit has to get to the target.
    pub range: f32,
}

impl ReadHeader for ActionMoveTo {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let range = input.read_f32::<LE>()?;
        Ok(Self { range })
    }
}

impl WriteHeader for ActionMoveTo {
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_f32::<LE>(self.range)?;
        Ok(())
    }
}

/// The data of an enter action.
#[derive(Debug, Default, Clone, Copy)]
pub struct ActionEnter {
    /// Not known yet; read as a `u32`.
    pub first_time: u32,
}

impl ReadHeader for ActionEnter {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let first_time = input.read_u32::<LE>()?;
        Ok(Self { first_time })
    }
}

impl WriteHeader for ActionEnter {
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_u32::<LE>(self.first_time)?;
        Ok(())
    }
}

/// The data of an attack action.
#[allow(dead_code)]
#[derive(Debug, Default, Clone, Copy)]
pub struct ActionAttack {
    range: f32,
    min_range: f32,
    missile_id: UnitKindId,
    frame_delay: u16,
    need_to_attack: u16,
    was_same_owner: u16,
    indirect_fire_flag: u8,
    move_sprite_id: Option<SpriteId>,
    fight_sprite_id: Option<SpriteId>,
    wait_sprite_id: Option<SpriteId>,
    last_target_position: (f32, f32, f32),
}

impl ReadHeader for ActionAttack {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        Ok(Self {
            range: input.read_f32::<LE>()?,
            min_range: input.read_f32::<LE>()?,
            missile_id: input.read_u16::<LE>()?.into(),
            frame_delay: input.read_u16::<LE>()?,
            need_to_attack: input.read_u16::<LE>()?,
            was_same_owner: input.read_u16::<LE>()?,
            indirect_fire_flag: input.read_u8()?,
            move_sprite_id: read_opt_u16(input)?,
            fight_sprite_id: read_opt_u16(input)?,
            wait_sprite_id: read_opt_u16(input)?,
            last_target_position: (
                input.read_f32::<LE>()?,
                input.read_f32::<LE>()?,
                input.read_f32::<LE>()?,
            ),
        })
    }
}

/// The data of a make action.
#[derive(Debug, Default, Clone, Copy)]
pub struct ActionMake {
    /// The time worked on it so far.
    pub work_timer: f32,
}

impl ReadHeader for ActionMake {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let work_timer = input.read_f32::<LE>()?;
        Ok(Self { work_timer })
    }
}

impl WriteHeader for ActionMake {
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_f32::<LE>(self.work_timer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
