//! The JSON data tables Age of Empires II: Definitive Edition loads from `resources/_common/dat`.
//!
//! Every key the installed files hold is typed, and an unknown key is an error: a game update that
//! adds one shows up instead of vanishing. A key some entries leave out is an `Option` and stays
//! out when written. Text values the game data defines (a map's style, a linked-tech group's type)
//! stay strings: new game versions add values.
//!
//! The files are read and written as values, not bytes: [`to_string`] indents by two spaces, while
//! the installed files differ in indentation.

#![deny(unsafe_code)]

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Read a table from its JSON text.
///
/// # Errors
///
/// Returns an error if `text` is not valid JSON, lacks a required key, holds an unknown key, or
/// holds a value of the wrong type.
pub fn from_str<'de, T: Deserialize<'de>>(text: &'de str) -> serde_json::Result<T> {
    serde_json::from_str(text)
}

/// The JSON text of a table, indented by two spaces.
///
/// # Errors
///
/// Returns an error if `T`'s `Serialize` implementation fails or gives a map with non-string keys;
/// the tables of this crate do neither.
pub fn to_string<T: Serialize>(table: &T) -> serde_json::Result<String> {
    serde_json::to_string_pretty(table)
}

/// `linkedTechs.json`: groups of techs that exclude, cycle or toggle each other.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkedTechs {
    /// The groups, in the file's order.
    #[serde(rename = "LinkedTechs")]
    pub linked_techs: Vec<LinkedTechGroup>,
}

/// A group of techs linked to each other: researching one of them affects the others.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "PascalCase")]
pub struct LinkedTechGroup {
    /// String table id of the group's name.
    pub name_id: i32,
    /// A name for the group, for the reader of the file (e.g. `Satrapies`).
    pub comment: String,
    /// Not known yet; some groups set it to `true`, the others leave it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<bool>,
    /// `MutuallyExclusive`, `Cycle` or `Toggleable` in the current game.
    #[serde(rename = "Type")]
    pub kind: String,
    /// The tech ids of the group.
    pub techs: Vec<i32>,
    /// The tech of the group that is in effect at the start, for groups that have one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starting_tech: Option<i32>,
    /// Set to `true` on some `Cycle` groups; its effect is not known yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instant: Option<bool>,
}

/// `dropsites.json`: which buildings take which resources back, and from whom.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DropSites {
    /// The drop sites, in the file's order. A building can have several entries, e.g. one for
    /// each kind of worker.
    pub drop_site_list: Vec<DropSite>,
}

/// A building that takes resources back: the workers that may bring them, and what they gather.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DropSite {
    /// The building's name, for the reader of the file.
    pub name: String,
    /// Unit id of the building.
    pub building_id: i32,
    /// Unit class of the workers that may drop resources here (4, civilians), if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_object_group: Option<i32>,
    /// Whether the building takes livestock, if given (e.g. the Town Center and the Mill).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepts_livestock: Option<bool>,
    /// What the workers gather and bring back here.
    pub target_list: Vec<DropSiteTarget>,
    /// The resource types for which the building updates the AI's drop site data, if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "update_ai_resource_types")]
    pub update_ai_resource_kinds: Option<Vec<AiResourceKind>>,
    /// Unit ids of the workers that may drop resources here, if given (e.g. the farmers of a
    /// Farm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_object_list: Option<Vec<i32>>,
    /// Not known yet; `false` on the Dock and Port entries, left out elsewhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_work: Option<bool>,
    /// The civilization ids the entry applies to, if it is limited to some.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_civs: Option<Vec<i32>>,
}

/// One kind of object the workers of a drop site gather from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DropSiteTarget {
    /// A name for the target, for the reader of the file (e.g. `Gold Mines`).
    pub name: String,
    /// Unit class of the target objects (e.g. 15, trees).
    pub object_group: i32,
    /// The object state the target must be in, if given (5 on animals).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_state: Option<i32>,
    /// Not known yet; given on hunted animals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_state_forced_return_state: Option<i32>,
    /// The resource (attribute) id the worker carries back from the target, if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "attribute_type")]
    pub attribute_kind: Option<i32>,
    /// Not known yet; 0 on farms, left out elsewhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_num: Option<i32>,
    /// Whether the target's distance to a drop site is updated, if given; some targets set it to
    /// `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_resource_distance_update: Option<bool>,
}

/// A resource type, as [`DropSite::update_ai_resource_kinds`] lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiResourceKind {
    /// The resource type id: 0 food, 1 wood, 2 stone, 3 gold.
    #[serde(rename = "resource_type")]
    pub resource_kind: i32,
}

/// `futuravailableunits.json` (and `paphosfutureavailableunits.json`): per civilization, in the
/// file's order, the buildings and what each makes available.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FutureUnits(
    /// Each civilization's entry, keyed by the civilization's name (`Gaia`, `Aztecs`, ...).
    pub IndexMap<String, CivFutureUnits>,
);

/// A civilization's entry in [`FutureUnits`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "PascalCase")]
pub struct CivFutureUnits {
    /// The objects that build, train or research something: despite the key's name, villagers
    /// and ships too.
    pub buildings: Vec<FutureBuilding>,
}

/// An object that builds, trains or researches, and what it makes available.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "PascalCase")]
pub struct FutureBuilding {
    /// Unit id of the object.
    #[serde(rename = "ID")]
    pub id: i32,
    /// The object's name, for the reader of the file.
    pub name: String,
    /// The techs it researches, if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub techs: Option<Vec<FutureTech>>,
    /// The units it trains or the buildings it builds, if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<Vec<FutureUnit>>,
    /// The age the object needs, if given: 1 is the Dark Age, 4 the Imperial Age.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_age: Option<i32>,
    /// The tech that must be researched before the object is available, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prereq_tech: Option<i32>,
    /// Icon set of the icon that shows the prerequisite, if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prereq_icon_set: Option<i32>,
    /// Index of the prerequisite's icon in its set, if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prereq_icon_index: Option<i32>,
    /// Not known yet; 0 wherever the current files give it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prereq_style: Option<i32>,
    /// String table id of the text that names the prerequisite, if given.
    #[serde(
        rename = "PrereqStringID",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub prereq_string_id: Option<i32>,
}

/// A tech an object researches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "PascalCase")]
pub struct FutureTech {
    /// Tech id.
    #[serde(rename = "ID")]
    pub id: i32,
    /// The tech's name, for the reader of the file.
    pub name: String,
    /// The age the tech needs: 1 is the Dark Age, 4 the Imperial Age.
    pub required_age: i32,
    /// The tech that must be researched first, if any.
    #[serde(
        rename = "RequiredTechID",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub required_tech_id: Option<i32>,
}

/// A unit an object trains, or a building a villager builds.
///
/// The prerequisite fields repeat in [`FutureBuilding`]: serde cannot refuse unknown keys through
/// a flattened struct.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "PascalCase")]
pub struct FutureUnit {
    /// Unit id.
    #[serde(rename = "ID")]
    pub id: i32,
    /// The unit's name, for the reader of the file.
    pub name: String,
    /// The age the unit needs, if given: 1 is the Dark Age, 4 the Imperial Age.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_age: Option<i32>,
    /// The tech that must be researched before the unit is available, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prereq_tech: Option<i32>,
    /// Icon set of the icon that shows the prerequisite, if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prereq_icon_set: Option<i32>,
    /// Index of the prerequisite's icon in its set, if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prereq_icon_index: Option<i32>,
    /// Not known yet; 0 wherever the current files give it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prereq_style: Option<i32>,
    /// String table id of the text that names the prerequisite, if given.
    #[serde(
        rename = "PrereqStringID",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub prereq_string_id: Option<i32>,
    /// The unit this one follows in its line, if any (e.g. the Capped Ram before the Siege Ram).
    #[serde(
        rename = "RequiredUnitID",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub required_unit_id: Option<i32>,
}

/// `maps.json`: the map list of the lobby.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Maps {
    /// [`MapEntry::storage_id`] of the map selected by default.
    #[serde(rename = "default_map_type")]
    pub default_map_kind: i32,
    /// [`MapEntry::storage_id`] of the map selected by default for Nomad games.
    #[serde(rename = "default_nomad_map_type")]
    pub default_nomad_map_kind: i32,
    /// [`MapEntry::storage_id`] of the map selected by default for Battle Royale games.
    #[serde(rename = "default_battle_royale_map_type")]
    pub default_battle_royale_map_kind: i32,
    /// The maps, in the file's order.
    pub map_list: Vec<MapEntry>,
}

/// A string reference: a string table id, or a key-value string's name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringId {
    /// A string table id.
    Number(i64),
    /// The name of a key-value string (e.g. `RMS_KILIMANJARO_ROLLOVER`).
    Name(String),
}

/// One map of the list. The `Option<bool>` flags are left out of the file when not set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapEntry {
    /// The map's constant name (e.g. `arabia`), if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub const_name: Option<String>,
    /// The map's internal name (e.g. `ARABIA-MAP`).
    pub data_name: String,
    /// String table id of the map's name.
    pub name_string_id: i64,
    /// The string of the map's tooltip.
    pub rollover_string_id: StringId,
    /// Name of the map's icon (e.g. `IconArabia`).
    pub icon_reference: String,
    /// File name of the map's preview image, without extension.
    pub map_icon_filename: String,
    /// File name of the map's random map script (`.rms`).
    pub script_filename: String,
    /// The map's number, as game settings store it (Arabia is 9).
    pub storage_id: i32,
    /// `random`, `real world` or `special` in the current game.
    pub style: String,
    /// Listed among the land maps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub land: Option<bool>,
    /// Listed among the open maps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<bool>,
    /// Offered in the Random Map game mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub random_map: Option<bool>,
    /// Offered in the Death Match game mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub death_match: Option<bool>,
    /// Offered in the Regicide game mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regicide: Option<bool>,
    /// Offered in the King of the Hill game mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub king_of_the_hill: Option<bool>,
    /// Offered in the Capture the Relic game mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_the_relic: Option<bool>,
    /// Offered in the Sudden Death game mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sudden_death: Option<bool>,
    /// Offered in the Defend the Wonder game mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defend_the_wonder: Option<bool>,
    /// Offered in the Wonder Race game mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wonder_race: Option<bool>,
    /// Offered in the Empire Wars game mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empire_wars: Option<bool>,
    /// Listed among the water maps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub water: Option<bool>,
    /// Listed among the closed maps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed: Option<bool>,
    /// Listed among the mixed (land and water) maps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mixed: Option<bool>,
    /// A game mode the map is not offered for, if any (e.g. `defend_the_wonder`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prohibited_game_mode: Option<String>,
    /// Marked as a migration map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration: Option<bool>,
    /// Marked as a nomad map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nomad: Option<bool>,
    /// File name of the scenario that comes with the script, if any (real world maps).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario_filename: Option<String>,
    /// Offered in the Battle Royale game mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battle_royale: Option<bool>,
    /// File name of the image the map pool shows, if it differs from `map_icon_filename`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mappool_icon_filename: Option<String>,
}

/// `buttons.json`: the command buttons of the unit panel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Buttons {
    /// The buttons, in the file's order.
    pub command_button_list: Vec<CommandButton>,
}

/// One command button.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandButton {
    /// The button's name, for the reader of the file.
    pub name: String,
    /// The action the button performs.
    pub button_action_id: i32,
    /// Not known yet; buttons that take turns in one place (setting and removing a gather point)
    /// share the value.
    pub sequence_id: i32,
    /// Index of the button's icon.
    pub icon_id: i32,
    /// String table id of the button's help text.
    pub help_string_id: i32,
    /// String table id of the button's longer help text, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more_help_string_id: Option<i32>,
    /// The action the button performs when clicked with Shift held, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button_shift_action_id: Option<i32>,
}
