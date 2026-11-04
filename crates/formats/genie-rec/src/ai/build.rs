//! The build AI module (its build list) and the construction AI module (its lots).

use crate::element::ReadableHeaderElement;
use crate::reader::RecordingHeaderReader;
use byteorder::{LE, ReadBytesExt};
use genie_error::GenieResult;
use genie_support::ReadStringsExt;
use std::io::Read;

/// One entry of the build AI's build list: something the AI wants to build or train.
#[derive(Debug, Default, Clone)]
pub struct BuildItem {
    /// The item's name.
    pub name: Option<String>,
    /// The unit type to build.
    pub kind_id: u32,
    /// The ID of the item's object in the game, as the reader names it; not verified.
    pub game_id: u32,
    /// The item's size along x, y and z.
    pub size: (f32, f32, f32),
    /// The item's skip counter.
    pub skip: u32,
    /// The item's build category.
    pub build_category: u32,
    /// Whether the item is being built, as the reader names it.
    pub in_progress: u32,
    /// Whether the item has been built, as the reader names it.
    pub built: u32,
    /// How often the AI tried to build the item.
    pub build_attempts: u32,
    /// Where the item is built from, as the reader names it; not verified.
    pub build_from: u32,
    /// The terrain set the item may be placed on.
    pub terrain_set: u32,
    /// The item's two terrain adjacency values.
    pub terrain_adjacency: (u32, u32),
    /// Whether the item is placed on elevation, as the reader names it.
    pub place_on_elevation: u32,
    /// Whether the item uses `UserPatch`'s forward placement. Stored after version 10.87, false
    /// before.
    pub is_forward: bool,
}

impl ReadableHeaderElement for BuildItem {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut item = Self {
            name: input.read_u32_length_prefixed_str()?,
            kind_id: input.read_u32::<LE>()?,
            ..Default::default()
        };

        let _a2 = input.read_u32::<LE>()?;
        item.game_id = input.read_u32::<LE>()?;
        let _v21 = input.read_u32::<LE>()?;
        let _v23 = input.read_u32::<LE>()?;
        let _v25 = input.read_u32::<LE>()?;
        item.size = (
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
        );
        item.skip = input.read_u32::<LE>()?;
        item.build_category = input.read_u32::<LE>()?;
        item.in_progress = input.read_u32::<LE>()?;
        item.built = input.read_u32::<LE>()?;
        item.build_attempts = input.read_u32::<LE>()?;
        item.build_from = input.read_u32::<LE>()?;
        item.terrain_set = input.read_u32::<LE>()?;
        item.terrain_adjacency = (input.read_u32::<LE>()?, input.read_u32::<LE>()?);
        item.place_on_elevation = input.read_u32::<LE>()?;
        let _v27 = input.read_u32::<LE>()?;
        let _v12 = input.read_u32::<LE>()?;
        let _v29 = input.read_u32::<LE>()?;
        let _v31 = input.read_u8()?;
        item.is_forward = if input.version() > 10.87 {
            input.read_u32::<LE>()? != 0
        } else {
            false
        };
        Ok(item)
    }
}

/// The build AI module: the build list and what the AI has queued.
#[derive(Debug, Default, Clone)]
pub struct BuildAi {
    /// The name of the build list.
    pub build_list_name: Option<String>,
    /// The name of the build item requested last.
    pub last_build_item_requested: Option<String>,
    /// The name of the build item requested now.
    pub current_build_item_requested: Option<String>,
    /// The name of the build item to request next.
    pub next_build_item_requested: Option<String>,
    /// The items of the build list.
    pub build_queue: Vec<BuildItem>,
    /// Queued counts per object type (600 entries).
    pub queued_object_count: Vec<u32>,
    /// The number of buildings queued.
    pub queued_building_count: u32,
    /// The number of units queued.
    pub queued_unit_count: u32,
}

impl ReadableHeaderElement for BuildAi {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut ai = Self::default();
        let build_list_len = input.read_u32::<LE>()?;
        ai.build_list_name = input.read_u32_length_prefixed_str()?;
        ai.last_build_item_requested = input.read_u32_length_prefixed_str()?;
        ai.current_build_item_requested = input.read_u32_length_prefixed_str()?;
        ai.next_build_item_requested = input.read_u32_length_prefixed_str()?;
        let _items_into_build_queue = if input.version() > 11.02 {
            input.read_u32::<LE>()?
        } else {
            build_list_len
        };

        let num_build_items = input.read_u32::<LE>()?;
        for _ in 0..num_build_items {
            ai.build_queue.push(BuildItem::read_from(input)?);
        }

        for _ in 0..600 {
            ai.queued_object_count.push(input.read_u32::<LE>()?);
        }
        ai.queued_building_count = input.read_u32::<LE>()?;
        ai.queued_unit_count = input.read_u32::<LE>()?;
        Ok(ai)
    }
}

/// A construction lot of the construction AI: a place where the AI plans a building.
#[derive(Debug, Default, Clone)]
pub struct ConstructionItem {
    /// The lot's name.
    pub name: Option<String>,
    /// The unit type planned for the lot.
    pub kind_id: u32,
    /// The lot's X position.
    pub x: f32,
    /// The lot's Y position.
    pub y: f32,
    /// The lot's Z position.
    pub z: f32,
    /// The lot's size along X.
    pub xs: f32,
    /// The lot's size along Y.
    pub ys: f32,
    /// The lot's size along Z.
    pub zs: f32,
    /// The lot's skip counter.
    pub skip: u32,
    /// Whether the building on the lot is in progress, as the reader names it.
    pub in_progress: u32,
    /// Whether the building on the lot is built, as the reader names it.
    pub built: u32,
}

impl ReadableHeaderElement for ConstructionItem {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut item = Self {
            name: input.read_u32_length_prefixed_str()?,
            kind_id: input.read_u32::<LE>()?,
            ..Default::default()
        };

        let _a2 = input.read_u32::<LE>()?;
        let _v27 = input.read_u32::<LE>()?;
        item.x = input.read_f32::<LE>()?;
        item.y = input.read_f32::<LE>()?;
        item.z = input.read_f32::<LE>()?;
        item.xs = input.read_f32::<LE>()?;
        item.ys = input.read_f32::<LE>()?;
        item.zs = input.read_f32::<LE>()?;
        item.skip = input.read_u32::<LE>()?;
        item.in_progress = input.read_u32::<LE>()?;
        item.built = input.read_u32::<LE>()?;
        let _v23 = input.read_u32::<LE>()?;
        Ok(item)
    }
}

/// The construction AI module: the construction plan and its lots.
#[derive(Debug, Default, Clone)]
pub struct ConstructionAi {
    /// The name of the construction plan.
    pub plan_name: Option<String>,
    /// The plan's reference point (x, y, z).
    pub reference_point: (f32, f32, f32),
    /// The map size in tiles (x, y).
    pub map_size: (u32, u32),
    /// The plan's construction lots.
    pub construction_lots: Vec<ConstructionItem>,
    /// The lots placed at random positions.
    pub random_construction_lots: Vec<ConstructionItem>,
}

impl ReadableHeaderElement for ConstructionAi {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut ai = Self::default();
        let num_lots = input.read_u32::<LE>()?;
        ai.plan_name = input.read_u32_length_prefixed_str()?;
        ai.reference_point = (
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
        );
        ai.map_size = (input.read_u32::<LE>()?, input.read_u32::<LE>()?);
        for _ in 0..num_lots {
            ai.construction_lots
                .push(ConstructionItem::read_from(input)?);
        }
        let num_lots = input.read_u32::<LE>()?;
        for _ in 0..num_lots {
            ai.random_construction_lots
                .push(ConstructionItem::read_from(input)?);
        }
        Ok(ai)
    }
}
