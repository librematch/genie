//! What the engine reads of a player's masters: the tasks a unit can do and the masters a unit
//! changes into to do them, and how an effect changes a master: the attribute setters (slots
//! 0x18, 0x20 and 0x28 of every master type, [`Master::set_attribute`]), the master copy of an
//! upgrade (slot 8, [`Master::copy_from`]) and the values the slice derives from a master
//! ([`follow`]). The values are game data: the player's masters in the save, with the
//! `.dat`'s values for what the save does not carry.

use genie_model::objects::masters::animated_part;
use genie_model::objects::masters::combat_level;
use genie_model::objects::masters::conversion_adjust;
use genie_model::objects::masters::cost_amount;
use genie_model::objects::masters::cost_slot;
mod derived;
mod lookup;
use genie_model::objects::masters::missile_part;
use genie_model::objects::masters::moving_part;
use genie_model::objects::masters::spacing_modifier;
use genie_model::objects::masters::target_owner;
use genie_model::objects::masters::task_id;
use genie_model::objects::masters::task_index;
use genie_model::objects::masters::task_terrain;
use genie_model::objects::masters::trail;
use genie_model::objects::masters::values;

pub use self::animated_part::AnimatedPart;
pub use self::combat_level::CombatLevel;
pub use self::cost_amount::CostAmount;
pub use self::cost_slot::{COST_SLOTS, CostSlot, entries as cost_entries};
pub use self::derived::{follow, follow_moving};
pub use self::lookup::{MasterTables, OwnerMasters, PlayersMasters, player_master, work_master};
pub use self::missile_part::MissilePart;
pub use self::moving_part::MovingPart;
pub use self::spacing_modifier::SpacingModifier;
pub use self::target_owner::TargetOwner;
pub use self::task_id::TaskId;
pub use self::task_index::TaskIndex;
pub use self::task_terrain::TaskTerrain;
pub use self::trail::{Trail, TrailDensity, TrailMode};
pub use self::values::AttributeWorld;
pub(crate) use self::values::cost_resource;
#[cfg(test)]
use crate::unit_ai::task_finder::world::TaskWorld;
#[cfg(test)]
use crate::unit_ai::task_finder::{Actor, TaskTarget};

/// The task types the action list's factory makes an action of: the TRIBE factory's
/// (`FUN_140aca4b0`) and the RGE factory's it falls back on (`FUN_140a97e00`). A default task of
/// any other type starts nothing (wild animals' Explore, type 6).
pub const ACTION_FACTORY_KINDS: [ActionKind; 28] = kinds([
    3, 0xc, 0xe, 0x65, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6e, 0x6f, 0x78, 0x79, 0x7a, 0x7b, 0x7d,
    0x83, 0x84, 0x87, 0x88, 0x95, 0x9a, 4, 5, 7, 10, 0xd, 0x15,
]);

pub use genie_model::objects::masters::action_kind;
use genie_model::objects::masters::action_kind::kinds;
use genie_model::objects::masters::adjacent_mode;
use genie_model::objects::masters::attack_priority;
pub use genie_model::objects::masters::attribute;
use genie_model::objects::masters::blast_level;
use genie_model::objects::masters::buildable_on;
use genie_model::objects::masters::carry_capacity;
use genie_model::objects::masters::charge_event;
use genie_model::objects::masters::charge_kind;
use genie_model::objects::masters::charge_target;
use genie_model::objects::masters::class_id;
mod class_mask;
use genie_model::objects::masters::action_part;
use genie_model::objects::masters::auto_gather_mode;
use genie_model::objects::masters::availability;
use genie_model::objects::masters::built_mode;
use genie_model::objects::masters::combat_ability;
use genie_model::objects::masters::combat_part;
use genie_model::objects::masters::creatable_part;
use genie_model::objects::masters::damage_amount;
use genie_model::objects::masters::damage_class;
use genie_model::objects::masters::damage_share;
use genie_model::objects::masters::damage_sprite;
use genie_model::objects::masters::disable_mode;
use genie_model::objects::masters::entries;
use genie_model::objects::masters::fly_mode;
use genie_model::objects::masters::fog_visibility;
use genie_model::objects::masters::formation_class;
use genie_model::objects::masters::garrison_capacity;
use genie_model::objects::masters::garrison_kinds;
use genie_model::objects::masters::gather_group;
use genie_model::objects::masters::ghost_mode;
use genie_model::objects::masters::hero_flags;
use genie_model::objects::masters::hill_mode;
use genie_model::objects::masters::hit_chance;
use genie_model::objects::masters::id_field;
use genie_model::objects::masters::interaction_mode;
use genie_model::objects::masters::interface_kind;
use genie_model::objects::masters::invulnerability;
use genie_model::objects::masters::master;
use genie_model::objects::masters::master_id;
use genie_model::objects::masters::master_kind;
use genie_model::objects::masters::minimap_mode;
use genie_model::objects::masters::object_flags;
use genie_model::objects::masters::obstruction_class;
use genie_model::objects::masters::obstruction_kind;
use genie_model::objects::masters::occlusion;
use genie_model::objects::masters::placement_terrains;
use genie_model::objects::masters::recycle_mode;
use genie_model::objects::masters::run_pattern;
use genie_model::objects::masters::selection_effect;
use genie_model::objects::masters::sound_events;
use genie_model::objects::masters::special_attack;
use genie_model::objects::masters::task;
use genie_model::objects::masters::task_flags;
use genie_model::objects::masters::task_group;
use genie_model::objects::masters::task_sprites;
use genie_model::objects::masters::volley_spread;
use genie_model::objects::masters::work_value;

pub use self::action_kind::ActionKind;
pub use self::action_part::ActionPart;
pub use self::adjacent_mode::AdjacentMode;
pub use self::attack_priority::AttackPriority;
pub use self::attribute::MasterAttribute;
pub use self::auto_gather_mode::AutoGatherMode;
pub use self::availability::Availability;
pub use self::blast_level::{BlastDefense, BlastLevel};
pub use self::buildable_on::BuildableOn;
pub use self::built_mode::BuiltMode;
pub use self::carry_capacity::CarryCapacity;
pub use self::charge_event::ChargeEvent;
pub use self::charge_kind::ChargeKind;
pub use self::charge_target::ChargeTarget;
pub use self::class_id::ClassId;
pub use self::class_mask::ClassMask;
pub use self::combat_ability::CombatAbility;
pub use self::combat_part::CombatPart;
pub use self::conversion_adjust::ConversionAdjust;
pub use self::creatable_part::CreatablePart;
pub use self::damage_amount::DamageAmount;
pub use self::damage_class::{ClassAmount, DamageClass};
pub use self::damage_share::DamageShare;
pub use self::damage_sprite::{DamageGraphic, DamageSpriteMode, HpPercent};
pub use self::disable_mode::DisableMode;
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
pub use self::obstruction_kind::ObstructionKind;
pub use self::occlusion::Occlusion;
pub use self::placement_terrains::PlacementTerrains;
pub use self::recycle_mode::RecycleMode;
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
