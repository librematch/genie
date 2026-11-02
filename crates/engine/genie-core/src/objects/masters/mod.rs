//! What the engine reads of a player's masters: the tasks a unit can do and the masters a unit
//! changes into to do them, and how an effect changes a master: the attribute setters (slots
//! 0x18, 0x20 and 0x28 of every master type, [`Master::set_attribute`]), the master copy of an
//! upgrade (slot 8, [`Master::copy_from`]) and the values the slice derives from a master
//! ([`Master::follow`]). The values are game data: the player's masters in the save, with the
//! `.dat`'s values for what the save does not carry.

mod combat_level;
mod combat_setters;
mod conversion_adjust;
mod cost_amount;
mod cost_slot;
mod derived;
mod dispatch;
mod lookup;
mod spacing_modifier;
mod static_setters;
mod target_owner;
mod task_id;
mod task_index;
mod task_terrain;
mod trail;
mod tribe_setters;
mod values;

pub use self::combat_level::CombatLevel;
pub use self::cost_amount::CostAmount;
pub use self::cost_slot::{COST_SLOTS, CostSlot, entries as cost_entries};
pub use self::lookup::{MasterTables, OwnerMasters, PlayersMasters, player_master};
pub use self::spacing_modifier::SpacingModifier;
pub use self::target_owner::TargetOwner;
pub use self::task_id::TaskId;
pub use self::task_index::TaskIndex;
pub use self::task_terrain::TaskTerrain;
pub use self::trail::{Trail, TrailDensity, TrailMode};
pub use self::values::AttributeWorld;
pub(crate) use self::values::cost_resource;
use self::values::{
    MISSILE_SPEED_CAP, PERCENT, ROUND, SPEED_CAP, add_short, add_sprite, class_amount, handicapped,
    id_of, insert_damage_graphic, insert_train_location, low8, low16, multiply_short,
    resize_damage_graphics, resize_train_locations, select_entry, set_sprite,
};
#[cfg(test)]
use crate::unit_ai::task_finder::{Actor, Diplomacy, TaskTarget};

/// The task types the action list's factory makes an action of: the TRIBE factory's
/// (`FUN_140aca4b0`) and the RGE factory's it falls back on (`FUN_140a97e00`). A default task of
/// any other type starts nothing (wild animals' Explore, type 6).
pub const ACTION_FACTORY_TYPES: [ActionType; 28] = action_type::types([
    3, 0xc, 0xe, 0x65, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6e, 0x6f, 0x78, 0x79, 0x7a, 0x7b, 0x7d,
    0x83, 0x84, 0x87, 0x88, 0x95, 0x9a, 4, 5, 7, 10, 0xd, 0x15,
]);

pub mod action_type;
mod adjacent_mode;
mod attack_priority;
pub mod attribute;
mod blast_level;
mod buildable_on;
mod carry_capacity;
mod charge_event;
mod charge_target;
mod charge_type;
pub mod class;
mod class_id;
mod class_mask;
mod combat_ability;
mod damage_amount;
mod damage_class;
mod damage_share;
mod damage_sprite;
mod entries;
mod fly_mode;
mod fog_visibility;
mod formation_class;
mod garrison_capacity;
mod garrison_kinds;
mod gather_group;
mod ghost_mode;
mod hero_flags;
mod hill_mode;
mod hit_chance;
mod id_field;
pub mod ids;
mod interaction_mode;
mod interface_kind;
mod invulnerability;
mod master;
mod master_id;
mod master_kind;
pub mod master_type;
mod minimap_mode;
mod object_flags;
mod obstruction_class;
mod obstruction_type;
mod occlusion;
mod placement_terrains;
mod run_pattern;
mod selection_effect;
mod sound_events;
mod special_attack;
mod task;
mod task_flags;
mod task_group;
mod task_sprites;
mod volley_spread;
mod work_value;

pub use self::action_type::ActionType;
pub use self::adjacent_mode::AdjacentMode;
pub use self::attack_priority::AttackPriority;
pub use self::attribute::MasterAttribute;
pub use self::blast_level::{BlastDefense, BlastLevel};
pub use self::buildable_on::BuildableOn;
pub use self::carry_capacity::CarryCapacity;
pub use self::charge_event::ChargeEvent;
pub use self::charge_target::ChargeTarget;
pub use self::charge_type::ChargeType;
pub use self::class_id::ClassId;
pub use self::class_mask::ClassMask;
pub use self::combat_ability::CombatAbility;
pub use self::conversion_adjust::ConversionAdjust;
pub use self::damage_amount::DamageAmount;
pub use self::damage_class::{ClassAmount, DamageClass};
pub use self::damage_share::DamageShare;
pub use self::damage_sprite::{DamageGraphic, DamageSpriteMode, HpPercent};
pub use self::entries::{DamageGraphicEntry, TrainLocationEntry};
pub use self::fly_mode::FlyMode;
pub use self::fog_visibility::FogVisibility;
pub use self::formation_class::FormationClass;
pub use self::garrison_capacity::GarrisonCapacity;
pub use self::garrison_kinds::GarrisonKinds;
pub use self::gather_group::GatherGroup;
pub use self::ghost_mode::GhostMode;
pub use self::hero_flags::HeroFlags;
pub use self::hill_mode::HillMode;
pub use self::hit_chance::HitChance;
pub use self::id_field::{MasterField, RestrictionField};
pub use self::interaction_mode::InteractionMode;
pub use self::interface_kind::InterfaceKind;
pub use self::invulnerability::InvulnerabilityLevel;
pub use self::master::BuildingMaster;
pub use self::master::Master;
pub use self::master::Masters;
pub use self::master::Shown;
pub use self::master::TrainLocation;
pub use self::master_id::MasterId;
pub use self::master_kind::MasterKind;
pub use self::minimap_mode::MinimapMode;
pub use self::object_flags::ObjectFlags;
pub use self::obstruction_class::ObstructionClass;
pub use self::obstruction_type::ObstructionType;
pub use self::occlusion::Occlusion;
pub use self::placement_terrains::PlacementTerrains;
pub use self::run_pattern::RunPattern;
pub use self::selection_effect::SelectionEffect;
pub use self::sound_events::{SoundEventId, SoundEvents};
pub use self::special_attack::SpecialAttack;
pub use self::task::Task;
pub use self::task_flags::TaskFlags;
pub use self::task_group::TaskGroup;
pub use self::task_sprites::TaskSprites;
pub use self::volley_spread::VolleySpread;
pub use self::work_value::WorkValue;

#[cfg(test)]
mod tests;
