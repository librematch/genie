//! The moving and action base classes' attributes: paths, movement and the action stack.

use crate::element::{ReadHeader, WriteHeader, read_counted, read_xyz};
use crate::header_reader::RecordingHeaderReader;
use crate::unit_action::UnitAction;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_error::GenieResult;
use genie_support::read_opt_u32;
use std::io::Result as IoResult;
use std::io::{Read, Write};

/// A path a moving object follows.
#[derive(Debug, Default, Clone, Copy)]
pub struct PathData {
    /// The path's id.
    pub id: u32,
    /// Not known yet; the type of a linked path.
    pub linked_path_kind: u32,
    /// Not known yet.
    pub waypoint_level: u32,
    /// Not known yet.
    pub path_id: u32,
    /// The waypoint the object is at.
    pub waypoint: u32,
    /// Disable flags; stored before save version 10.25 only.
    pub disable_flags: Option<u32>,
    /// Enable flags; stored in save versions 10.20 to 10.24 only.
    pub enable_flags: Option<u32>,
    /// The path's state.
    pub state: u32,
    /// How close the object has to get to the target.
    pub range: f32,
    /// The target object.
    pub target_id: u32,
    /// How long the object pauses.
    pub pause_time: f32,
    /// Not known yet.
    pub continue_counter: u32,
    /// Path flags; not known yet in detail.
    pub flags: u32,
}

impl ReadHeader for PathData {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut path = Self {
            id: input.read_u32::<LE>()?,
            linked_path_kind: input.read_u32::<LE>()?,
            waypoint_level: input.read_u32::<LE>()?,
            path_id: input.read_u32::<LE>()?,
            waypoint: input.read_u32::<LE>()?,
            ..Default::default()
        };
        if input.version() < 10.25 {
            path.disable_flags = Some(input.read_u32::<LE>()?);
            if input.version() >= 10.20 {
                path.enable_flags = Some(input.read_u32::<LE>()?);
            }
        }
        path.state = input.read_u32::<LE>()?;
        path.range = input.read_f32::<LE>()?;
        path.target_id = input.read_u32::<LE>()?;
        path.pause_time = input.read_f32::<LE>()?;
        path.continue_counter = input.read_u32::<LE>()?;
        path.flags = input.read_u32::<LE>()?;
        Ok(path)
    }
}

/// A moving object's velocity and acceleration.
#[derive(Debug, Default, Clone, Copy)]
pub struct MovementData {
    /// The velocity (x, y, z).
    pub velocity: (f32, f32, f32),
    /// The acceleration (x, y, z).
    pub acceleration: (f32, f32, f32),
}

impl MovementData {
    pub(super) fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let velocity = (
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
        );
        let acceleration = (
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
        );
        Ok(Self {
            velocity,
            acceleration,
        })
    }

    /// Write the velocity and acceleration.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails.
    pub fn write_to(&self, mut output: impl Write) -> GenieResult<()> {
        output.write_f32::<LE>(self.velocity.0)?;
        output.write_f32::<LE>(self.velocity.1)?;
        output.write_f32::<LE>(self.velocity.2)?;
        output.write_f32::<LE>(self.acceleration.0)?;
        output.write_f32::<LE>(self.acceleration.1)?;
        output.write_f32::<LE>(self.acceleration.2)?;
        Ok(())
    }
}

/// The attributes the moving base class adds: movement and paths.
#[derive(Debug, Default, Clone)]
pub struct MovingUnitAttributes {
    /// Not known yet.
    pub trail_remainder: u32,
    /// The velocity (x, y, z).
    pub velocity: (f32, f32, f32),
    /// The angle the object faces.
    pub angle: f32,
    /// Not known yet; a timing of its turn towards a target.
    pub turn_towards_time: u32,
    /// The turning timer.
    pub turn_timer: u32,
    /// Not known yet.
    pub continue_counter: u32,
    /// Two terrain exceptions; not known yet.
    pub current_terrain_exception: (Option<u32>, Option<u32>),
    /// Whether the object waits to move.
    pub waiting_to_move: u8,
    /// How often the object has waited for its move.
    pub wait_delays_count: u8,
    /// Whether the object is on the ground.
    pub on_ground: u8,
    /// The paths the object follows.
    pub path_data: Vec<PathData>,
    /// The next path, if one is planned.
    pub future_path_data: Option<PathData>,
    /// The velocity and acceleration, if stored.
    pub movement_data: Option<MovementData>,
    /// A position (x, y, z); not known yet how it differs from the static attributes' one.
    pub position: (f32, f32, f32),
    /// The forward direction vector.
    pub orientation_forward: (f32, f32, f32),
    /// The right direction vector.
    pub orientation_right: (f32, f32, f32),
    /// The time the object last moved.
    pub last_move_time: u32,
    /// The waypoints the player set.
    pub user_defined_waypoints: Vec<(f32, f32, f32)>,
    /// A substitute position, if set; not known yet what for.
    pub substitute_position: Option<(f32, f32, f32)>,
    /// How many substitute positions in a row the object has used.
    pub consecutive_substitute_count: u32,
}

impl ReadHeader for MovingUnitAttributes {
    // Reads the moving unit's fields in file order, section by section.
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut attrs = Self::read_motion(input)?;
        attrs.read_future_path_and_movement(input)?;
        attrs.read_placement(input)?;
        attrs.read_waypoints_and_substitute(input)?;
        Ok(attrs)
    }
}

impl MovingUnitAttributes {
    /// The trail, the velocity and turning, the terrain exception, the waiting flags and the
    /// paths: the first fields.
    fn read_motion<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        Ok(Self {
            trail_remainder: input.read_u32::<LE>()?,
            velocity: read_xyz(input)?,
            angle: input.read_f32::<LE>()?,
            turn_towards_time: input.read_u32::<LE>()?,
            turn_timer: input.read_u32::<LE>()?,
            continue_counter: input.read_u32::<LE>()?,
            current_terrain_exception: (read_opt_u32(input)?, read_opt_u32(input)?),
            waiting_to_move: input.read_u8()?,
            wait_delays_count: input.read_u8()?,
            on_ground: input.read_u8()?,
            path_data: read_counted(input)?,
            ..Default::default()
        })
    }

    /// The future path and the movement data, each behind a flag.
    fn read_future_path_and_movement<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        if input.read_u32::<LE>()? != 0 {
            self.future_path_data = Some(PathData::read_from(input)?);
        }
        if input.read_u32::<LE>()? != 0 {
            self.movement_data = Some(MovementData::read_from(input)?);
        }
        Ok(())
    }

    /// The position, the forward and right orientation and the last move's time.
    fn read_placement<R: Read>(&mut self, input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
        self.position = read_xyz(input)?;
        self.orientation_forward = read_xyz(input)?;
        self.orientation_right = read_xyz(input)?;
        self.last_move_time = input.read_u32::<LE>()?;
        Ok(())
    }

    /// The user's waypoints (a negative count reads none), the substitute position (always
    /// stored, kept only behind its flag) and the count of substitutes in a row.
    fn read_waypoints_and_substitute<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        let num_waypoints = input.read_i32::<LE>()?.max(0);
        self.user_defined_waypoints = (0..num_waypoints)
            .map(|_| read_xyz(input))
            .collect::<IoResult<_>>()?;
        let exists = input.read_u32::<LE>()? != 0;
        let position = read_xyz(input)?;
        self.substitute_position = exists.then_some(position);
        self.consecutive_substitute_count = input.read_u32::<LE>()?;
        Ok(())
    }
}

impl WriteHeader for MovingUnitAttributes {}

/// The attributes the action base class adds: the action stack.
#[derive(Debug, Default, Clone)]
pub struct ActionUnitAttributes {
    /// Whether the unit is waiting.
    pub waiting: bool,
    /// Not known yet; stored from save version 6.5 on.
    pub command_flag: u8,
    /// Control group information; stored from save version 11.58 on.
    pub selected_group_info: u16,
    /// The unit's action stack.
    pub actions: Vec<UnitAction>,
}

impl ReadHeader for ActionUnitAttributes {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut attrs = Self {
            waiting: input.read_u8()? != 0,
            ..Default::default()
        };

        if input.version() >= 6.5 {
            attrs.command_flag = input.read_u8()?;
        }
        if input.version() >= 11.58 {
            attrs.selected_group_info = input.read_u16::<LE>()?;
        }
        attrs.actions = UnitAction::read_list_from(input)?;
        Ok(attrs)
    }
}

impl WriteHeader for ActionUnitAttributes {}

#[cfg(test)]
mod tests;
