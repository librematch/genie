//! Player actions executed during a game.
//!
//! This is the reader for the body of older recorded games (`AoC`, `UserPatch`, HD and early DE).
//! Command layouts and names follow genie-rs and aoc-mgz; where the meaning of a stored value is
//! not known, its docs say so.

use crate::fits;
use crate::{ObjectId, PlayerId};
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_error::{GenieError, GenieResult};
use genie_support::{ReadSkipExt, ReadStringsExt};
use std::io::{Read, Write};

mod buildings;
mod game;
mod groups;
mod orders;
mod tasks;

pub use self::buildings::{
    BuildCommand, BuildWallCommand, CancelBuildCommand, MakeCommand, ResearchCommand,
};
pub use self::game::GameCommand;
pub use self::groups::{
    FollowCommand, FormFormationCommand, GroupWaypointCommand, GuardCommand, PatrolCommand,
    ResignCommand, UnitAiStateCommand, UserPatchAiCommand,
};
pub use self::orders::{
    AddResourceCommand, AiOrderCommand, CreateCommand, MoveCommand, OrderCommand, StopCommand,
    WorkCommand,
};
pub use self::tasks::{
    AttackGroundCommand, BackToWorkCommand, BuyResourceCommand, FlareCommand, QueueCommand,
    RepairCommand, SellResourceCommand, SetGatherPointCommand, UngarrisonCommand, UnitOrderCommand,
    Unknown7fCommand,
};
use std::io::{copy, sink};

/// A location with an X and Y coordinate.
pub type Location2 = (f32, f32);
/// A location with an X, Y, and Z coordinate.
///
/// The Z coordinate is usually meaningless.
pub type Location3 = (f32, f32, f32);

/// A viewpoint update, recording where the player is currently looking.
///
/// This is used for the View Lock feature when watching a game.
#[derive(Debug, Default, Clone, Copy)]
pub struct ViewLock {
    /// The X coordinate the player is looking at.
    pub x: f32,
    /// The Y coordinate the player is looking at.
    pub y: f32,
    /// The ID of the POV player.
    pub player: PlayerId,
}

impl ViewLock {
    /// Read a view lock action from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if the stored player ID is negative or above 255.
    pub fn read_from(mut input: impl Read) -> GenieResult<Self> {
        let x = input.read_f32::<LE>()?;
        let y = input.read_f32::<LE>()?;
        let player = fits(input.read_i32::<LE>()?)?;
        Ok(Self { x, y, player })
    }

    /// Write a view lock action to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_f32::<LE>(self.x)?;
        output.write_f32::<LE>(self.y)?;
        output.write_i32::<LE>(i32::from(self.player))?;
        Ok(())
    }
}

/// A list of objects that a command applies to.
///
/// The game uses a special value if a command applies to the same objects as the previous command.
/// That way it does not have to resend 40 object IDs every time a player moves their army. It's
/// encoded as `ObjectsList::SameAsLast` here.
#[derive(Debug, Clone)]
pub enum ObjectsList {
    /// Use the same objects as the previous command.
    SameAsLast,
    /// Use this list of objects.
    List(Vec<ObjectId>),
}

impl Default for ObjectsList {
    fn default() -> Self {
        Self::List(vec![])
    }
}

impl ObjectsList {
    /// Read a list of objects from an input stream.
    ///
    /// A `count` of 0xFF or more means [`ObjectsList::SameAsLast`] and reads nothing.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if an object ID in the list is negative.
    pub fn read_from(mut input: impl Read, count: i32) -> GenieResult<Self> {
        if count < 0xFF {
            let mut list = vec![];
            for _ in 0..count {
                list.push(fits(input.read_i32::<LE>()?)?);
            }
            Ok(Self::List(list))
        } else {
            Ok(Self::SameAsLast)
        }
    }

    /// Write a list of objects to an output stream.
    ///
    /// [`ObjectsList::SameAsLast`] writes nothing.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        if let Self::List(list) = self {
            for entry in list.iter().copied() {
                output.write_u32::<LE>(entry.into())?;
            }
        }
        Ok(())
    }

    /// The amount of objects contained in this list.
    ///
    /// For `ObjectsList::SameAsLast` this returns 0.
    #[must_use]
    pub const fn len(&self) -> usize {
        match self {
            Self::SameAsLast => 0,
            Self::List(list) => list.len(),
        }
    }

    /// Does this list contain 0 objects?
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A player command.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Command {
    /// Command `0x00`, see [`OrderCommand`].
    Order(OrderCommand),
    /// Command `0x01`, see [`StopCommand`].
    Stop(StopCommand),
    /// Command `0x02`, see [`WorkCommand`].
    Work(WorkCommand),
    /// Command `0x03`, see [`MoveCommand`].
    Move(MoveCommand),
    /// Command `0x04`, see [`CreateCommand`].
    Create(CreateCommand),
    /// Command `0x05`, see [`AddResourceCommand`].
    AddResource(AddResourceCommand),
    /// Command `0x0a`, see [`AiOrderCommand`].
    AiOrder(AiOrderCommand),
    /// Command `0x0b`, see [`ResignCommand`].
    Resign(ResignCommand),
    /// Command `0x10`, see [`GroupWaypointCommand`].
    GroupWaypoint(GroupWaypointCommand),
    /// Command `0x12`, see [`UnitAiStateCommand`].
    UnitAiState(UnitAiStateCommand),
    /// Command `0x13`, see [`GuardCommand`].
    Guard(GuardCommand),
    /// Command `0x14`, see [`FollowCommand`].
    Follow(FollowCommand),
    /// Command `0x15`, see [`PatrolCommand`].
    Patrol(PatrolCommand),
    /// Command `0x17`, see [`FormFormationCommand`].
    FormFormation(FormFormationCommand),
    /// Command `0x35`, see [`UserPatchAiCommand`].
    UserPatchAi(UserPatchAiCommand),
    /// Command `0x64`, see [`MakeCommand`].
    Make(MakeCommand),
    /// Command `0x65`, see [`ResearchCommand`].
    Research(ResearchCommand),
    /// Command `0x66`, see [`BuildCommand`].
    Build(BuildCommand),
    /// Command `0x67`, see [`GameCommand`].
    Game(GameCommand),
    /// Command `0x69`, see [`BuildWallCommand`].
    BuildWall(BuildWallCommand),
    /// Command `0x6a`, see [`CancelBuildCommand`].
    CancelBuild(CancelBuildCommand),
    /// Command `0x6b`, see [`AttackGroundCommand`].
    AttackGround(AttackGroundCommand),
    /// Command `0x6e`, see [`RepairCommand`].
    Repair(RepairCommand),
    /// Command `0x6f`, see [`UngarrisonCommand`].
    Ungarrison(UngarrisonCommand),
    /// Command `0x73`, see [`FlareCommand`].
    Flare(FlareCommand),
    /// Command `0x75`, see [`UnitOrderCommand`].
    UnitOrder(UnitOrderCommand),
    /// Command `0x77`, see [`QueueCommand`].
    Queue(QueueCommand),
    /// Command `0x78`, see [`SetGatherPointCommand`].
    SetGatherPoint(SetGatherPointCommand),
    /// Command `0x7a`, see [`SellResourceCommand`].
    SellResource(SellResourceCommand),
    /// Command `0x7b`, see [`BuyResourceCommand`].
    BuyResource(BuyResourceCommand),
    /// Command `0x7f`, see [`Unknown7fCommand`].
    Unknown7F(Unknown7fCommand),
    /// Command `0x80`, see [`BackToWorkCommand`].
    BackToWork(BackToWorkCommand),
    /// A command of an id this reader does not decode: its id and the bytes after it, kept
    /// whole.
    Unknown {
        /// The command's id.
        id: u8,
        /// The bytes after the id.
        bytes: Vec<u8>,
    },
}

impl Command {
    /// Read a command from an input stream: its length, its bytes, then the world time.
    ///
    /// Bytes of the command that its reader does not consume are skipped.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if the command type is not one listed in [`Command`], or if the command's own
    /// reader panics.
    pub fn read_from<R: Read>(input: &mut R) -> GenieResult<Self> {
        let len = input.read_u32::<LE>()?;

        let mut cursor = input.by_ref().take(len.into());
        let id = cursor.read_u8()?;
        let command = if id < 0x64 {
            read_unit_command(id, &mut cursor, len)
        } else {
            read_building_command(id, &mut cursor)
        };
        // Consume any excess bytes; how many there were does not matter.
        let _ = copy(&mut cursor, &mut sink())?;

        let _world_time = input.read_u32::<LE>()?;
        command
    }
}

/// The commands to units and groups (ids below 0x64) after their id; `len` is the command's
/// length, which `UserPatch`'s AI command needs.
///
/// # Errors
///
/// Returns an error if the command's reader fails; an id no command has reads as
/// [`Command::Unknown`].
fn read_unit_command(id: u8, cursor: &mut impl Read, len: u32) -> GenieResult<Command> {
    match id {
        0x00 => OrderCommand::read_from(&mut *cursor).map(Command::Order),
        0x01 => StopCommand::read_from(&mut *cursor).map(Command::Stop),
        0x02 => WorkCommand::read_from(&mut *cursor).map(Command::Work),
        0x03 => MoveCommand::read_from(&mut *cursor).map(Command::Move),
        0x04 => CreateCommand::read_from(&mut *cursor).map(Command::Create),
        0x05 => AddResourceCommand::read_from(&mut *cursor).map(Command::AddResource),
        0x0a => AiOrderCommand::read_from(&mut *cursor).map(Command::AiOrder),
        0x0b => ResignCommand::read_from(&mut *cursor).map(Command::Resign),
        0x10 => GroupWaypointCommand::read_from(&mut *cursor).map(Command::GroupWaypoint),
        0x12 => UnitAiStateCommand::read_from(&mut *cursor).map(Command::UnitAiState),
        0x13 => GuardCommand::read_from(&mut *cursor).map(Command::Guard),
        0x14 => FollowCommand::read_from(&mut *cursor).map(Command::Follow),
        0x15 => PatrolCommand::read_from(&mut *cursor).map(Command::Patrol),
        0x17 => FormFormationCommand::read_from(&mut *cursor).map(Command::FormFormation),
        0x35 => UserPatchAiCommand::read_from(&mut *cursor, len).map(Command::UserPatchAi),
        id => unknown(id, cursor),
    }
}

/// The commands to buildings, the game and the market (ids from 0x64 on) after their id.
///
/// # Errors
///
/// Returns an error if the command's reader fails; an id no command has reads as
/// [`Command::Unknown`].
fn read_building_command(id: u8, cursor: &mut impl Read) -> GenieResult<Command> {
    match id {
        0x64 => MakeCommand::read_from(&mut *cursor).map(Command::Make),
        0x65 => ResearchCommand::read_from(&mut *cursor).map(Command::Research),
        0x66 => BuildCommand::read_from(&mut *cursor).map(Command::Build),
        0x67 => GameCommand::read_from(&mut *cursor).map(Command::Game),
        0x69 => BuildWallCommand::read_from(&mut *cursor).map(Command::BuildWall),
        0x6a => CancelBuildCommand::read_from(&mut *cursor).map(Command::CancelBuild),
        0x6b => AttackGroundCommand::read_from(&mut *cursor).map(Command::AttackGround),
        0x6e => RepairCommand::read_from(&mut *cursor).map(Command::Repair),
        0x6f => UngarrisonCommand::read_from(&mut *cursor).map(Command::Ungarrison),
        0x73 => FlareCommand::read_from(&mut *cursor).map(Command::Flare),
        0x75 => UnitOrderCommand::read_from(&mut *cursor).map(Command::UnitOrder),
        0x77 => QueueCommand::read_from(&mut *cursor).map(Command::Queue),
        0x78 => SetGatherPointCommand::read_from(&mut *cursor).map(Command::SetGatherPoint),
        0x7a => SellResourceCommand::read_from(&mut *cursor).map(Command::SellResource),
        0x7b => BuyResourceCommand::read_from(&mut *cursor).map(Command::BuyResource),
        0x7f => Unknown7fCommand::read_from(&mut *cursor).map(Command::Unknown7F),
        0x80 => BackToWorkCommand::read_from(&mut *cursor).map(Command::BackToWork),
        id => unknown(id, cursor),
    }
}

/// A command of an id no reader decodes ([`Command::Unknown`]): the bytes after its id, whole.
///
/// # Errors
///
/// Returns an error if reading from `cursor` fails.
fn unknown(id: u8, cursor: &mut impl Read) -> GenieResult<Command> {
    let mut bytes = Vec::new();
    let _ = cursor.read_to_end(&mut bytes)?;
    Ok(Command::Unknown { id, bytes })
}

/// A time step in the body: how much game time passed since the previous one.
#[derive(Debug, Default, Clone, Copy)]
// `time` is the public field name callers use.
#[allow(clippy::struct_field_names)]
pub struct Time {
    /// The game time that passed in this step, in milliseconds.
    pub time: u32,
    old_world_time: u32,
    unknown: u32,
}

impl Time {
    /// Read a time step from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from<R: Read>(input: &mut R) -> GenieResult<Self> {
        let mut time = Self {
            time: input.read_u32::<LE>()?,
            ..Default::default()
        };
        let is_old_record = false;
        if is_old_record {
            time.old_world_time = input.read_u32::<LE>()?;
            time.unknown = input.read_u32::<LE>()?;
        }
        Ok(time)
    }
}

/// A Sync message, used to ensure that all players agree on the game state by comparing checksums
/// and world time.
#[derive(Debug, Default, Clone, Copy)]
pub struct Sync {
    /// A checksum of the game state.
    pub checksum: u32,
    /// A checksum of the objects' positions.
    pub position_checksum: u32,
    /// A checksum of the commands. When it is non-zero, 332 bytes of further data follow; they
    /// are skipped.
    pub action_checksum: u32,
    /// The world time of the next sync, as the reader names it; not verified.
    pub next_world_time: u32,
}

impl Sync {
    /// Read a sync message from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from<R: Read>(input: &mut R) -> GenieResult<Self> {
        let mut sync = Self::default();
        let _always_zero = input.read_u32::<LE>()?;
        sync.checksum = input.read_u32::<LE>()?;
        sync.position_checksum = input.read_u32::<LE>()?;
        sync.action_checksum = input.read_u32::<LE>()?;

        if sync.action_checksum != 0 {
            // From happyleaves:
            // https://github.com/happyleavesaoc/aoc-mgz/blob/30079d29a1cb448b58f83fba5f639017fbd5a2b5/mgz/body/__init__.py#L98
            input.skip(332)?;
        }

        let _always_zero = input.read_u32::<LE>()?;
        sync.next_world_time = input.read_u32::<LE>()?;
        Ok(sync)
    }
}

/// Action at the start of the game, contains settings affecting the rec format.
#[derive(Debug, Default, Clone, Copy)]
pub struct Meta {
    /// The version of the action log format.
    /// `3` for `AoC` 1.0, `4` for `AoC` 1.0c and `UserPatch`.
    pub log_version: Option<u32>,
    /// How many time steps pass between two sync messages.
    pub checksum_interval: u32,
    /// Whether the game was a multiplayer game.
    pub is_multiplayer: bool,
    /// Whether every body operation is preceded by a one-byte sequence number.
    pub use_sequence_numbers: bool,
    /// The ID of the player who recorded the game.
    pub local_player_id: PlayerId,
    /// The fourth `u32` of the metadata. genie-rs reads it as a header position, aoc-mgz as the
    /// reveal-map setting; not verified.
    pub header_position: u32,
    /// The amount of saved chapters in this rec / save game. This is only set if the game version
    /// that generated the file supports saved chapters (i.e. The Conquerors and up).
    pub num_chapters: Option<u32>,
}

impl Meta {
    /// Read the chunk of recorded game body metadata that's the same across all versions.
    fn read_from_inner(mut input: impl Read) -> GenieResult<Self> {
        let checksum_interval = input.read_u32::<LE>()?;
        let is_multiplayer = input.read_u32::<LE>()? != 0;
        let local_player_id = fits(input.read_u32::<LE>()?)?;
        let header_position = input.read_u32::<LE>()?;
        let use_sequence_numbers = input.read_u32::<LE>()? != 0;
        Ok(Self {
            checksum_interval,
            is_multiplayer,
            use_sequence_numbers,
            local_player_id,
            header_position,
            ..Default::default()
        })
    }

    /// Read recorded game body metadata in the `mgl` format used by Age of Empires 2: The
    /// Age Of Kings.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if the stored local player ID is above 255.
    pub fn read_from_mgl(mut input: impl Read) -> GenieResult<Self> {
        let meta = Self::read_from_inner(&mut input)?;
        let _exe_file_size = input.read_u64::<LE>()?;
        let _unknown = input.read_f32::<LE>()?;
        let _unknown = input.read_f32::<LE>()?;

        // TODO if `is_multiplayer` flag contains 2 or 3, the `remaining_syncs_until_checksum`
        // value is stored here as u32

        Ok(meta)
    }

    /// Read recorded game body metadata in the `mgx` format used by Age of Empires 2: The
    /// Conquerors and all subsequent versions.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if the log version is not 3, 4 or 5, or if the stored local player ID is above 255.
    pub fn read_from_mgx(mut input: impl Read) -> GenieResult<Self> {
        let log_version = input.read_u32::<LE>()?;
        if !matches!(log_version, 3..=5) {
            return Err(GenieError::invalid_input(
                "a log version other than 3, 4 or 5",
            ));
        }
        let mut meta = Self::read_from_inner(&mut input)?;
        meta.log_version = Some(log_version);
        if log_version == 5 {
            // One of these is likely num_chapters, but not sure which.
            let _unknown = input.read_u32::<LE>()?;
            let _unknown = input.read_u32::<LE>()?;
        } else {
            meta.num_chapters = Some(input.read_u32::<LE>()?);
        }
        Ok(meta)
    }
}

/// A chat message sent during the game.
#[derive(Debug, Clone)]
pub struct Chat {
    #[allow(dead_code)]
    message: String,
}

impl Chat {
    /// Read a chat message from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails or the text is not valid WINDOWS-1252.
    ///
    /// Also fails if the `i32` stored before the text is not -1.
    pub fn read_from<R: Read>(input: &mut R) -> GenieResult<Self> {
        if input.read_i32::<LE>()? != -1 {
            return Err(GenieError::invalid_input(
                "a chat message without its -1 marker",
            ));
        }
        let message = input.read_u32_length_prefixed_str()?.unwrap_or_default();
        Ok(Self { message })
    }
}

/// One operation of the recorded game body.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Action {
    /// A player command (operation 1).
    Command(Command),
    /// A time step (operation 2).
    Time(Time),
    /// A sync message (operation 0).
    Sync(Sync),
    /// A view lock (operation 3).
    ViewLock(ViewLock),
    /// A chat message (operation 4).
    Chat(Chat),
}

#[cfg(test)]
mod tests;
