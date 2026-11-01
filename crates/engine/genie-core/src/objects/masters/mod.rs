//! What the engine reads of a player's masters: the tasks a unit can do and the masters a unit
//! changes into to do them, and how an effect changes a master: the attribute setters (slots
//! 0x18, 0x20 and 0x28 of every master type, [`Master::set_attribute`]), the master copy of an
//! upgrade (slot 8, [`Master::copy_from`]) and the values the slice derives from a master
//! ([`Master::follow`]). The values are game data: the player's masters in the save, with the
//! `.dat`'s values for what the save does not carry.

use crate::GenieError;
use crate::combat::Charge;
use crate::combat::ChargeRate;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GroundOffset;
use crate::map::Offset;
use crate::map::TerrainId;
use crate::movement::MoveStats;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::SpriteId;
use crate::objects::WorkRate;
use crate::players::Amount;
use crate::players::attribute::AttributeId;
use crate::technology::TechId;
use crate::unit_ai::UnitAiType;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use genie_diff::Diff;

mod combat_level;
mod combat_setters;
mod conversion_adjust;
mod cost_slot;
mod derived;
mod dispatch;
mod lookup;
mod static_setters;
mod target_owner;
mod task_id;
mod task_index;
mod task_terrain;
mod tribe_setters;
mod values;

pub use self::combat_level::CombatLevel;
pub use self::cost_slot::{COST_SLOTS, CostSlot, entries as cost_entries};
pub use self::lookup::{PlayersMasters, player_master};
pub use self::target_owner::TargetOwner;
pub use self::task_id::TaskId;
pub use self::task_index::TaskIndex;
pub use self::task_terrain::TaskTerrain;
pub use self::values::AttributeWorld;
pub(crate) use self::values::cost_resource;
use self::values::{
    MISSILE_SPEED_CAP, PERCENT, ROUND, SPEED_CAP, add_short, add_sprite, class_amount, handicapped,
    id_of, insert_damage_graphic, insert_train_location, low8, low16, multiply_short,
    resize_damage_graphics, resize_train_locations, select_entry, set_sprite,
};
#[cfg(test)]
use crate::unit_ai::task_finder::{Actor, Diplomacy, TaskTarget};

/// One task of a master's task list (`RGE_Task`), with the memory offsets the task finder reads
/// (the save's names; the `.dat`'s in brackets where they differ).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Task {
    /// `+0xa`.
    pub id: TaskId,
    /// `+0xe`: the action the task makes the unit do.
    pub action_type: ActionType,
    /// `+0x10`: the target class the task is for, none (−1) for any.
    pub object_group: Option<ClassId>,
    /// `+0x12`: the target master, -1 for any.
    pub object_id: Option<MasterId>,
    /// `+0x14`, `combat_level` (`enable_targeting`): the task is for a target's combat level.
    pub combat_level: u8,
    /// `+0x15`, `combat_level_flag`: a generation task (151, [`crate::combat::abilities`]) of a
    /// combat unit with [`TaskFlags::UNITS_ALWAYS`] or of a building with
    /// [`TaskFlags::BUILDINGS_ALWAYS`] works on its own; a removal reward task (161) with any
    /// flag pays the master's cost of its resource times the rate.
    pub combat_level_flag: TaskFlags,
    /// `+0x16`: the terrain the task is for, -1 for none.
    pub terrain: TaskTerrain,
    /// `+0x18`, `owner_type` (`target_diplomacy`): whose targets.
    pub owner_type: TargetOwner,
    /// `+0x19`, `holding_attr` (`carry_check`): 1 wants a target holding the task's resource.
    pub holding_attr: u8,
    /// `+0x20` to `+0x26`, `attribute_type` to `attribute_type4` (`resource_in`,
    /// `resource_multiplier`: the attribute that scales the gathering, `resource_out`,
    /// `unused_resource`).
    pub attribute_type: Option<AttributeId>,
    /// `+0x22`, `attribute_type2` (`resource_multiplier`): the attribute that scales the gathering.
    pub attribute_type2: Option<AttributeId>,
    /// `+0x24`, `attribute_type3` (`resource_out`).
    pub attribute_type3: Option<AttributeId>,
    /// `+0x26`, `attribute_type4` (`unused_resource`).
    pub attribute_type4: Option<AttributeId>,
    /// `+0x1c`, `search_wait_time`: a Fly task's rest between flights.
    pub wait_time: Seconds,
    /// `+0x28`, the save's `work_val1`; `+0x2c`, `work_val2`.
    pub work_val1: f32,
    /// `+0x2c`, the save's `work_val2`.
    pub work_val2: f32,
    /// `+0x30`: how near the target the work is done.
    pub work_range: Distance,
    /// `+0x34`, `work_flag` (the `.dat`'s `gather_type`): a Make adds to its player's attribute,
    /// not to its object's holding ([`crate::gathering::make`]).
    pub work_flag: i16,
    /// `+0x38`, `enablement_resource_id` (`enabled`): the resource that enables the task, -1 none.
    pub enablement: Option<AttributeId>,
    /// `+0x40`..`+0x58`: the moving, proceeding, working and carrying sprites.
    pub sprites: TaskSprites,
    /// `+0x60` and `+0x78`: the event ids (`mEventId`) of the gathering and the deposit sound,
    /// as the task's load (`FUN_141062470`) leaves them; a terrain look task (159) names its
    /// dying and its undead sprite with them ([`crate::combat::abilities`]).
    pub sound_events: SoundEvents,
}

impl Task {
    /// The task is for objects of class `class`: the game compares its `+0x10` short with the
    /// class, so a task for any class (−1) is for a class of −1 too.
    #[must_use]
    pub const fn for_class(&self, class: ClassId) -> bool {
        ClassId::field(self.object_group) == class.get()
    }
}

/// A task for any target, with no resource, terrain or requirement.
impl Default for Task {
    fn default() -> Self {
        Self {
            id: TaskId::default(),
            action_type: ActionType::default(),
            object_group: None,
            object_id: None,
            combat_level: 0,
            combat_level_flag: TaskFlags::NONE,
            terrain: TaskTerrain::NONE,
            owner_type: TargetOwner::ANY,
            holding_attr: 0,
            attribute_type: None,
            attribute_type2: None,
            attribute_type3: None,
            attribute_type4: None,
            wait_time: Seconds::ZERO,
            work_val1: 0.0,
            work_val2: 0.0,
            work_range: Distance::ZERO,
            work_flag: 0,
            enablement: None,
            sprites: TaskSprites::NONE,
            sound_events: SoundEvents::new(0, 0),
        }
    }
}

/// The task types the action list's factory makes an action of: the TRIBE factory's
/// (`FUN_140aca4b0`) and the RGE factory's it falls back on (`FUN_140a97e00`). A default task of
/// any other type starts nothing (wild animals' Explore, type 6).
pub const ACTION_FACTORY_TYPES: [ActionType; 28] = action_type::types([
    3, 0xc, 0xe, 0x65, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6e, 0x6f, 0x78, 0x79, 0x7a, 0x7b, 0x7d,
    0x83, 0x84, 0x87, 0x88, 0x95, 0x9a, 4, 5, 7, 10, 0xd, 0x15,
]);

impl Master {
    /// Its default task (`FUN_14109e0c0`: the task list's entry at the default task's index,
    /// `+0x788`), if it has one.
    #[must_use]
    pub fn default_task(&self) -> Option<&Task> {
        self.tasks.get(self.default_task?.index())
    }

    /// What a new object of this master starts once its action list is empty in state 2
    /// (`RGE_Action_Object::update`, slot 0x7f0): a Make ([`crate::gathering::make`]: true), or the
    /// default task's action the slice does not model; nothing for a type the factory makes no
    /// action of.
    #[must_use]
    pub fn default_start(&self) -> (bool, Option<GenieError>) {
        match self.default_task() {
            Some(t) if t.action_type == crate::gathering::make::MAKE => (true, None),
            Some(t) if ACTION_FACTORY_TYPES.contains(&t.action_type) => (
                false,
                Some(GenieError::unsupported(
                    "a new object's default task other than a Make",
                )),
            ),
            _ => (false, None),
        }
    }

    /// The unit AI type the factory (`FUN_141215a90`) gives a new object of this master, by
    /// the master's id `id` ([`crate::unit_ai::factory_type`]).
    #[must_use]
    pub const fn unit_ai_type(&self, id: MasterId) -> Option<UnitAiType> {
        crate::unit_ai::factory_type(self.class, id, self.volley_missile, self.combat_level)
    }
}

/// What a building's completion (`FUN_1411dbb70` with 2) reads of its master, the `.dat`'s type
/// 80 values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct BuildingMaster {
    /// `+0x980`, `adjacent_mode`: walls and gates join their neighbours (`FUN_1411ce890`).
    pub adjacent_mode: AdjacentMode,
    /// `+0x984`, `disappears_when_built`.
    pub disappears_when_built: bool,
    /// `+0x986`, `stack_unit_id`: the building becomes another (−1 for none).
    pub stack_unit: Option<MasterId>,
    /// `+0x988`, `foundation_terrain_id` (the save's `on_build_make_tile`), none for −1.
    pub foundation_terrain: Option<TerrainId>,
    /// `+0x98c`, `tech_id`: the technology its completion researches (−1 for none).
    pub tech: Option<TechId>,
    /// `+0x9bc`, `garrison_type`: the kinds of units it takes in (`FUN_14120eb10`).
    pub garrison_type: GarrisonKinds,
    /// `+0x9c0`, `garrison_heal_rate`: the hit points a second it gives the units inside.
    pub garrison_heal_rate: HitPoints,
    /// `+0x9c4`, `garrison_repair_rate`: its repair a second per villager inside.
    pub garrison_repair_rate: HitPoints,
    /// `+0x990`, the masters of its annexes (−1 for none; their places beside it at `+0x998`).
    pub annexes: [Option<MasterId>; 4],
    /// `+0x998` (x) and `+0x9a8` (y), each annex slot's place off the building's (the `.dat`'s
    /// building annexes' `misplacement`), which the building constructor (`FUN_1411de5f0`) makes
    /// its annexes at.
    pub annex_offsets: [GroundOffset; 4],
    /// `+0x9b8`, `head_unit`: the building it is an annex of (−1 for none).
    pub head_unit: Option<MasterId>,
    /// `+0x9ba`, `transform_unit`: the master it transforms into (−1 for none); a gate with one
    /// is in its player's gate list (`+0x1360`, `FUN_141616200`).
    pub transform_unit: Option<MasterId>,
    /// `+0x960`, the `.dat`'s `destruction_graphic_id` (the save's third building sprite): the
    /// sprite a building that dies before it is built shows (`RGE_Static_Object::update`), −1
    /// for none.
    pub destruction_sprite: Option<SpriteId>,
    /// `+0x950`, the `.dat`'s `construction_graphic_id` (the save's first building sprite): the
    /// sprite a building under construction shows, −1 for none.
    pub construction_sprite: Option<SpriteId>,
}

/// The master values the task finder and the work command read.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct Master {
    /// The master's id.
    pub id: MasterId,
    /// The master's type (`+0x8`): 70 for a combat unit, 80 (`'P'`) for a building.
    pub kind: MasterKind,
    /// `+0x158`, the `.dat`'s `copy_id`: the master a garrison order asks a building about
    /// (`FUN_141621220`).
    pub copy_id: Option<MasterId>,
    /// `+0x260`, the `.dat`'s `base_id` (the master load `FUN_1411319f0` turns 768 into 420 and
    /// 770 into 691): the master whose availability says whether the player can make this one
    /// (`FUN_141663f80`, [`crate::world::World::can_make`]); an upgrade's copy keeps it.
    pub base_id: Option<MasterId>,
    /// `+0x410`.
    pub class: ClassId,
    /// `+0x7b8`, the `.dat`'s `TaskSwapGroup` (the save does not carry it); 0 for none.
    pub task_by_group: TaskGroup,
    /// `radius_x`, `radius_y` (`+0x5d8`, `+0x5dc`).
    pub radius: Footprint,
    /// `radius_z` (`+0x5e0`) is above 0: its objects count as tall on their tiles.
    pub tall: bool,
    /// `radius_z` (`+0x5e0`): how high a missile may pass over its objects and hit them
    /// (`FUN_14113f740`).
    pub height: f32,
    /// `line_of_sight` (`+0x5d0`).
    pub line_of_sight: Distance,
    /// `+0x606`, its terrain restriction (the save's `terrain_restriction_mask_id`; −1 for none),
    /// which a placement test reads.
    pub terrain_restriction: Option<crate::map::RestrictionId>,
    /// `obstruction_type` (`+0x651`) and `can_be_built_on` (`+0x5e9`).
    pub obstruction_type: ObstructionType,
    /// `can_be_built_on` (`+0x5e9`): a new building deletes an object of it off its footprint.
    pub buildable_on: u8,
    /// `obstruction_class` (`+0x652`).
    pub obstruction_class: ObstructionClass,
    /// `object_flags` (`+0x654`): its traits and the trait piece ([`ObjectFlags`]).
    pub object_flags: ObjectFlags,
    /// The `.dat`'s `combat_level` (`+0x62c`).
    pub combat_level: CombatLevel,
    /// Its task list.
    pub tasks: Vec<Task>,
    /// `+0x790`, `default_task`: the id of the task its object's empty action list starts in
    /// state 2 (`RGE_Action_Object::update`, slot 0x7f0); None for the game's −1.
    pub default_task: Option<TaskIndex>,
    /// `+0x880`, the cost slots in their places (an empty one among them).
    pub cost: [CostSlot; COST_SLOTS],
    /// `work_rate` (`+0x798`): what a builder of this master adds to the work per second.
    pub work_rate: WorkRate,
    /// `search_radius` (`+0x794`, the object's slot 0x2c8).
    pub search_radius: Distance,
    /// The `.dat`'s `drop_sites` (`+0x7a0`..`+0x7a8`, `FUN_14109d4f0`): the buildings a unit of
    /// this master takes its load to.
    pub drop_sites: Vec<MasterId>,
    /// The hit points (`+0x530`) and the build time: the first train location's train time
    /// (`+0x898`'s first entry), 0 without one.
    pub hit_points: MasterHitPoints,
    /// The build time: the first train location's train time, 0 without one.
    pub build_time: WholeSeconds,
    /// The train locations' ids (`+0x898`, each entry's first short): the units that build a
    /// building, the buildings that train a unit (`FUN_141263610`).
    pub trained_at: Vec<MasterId>,
    /// The combat master's `weapon_range` (`+0x828`, object slot 0x230) and
    /// `minimum_weapon_range` (`+0x850`, slot 0x240).
    pub weapon_range: Distance,
    /// `minimum_weapon_range` (`+0x850`, slot 0x240).
    pub min_range: Distance,
    /// `charge_type` (`+0x922`): 6 and 7 add `charge_event` (`+0x920`) to the range against some
    /// targets (slot 0x2f8).
    pub charge_type: ChargeType,
    /// `+0x61c`, `attribute_max_amount`: how much an object of this master carries.
    pub capacity: CarryCapacity,
    /// `+0x8bc`, `mFormationCategory`: 0 (villagers) keeps a unit out of the groups an order's
    /// splitter forms (`FUN_140fd76d0`).
    pub formation_category: FormationClass,
    /// `+0x8d9`, `hero_flag`: a combat master it grants
    /// [`HeroFlags::PROTECTED_FORMATION`] joins a group's fifth sub-formation (`FUN_141011300`).
    pub hero_flag: HeroFlags,
    /// `+0x8b8`, `mSpacingModifier`: with the radius on x the unit's radius in a group's
    /// formation (`FUN_141025920`). The player's masters copy it from the data file's.
    pub spacing_modifier: f32,
    /// `+0x688`, `+0x68c`: the `.dat`'s `outline_size` (x, y), an obstruction-type-11 object's
    /// shape radii (`FUN_141054fe0`).
    pub outline: Footprint,
    /// `+0x690`: the `.dat`'s `outline_size` z, the height BUILD's scan takes for an object of
    /// obstruction types 11 to 13 (`FUN_14123aee0`).
    pub outline_height: f32,
    /// `+0x5fc`, `clearance_size`: a completed building's foundation terrain covers its position
    /// plus and minus this.
    pub clearance: Footprint,
    /// `+0x820`, the `.dat`'s `defense_terrain_bonus` (a terrain restriction): a completed
    /// building keeps to restore a tile's terrain whose value in it is neither 0 nor 1.
    pub terrain_bonus: RestrictionField,
    /// What a building's completion reads (type 80), None for other masters.
    pub building: Option<BuildingMaster>,
    /// The resource storage (`attribute_type_held`, `attribute_amount_held`, `attribute_flag`:
    /// `+0x60a`, `+0x610`, `+0x628`): what an object of this master adds to its player's
    /// attributes, by its flag.
    pub storage: Vec<(Option<AttributeId>, Amount, u8)>,
    /// The master has damage sprites (`+0x638`..`+0x640`): a master change checks them
    /// (`FUN_1411859a0`).
    pub damage_sprites: bool,
    /// The `.dat`'s `fly_mode` (`+0x608`): 1 flies above the ground (a bird's Fly lifts it).
    pub fly_mode: FlyMode,
    /// What a hit on an object of this master reads (`FUN_141082530`): its armor classes (class,
    /// amount; `+0x808`, count `+0x802`), its base armor for a class it does not list (`+0x800`),
    /// `bonus_damage_resistance` (`+0x824`), `combat_ability` (`+0x840`), `damage_reflection`
    /// (`+0x864`), `invulnerability_level` (`+0x930`: a hit point floor, `FUN_141202a80`) and
    /// `special_attack_flag` (`+0x8d8`), `friendly_fire_damage` (`+0x868`), its special graphic
    /// (`+0x8e8`, the `.dat`'s `special_graphic`) and `area_effect_object_level` (`+0x62b`, the
    /// blasts that reach it); and what an attacker of it reads: `area_effect_range` (`+0x82c`),
    /// `area_effect_level` (`+0x830`), `area_damage` (`+0x834`), `max_charge` (`+0x918`) and
    /// `charge_target` (`+0x924`).
    pub base_armor: i16,
    /// Its armor classes (class, amount; `+0x808`, count `+0x802`).
    pub armors: Vec<ClassAmount>,
    /// `bonus_damage_resistance` (`+0x824`).
    pub resistance: f32,
    /// `combat_ability` (`+0x840`).
    pub combat_ability: CombatAbility,
    /// `damage_reflection` (`+0x864`).
    pub reflection: f32,
    /// `invulnerability_level` (`+0x930`): a hit point floor.
    pub invulnerability: f32,
    /// `attack_priority` (`+0x92c`, `FUN_141202220`): 3 attacks only the classes of
    /// `FUN_140fddf10`.
    pub attack_priority: AttackPriority,
    /// `special_attack_flag` (`+0x8d8`).
    pub special_attack: SpecialAttack,
    /// `friendly_fire_damage` (`+0x868`).
    pub friendly_fire: f32,
    /// It has a special graphic (`+0x8e8`).
    pub special_graphic: bool,
    /// The special graphic's sprite (the `.dat`'s `special_graphic`), −1 for none: a herder
    /// leading livestock shows it.
    pub special_sprite: Option<SpriteId>,
    /// `area_effect_object_level` (`+0x62b`): the blasts that reach it.
    pub blast_defense: BlastDefense,
    /// `area_effect_range` (`+0x82c`): its blast's radius.
    pub area_effect: Distance,
    /// `area_effect_level` (`+0x830`).
    pub blast_level: BlastLevel,
    /// `area_damage` (`+0x834`).
    pub area_damage: f32,
    /// `max_charge` (`+0x918`).
    pub max_charge: Charge,
    /// `charge_target` (`+0x924`).
    pub charge_target: ChargeTarget,
    /// `+0x5d4`, `obj_max` (the `.dat`'s `garrison_capacity`): a building that garrisons takes a
    /// gather point (`FUN_140e7e9a0`).
    pub garrison: GarrisonCapacity,
    /// `+0x680`, the `.dat`'s `recyclable`: its objects take ids from the negative range
    /// (`FUN_1415301b0`).
    pub recyclable: bool,
    /// `+0x681`, `enable_auto_gather`: an object of it blocking a spawn point stays
    /// (`FUN_141194170`).
    pub auto_gather: bool,
    /// `+0x683`, `resource_gather_group`: the resource kind a gatherer's search
    /// (`ClosestResourceObject`) takes it for ([`GatherGroup`]).
    pub gather_group: GatherGroup,
    /// `+0x604`, `hill_mode`: how a placement test checks the elevation.
    pub hill_mode: HillMode,
    /// `+0x605`, `fog_visibility`: an object of it the player only explored stays a target
    /// (the Attack's update).
    pub fog_visibility: FogVisibility,
    /// `+0x5f0`, `available`: the player may make it (an effect's enable command, kind 2,
    /// changes it).
    pub available: bool,
    /// `+0x5f1`, `disabled`: an enable command leaves `available` as it is.
    pub disabled: bool,
    /// `+0x682`, `create_doppelganger_on_death`: 2 leaves ghosts of an object of it for the
    /// players who explored its tile but do not see it, at its death and once they lose sight
    /// of it ([`crate::vision::ghost`]).
    pub ghost_on_death: GhostMode,
    /// The placement terrains (`+0x5f2`..`+0x5f8`).
    pub placement: PlacementTerrains,
    /// The death sprite (`+0x480`, −1 for none) and the death spawns (`+0x5e4`, `+0x5e6`, −1 for
    /// none), and what a dead object's steps read of the master
    /// ([`crate::objects::lifecycle::MasterData`]), for a new object's [`crate::objects::lifecycle::Lifecycle`].
    pub death_sprite: Option<SpriteId>,
    /// The death spawns (`+0x5e4`, `+0x5e6`, −1 for none).
    pub death_spawns: [Option<MasterId>; 2],
    /// What a dead object's steps read of the master.
    pub life: Option<crate::objects::lifecycle::MasterData>,
    /// What a combat master's attack reads: its standing sprite (`+0x470`), its attack sprite
    /// (`+0x7f8`, `fight_sprite_id`), a second attack sprite
    /// (`+0x878`, the `.dat`'s `attack_graphic2`), `missile_id`, `fire_missile_at_frame`,
    /// `interrupt_frame`, `base_hit_chance`, `speed_of_attack` (the reload time) and its weapons
    /// (class, amount); and the sprite it waits with between swings (`+0x910`, the `.dat`'s
    /// `idle_attack_graphic`, −1 for none).
    pub standing_sprite: Option<SpriteId>,
    /// Its second standing sprite (`+0x478`, the `.dat`'s `standing_graphic[1]`), whose idle
    /// animation the object swaps in (`0x141071880`); None for none.
    pub standing_sprite_2: Option<SpriteId>,
    /// The sprite it waits with between swings (`+0x910`, −1 for none).
    pub idle_attack_sprite: Option<SpriteId>,
    /// Its walking sprite (`+0x748`, `move_sprite_id`; the unit's slot 0x7c8) and its running
    /// one (`+0x750`, `run_sprite_id`; slot 0x7d0), −1 for none.
    pub move_sprite: Option<SpriteId>,
    /// Its running sprite (`+0x750`), −1 for none.
    pub run_sprite: Option<SpriteId>,
    /// Its attack sprite (`+0x7f8`, `fight_sprite_id`).
    pub attack_sprite: Option<SpriteId>,
    /// A second attack sprite (`+0x878`).
    pub attack_sprite2: Option<SpriteId>,
    /// `missile_id`, −1 for none: every short the effects leave in it is kept.
    pub missile: MasterField,
    /// `fire_missile_at_frame`.
    pub fire_frame: i16,
    /// `interrupt_frame`.
    pub interrupt_frame: i16,
    /// `base_hit_chance`.
    pub hit_chance: HitChance,
    /// `speed_of_attack`: the reload time.
    pub reload: Seconds,
    /// Its weapons (class, amount).
    pub weapons: Vec<ClassAmount>,
    /// Where its missile starts from its place, turned by its facing (`+0x844`..`+0x84c`,
    /// `weapon_offset`), and how far a missed missile strays per tile of the way
    /// (`missed_missile_spread`, `+0x860`).
    pub weapon_offset: Offset,
    /// How far a missed missile strays per tile of the way (`missed_missile_spread`, `+0x860`).
    pub missed_spread: f32,
    /// `garrison_firepower` (`+0x870`): what a unit adds to the volleys of a building it is
    /// inside; one at most 0 adds its magnitude.
    pub garrison_firepower: f32,
    /// `volley_fire_amount` (`+0x8c0`, the `.dat`'s total projectiles): a new object's missiles
    /// per volley (`+0xa00`).
    pub volley_fire: f32,
    /// `max_attacks_in_volley` (`+0x8c4`, the `.dat`'s max total projectiles).
    pub max_volley: u8,
    /// `volley_x_spread`, `volley_y_spread` and `volley_start_spread_adjustment` (`+0x8c8`..
    /// `+0x8d0`, the `.dat`'s projectile spawning area): how far a volley's missiles scatter.
    pub volley_spread: VolleySpread,
    /// `volley_missile_id` (`+0x8d4`, the `.dat`'s secondary projectile), None for −1.
    pub volley_missile: Option<MasterId>,
    /// `mChargeProjectileUnit` (`+0x928`), None for −1.
    pub charge_projectile: Option<MasterId>,
    /// A missile master's flight (type 60, [`crate::combat::missile::MissileStats`]).
    pub missile_stats: Option<crate::combat::missile::MissileStats>,
    /// What a unit of this master moves with, its speed with the master's walking sprite (the
    /// run sprite `+0x750` if it has one, else the move sprite `+0x748`); None for a master that
    /// does not move.
    pub moving: Option<MoveStats>,
    /// `+0x740`, `mMaxSpeed` (the `.dat`'s `speed`): the top speed before a sprite's factor, from
    /// which [`Master::moving`]'s speeds and [`crate::combat::missile::MissileStats::speed`] follow.
    pub max_speed: Speed,
    /// `+0x758`, `turn_speed` (the `.dat`'s `rotation_speed`), which [`MoveStats::turn_time`]
    /// follows.
    pub turn_speed: Seconds,
    /// `+0x488`, the undead sprite (`undeadSpriteId`, −1 for none).
    pub undead_sprite: Option<SpriteId>,
    /// `+0x650`, `occlusion_flag` (the `.dat`'s `occlusion_mode`).
    pub occlusion: Occlusion,
    /// `+0x684`, the `.dat`'s `selection_effect`.
    pub selection_effect: SelectionEffect,
    /// `+0x62f`, `unit_level` (the `.dat`'s `interface_kind`).
    pub unit_level: InterfaceKind,
    /// `+0x62d`, `select_level` (the `.dat`'s `interaction_mode`).
    pub select_level: InteractionMode,
    /// `+0x62e`, `map_draw_level` (the `.dat`'s `minimap_mode`).
    pub map_draw_level: MinimapMode,
    /// The damage sprites (`+0x638`, 16 bytes each; the save's `damage_sprite`), which
    /// [`Master::damage_sprites`] follows.
    pub damage_graphics: Vec<DamageGraphic>,
    /// `+0x731`, `mDamageSpriteEntryMod`: the damage sprite the attributes 91, 156 and 157 change.
    pub damage_graphic_entry: u8,
    /// A moving master's trail (`+0x75e`..`+0x764`).
    pub trail: Trail,
    /// `+0x7b9`, `run_pattern`.
    pub run_pattern: RunPattern,
    /// `+0x856`, `+0x858`, `+0x854` and `+0x8b0`: the attack, range, melee and pierce armor a
    /// combat master shows (`orig_weapon`, `orig_weapon_range`, `orig_armor`,
    /// `orig_pierce_armor`).
    pub shown: Shown,
    /// The train locations (`+0x898`, 12 bytes each; the save's `train_location`), which
    /// [`Master::trained_at`] and [`Master::build_time`] follow.
    pub train_locations: Vec<TrainLocation>,
    /// `+0x940`, `mTrainLocationEntryMod`: the train location the attributes 42, 43, 58 and 101
    /// change.
    pub train_location_entry: i16,
    /// `+0x8b4`, `regeneration_rate`.
    pub regeneration: f32,
    /// `+0x91c`, `mRechargeRate`.
    pub recharge_rate: ChargeRate,
    /// `+0x920`, `mChargeEvent`.
    pub charge_event: ChargeEvent,
    /// `+0x934`, `+0x938`, `+0x93c`: `mMinConversionTimeAdj`, `mMaxConversionTimeAdj`,
    /// `mConversionChanceAdj`.
    pub conversion: ConversionAdjust,
    /// `+0x908`, the hero glow sprite (−1 for none).
    pub hero_glow_sprite: Option<SpriteId>,
    /// `+0x8f0`, the garrison sprite (−1 for none).
    pub garrison_sprite: Option<SpriteId>,
    /// A building master's `+0x982`, `building_facet` (the attribute list's "Building Icon
    /// Override").
    pub building_facet: i16,
}

/// One damage sprite of a master (`+0x638`'s entries; the save's `damage_sprite`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Diff)]
pub struct DamageGraphic {
    /// `+0x0`, the sprite (`tempId`, −1 for none).
    pub sprite: Option<SpriteId>,
    /// `+0x8`, `damage_percent`.
    pub percent: u8,
    /// `+0x9`, the `flag` (the `.dat`'s `apply_mode`).
    pub apply_mode: u8,
}

/// A new damage sprite entry: no sprite, zero bytes (the game zeroes its 16 bytes).
impl Default for DamageGraphic {
    fn default() -> Self {
        Self {
            sprite: None,
            percent: 0,
            apply_mode: 0,
        }
    }
}

/// One train location of a combat master (`+0x898`'s 12-byte entries; the save's
/// `train_location`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct TrainLocation {
    /// `+0x0`, `trainLocationID`: the master that makes it.
    pub location: Option<MasterId>,
    /// `+0x2`, `trainTime`.
    pub time: WholeSeconds,
    /// `+0x4`, `buttonLocationID`.
    pub button: crate::production::LocationButton,
    /// `+0x8`, `hotkeyID`.
    pub hotkey: i32,
}

/// A moving master's trail (`obj_trail_id`, `obj_trail_options`, `obj_trail_spacing`; the
/// `.dat`'s tracking unit, its mode and density).
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct Trail {
    /// `+0x75e`: the master it leaves behind.
    pub unit: MasterField,
    /// `+0x760`.
    pub mode: u8,
    /// `+0x764`.
    pub density: f32,
}

/// What a combat master shows of its values (`+0x856`, `+0x858`, `+0x854`, `+0x8b0`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct Shown {
    /// `+0x856`, `orig_weapon`.
    pub attack: i16,
    /// `+0x858`, `orig_weapon_range`.
    pub range: Distance,
    /// `+0x854`, `orig_armor`.
    pub melee_armor: i16,
    /// `+0x8b0`, `orig_pierce_armor`.
    pub pierce_armor: i16,
}

/// A player's masters by id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Masters {
    by_id: Vec<Option<Master>>,
}

pub mod action_type;
mod adjacent_mode;
mod attack_priority;
pub mod attribute;
mod blast_level;
mod carry_capacity;
mod charge_event;
mod charge_target;
mod charge_type;
pub mod class;
mod class_id;
mod class_mask;
mod combat_ability;
mod damage_class;
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
mod task_flags;
mod task_group;
mod task_sprites;
mod volley_spread;

pub use self::action_type::ActionType;
pub use self::adjacent_mode::AdjacentMode;
pub use self::attack_priority::AttackPriority;
pub use self::attribute::MasterAttribute;
pub use self::blast_level::{BlastDefense, BlastLevel};
pub use self::carry_capacity::CarryCapacity;
pub use self::charge_event::ChargeEvent;
pub use self::charge_target::ChargeTarget;
pub use self::charge_type::ChargeType;
pub use self::class_id::ClassId;
pub use self::class_mask::ClassMask;
pub use self::combat_ability::CombatAbility;
pub use self::conversion_adjust::ConversionAdjust;
pub use self::damage_class::{ClassAmount, DamageClass};
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
pub use self::task_flags::TaskFlags;
pub use self::task_group::TaskGroup;
pub use self::task_sprites::TaskSprites;
pub use self::volley_spread::VolleySpread;

#[cfg(test)]
mod tests;
