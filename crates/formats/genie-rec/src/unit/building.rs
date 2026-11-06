//! The building base class's attributes: the gather point and the production queue.

use crate::ObjectId;
use crate::element::{ReadHeader, read_xyz};
use crate::fits;
use crate::header_reader::RecordingHeaderReader;
use crate::unit_action::UnitAction;
use arrayvec::ArrayVec;
use byteorder::{LE, ReadBytesExt};
use genie_dat::terrain::TerrainId;
use genie_error::GenieResult;
use genie_support::{UnitKindId, read_opt_u32};
use std::io::Read;

/// Where a building sends the units it trains.
#[derive(Debug, Clone, Copy)]
pub enum GatherPoint {
    /// A position.
    Location {
        /// The x coordinate.
        x: f32,
        /// The y coordinate.
        y: f32,
        /// The z coordinate (height).
        z: f32,
    },
    /// An object, with its unit type.
    Object {
        /// The object.
        id: ObjectId,
        /// The object's unit type.
        unit_kind: UnitKindId,
    },
}

/// An entry of a building's production queue.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProductionQueueEntry {
    /// The unit type to train.
    pub unit_kind_id: UnitKindId,
    /// How many to train.
    pub count: u16,
}

impl ReadHeader for ProductionQueueEntry {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let unit_kind_id = input.read_u16::<LE>()?.into();
        let count = input.read_u16::<LE>()?;
        Ok(Self {
            unit_kind_id,
            count,
        })
    }
}

/// The attributes the building base class adds.
// The bools are the format's on/off flags.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Default, Clone)]
pub struct BuildingUnitAttributes {
    /// Is this building fully built?
    pub built: bool,
    /// Number of build points: how much work villagers have to do to build this building.
    pub build_points: f32,
    /// The build item ID for this building. Only used by AIs.
    pub unique_build_id: Option<u32>,
    /// The culture / architecture graphics set to use for this building.
    pub culture: u8,
    /// Is this building on fire?
    pub burning: u8,
    /// The time the building last burned.
    pub last_burn_time: u32,
    /// The time a unit last garrisoned in the building.
    pub last_garrison_time: u32,
    /// The number of relics currently stored inside this building.
    pub relic_count: u32,
    /// The number of "specific relics" currently stored inside this building(?).
    ///
    /// This specific relic count generates 2× as much gold as normal relics, but appears to be
    /// otherwise unused.
    pub specific_relic_count: u32,
    /// Gather point for units trained from this building.
    pub gather_point: Option<GatherPoint>,
    /// Whether the building has been made passable.
    pub desolid_flag: bool,
    /// Not known yet; stored from save version 10.54 on.
    pub pending_order: u32,
    /// The "owner" building, if this building object is part of a larger building like a Town
    /// Center.
    pub linked_owner: Option<ObjectId>,
    /// The IDs of the children of this building object, also known as "annex buildings".
    pub linked_children: ArrayVec<ObjectId, 4>,
    /// Not known yet; the name suggests a count of captured units.
    pub captured_unit_count: u8,
    /// Further actions; not known yet what for.
    pub extra_actions: Vec<UnitAction>,
    /// The research actions.
    pub research_actions: Vec<UnitAction>,
    /// The current active production queue.
    pub production_queue: Vec<ProductionQueueEntry>,
    /// Cumulative count of queued units.
    pub production_queue_total_units: u16,
    /// Whether the production queue is enabled.
    pub production_queue_enabled: bool,
    /// The actions currently in the production queue.
    pub production_queue_actions: Vec<UnitAction>,
    /// A gate end point (x, y, z). The file has two; the game reads both into one field, so
    /// the second wins.
    pub endpoint: (f32, f32, f32),
    /// Whether the gate is locked.
    pub gate_locked: u32,
    /// Not known yet; stored from save version 10.65 on.
    pub first_update: u32,
    /// The gate's close timer.
    pub close_timer: u32,
    /// A terrain, probably the one under the building; stored from save version 10.67 on.
    pub terrain_kind: Option<TerrainId>,
    /// Whether the building is semi-asleep (updated less often); stored from save version
    /// 11.43 on.
    pub semi_asleep: bool,
    /// The snow flag (`snow_flag`): 0 no snow, 1 a completion on snow (its decal), 2 a farm's or
    /// a pasture's kept snow; stored from save version 11.54 on.
    pub snow_flag: u8,
}

impl ReadHeader for BuildingUnitAttributes {
    // Reads the building's fields in file order, section by section.
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut attrs = Self {
            built: input.read_u8()? != 0,
            build_points: input.read_f32::<LE>()?,
            unique_build_id: read_opt_u32(input)?,
            culture: input.read_u8()?,
            burning: input.read_u8()?,
            last_burn_time: input.read_u32::<LE>()?,
            last_garrison_time: input.read_u32::<LE>()?,
            relic_count: input.read_u32::<LE>()?,
            specific_relic_count: input.read_u32::<LE>()?,
            gather_point: GatherPoint::read_from(input)?,
            desolid_flag: input.read_u8()? != 0,
            ..Default::default()
        };
        attrs.read_links(input)?;
        attrs.read_queues(input)?;
        attrs.read_later_versions(input)?;
        Ok(attrs)
    }
}

impl GatherPoint {
    /// The gather point: a flag, a location, then an object and its type (both -1 for the
    /// location). Without the flag there is none.
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Option<Self>> {
        let exists = input.read_u32::<LE>()? != 0;
        let location = Self::Location {
            x: input.read_f32::<LE>()?,
            y: input.read_f32::<LE>()?,
            z: input.read_f32::<LE>()?,
        };
        let object_id = input.read_i32::<LE>()?;
        let unit_kind_id = input.read_i16::<LE>()?;
        Ok(match (exists, object_id, unit_kind_id) {
            (false, _, _) => None,
            (true, -1, -1) => Some(location),
            (true, id, unit_kind_id) => Some(Self::Object {
                id: fits(id)?,
                unit_kind: fits(unit_kind_id)?,
            }),
        })
    }
}

impl BuildingUnitAttributes {
    /// The pending order (10.54), the linked owner, up to four linked children (-1 is none)
    /// and the count of captured units.
    fn read_links<R: Read>(&mut self, input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
        if input.version() >= 10.54 {
            self.pending_order = input.read_u32::<LE>()?;
        }
        self.linked_owner = read_opt_u32(input)?;
        for _ in 0..4 {
            let id = input.read_i32::<LE>()?;
            if id != -1 {
                self.linked_children.push(fits(id)?);
            }
        }
        self.captured_unit_count = input.read_u8()?;
        Ok(())
    }

    /// The extra and research actions, the production queue (its capacity, the entries and
    /// a size the reader skips), the queue's total and flag and its actions.
    fn read_queues<R: Read>(&mut self, input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
        self.extra_actions = UnitAction::read_list_from(input)?;
        self.research_actions = UnitAction::read_list_from(input)?;
        let capacity = input.read_u16::<LE>()?;
        self.production_queue = (0..capacity)
            .map(|_| ProductionQueueEntry::read_from(input))
            .collect::<GenieResult<_>>()?;
        let _size = input.read_u16::<LE>()?;
        self.production_queue_total_units = input.read_u16::<LE>()?;
        self.production_queue_enabled = input.read_u8()? != 0;
        self.production_queue_actions = UnitAction::read_list_from(input)?;
        Ok(())
    }

    /// The gate's fields (10.65), the terrain type (10.67), the semi-asleep flag (11.43) and
    /// the snow flag (11.54).
    fn read_later_versions<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        if input.version() >= 10.65 {
            // game reads into the same value twice, while there are two separate fields of this
            // type. likely a bug, but it doesn't appear to cause issues? is this unused?
            self.endpoint = read_xyz(input)?;
            self.endpoint = read_xyz(input)?;
            self.gate_locked = input.read_u32::<LE>()?;
            self.first_update = input.read_u32::<LE>()?;
            self.close_timer = input.read_u32::<LE>()?;
        }
        if input.version() >= 10.67 {
            self.terrain_kind = Some(input.read_u8()?.into());
        }
        if input.version() >= 11.43 {
            self.semi_asleep = input.read_u8()? != 0;
        }
        if input.version() >= 11.54 {
            self.snow_flag = input.read_u8()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
