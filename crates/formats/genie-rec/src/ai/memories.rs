//! What the information AI remembers: important objects, building lots, walls, attacks,
//! resources, influence maps and quadrants.

use crate::area;
use crate::element::ReadableHeaderElement;
use crate::fits;
use crate::reader::RecordingHeaderReader;
use crate::{ObjectId, PlayerId, Result};
use byteorder::{LE, ReadBytesExt};
use genie_support::{ReadSkipExt, UnitKindId, read_i8s, read_opt_u16, read_opt_u32};
use std::io::Read;

/// What the information AI remembers about an important object.
#[derive(Debug, Default, Clone, Copy)]
pub struct ImportantObjectMemory {
    /// The object's ID; `None` marks an empty slot.
    pub id: Option<ObjectId>,
    /// The object's unit type.
    pub unit_kind_id: Option<UnitKindId>,
    /// The object's unit class.
    pub unit_class: Option<u16>,
    /// The object's last known tile position (x, y, z).
    pub location: (u8, u8, u8),
    /// The object's owner.
    pub owner: PlayerId,
    /// The object's last known hit points.
    pub hit_points: u16,
    /// How often the AI attacked the object.
    pub attack_attempts: u32,
    /// The number of kills the AI attributes to the object.
    pub kills: u8,
    /// The object's damage capability, as the AI estimates it.
    pub damage_capability: f32,
    /// The object's rate of fire.
    pub rate_of_fire: f32,
    /// The object's attack range.
    pub range: f32,
    /// The game time when the AI last saw the object, if it did.
    pub time_seen: Option<u32>,
    /// Whether the object is garrisoned (non-zero).
    pub is_garrisoned: u32,
}

impl ReadableHeaderElement for ImportantObjectMemory {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        Ok(Self {
            id: read_opt_u32(input)?,
            unit_kind_id: read_opt_u16(input)?,
            unit_class: read_opt_u16(input)?,
            location: (input.read_u8()?, input.read_u8()?, input.read_u8()?),
            owner: input.read_u8()?.into(),
            hit_points: input.read_u16::<LE>()?,
            attack_attempts: input.read_u32::<LE>()?,
            kills: input.read_u8()?,
            damage_capability: input.read_f32::<LE>()?,
            rate_of_fire: input.read_f32::<LE>()?,
            range: input.read_f32::<LE>()?,
            time_seen: read_opt_u32(input)?,
            is_garrisoned: input.read_u32::<LE>()?,
        })
    }
}

/// A building lot of the information AI: a place reserved for a building.
#[derive(Debug, Default, Clone, Copy)]
pub struct BuildingLot {
    /// The building type planned for the lot; `None` marks an empty slot.
    pub unit_kind_id: Option<UnitKindId>,
    /// The lot's status.
    pub status: u8,
    /// The lot's tile position (x, y).
    pub location: (u8, u8),
}

impl ReadableHeaderElement for BuildingLot {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let unit_kind_id = read_opt_u32(input)?;
        let status = input.read_u8()?;
        let x = input.read_u8()?;
        let y = input.read_u8()?;
        input.skip(1)?;
        Ok(Self {
            unit_kind_id,
            status,
            location: (x, y),
        })
    }
}

/// One straight line of a perimeter wall.
#[derive(Debug, Default, Clone, Copy)]
pub struct WallLine {
    /// The kind of line. Stored from version 10.78, 0 before.
    pub line_kind: u32,
    /// The wall unit type, if set.
    pub wall_kind: Option<u32>,
    /// The number of gates in the line. Stored from version 10.78, 0 before.
    pub gate_count: u32,
    /// The number of wall segments in the line.
    pub segment_count: u32,
    /// The number of invisible segments. Stored from version 11.34, 0 before.
    pub invisible_segment_count: u32,
    /// The number of unfinished segments. Stored from version 11.29, 0 before.
    pub unfinished_segment_count: u32,
    /// The tile (x, y) where the line starts.
    pub line_start: (u32, u32),
    /// The tile (x, y) where the line ends.
    pub line_end: (u32, u32),
}

impl ReadableHeaderElement for WallLine {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut line = Self::default();
        if input.version() >= 10.78 {
            line.line_kind = input.read_u32::<LE>()?;
        }
        line.wall_kind = read_opt_u32(input)?;
        if input.version() >= 10.78 {
            line.gate_count = input.read_u32::<LE>()?;
        }
        line.segment_count = input.read_u32::<LE>()?;
        if input.version() >= 11.34 {
            line.invisible_segment_count = input.read_u32::<LE>()?;
        }
        if input.version() >= 11.29 {
            line.unfinished_segment_count = input.read_u32::<LE>()?;
        }
        line.line_start = (input.read_u32::<LE>()?, input.read_u32::<LE>()?);
        line.line_end = (input.read_u32::<LE>()?, input.read_u32::<LE>()?);
        Ok(line)
    }
}

/// A wall the AI plans around its town.
#[derive(Debug, Default, Clone)]
pub struct PerimeterWall {
    /// Whether the AI uses this wall. Stored from version 11.22; true before.
    pub enabled: bool,
    /// The lines the wall is made of.
    pub lines: Vec<WallLine>,
    /// The number of gates in the wall. Stored from version 11.20, 0 before.
    pub gate_count: u32,
    /// The number of lines a gate fits in. The reader does not read it, so it is always 0.
    pub gate_fitting_line_count: u32,
    /// How much of the wall is built, in percent. Stored from version 11.20, 0 before.
    pub percentage_complete: u32,
    /// The number of wall segments. Stored from version 11.20, 0 before.
    pub segment_count: u32,
    /// The number of invisible segments. Stored from version 11.34, 0 before.
    pub invisible_segment_count: u32,
    /// The number of unfinished segments. Stored from version 11.20, 0 before.
    pub unfinished_segment_count: u32,
    /// The index of the line the AI updates next. Stored from version 11.29, 0 before.
    pub next_line_to_refresh: u32,
    /// The index of the segment the AI updates next. Stored from version 11.29, 0 before.
    pub next_segment_to_refresh: u32,
}

impl ReadableHeaderElement for PerimeterWall {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut wall = Self {
            enabled: if input.version() >= 11.22 {
                input.read_u32::<LE>()? != 0
            } else {
                true
            },
            ..Default::default()
        };
        let num_lines = input.read_u32::<LE>()?;
        if input.version() >= 11.20 {
            wall.gate_count = input.read_u32::<LE>()?;
            wall.percentage_complete = input.read_u32::<LE>()?;
            wall.segment_count = input.read_u32::<LE>()?;
            if input.version() >= 11.34 {
                wall.invisible_segment_count = input.read_u32::<LE>()?;
            }
            wall.unfinished_segment_count = input.read_u32::<LE>()?;
        }
        if input.version() >= 11.29 {
            wall.next_line_to_refresh = input.read_u32::<LE>()?;
            wall.next_segment_to_refresh = input.read_u32::<LE>()?;
        }

        wall.lines = {
            let mut list = vec![];
            for _ in 0..num_lines {
                list.push(WallLine::read_from(input)?);
            }
            list
        };

        Ok(wall)
    }
}

/// What the information AI remembers about an attack.
#[derive(Debug, Default, Clone, Copy)]
pub struct AttackMemory {
    /// The attack's ID, if set.
    pub id: Option<i32>,
    /// The kind of attack.
    pub typ: u8,
    /// The smallest X tile of the attacked area.
    pub min_x: u8,
    /// The smallest Y tile of the attacked area.
    pub min_y: u8,
    /// The largest X tile of the attacked area.
    pub max_x: u8,
    /// The largest Y tile of the attacked area.
    pub max_y: u8,
    /// The attacking player, if known.
    pub attacking_owner: Option<u8>,
    /// The attacked player, if known.
    pub target_owner: Option<u8>,
    /// The number of kills in the attack.
    pub kills: u16,
    /// Whether the attack succeeded.
    pub success: bool,
    /// The game time of the attack, if set.
    pub timestamp: Option<u32>,
    /// The AI play (attack plan) used for the attack, if any.
    pub play: Option<i32>,
}

impl ReadableHeaderElement for AttackMemory {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut mem = Self {
            id: read_opt_u32(input)?,
            typ: input.read_u8()?,
            min_x: input.read_u8()?,
            min_y: input.read_u8()?,
            max_x: input.read_u8()?,
            max_y: input.read_u8()?,
            attacking_owner: match input.read_i8()? {
                -1 => None,
                id => Some(fits(id)?),
            },
            target_owner: match input.read_i8()? {
                -1 => None,
                id => Some(fits(id)?),
            },
            ..Default::default()
        };
        input.skip(1)?;
        mem.kills = input.read_u16::<LE>()?;
        mem.success = input.read_u8()? != 0;
        input.skip(1)?;
        mem.timestamp = read_opt_u32(input)?;
        mem.play = read_opt_u32(input)?;
        Ok(mem)
    }
}

/// What the information AI remembers about a resource object.
#[derive(Debug, Default, Clone, Copy)]
pub struct ResourceMemory {
    /// The resource object's ID.
    pub id: ObjectId,
    /// The resource's tile position (x, y).
    pub location: (u8, u8),
    /// How often the AI tried to gather from it.
    pub gather_attempts: u8,
    /// The amount gathered from it, as the reader names it.
    pub gather: u32,
    /// Whether the entry is valid.
    pub valid: bool,
    /// Whether the resource is gone.
    pub gone: bool,
    /// The distance to the drop site.
    pub drop_distance: u8,
    /// The resource type.
    pub resource_kind: u8,
    /// The ID of the drop site used for the resource.
    pub dropsite_id: ObjectId,
    /// The game time when gatherers at the resource were last attacked, if they were. Stored
    /// from version 10.91.
    pub attacked_time: Option<u32>,
}

impl ReadableHeaderElement for ResourceMemory {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut mem = Self {
            id: input.read_u32::<LE>()?.into(),
            location: (input.read_u8()?, input.read_u8()?),
            gather_attempts: input.read_u8()?,
            gather: input.read_u32::<LE>()?,
            valid: input.read_u8()? != 0,
            gone: input.read_u8()? != 0,
            drop_distance: input.read_u8()?,
            resource_kind: input.read_u8()?,
            dropsite_id: input.read_u32::<LE>()?.into(),
            ..Default::default()
        };

        if input.version() >= 10.91 {
            mem.attacked_time = read_opt_u32(input)?;
        }
        Ok(mem)
    }
}

/// An influence map: one value per tile, used by the AI for decisions such as pathing.
#[derive(Debug, Default, Clone)]
pub struct InfluenceMap {
    /// The map's width in tiles.
    pub width: u32,
    /// The map's height in tiles.
    pub height: u32,
    /// The map's reference point (x, y).
    pub reference_point: (u32, u32),
    /// The map's unchangeable limit, as the reader names it; its effect is not known yet.
    pub unchangeable_limit: u8,
    /// The values, `width * height` of them, row by row.
    pub values: Vec<i8>,
}

impl ReadableHeaderElement for InfluenceMap {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut map = Self::default();
        map.width = input.read_u32::<LE>()?;
        map.height = input.read_u32::<LE>()?;
        map.reference_point = (input.read_u32::<LE>()?, input.read_u32::<LE>()?);
        map.unchangeable_limit = input.read_u8()?;
        map.values = read_i8s(input, area(map.width, map.height)?)?;
        Ok(map)
    }
}

/// The information AI's statistics for one quadrant of the map.
#[derive(Debug, Default, Clone, Copy)]
pub struct QuadrantLog {
    /// The number of explored tiles in the quadrant.
    pub explored_tiles: u32,
    /// The number of attacks on the AI in the quadrant.
    pub attacks_on_us: u32,
    /// The number of attacks by the AI in the quadrant.
    pub attacks_by_us: u32,
}

impl ReadableHeaderElement for QuadrantLog {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let explored_tiles = input.read_u32::<LE>()?;
        let attacks_on_us = input.read_u32::<LE>()?;
        let attacks_by_us = input.read_u32::<LE>()?;
        Ok(Self {
            explored_tiles,
            attacks_on_us,
            attacks_by_us,
        })
    }
}
