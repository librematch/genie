//! The unit types' parts: movement, action, combat, projectile, creation and building.

use crate::de::{Task, Version};
use binrw::binrw;

/// The movement part of a unit (genieutils' `DeadFish`, unit types 30 and up).
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeadFish {
    /// Graphic while walking.
    pub walking_graphic: i16,
    /// Graphic while running.
    pub running_graphic: i16,
    /// Rotation speed.
    pub rotation_speed: f32,
    /// A size class older versions used; not used in DE as far as known.
    pub old_size_class: u8,
    /// Unit left behind as a trail (e.g. footprints); -1 for none.
    pub tracking_unit: i16,
    /// When the trail is left; its values are not known yet.
    pub tracking_unit_mode: u8,
    /// Spacing of the trail units.
    pub tracking_unit_density: f32,
    /// A move algorithm older versions used; not used in DE as far as known.
    pub old_move_algorithm: u8,
    /// Turn radius.
    pub turn_radius: f32,
    /// Speed while turning.
    pub turn_radius_speed: f32,
    /// Highest turn rate while moving.
    pub max_yaw_per_second_moving: f32,
    /// Time for a full turn while standing.
    pub stationary_yaw_revolution_time: f32,
    /// Highest turn rate while standing.
    pub max_yaw_per_second_stationary: f32,
    /// Smallest multiplier on the collision size; its use is not known yet.
    pub min_collision_size_multiplier: f32,
}

/// The action part of a unit (genieutils' `Bird`).
#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct Bird {
    /// Id of the default task in `tasks`; -1 for none.
    pub default_task_id: i16,
    /// Radius in which the unit looks for targets.
    pub search_radius: f32,
    /// Work rate (e.g. gathering or building speed).
    pub work_rate: f32,
    /// From VER 7.8 (`GV_C21`) the drop sites are counted; before, there are three.
    #[br(if(v.at_least(21)))]
    pub drop_site_count: Option<i16>,
    /// Units the unit delivers resources to; -1 for none.
    #[br(count = drop_site_count.map_or(3, |n| usize::try_from(n).unwrap_or(0)))]
    pub drop_sites: Vec<i16>,
    /// Group of units that turn into each other with the task (e.g. villager kinds).
    pub task_swap_group: u8,
    /// Sound when a command starts.
    pub attack_sound: i16,
    /// Sound when a command ends (e.g. on arrival).
    pub move_sound: i16,
    /// Wwise event id of the command-start sound.
    pub wwise_attack_sound_id: u32,
    /// Wwise event id of the command-end sound.
    pub wwise_move_sound_id: u32,
    /// How the unit runs about (e.g. a fleeing animal).
    pub run_pattern: u8,
    /// Number of tasks; from VER 7.2 (`GV_C15`).
    #[br(if(v.at_least(15)))]
    pub task_count: Option<i16>,
    /// The unit's own tasks.
    #[br(count = task_count.unwrap_or(0), args { inner: (v,) })]
    pub tasks: Vec<Task>,
}

impl Bird {
    /// The unit's default task, from its own task list.
    #[must_use]
    pub fn default_task(&self) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == self.default_task_id)
    }
}

/// The combat part of a unit (genieutils' `Type50`).
#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct Type50 {
    /// Armour against attack classes the unit has no armour entry for.
    pub base_armor: i16,
    /// Number of attacks.
    pub attack_count: i16,
    /// Attack amounts, one per armour class they hit.
    #[br(count = attack_count)]
    pub attacks: Vec<AttackOrArmor>,
    /// Number of armours.
    pub armour_count: i16,
    /// Armour amounts, one per class.
    #[br(count = armour_count)]
    pub armours: Vec<AttackOrArmor>,
    /// Terrain restriction whose values multiply the damage the unit takes on each terrain.
    pub defense_terrain_bonus: i16,
    /// Resistance to bonus damage; from VER 7.3 (`GV_C16`).
    #[br(if(v.at_least(16)))]
    pub bonus_damage_resistance: Option<f32>,
    /// Maximum attack range.
    pub max_range: f32,
    /// Radius of the blast damage.
    pub blast_width: f32,
    /// Time between attacks, in seconds.
    pub reload_time: f32,
    /// Projectile the unit fires; -1 for none.
    pub projectile_unit_id: i16,
    /// Chance to hit, in percent.
    pub accuracy_percent: i16,
    /// Not used as far as known; read as a u8.
    pub break_off_combat: u8,
    /// Frame of the attack graphic at which the attack lands or the projectile leaves.
    pub frame_delay: i16,
    /// Offset at which projectiles start, along x, y and z.
    pub graphic_displacement: [f32; 3],
    /// Blast attack level: which units the blast damages.
    pub blast_attack_level: u8,
    /// Minimum attack range.
    pub min_range: f32,
    /// Spread of the projectiles that miss.
    pub accuracy_dispersion: f32,
    /// Graphic while attacking.
    pub attack_graphic: i16,
    /// Melee armour the interface shows.
    pub displayed_melee_armour: i16,
    /// Attack the interface shows.
    pub displayed_attack: i16,
    /// Range the interface shows.
    pub displayed_range: f32,
    /// Reload time the interface shows.
    pub displayed_reload_time: f32,
    /// Share of the damage the blast deals; from VER 7.7 (`GV_C20`).
    #[br(if(v.at_least(20)))]
    pub blast_damage: Option<f32>,
    /// Share of the damage taken that is reflected to the attacker; from VER 7.9 (`GV_C22`).
    #[br(if(v.at_least(22)))]
    pub damage_reflection: Option<f32>,
    /// Share of the damage dealt to friendly units; from VER 7.9 (`GV_C22`).
    #[br(if(v.at_least(22)))]
    pub friendly_fire_damage: Option<f32>,
    /// Frame from which an attack can be interrupted; from VER 8.4 (`GV_C27`).
    #[br(if(v.at_least(27)))]
    pub interrupt_frame: Option<i16>,
    /// Firepower the unit adds when garrisoned; from VER 8.4 (`GV_C27`).
    #[br(if(v.at_least(27)))]
    pub garrison_firepower: Option<f32>,
    /// Second attack graphic; from VER 8.4 (`GV_C27`).
    #[br(if(v.at_least(27)))]
    pub attack_graphic2: Option<i16>,
}

/// An attack or armour amount for one armour class.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttackOrArmor {
    /// Armour class.
    pub class: i16,
    /// Amount.
    pub amount: i16,
}

/// The projectile part of a unit (unit type 60).
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projectile {
    /// Projectile type (e.g. falling straight down); its values are not all known yet.
    pub projectile_kind: u8,
    /// Whether the projectile aims at the target's predicted position.
    pub smart_mode: u8,
    /// Whether the projectile vanishes on hitting.
    pub hit_mode: u8,
    /// Whether the projectile's graphic passes through what it hits.
    pub vanish_mode: u8,
    /// Bit flags of special area effects; not known yet.
    pub area_effect_specials: u8,
    /// Arc of the flight.
    pub projectile_arc: f32,
}

/// One resource cost of a trainable unit.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ResourceCost {
    /// Resource id; -1 for none.
    pub kind: i16,
    /// Amount.
    pub amount: i16,
    /// Whether the amount is paid (non-zero) or only listed.
    pub flag: i16,
}

/// A unit or building that trains a unit, with the training time and button.
#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrainLocation {
    /// Training time.
    pub queue_time: i16,
    /// Unit or building that trains the unit.
    pub location_id: i16,
    /// Position of the button on the command panel.
    pub button_id: u8,
    /// Hotkey id; from VER 8.7 (`GV_C30`).
    #[br(if(v.at_least(30)))]
    pub hot_key_id: Option<i32>,
}

/// The creation part of a unit (unit types 70 and up): costs, train locations, garrison
/// projectiles, charge and conversion settings.
#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct Creatable {
    /// Up to three costs.
    pub resource_costs: [ResourceCost; 3],
    /// From VER 8.6 (`GV_C29`) the train locations are counted; before, there is one.
    #[br(if(v.at_least(29)))]
    pub train_location_count: Option<i16>,
    /// Where the unit is trained.
    #[br(
        count = train_location_count.map_or(1, |n| usize::try_from(n).unwrap_or(0)),
        args { inner: (v,) }
    )]
    pub train_locations: Vec<TrainLocation>,
    /// Damage multiplier for attacks from behind (the name of earlier games); DE's master loads
    /// it to `+0x8b4`, the save's `regeneration_rate`.
    pub rear_attack_modifier: f32,
    /// Damage multiplier for attacks from the side (the name of earlier games); DE's master loads
    /// it to `+0x8b8`, `mSpacingModifier`: a unit's radius in a group's formation is its
    /// `collision_size` on x times this (build 158041, `FUN_141025920`).
    pub flank_attack_modifier: f32,
    /// Kind of creatable unit; DE's `mFormationCategory` (`+0x8bc`): the sub-formation of a
    /// group the unit joins, 0 for units the order splitter never groups (villagers).
    pub creatable_kind: u8,
    /// Hero bit flags (e.g. no conversion, healing); DE's `hero_flag` (`+0x8d9`).
    pub hero_mode: u8,
    /// Graphic shown while units are garrisoned.
    pub garrison_graphic: i32,
    /// Graphic when the unit is spawned.
    pub spawning_graphic: i16,
    /// Graphic when the unit is upgraded.
    pub upgrade_graphic: i16,
    /// Glow graphic of a hero; from VER 7.4 (`GV_C17`).
    #[br(if(v.at_least(17)))]
    pub hero_glow_graphic: Option<i16>,
    /// Idle graphic in combat; from VER 8.3 (`GV_C26`).
    #[br(if(v.at_least(26)))]
    pub idle_attack_graphic: Option<i16>,
    /// Highest charge of the unit's charge ability; from VER 7.3 (`GV_C16`).
    #[br(if(v.at_least(16)))]
    pub max_charge: Option<f32>,
    /// Charge regained over time; from VER 7.3 (`GV_C16`).
    #[br(if(v.at_least(16)))]
    pub recharge_rate: Option<f32>,
    /// Event that uses up the charge; from VER 7.3 (`GV_C16`).
    #[br(if(v.at_least(16)))]
    pub charge_event: Option<i16>,
    /// What the charge does (e.g. a charged attack); from VER 7.3 (`GV_C16`).
    #[br(if(v.at_least(16)))]
    pub charge_kind: Option<i16>,
    /// What a charged ability targets; from VER 8.0 (`GV_C23`).
    #[br(if(v.at_least(23)))]
    pub charge_target: Option<i16>,
    /// Projectile of a charged attack; from VER 8.2 (`GV_C25`).
    #[br(if(v.at_least(25)))]
    pub charge_projectile_unit: Option<i32>,
    /// Priority as an attack target; from VER 8.2 (`GV_C25`).
    #[br(if(v.at_least(25)))]
    pub attack_priority: Option<u8>,
    /// Invulnerability level; from VER 8.2 (`GV_C25`).
    #[br(if(v.at_least(25)))]
    pub invulnerability_level: Option<f32>,
    /// Icon of the unit's ability button; from VER 8.1 (`GV_C24`).
    #[br(if(v.at_least(24)))]
    pub button_icon_id: Option<i16>,
    /// Language string id of that button's short tooltip; from VER 8.1 (`GV_C24`).
    #[br(if(v.at_least(24)))]
    pub button_short_tooltip_id: Option<i32>,
    /// Language string id of that button's long tooltip; from VER 8.1 (`GV_C24`).
    #[br(if(v.at_least(24)))]
    pub button_extended_tooltip_id: Option<i32>,
    /// Hotkey action of that button; from VER 8.1 (`GV_C24`).
    #[br(if(v.at_least(24)))]
    pub button_hotkey_action: Option<i16>,
    /// Modifier on the shortest time to convert the unit; from VER 7.6 (`GV_C19`).
    #[br(if(v.at_least(19)))]
    pub min_conversion_time_mod: Option<f32>,
    /// Modifier on the longest time to convert the unit; from VER 7.6 (`GV_C19`).
    #[br(if(v.at_least(19)))]
    pub max_conversion_time_mod: Option<f32>,
    /// Modifier on the chance to convert the unit; from VER 7.6 (`GV_C19`).
    #[br(if(v.at_least(19)))]
    pub conversion_chance_mod: Option<f32>,
    /// Projectiles fired with nothing garrisoned.
    pub total_projectiles: f32,
    /// Projectiles fired when fully garrisoned.
    pub max_total_projectiles: u8,
    /// Area the projectiles start from: width, length and randomness.
    pub projectile_spawning_area: [f32; 3],
    /// Projectile of the extra (garrison) projectiles; -1 for the main one.
    pub secondary_projectile_unit: i32,
    /// Graphic of the special ability.
    pub special_graphic: i32,
    /// Special ability: none, block, counter charge or charge.
    pub special_ability: u8,
    /// Pierce armour the interface shows.
    pub displayed_pierce_armour: i16,
}

/// A unit attached to a building, at an offset.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BuildingAnnex {
    /// The attached unit; -1 for none.
    pub unit_id: i16,
    /// Its offset from the building along x and y.
    pub misplacement: [f32; 2],
}

/// The building part of a unit (unit type 80).
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Building {
    /// Graphic while under construction.
    pub construction_graphic_id: i16,
    /// Graphic in snow.
    pub snow_graphic_id: i16,
    /// Graphic when destroyed.
    pub destruction_graphic_id: i16,
    /// Graphic of the rubble.
    pub destruction_rubble_graphic_id: i16,
    /// Graphic while researching.
    pub researching_graphic: i16,
    /// Graphic when a research completes.
    pub research_completed_graphic: i16,
    /// Whether the graphic adapts to adjacent buildings (walls).
    pub adjacent_mode: u8,
    /// Angle of the graphic.
    pub graphics_angle: i16,
    /// Whether the building disappears once built (e.g. a placer for a stack).
    pub disappears_when_built: u8,
    /// Building placed on top of this one; -1 for none.
    pub stack_unit_id: i16,
    /// Terrain laid under the building when built; -1 for none.
    pub foundation_terrain_id: i16,
    /// An overlay id from Age of Empires (roads); not used in DE as far as known.
    pub old_overlay_id: i16,
    /// Tech researched when the building completes (e.g. an age); -1 for none.
    pub tech_id: i16,
    /// Whether the building shows fire when damaged.
    pub can_burn: u8,
    /// Up to four attached units.
    pub annexes: [BuildingAnnex; 4],
    /// Building the unit is an annex of; -1 for none.
    pub head_unit: i16,
    /// Unit the building transforms into; -1 for none.
    pub transform_unit: i16,
    /// Sound when it transforms.
    pub transform_sound: i16,
    /// Sound while it is built.
    pub construction_sound: i16,
    /// Wwise event id of the transform sound.
    pub wwise_transform_sound_id: u32,
    /// Wwise event id of the construction sound.
    pub wwise_construction_sound_id: u32,
    /// Bit flags of the unit kinds that can garrison (villagers, infantry, …).
    pub garrison_kind: u8,
    /// Rate at which garrisoned units heal.
    pub garrison_heal_rate: f32,
    /// Rate at which the building repairs itself with units garrisoned.
    pub garrison_repair_rate: f32,
    /// Not known yet; read as a unit id (i16).
    pub pile_unit: i16,
    /// Six looting entries; not known yet how the game uses them.
    pub looting_table: [u8; 6],
}
