//! The tech effects, the techs and the tech tree screen.

use crate::de::{DebugString, Version};
use binrw::binrw;

/// A tech effect (genieutils' `Effect`, the "techage").
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    /// Internal name.
    pub name: DebugString,
    /// Number of commands.
    pub command_count: i16,
    /// The commands, applied in order.
    #[br(count = command_count)]
    pub commands: Vec<EffectCommand>,
}

/// One command of a tech effect: its type and its parameters.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectCommand {
    /// Command type (set, add or multiply an attribute or resource, enable or upgrade a unit, …).
    pub kind: u8,
    /// First parameter; its meaning depends on `kind` (for an attribute command, the unit).
    pub first: i16,
    /// Second parameter; its meaning depends on `kind` (for an attribute command, the class).
    pub second: i16,
    /// Third parameter; its meaning depends on `kind` (for an attribute command, the attribute).
    pub third: i16,
    /// The float parameter; its meaning depends on `kind` (usually the amount).
    pub amount: f32,
}

/// One resource cost of a tech.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ResearchResourceCost {
    /// Resource id; -1 for none.
    pub kind: i16,
    /// Amount.
    pub amount: i16,
    /// Whether the amount is paid (non-zero) or only listed.
    pub flag: u8,
}

/// A unit or building that researches a tech, with the research time and button (VER 8.8 and
/// later).
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResearchLocation {
    /// Unit or building that researches the tech.
    pub location_id: i16,
    /// Research time.
    pub queue_time: i16,
    /// Position of the button on the command panel.
    pub button_id: u8,
    /// Hotkey id.
    pub hot_key_id: i32,
}

/// A tech (genieutils' `Tech`, `Research.cpp`).
#[binrw]
#[brw(little)]
#[br(import(version: Version))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tech {
    /// Techs the tech requires; -1 for none.
    pub required_techs: [i16; 6],
    /// Up to three costs.
    pub resource_costs: [ResearchResourceCost; 3],
    /// How many of `required_techs` must be researched.
    pub required_tech_count: i16,
    /// Civilization the tech is for; -1 for every civilization.
    pub civ: i16,
    /// Whether the tech is available with the full tech tree setting.
    pub full_tech_mode: i16,
    /// Before VER 8.8 (`GV_C31`) one research location's fields are spread over the tech.
    #[br(if(!version.at_least(31)))]
    pub location_id: Option<i16>,
    /// Language string id of the name; from VER 7.5 (`GV_C18`).
    #[br(if(version.at_least(18)))]
    pub language_dll_name: Option<i32>,
    /// Language string id of the description; from VER 7.5 (`GV_C18`).
    #[br(if(version.at_least(18)))]
    pub language_dll_description: Option<i32>,
    /// Language string id of the name as an i16; before VER 7.5 (`GV_C18`).
    #[br(if(!version.at_least(18)))]
    pub language_dll_name_u16: Option<i16>,
    /// Language string id of the description as an i16; before VER 7.5 (`GV_C18`).
    #[br(if(!version.at_least(18)))]
    pub language_dll_description_u16: Option<i16>,
    /// Research time; before VER 8.8 (`GV_C31`).
    #[br(if(!version.at_least(31)))]
    pub queue_time: Option<i16>,
    /// Effect applied when the tech is researched; -1 for none.
    pub effect_id: i16,
    /// Tech type: 0 normal, 2 shown on the age progress bar.
    pub kind: i16,
    /// Frame of the tech's icon.
    pub icon_id: i16,
    /// Position of the button on the command panel; before VER 8.8 (`GV_C31`).
    #[br(if(!version.at_least(31)))]
    pub button_id: Option<u8>,
    /// Language string id of the help text.
    pub language_dll_help: i32,
    /// Language string id of the tech tree text.
    pub language_dll_tech_tree: i32,
    /// Hotkey id; before VER 8.8 (`GV_C31`).
    #[br(if(!version.at_least(31)))]
    pub hot_key_id: Option<i32>,
    /// Internal name.
    pub name: DebugString,
    /// Whether the tech can be researched more than once; from VER 7.2 (`GV_C15`).
    #[br(if(version.at_least(15)))]
    pub repeatable: Option<u8>,
    /// Number of research locations; from VER 8.8 (`GV_C31`).
    #[br(if(version.at_least(31)))]
    pub research_location_count: Option<i16>,
    /// Where the tech is researched.
    #[br(count = research_location_count.unwrap_or(0))]
    pub research_locations: Vec<ResearchLocation>,
}

/// genieutils' `TechTreeAge::getZoneCount` and `Common::getSlots` for `AoK` and later.
pub(super) const ZONES: usize = 10;
/// genieutils' `BuildingConnection::AGES`.
pub(super) const AGES: usize = 5;

/// The tech tree screen: its ages, buildings, units and researches.
#[binrw]
#[brw(little)]
#[br(import(version: Version))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TechTree {
    /// Number of ages.
    pub age_count: u8,
    /// Number of buildings.
    pub building_count: u8,
    /// From VER 8.9 (`GV_C32`) the unit connections are counted in an i16.
    #[br(if(version.at_least(32)))]
    pub unit_count_wide: Option<i16>,
    /// Number of units as a u8; before VER 8.9 (`GV_C32`).
    #[br(if(!version.at_least(32)))]
    pub unit_count: Option<u8>,
    /// Number of researches.
    pub research_count: u8,
    /// Number of unit and tech groups; its use is not known yet.
    pub total_unit_tech_groups: i32,
    /// The ages.
    #[br(count = age_count)]
    pub ages: Vec<TechTreeAge>,
    /// The buildings.
    #[br(count = building_count)]
    pub buildings: Vec<BuildingConnection>,
    /// The units.
    #[br(count = unit_count_wide.map_or_else(
        || usize::from(unit_count.unwrap_or(0)),
        |count| usize::try_from(count).unwrap_or(0),
    ))]
    pub units: Vec<UnitConnection>,
    /// The researches.
    #[br(count = research_count)]
    pub researches: Vec<ResearchConnection>,
}

/// The prerequisite slots every tech tree entry has.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TechTreeCommon {
    /// Number of slots in use.
    pub slots_used: i32,
    /// Per slot: a unit or research id.
    pub unit_research: [i32; ZONES],
    /// Per slot: what the id is (e.g. unit, research, age).
    pub mode: [i32; ZONES],
}

/// Three counted id lists: buildings, units, techs.
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connections {
    /// Number of buildings.
    pub building_count: u8,
    /// Building ids.
    #[br(count = building_count)]
    pub buildings: Vec<i32>,
    /// Number of units.
    pub unit_count: u8,
    /// Unit ids.
    #[br(count = unit_count)]
    pub units: Vec<i32>,
    /// Number of techs.
    pub tech_count: u8,
    /// Tech ids.
    #[br(count = tech_count)]
    pub techs: Vec<i32>,
}

/// An age on the tech tree.
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TechTreeAge {
    /// The age's id.
    pub id: i32,
    /// Status: 2 is the default, 3 not available; other values are not all known yet.
    pub status: u8,
    /// The buildings, units and techs of the age.
    pub connections: Connections,
    /// The prerequisite slots.
    pub common: TechTreeCommon,
    /// Number of building rows.
    pub num_building_levels: u8,
    /// Number of buildings in each zone.
    pub buildings_per_zone: [u8; ZONES],
    /// Group length in each zone.
    pub group_length_per_zone: [u8; ZONES],
    /// Not known yet; read as a u8.
    pub max_age_length: u8,
    /// Line mode; its values are not known yet.
    pub line_mode: i32,
}

/// A building on the tech tree.
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildingConnection {
    /// The building's unit id.
    pub id: i32,
    /// Status: 2 is the default, 3 not available; other values are not all known yet.
    pub status: u8,
    /// The buildings, units and techs the building makes available.
    pub connections: Connections,
    /// The prerequisite slots.
    pub common: TechTreeCommon,
    /// Age in which the building becomes available.
    pub location_in_age: u8,
    /// Number of units and techs in each age.
    pub units_techs_total: [u8; AGES],
    /// Number of units and techs in each age that come first in their line.
    pub units_techs_first: [u8; AGES],
    /// Line mode: 5 with connections, 6 without.
    pub line_mode: i32,
    /// Research that unlocks the building; -1 for none.
    pub enabling_research: i32,
}

/// A unit on the tech tree.
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitConnection {
    /// The unit's id.
    pub id: i32,
    /// Status: 2 is the default, 3 not available; other values are not all known yet.
    pub status: u8,
    /// Building that trains the unit.
    pub upper_building: i32,
    /// The prerequisite slots.
    pub common: TechTreeCommon,
    /// Not known yet; read as an i32.
    pub vertical_line: i32,
    /// Number of units in `units`.
    pub unit_count: u8,
    /// Units the unit leads to.
    #[br(count = unit_count)]
    pub units: Vec<i32>,
    /// Position in its age: 0 hidden, 1 first, 2 second.
    pub location_in_age: i32,
    /// Research the unit requires; -1 for none.
    pub required_research: i32,
    /// Line mode: 2 first unit of its line, 3 depends on an earlier research.
    pub line_mode: i32,
    /// Research that unlocks the unit; -1 for none.
    pub enabling_research: i32,
}

/// A research on the tech tree.
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResearchConnection {
    /// The tech's id.
    pub id: i32,
    /// Status: 2 is the default, 3 not available; other values are not all known yet.
    pub status: u8,
    /// Building that researches the tech.
    pub upper_building: i32,
    /// The buildings, units and techs the research leads to.
    pub connections: Connections,
    /// The prerequisite slots.
    pub common: TechTreeCommon,
    /// Not known yet; read as an i32.
    pub vertical_line: i32,
    /// Position in its age.
    pub location_in_age: i32,
    /// Line mode; its values are not known yet.
    pub line_mode: i32,
}
