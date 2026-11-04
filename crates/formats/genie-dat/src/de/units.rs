//! The civilizations and their units ([`Unit`]): the shared task lists, the tasks, the
//! resource storage and the damage graphics.

use crate::de::{
    Bird, Building, Creatable, DeadFish, DebugString, Projectile, Type50, Version, read_present,
    unit_kind, write_present,
};
use binrw::binrw;

/// The shared task list of a unit id (DE keeps the tasks in each unit's own list as well).
#[binrw]
#[brw(little)]
#[br(import(version: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct UnitHeader {
    /// Non-zero if the unit id has a task list.
    pub exists: u8,
    /// Number of tasks; present when `exists` is non-zero.
    #[br(if(exists != 0))]
    pub task_count: Option<i16>,
    /// The tasks.
    #[br(count = task_count.unwrap_or(0), args { inner: (version,) })]
    pub tasks: Vec<Task>,
}

/// A task: an action a unit can perform, with its targets, rates, graphics and sounds.
#[binrw]
#[brw(little)]
#[br(import(version: Version))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Task {
    /// Task type: 0 generic, 1 tribe (the kind Age of Kings and later use).
    pub task_kind: i16,
    /// The task's id, unique within its unit's task list.
    pub id: i16,
    /// Non-zero for the unit's default task.
    pub is_default: u8,
    /// Action the task performs (gather, build, attack, convert, …).
    pub action_kind: i16,
    /// Class of the targets; -1 for any.
    pub class_id: i16,
    /// Target unit; -1 for any.
    pub unit_id: i16,
    /// Target terrain; -1 for any.
    pub terrain_id: i16,
    /// Resource the unit picks up while working.
    pub resource_in: i16,
    /// Resource whose amount multiplies what the unit gathers.
    pub resource_multiplier: i16,
    /// Resource the unit delivers.
    pub resource_out: i16,
    /// A resource field the game does not use, as far as known.
    pub unused_resource: i16,
    /// First work value; its meaning depends on the action.
    pub work_value1: f32,
    /// Second work value; its meaning depends on the action.
    pub work_value2: f32,
    /// Range at which the task works.
    pub work_range: f32,
    /// Whether the unit looks for targets of this task by itself.
    pub auto_search_targets: u8,
    /// Time between two target searches.
    pub search_wait_time: f32,
    /// Whether the task can be given by targeting an object.
    pub enable_targeting: u8,
    /// Combat level flag; its values are not known yet.
    pub combat_level_flag: u8,
    /// Gather type; its values are not known yet.
    pub gather_kind: i16,
    /// Second work flag; its values are not known yet.
    pub work_flag2: i16,
    /// Which owners' objects may be targeted (own, allied, enemy, …).
    pub target_diplomacy: u8,
    /// Whether the target must still hold resources.
    pub carry_check: u8,
    /// Whether builders pick this task for a construction site.
    pub pick_for_construction: u8,
    /// Graphic while moving to the target.
    pub moving_graphic_id: i16,
    /// Graphic while proceeding with the work (e.g. the gathering swing).
    pub proceeding_graphic_id: i16,
    /// Graphic while working.
    pub working_graphic_id: i16,
    /// Graphic while carrying resources.
    pub carrying_graphic_id: i16,
    /// Sound while gathering.
    pub resource_gathering_sound_id: i16,
    /// Sound when delivering resources.
    pub resource_deposit_sound_id: i16,
    /// Wwise event id of the gathering sound.
    pub wwise_resource_gathering_sound_id: u32,
    /// Wwise event id of the delivery sound.
    pub wwise_resource_deposit_sound_id: u32,
    /// Whether the task is enabled; from VER 8.5 (`GV_C28`).
    #[br(if(version.at_least(28)))]
    pub enabled: Option<i16>,
}

/// A civilization: its starting resources and its own copy of every unit.
#[binrw]
#[brw(little)]
#[br(import(version: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct Civ {
    /// Player type; its values are not known yet.
    pub player_kind: u8,
    /// Internal name.
    pub name: DebugString,
    /// Number of entries in `resources`.
    pub resource_count: i16,
    /// Effect that sets up the civilization's tech tree (what it cannot build); -1 for none.
    pub tech_tree_id: i16,
    /// Effect of the civilization's team bonus; -1 for none.
    pub team_bonus_id: i16,
    /// Starting value of every resource (player attribute).
    #[br(count = resource_count)]
    pub resources: Vec<f32>,
    /// Architecture set of the civilization's icons.
    pub icon_set: u8,
    /// Number of unit slots.
    pub unit_count: i16,
    /// One pointer per unit slot; a zero pointer marks an empty slot, which has no entry in
    /// `units`.
    #[br(count = unit_count)]
    pub unit_pointers: Vec<i32>,
    /// The civilization's units, indexed by unit id; `None` where the slot's pointer is zero.
    #[br(parse_with = |reader, endian, (): ()| read_present(reader, endian, &unit_pointers, (version,)))]
    #[bw(write_with = |list, writer, endian, (): ()| write_present(list, writer, endian))]
    pub units: Vec<Option<Unit>>,
}

/// A unit definition: the part every unit type has, then the parts its `unit_kind` reaches.
#[binrw]
#[brw(little)]
#[br(import(version: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    /// Unit type (see [`unit_kind`]); it decides which of the optional parts follow.
    pub unit_kind: u8,
    /// The unit's id.
    pub id: i16,
    /// Language string id of the name; from VER 7.2 (`GV_C15`).
    #[br(if(version.at_least(15)))]
    pub language_dll_name: Option<i32>,
    /// Language string id of the creation text; from VER 7.2 (`GV_C15`).
    #[br(if(version.at_least(15)))]
    pub language_dll_creation: Option<i32>,
    /// Language string id of the name as an i16; before VER 7.2 (`GV_C15`).
    #[br(if(!version.at_least(15)))]
    pub language_dll_name_u16: Option<i16>,
    /// Language string id of the creation text as an i16; before VER 7.2 (`GV_C15`).
    #[br(if(!version.at_least(15)))]
    pub language_dll_creation_u16: Option<i16>,
    /// Unit class (archer, infantry, building, tree, …).
    pub class: i16,
    /// The two standing graphics; -1 for none.
    pub standing_graphic: [i16; 2],
    /// Graphic while dying.
    pub dying_graphic: i16,
    /// Graphic of the undead state.
    pub undead_graphic: i16,
    /// Whether the unit has an undead state after dying.
    pub undead_mode: u8,
    /// Hit points; -1 makes the unit die at once.
    pub hit_points: i16,
    /// Line of sight, in tiles.
    pub line_of_sight: f32,
    /// Number of units that can garrison inside.
    pub garrison_capacity: u8,
    /// Radius of the collision box along x, y and z, in tiles.
    pub collision_size: [f32; 3],
    /// Sound when the unit is created.
    pub train_sound: i16,
    /// Sound when the unit is damaged.
    pub damage_sound: i16,
    /// Unit it turns into when it dies; -1 for none.
    pub dead_unit_id: i16,
    /// Unit spawned as its blood; -1 for none.
    pub blood_unit_id: i16,
    /// Draw order among objects on the same spot.
    pub sort_number: u8,
    /// Whether buildings may be placed over the unit.
    pub can_be_built_on: u8,
    /// Frame of the unit's icon.
    pub icon_id: i16,
    /// Whether the scenario editor hides the unit.
    pub hide_in_editor: u8,
    /// A portrait id older versions used; not used in DE as far as known.
    pub old_portrait_pict: i16,
    /// Whether the unit is available from the start rather than enabled by a tech.
    pub enabled: u8,
    /// Whether the unit is disabled.
    pub disabled: u8,
    /// Terrains needed beside the foundation (e.g. a dock's water); -1 for none.
    pub placement_side_terrain: [i16; 2],
    /// Terrains needed under the foundation; -1 for none.
    pub placement_terrain: [i16; 2],
    /// Free space needed to place the unit, along x and y.
    pub clearance_size: [f32; 2],
    /// How the unit may be placed on slopes.
    pub hill_mode: u8,
    /// How the unit shows under fog of war.
    pub fog_visibility: u8,
    /// Terrain restriction id: where the unit may be placed and move.
    pub terrain_restriction: i16,
    /// Whether the unit flies.
    pub fly_mode: u8,
    /// Amount of resources the unit can carry.
    pub resource_capacity: i16,
    /// Rate at which the unit's resources decay (e.g. a carcass rotting).
    pub resource_decay: f32,
    /// Blast defense level: which blast attacks damage the unit.
    pub blast_defense_level: u8,
    /// Combat level; its values are not known yet.
    pub combat_level: u8,
    /// What the player can do with the unit (select, move, attack, …).
    pub interaction_mode: u8,
    /// How the unit shows on the minimap.
    pub minimap_mode: u8,
    /// Which command buttons the unit shows.
    pub interface_kind: u8,
    /// Not known yet; read as an f32.
    pub multiple_attribute_mode: f32,
    /// Palette index of the unit's minimap dot.
    pub minimap_color: u8,
    /// Language string id of the help text.
    pub language_dll_help: i32,
    /// Language string id of the hotkey text.
    pub language_dll_hot_key_text: i32,
    /// Before VER 8.7 (`GV_C30`) the hotkey of the (single) train location sits here.
    #[br(if(!version.at_least(30)))]
    pub hot_key_id: Option<i32>,
    /// Whether the unit can be recycled; its exact effect is not known yet.
    pub recyclable: u8,
    /// Whether villagers gather from the unit by themselves.
    pub enable_auto_gather: u8,
    /// Whether the unit leaves a doppelganger in the fog when it dies.
    pub create_doppelganger_on_death: u8,
    /// Resource group gatherers treat the unit as.
    pub resource_gather_group: u8,
    /// Bit flags for how the unit is masked behind other objects.
    pub occlusion_mode: u8,
    /// Shape of the unit's obstruction (none, square, round, …).
    pub obstruction_kind: u8,
    /// Obstruction class; its values are not known yet.
    pub obstruction_class: u8,
    /// Bit flags of the unit's traits; not all known yet.
    pub trait_: u8,
    /// Civilization the unit belongs to.
    pub civilization: u8,
    /// Not used as far as known; read as an i16.
    pub nothing: i16,
    /// Effect shown when the unit is selected.
    pub selection_effect: u8,
    /// Colour of the unit's selection in the scenario editor.
    pub editor_selection_colour: u8,
    /// Size of the selection outline along x, y and z.
    pub outline_size: [f32; 3],
    /// Two words genieutils writes as -2 and 0 and drops on read (`GV_CK` and later).
    pub unknown: [i32; 2],
    /// Three resources the unit holds or provides.
    pub resource_storages: [ResourceStorage; 3],
    /// Number of damage graphics.
    pub damage_graphic_count: u8,
    /// Graphics shown as the unit loses hit points.
    #[br(count = damage_graphic_count)]
    pub damage_graphics: Vec<DamageGraphic>,
    /// Sound when the unit is selected.
    pub selection_sound: i16,
    /// Sound when the unit dies.
    pub dying_sound: i16,
    /// Wwise event id of the creation sound.
    pub wwise_train_sound_id: u32,
    /// Wwise event id of the damage sound.
    pub wwise_damage_sound_id: u32,
    /// Wwise event id of the selection sound.
    pub wwise_selection_sound_id: u32,
    /// Wwise event id of the dying sound.
    pub wwise_dying_sound_id: u32,
    /// An attack reaction older versions used; not used in DE as far as known.
    pub old_attack_reaction: u8,
    /// A leftover flag for converting the terrain under the unit; not used as far as known.
    pub convert_terrain: u8,
    /// Internal name.
    pub name: DebugString,
    /// Unit this one was copied from.
    pub copy_id: i16,
    /// Base unit; its exact role is not known yet.
    pub base_id: i16,
    /// Movement speed; for unit types 20 and up, except 90.
    #[br(if(unit_kind >= unit_kind::FLAG && unit_kind != unit_kind::AOE_TREES))]
    pub speed: Option<f32>,
    /// Movement part; for unit types 30 and up, except 90.
    #[br(if(unit_kind >= unit_kind::DEAD_FISH && unit_kind != unit_kind::AOE_TREES))]
    pub dead_fish: Option<DeadFish>,
    /// Action part; for unit types 40 and up, except 90.
    #[br(if(unit_kind >= unit_kind::BIRD && unit_kind != unit_kind::AOE_TREES), args(version))]
    pub bird: Option<Bird>,
    /// Combat part; for unit types 50 and up, except 90.
    #[br(if(unit_kind >= unit_kind::COMBATANT && unit_kind != unit_kind::AOE_TREES), args(version))]
    pub type50: Option<Type50>,
    /// Projectile part; for unit type 60 only.
    #[br(if(unit_kind == unit_kind::PROJECTILE))]
    pub projectile: Option<Projectile>,
    /// Creation part; for unit types 70 and up, except 90.
    #[br(if(unit_kind >= unit_kind::CREATABLE && unit_kind != unit_kind::AOE_TREES), args(version))]
    pub creatable: Option<Creatable>,
    /// Building part; for unit type 80 only.
    #[br(if(unit_kind == unit_kind::BUILDING))]
    pub building: Option<Building>,
}

/// A resource a unit holds or provides (e.g. its food, or the population room it gives).
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ResourceStorage {
    /// Resource id; -1 for none.
    pub kind: i16,
    /// Amount of the resource.
    pub amount: f32,
    /// How the resource is stored or given; its values are not known yet.
    pub flag: u8,
}

/// A graphic shown once a unit has lost a share of its hit points (e.g. fire on a building).
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageGraphic {
    /// The graphic.
    pub graphic_id: i16,
    /// Share of hit points lost, in percent, from which the graphic shows.
    pub damage_percent: i16,
    /// How the graphic is applied (over or instead of the unit's own); its values are not known
    /// yet.
    pub apply_mode: u8,
}
