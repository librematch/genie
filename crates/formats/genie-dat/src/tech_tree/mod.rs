//! Types related to the tech tree data.

use crate::civ::CivilizationId;
use crate::conv::fits;
use crate::unit_kind::UnitKindId;
use arrayvec::ArrayVec;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_error::{GenieResult, invalid_input};
use genie_support::TechId;
use std::io::{Read, Write};

mod nodes;

pub use self::nodes::{TechTreeAge, TechTreeBuilding, TechTreeTech, TechTreeUnit};

/// The status of a tech tree node for a player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum TechTreeStatus {
    /// No particular status (stored as 1).
    #[default]
    None,
    /// This building/unit/technology is available to the player.
    AvailablePlayer,
    /// This building/unit/technology is not available to the player.
    NotAvailablePlayer,
    /// Researching or constructing or creating.
    Researching,
    /// Researched or built.
    ResearchedCompleted,
    /// This building/unit/technology is available to the player if someone on their team is this
    /// civilization.
    AvailableTeam {
        /// The civilization a team member must play (stored as the status minus 10).
        civilization_id: CivilizationId,
    },
    /// A status byte no variant names (0 and 6 to 9), kept to be written back.
    Other(u8),
}

impl From<u8> for TechTreeStatus {
    fn from(byte: u8) -> Self {
        match byte {
            1 => Self::None,
            2 => Self::AvailablePlayer,
            3 => Self::NotAvailablePlayer,
            4 => Self::Researching,
            5 => Self::ResearchedCompleted,
            10..=255 => Self::AvailableTeam {
                civilization_id: (byte - 10).into(),
            },
            other => Self::Other(other),
        }
    }
}

impl From<TechTreeStatus> for u8 {
    fn from(status: TechTreeStatus) -> Self {
        match status {
            TechTreeStatus::None => 1,
            TechTreeStatus::AvailablePlayer => 2,
            TechTreeStatus::NotAvailablePlayer => 3,
            TechTreeStatus::Researching => 4,
            TechTreeStatus::ResearchedCompleted => 5,
            TechTreeStatus::AvailableTeam { civilization_id } => Self::from(civilization_id) + 10,
            TechTreeStatus::Other(other) => other,
        }
    }
}

/// Kinds of tech tree nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::FromPrimitive, num_enum::IntoPrimitive)]
#[repr(i32)]
#[non_exhaustive]
pub enum TechTreeKind {
    /// No kind.
    None = 0,
    /// An age.
    Age = 1,
    /// A unit.
    Unit = 2,
    /// A unit that is an upgrade of another unit.
    UnitUpgrade = 3,
    /// A technology.
    Research = 4,
    /// A building that a technology unlocks.
    BuildingTech = 5,
    /// A building that needs no technology.
    BuildingNonTech = 6,
    /// A civilization's unique unit.
    UniqueUnit = 7,
    /// A kind no variant names, kept to be written back.
    #[num_enum(catch_all)]
    Other(i32),
}

impl Default for TechTreeKind {
    /// No kind.
    fn default() -> Self {
        Self::None
    }
}

/// The tech tree shown in the game: its age, building, unit and tech nodes.
#[derive(Debug, Default, Clone)]
pub struct TechTree {
    /// The age nodes.
    pub ages: Vec<TechTreeAge>,
    /// The building nodes.
    pub buildings: Vec<TechTreeBuilding>,
    /// The unit nodes.
    pub units: Vec<TechTreeUnit>,
    /// The tech nodes.
    pub techs: Vec<TechTreeTech>,
    num_groups: i32,
}

/// A prerequisite of a tech tree node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TechTreeDependency {
    /// A dependency on an age being researched.
    ///
    /// This can typically only be 0-4, for Dark Age through Post-Imperial Age, or up to 5 in mods
    /// that add a new age. However the dat file format has space for 32 bits, and some data files
    /// in the wild contain incorrect data with much higher "age" IDs, so we have to follow suit.
    Age(i32),
    /// A dependency on a building.
    Building(UnitKindId),
    /// A dependency on a unit.
    Unit(UnitKindId),
    /// A dependency on a research/tech.
    Research(TechId),
    /// A dependency of a kind no variant names, its kind and id kept to be written back.
    Other {
        /// The dependency's kind (`+0x28` onwards: not 0 to 3).
        kind: i32,
        /// Its id.
        id: i32,
    },
}

impl TechTreeDependency {
    const fn dependency_kind(self) -> TechTreeDependencyKind {
        match self {
            Self::Age(_) => TechTreeDependencyKind::Age,
            Self::Building(_) => TechTreeDependencyKind::Building,
            Self::Unit(_) => TechTreeDependencyKind::Unit,
            Self::Research(_) => TechTreeDependencyKind::Research,
            Self::Other { kind, .. } => TechTreeDependencyKind::Other(kind),
        }
    }

    fn raw_id(self) -> i32 {
        match self {
            Self::Age(id) | Self::Other { id, .. } => id,
            Self::Building(id) | Self::Unit(id) => id.into(),
            Self::Research(id) => {
                let id: u16 = id.into();
                i32::from(id)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::FromPrimitive, num_enum::IntoPrimitive)]
#[repr(i32)]
enum TechTreeDependencyKind {
    Age = 0,
    Building = 1,
    Unit = 2,
    Research = 3,
    #[num_enum(catch_all)]
    Other(i32),
}

/// The prerequisites of a tech tree node, at most 10.
#[derive(Debug, Default, Clone)]
pub struct TechTreeDependencies(ArrayVec<TechTreeDependency, 10>);

impl TechTree {
    /// Read the tech tree from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if a value is not valid for its field.
    ///
    /// Also fails if an age node has more than 10 building levels or is not of type
    /// [`TechTreeKind::Age`].
    pub fn read_from(mut input: impl Read) -> GenieResult<Self> {
        let num_ages = input.read_u8()?;
        let num_buildings = input.read_u8()?;
        let num_units = input.read_u8()?;
        let num_techs = input.read_u8()?;
        let num_groups = input.read_i32::<LE>()?;

        let mut ages = vec![];
        for _ in 0..num_ages {
            ages.push(TechTreeAge::read_from(&mut input)?);
        }

        let mut buildings = vec![];
        for _ in 0..num_buildings {
            buildings.push(TechTreeBuilding::read_from(&mut input)?);
        }

        let mut units = vec![];
        for _ in 0..num_units {
            units.push(TechTreeUnit::read_from(&mut input)?);
        }

        let mut techs = vec![];
        for _ in 0..num_techs {
            techs.push(TechTreeTech::read_from(&mut input)?);
        }

        Ok(Self {
            ages,
            buildings,
            units,
            techs,
            num_groups,
        })
    }

    /// Write the tech tree to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    // The format stores the node counts as u8 (truncated, as before).
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to(&self, mut output: impl Write) -> GenieResult<()> {
        output.write_u8(self.ages.len() as u8)?;
        output.write_u8(self.buildings.len() as u8)?;
        output.write_u8(self.units.len() as u8)?;
        output.write_u8(self.techs.len() as u8)?;
        output.write_i32::<LE>(self.num_groups)?;

        for age in &self.ages {
            age.write_to(&mut output)?;
        }
        for building in &self.buildings {
            building.write_to(&mut output)?;
        }
        for unit in &self.units {
            unit.write_to(&mut output)?;
        }
        for tech in &self.techs {
            tech.write_to(&mut output)?;
        }

        Ok(())
    }
}

impl TechTreeDependencies {
    /// Read a node's prerequisites from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if a value is not valid for its field.
    pub fn read_from<R: Read>(input: &mut R) -> GenieResult<Self> {
        let mut deps = Self::default();
        let num = input.read_u8()?;
        let _padding = input.read_u8()?;
        let _padding = input.read_u8()?;
        let _padding = input.read_u8()?;

        let mut ids = [-1i32; 10];
        for id in &mut ids {
            *id = input.read_i32::<LE>()?;
        }
        let mut kinds = [-1i32; 10];
        for ty in &mut kinds {
            *ty = input.read_i32::<LE>()?;
        }

        for (&id, &ty) in ids.iter().zip(kinds.iter()).take(num as usize) {
            deps.0.push(match TechTreeDependencyKind::from(ty) {
                TechTreeDependencyKind::Age => TechTreeDependency::Age(id),
                TechTreeDependencyKind::Building => TechTreeDependency::Building(fits(id)?),
                TechTreeDependencyKind::Unit => TechTreeDependency::Unit(fits(id)?),
                TechTreeDependencyKind::Research => TechTreeDependency::Research(fits(id)?),
                TechTreeDependencyKind::Other(kind) => TechTreeDependency::Other { kind, id },
            });
        }

        Ok(deps)
    }

    /// Write the prerequisites to an output stream, padded to 10 slots.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than 10 prerequisites, which the list's capacity rules out.
    // At most 10 (checked), so the count fits the format's u8.
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        if 10 < self.len() {
            return Err(invalid_input("more than 10 prerequisites"));
        }
        output.write_u8(self.len() as u8)?;
        output.write_all(&[0, 0, 0])?;
        for index in 0..10 {
            output.write_i32::<LE>(
                self.0
                    .get(index)
                    .copied()
                    .map_or(0, TechTreeDependency::raw_id),
            )?;
        }
        for index in 0..10 {
            output.write_i32::<LE>(
                self.0
                    .get(index)
                    .copied()
                    .map(TechTreeDependency::dependency_kind)
                    .map_or(0, Into::into),
            )?;
        }
        Ok(())
    }

    /// The number of prerequisites.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no prerequisites.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate over the prerequisites.
    pub fn iter(&self) -> impl Iterator<Item = &TechTreeDependency> {
        self.0.iter()
    }
}
