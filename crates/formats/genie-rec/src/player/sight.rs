//! What a player sees: its visible map and the resource objects it knows of.

use crate::area;
use crate::element::ReadableHeaderElement;
use crate::fits;
use crate::reader::RecordingHeaderReader;
use crate::{ObjectId, PlayerId, Result};
use byteorder::{LE, ReadBytesExt};
use genie_support::{read_i8s, read_list, reserved};
use std::io::Read;

/// A player's view of the map: which tiles it has explored and currently sees.
#[derive(Debug, Default, Clone)]
pub struct VisibleMap {
    /// The map width in tiles.
    pub width: u32,
    /// The map height in tiles.
    pub height: u32,
    /// The number of tiles the player has explored; 0 before save version 6.70.
    pub explored_tiles_count: u32,
    /// The player the map belongs to.
    pub player_id: PlayerId,
    /// The visibility of each tile, row by row.
    pub tiles: Vec<i8>,
}

impl ReadableHeaderElement for VisibleMap {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut map = Self {
            width: input.read_u32::<LE>()?,
            height: input.read_u32::<LE>()?,
            ..Default::default()
        };
        if input.version() >= 6.70 {
            map.explored_tiles_count = input.read_u32::<LE>()?;
        }
        map.player_id = fits(input.read_u16::<LE>()?)?;
        map.tiles = read_i8s(input, area(map.width, map.height)?)?;
        Ok(map)
    }
}

/// A resource object a player knows of.
#[derive(Debug, Default, Clone, Copy)]
pub struct VisibleResource {
    /// The resource object.
    pub object_id: ObjectId,
    /// Its distance, in tiles; the point it is measured from is not known yet.
    pub distance: u8,
    /// The zone it is in.
    pub zone: i8,
    /// Its tile position (x, y).
    pub location: (u8, u8),
}

impl ReadableHeaderElement for VisibleResource {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        Ok(Self {
            object_id: input.read_u32::<LE>()?.into(),
            distance: input.read_u8()?,
            zone: input.read_i8()?,
            location: (input.read_u8()?, input.read_u8()?),
        })
    }
}

/// The resource objects a player knows of, in several lists (probably one per resource kind).
#[allow(dead_code)]
#[derive(Debug, Default, Clone)]
pub struct VisibleResources {
    pub(super) lists: Vec<Vec<VisibleResource>>,
}

impl ReadableHeaderElement for VisibleResources {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let num_lists = input.read_u32::<LE>()?;
        let sizes = read_list(fits(num_lists)?, || {
            let _capacity = input.read_u32::<LE>()?;
            input.read_u32::<LE>()
        })?;
        let mut lists = reserved(sizes.len());
        for size in sizes {
            lists.push(read_list(fits(size)?, || {
                VisibleResource::read_from(input)
            })?);
        }
        Ok(Self { lists })
    }
}
