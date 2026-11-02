//! The tech tree's nodes: ages, buildings, units and techs, read and written.

use crate::conv::invalid_data;
use crate::tech_tree::{TechTreeDependencies, TechTreeStatus, TechTreeType};
use crate::unit_type::UnitTypeId;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::{TechId, read_opt_u32};
use std::convert::{TryFrom, TryInto};
use std::io::{Read, Result, Write};

/// An age node of the tech tree.
#[derive(Debug, Default, Clone)]
pub struct TechTreeAge {
    pub(super) age_id: i32,
    pub(super) status: TechTreeStatus,
    pub(super) node_type: TechTreeType,
    /// The buildings that become available in this age.
    pub dependent_buildings: Vec<UnitTypeId>,
    /// The units that become available in this age.
    pub dependent_units: Vec<UnitTypeId>,
    /// The techs that become available in this age.
    pub dependent_techs: Vec<TechId>,
    pub(super) prerequisites: TechTreeDependencies,
    pub(super) building_levels: u8,
    pub(super) buildings_per_zone: [u8; 10],
    pub(super) group_length_per_zone: [u8; 10],
    pub(super) max_age_length: u8,
}

/// A building node of the tech tree.
#[derive(Debug, Default, Clone)]
pub struct TechTreeBuilding {
    pub(super) building_id: UnitTypeId,
    pub(super) status: TechTreeStatus,
    pub(super) node_type: TechTreeType,
    /// The tech ID that makes this building available. `None` if the building is available without
    /// requiring any techs.
    pub depends_tech_id: Option<TechId>,
    /// The buildings that become available by building this building.
    pub dependent_buildings: Vec<UnitTypeId>,
    /// The units that become available by building this building.
    pub dependent_units: Vec<UnitTypeId>,
    /// The techs that become available by building this building.
    pub dependent_techs: Vec<TechId>,
    pub(super) prerequisites: TechTreeDependencies,
    /// ?
    pub(super) level_no: u8,
    /// Total units and techs at this building by age, including ones that require research to
    /// unlock.
    pub(super) total_children_by_age: [u8; 5],
    /// Initial units and techs at this building by age, excluding ones that require research to
    /// unlock.
    pub(super) initial_children_by_age: [u8; 5],
}

/// A unit node of the tech tree.
#[derive(Debug, Default, Clone)]
pub struct TechTreeUnit {
    pub(super) unit_id: UnitTypeId,
    pub(super) status: TechTreeStatus,
    pub(super) node_type: TechTreeType,
    pub(super) depends_tech_id: Option<TechId>,
    pub(super) building: UnitTypeId,
    pub(super) requires_tech_id: Option<TechId>,
    pub(super) dependent_units: Vec<UnitTypeId>,
    pub(super) prerequisites: TechTreeDependencies,
    pub(super) group_id: i32,
    pub(super) level_no: i32,
}

/// A tech node of the tech tree.
#[derive(Debug, Default, Clone)]
pub struct TechTreeTech {
    pub(super) tech_id: TechId,
    pub(super) status: TechTreeStatus,
    pub(super) node_type: TechTreeType,
    pub(super) building: UnitTypeId,
    pub(super) dependent_buildings: Vec<UnitTypeId>,
    pub(super) dependent_units: Vec<UnitTypeId>,
    pub(super) dependent_techs: Vec<TechId>,
    pub(super) prerequisites: TechTreeDependencies,
    pub(super) group_id: i32,
    pub(super) level_no: i32,
}

/// Read a list of dependent "Thing" IDs for a tech tree node entry. The "Thing"
/// may be unit, building, tech IDs.
pub(super) fn read_dependents<R, T>(input: &mut R) -> Result<Vec<T>>
where
    R: Read,
    T: TryFrom<i32>,
    // so `invalid_data` can convert it
    T::Error: std::error::Error + Send + Sync + 'static,
{
    let num = input.read_u8()?;
    let mut list = vec![];
    for _ in 0..num {
        list.push(input.read_i32::<LE>()?.try_into().map_err(invalid_data)?);
    }
    Ok(list)
}

impl TechTreeAge {
    /// Read an age node from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if a value is not valid for its field.
    ///
    /// Also fails if the node has more than 10 building levels or is not of type
    /// [`TechTreeType::Age`].
    pub fn read_from<R: Read>(input: &mut R) -> Result<Self> {
        let mut age = Self {
            age_id: input.read_i32::<LE>()?,
            status: input.read_u8()?.try_into().map_err(invalid_data)?,
            dependent_buildings: read_dependents(input)?,
            dependent_units: read_dependents(input)?,
            dependent_techs: read_dependents(input)?,
            prerequisites: TechTreeDependencies::read_from(input)?,
            building_levels: input.read_u8()?,
            ..Default::default()
        };
        if 10 < age.building_levels {
            return Err(invalid_data("an age with more than 10 building levels"));
        }
        for building in &mut age.buildings_per_zone {
            *building = input.read_u8()?;
        }
        for group_length in &mut age.group_length_per_zone {
            *group_length = input.read_u8()?;
        }
        age.max_age_length = input.read_u8()?;
        age.node_type = input.read_i32::<LE>()?.try_into().map_err(invalid_data)?;
        if age.node_type != TechTreeType::Age {
            return Err(invalid_data("an age node of another node type"));
        }
        Ok(age)
    }

    /// Write this age node to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    // The format stores each list's length as a u8 (truncated, as before).
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to(&self, mut output: impl Write) -> Result<()> {
        output.write_i32::<LE>(self.age_id)?;
        output.write_u8(self.status.into())?;
        output.write_u8(self.dependent_buildings.len() as u8)?;
        for dependent in &self.dependent_buildings {
            output.write_u32::<LE>((*dependent).into())?;
        }
        output.write_u8(self.dependent_units.len() as u8)?;
        for dependent in &self.dependent_units {
            output.write_u32::<LE>((*dependent).into())?;
        }
        output.write_u8(self.dependent_techs.len() as u8)?;
        for dependent in &self.dependent_techs {
            output.write_u32::<LE>(u32::from(u16::from(*dependent)))?;
        }
        self.prerequisites.write_to(&mut output)?;
        output.write_u8(self.building_levels)?;
        output.write_all(&self.buildings_per_zone)?;
        output.write_all(&self.group_length_per_zone)?;
        output.write_u8(self.max_age_length)?;
        output.write_u32::<LE>(self.node_type.into())?;
        Ok(())
    }
}

impl TechTreeBuilding {
    /// Read a building node from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if a value is not valid for its field.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut building = Self {
            building_id: input.read_i32::<LE>()?.try_into().map_err(invalid_data)?,
            status: input.read_u8()?.try_into().map_err(invalid_data)?,
            dependent_buildings: read_dependents(&mut input)?,
            dependent_units: read_dependents(&mut input)?,
            dependent_techs: read_dependents(&mut input)?,
            prerequisites: TechTreeDependencies::read_from(&mut input)?,
            level_no: input.read_u8()?,
            ..Default::default()
        };
        for children in &mut building.total_children_by_age {
            *children = input.read_u8()?;
        }
        for children in &mut building.initial_children_by_age {
            *children = input.read_u8()?;
        }
        building.node_type = input.read_i32::<LE>()?.try_into().map_err(invalid_data)?;
        building.depends_tech_id = read_opt_u32(&mut input)?;
        Ok(building)
    }

    /// Write this building node to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    // The format stores each list's length as a u8 (truncated, as before).
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to(&self, mut output: impl Write) -> Result<()> {
        output.write_u32::<LE>(self.building_id.into())?;
        output.write_u8(self.status.into())?;
        output.write_u8(self.dependent_buildings.len() as u8)?;
        for dependent in &self.dependent_buildings {
            output.write_u32::<LE>((*dependent).into())?;
        }
        output.write_u8(self.dependent_units.len() as u8)?;
        for dependent in &self.dependent_units {
            output.write_u32::<LE>((*dependent).into())?;
        }
        output.write_u8(self.dependent_techs.len() as u8)?;
        for dependent in &self.dependent_techs {
            output.write_u32::<LE>(u32::from(u16::from(*dependent)))?;
        }
        self.prerequisites.write_to(&mut output)?;
        output.write_u8(self.level_no)?;
        output.write_all(&self.total_children_by_age)?;
        output.write_all(&self.initial_children_by_age)?;
        output.write_u32::<LE>(self.node_type.into())?;
        output.write_u32::<LE>(
            self.depends_tech_id
                .map_or(0xFFFF_FFFF, |tech_id| u32::from(u16::from(tech_id))),
        )?;
        Ok(())
    }
}

impl TechTreeUnit {
    /// Read a unit node from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if a value is not valid for its field.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        Ok(Self {
            unit_id: input.read_i32::<LE>()?.try_into().map_err(invalid_data)?,
            status: input.read_u8()?.try_into().map_err(invalid_data)?,
            building: input.read_i32::<LE>()?.try_into().map_err(invalid_data)?,
            prerequisites: TechTreeDependencies::read_from(&mut input)?,
            group_id: input.read_i32::<LE>()?,
            dependent_units: read_dependents(&mut input)?,
            level_no: input.read_i32::<LE>()?,
            requires_tech_id: read_opt_u32(&mut input)?,
            node_type: input.read_i32::<LE>()?.try_into().map_err(invalid_data)?,
            depends_tech_id: read_opt_u32(&mut input)?,
        })
    }

    /// Write this unit node to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    // The format stores each list's length as a u8 (truncated, as before).
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to(&self, mut output: impl Write) -> Result<()> {
        output.write_u32::<LE>(u16::from(self.unit_id).into())?;
        output.write_u8(self.status.into())?;
        output.write_u32::<LE>(self.building.into())?;
        self.prerequisites.write_to(&mut output)?;
        output.write_i32::<LE>(self.group_id)?;
        output.write_u8(self.dependent_units.len() as u8)?;
        for dependent in &self.dependent_units {
            output.write_u32::<LE>((*dependent).into())?;
        }
        output.write_i32::<LE>(self.level_no)?;
        output.write_u32::<LE>(
            self.requires_tech_id
                .map_or(0xFFFF_FFFF, |tech_id| u32::from(u16::from(tech_id))),
        )?;
        output.write_u32::<LE>(self.node_type.into())?;
        output.write_u32::<LE>(
            self.depends_tech_id
                .map_or(0xFFFF_FFFF, |tech_id| u32::from(u16::from(tech_id))),
        )?;
        Ok(())
    }
}

impl TechTreeTech {
    /// Read a tech node from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if a value is not valid for its field.
    pub fn read_from<R: Read>(input: &mut R) -> Result<Self> {
        Ok(Self {
            tech_id: input.read_i32::<LE>()?.try_into().map_err(invalid_data)?,
            status: input.read_u8()?.try_into().map_err(invalid_data)?,
            building: input.read_i32::<LE>()?.try_into().map_err(invalid_data)?,
            dependent_buildings: read_dependents(input)?,
            dependent_units: read_dependents(input)?,
            dependent_techs: read_dependents(input)?,
            prerequisites: TechTreeDependencies::read_from(input)?,
            group_id: input.read_i32::<LE>()?,
            level_no: input.read_i32::<LE>()?,
            node_type: input.read_i32::<LE>()?.try_into().map_err(invalid_data)?,
        })
    }

    /// Write this tech node to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    // The format stores each list's length as a u8 (truncated, as before).
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to(&self, mut output: impl Write) -> Result<()> {
        output.write_u32::<LE>(u16::from(self.tech_id).into())?;
        output.write_u8(self.status.into())?;
        output.write_u32::<LE>(self.building.into())?;
        output.write_u8(self.dependent_buildings.len() as u8)?;
        for dependent in &self.dependent_buildings {
            output.write_u32::<LE>((*dependent).into())?;
        }
        output.write_u8(self.dependent_units.len() as u8)?;
        for dependent in &self.dependent_units {
            output.write_u32::<LE>((*dependent).into())?;
        }
        output.write_u8(self.dependent_techs.len() as u8)?;
        for dependent in &self.dependent_techs {
            output.write_u32::<LE>(u32::from(u16::from(*dependent)))?;
        }
        self.prerequisites.write_to(&mut output)?;
        output.write_i32::<LE>(self.group_id)?;
        output.write_i32::<LE>(self.level_no)?;
        output.write_u32::<LE>(self.node_type.into())?;
        Ok(())
    }
}
