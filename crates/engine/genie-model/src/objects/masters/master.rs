//! A master and the players' tables of them: the `.dat`'s unit records as the engine reads them (FD-046).

use crate::combat::Charge;
use crate::combat::ChargeRate;
use crate::combat::missile::{MissSpread, MissileStats};
use crate::combat::{Firepower, MissileCount};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GroundOffset;
use crate::map::Offset;
use crate::map::RestrictionId;
use crate::map::TerrainId;
use crate::movement::MoveStats;
use crate::movement::Speed;
use crate::objects::Graphics;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::WorkRate;
use crate::objects::facing::SpriteFacets;
use crate::objects::lifecycle::MasterData;
use crate::objects::masters::AdjacentMode;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::BlastDefense;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::COST_SLOTS;
use crate::objects::masters::CarryCapacity;
use crate::objects::masters::ChargeEvent;
use crate::objects::masters::ChargeTarget;
use crate::objects::masters::ChargeType;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::CombatLevel;
use crate::objects::masters::ConversionAdjust;
use crate::objects::masters::CostSlot;
use crate::objects::masters::DamageAmount;
use crate::objects::masters::DamageGraphic;
use crate::objects::masters::DamageGraphicEntry;
use crate::objects::masters::DamageShare;
use crate::objects::masters::FlyMode;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::FormationClass;
use crate::objects::masters::GarrisonCapacity;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::GhostMode;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::HillMode;
use crate::objects::masters::HitChance;
use crate::objects::masters::InteractionMode;
use crate::objects::masters::InterfaceKind;
use crate::objects::masters::InvulnerabilityLevel;
use crate::objects::masters::MasterField;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MinimapMode;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::Occlusion;
use crate::objects::masters::PlacementTerrains;
use crate::objects::masters::RestrictionField;
use crate::objects::masters::RunPattern;
use crate::objects::masters::SelectionEffect;
use crate::objects::masters::SpacingModifier;
use crate::objects::masters::SpecialAttack;
use crate::objects::masters::Task;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::TaskIndex;
use crate::objects::masters::Trail;
use crate::objects::masters::TrainLocationEntry;
use crate::objects::masters::VolleySpread;
use crate::objects::masters::master_type;
use crate::objects::{Frame, Regeneration};
use crate::players::Amount;
use crate::players::attribute::AttributeId;
use crate::production::{HotkeyId, LocationButton};
use crate::technology::TechId;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use genie_diff::Diff;

impl Master {
    /// Its default task (`FUN_14109e0c0`: the task list's entry at the default task's index,
    /// `+0x788`), if it has one.
    #[must_use]
    pub fn default_task(&self) -> Option<&Task> {
        self.tasks.get(self.default_task?.index())
    }

    /// Whether its objects move: a moving master (types 30 and up) whose speed (`+0x740`, the
    /// master's slot 0xe8, [`MoveStats::base`]) is above 0 (`FUN_14123bb20`'s test).
    #[must_use]
    pub fn moves(&self) -> bool {
        self.moving.is_some_and(|m| Speed::ZERO < m.base)
    }

    /// The facets the facing routines read of a unit of this master ([`MoveStats::facets`]):
    /// those of its standing sprites, its moving ones (types 30 and up), its attack sprite (50
    /// and up) and its tasks' sprites, when every one the graphics hold has the same count and
    /// bit 1 (as the masters' loader derives them).
    #[must_use]
    pub fn sprite_facets(&self, graphics: &Graphics) -> Option<SpriteFacets> {
        let mut sprites = vec![self.standing_sprite, self.standing_sprite_2];
        if master_type::MOVING <= self.kind {
            sprites.extend([self.move_sprite, self.run_sprite]);
        }
        if master_type::RGE_COMBAT <= self.kind {
            sprites.push(self.attack_sprite);
        }
        for t in &self.tasks {
            sprites.extend(t.sprites.ids());
        }
        let mut seen = sprites
            .into_iter()
            .flatten()
            .filter_map(|id| graphics.get(id))
            .map(|g| SpriteFacets {
                count: g.facets,
                turns: g.flags.contains(SpriteFlags::TURNS),
            });
        let first = seen.next()?;
        seen.all(|f| f == first).then_some(first)
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
    /// (`FUN_141663f80`, `genie_core::world::World::can_make`); an upgrade's copy keeps it.
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
    pub height: Distance,
    /// `line_of_sight` (`+0x5d0`).
    pub line_of_sight: Distance,
    /// `+0x606`, its terrain restriction (the save's `terrain_restriction_mask_id`; −1 for none),
    /// which a placement test reads.
    pub terrain_restriction: Option<RestrictionId>,
    /// `obstruction_type` (`+0x651`) and `can_be_built_on` (`+0x5e9`).
    pub obstruction_type: ObstructionType,
    /// `can_be_built_on` (`+0x5e9`): a new building deletes an object of it off its footprint.
    pub buildable_on: BuildableOn,
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
    pub spacing_modifier: SpacingModifier,
    /// `+0x688`, `+0x68c`: the `.dat`'s `outline_size` (x, y), an obstruction-type-11 object's
    /// shape radii (`FUN_141054fe0`).
    pub outline: Footprint,
    /// `+0x690`: the `.dat`'s `outline_size` z, the height BUILD's scan takes for an object of
    /// obstruction types 11 to 13 (`FUN_14123aee0`).
    pub outline_height: Distance,
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
    pub base_armor: DamageAmount,
    /// Its armor classes (class, amount; `+0x808`, count `+0x802`).
    pub armors: Vec<ClassAmount>,
    /// `bonus_damage_resistance` (`+0x824`).
    pub resistance: DamageShare,
    /// `combat_ability` (`+0x840`).
    pub combat_ability: CombatAbility,
    /// `damage_reflection` (`+0x864`).
    pub reflection: DamageShare,
    /// `invulnerability_level` (`+0x930`): a hit point floor.
    pub invulnerability: InvulnerabilityLevel,
    /// `attack_priority` (`+0x92c`, `FUN_141202220`): 3 attacks only the classes of
    /// `FUN_140fddf10`.
    pub attack_priority: AttackPriority,
    /// `special_attack_flag` (`+0x8d8`).
    pub special_attack: SpecialAttack,
    /// `friendly_fire_damage` (`+0x868`).
    pub friendly_fire: DamageShare,
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
    pub area_damage: DamageShare,
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
    /// of it (`genie_core::vision::ghost`).
    pub ghost_on_death: GhostMode,
    /// The placement terrains (`+0x5f2`..`+0x5f8`).
    pub placement: PlacementTerrains,
    /// The death sprite (`+0x480`, −1 for none) and the death spawns (`+0x5e4`, `+0x5e6`, −1 for
    /// none), and what a dead object's steps read of the master
    /// ([`crate::objects::lifecycle::MasterData`]), for a new object's `genie_core::objects::lifecycle::Lifecycle`.
    pub death_sprite: Option<SpriteId>,
    /// The death spawns (`+0x5e4`, `+0x5e6`, −1 for none).
    pub death_spawns: [Option<MasterId>; 2],
    /// What a dead object's steps read of the master.
    pub life: Option<MasterData>,
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
    pub fire_frame: Frame,
    /// `interrupt_frame`.
    pub interrupt_frame: Frame,
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
    pub missed_spread: MissSpread,
    /// `garrison_firepower` (`+0x870`): what a unit adds to the volleys of a building it is
    /// inside; one at most 0 adds its magnitude.
    pub garrison_firepower: Firepower,
    /// `volley_fire_amount` (`+0x8c0`, the `.dat`'s total projectiles): a new object's missiles
    /// per volley (`+0xa00`).
    pub volley_fire: Firepower,
    /// `max_attacks_in_volley` (`+0x8c4`, the `.dat`'s max total projectiles).
    pub max_volley: MissileCount,
    /// `volley_x_spread`, `volley_y_spread` and `volley_start_spread_adjustment` (`+0x8c8`..
    /// `+0x8d0`, the `.dat`'s projectile spawning area): how far a volley's missiles scatter.
    pub volley_spread: VolleySpread,
    /// `volley_missile_id` (`+0x8d4`, the `.dat`'s secondary projectile), None for −1.
    pub volley_missile: Option<MasterId>,
    /// `mChargeProjectileUnit` (`+0x928`), None for −1.
    pub charge_projectile: Option<MasterId>,
    /// A missile master's flight (type 60, [`crate::combat::missile::MissileStats`]).
    pub missile_stats: Option<MissileStats>,
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
    pub damage_graphic_entry: DamageGraphicEntry,
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
    pub train_location_entry: TrainLocationEntry,
    /// `+0x8b4`, `regeneration_rate`.
    pub regeneration: Regeneration,
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

/// One train location of a combat master (`+0x898`'s 12-byte entries; the save's
/// `train_location`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct TrainLocation {
    /// `+0x0`, `trainLocationID`: the master that makes it.
    pub location: Option<MasterId>,
    /// `+0x2`, `trainTime`.
    pub time: WholeSeconds,
    /// `+0x4`, `buttonLocationID`.
    pub button: LocationButton,
    /// `+0x8`, `hotkeyID`.
    pub hotkey: HotkeyId,
}

/// What a combat master shows of its values (`+0x856`, `+0x858`, `+0x854`, `+0x8b0`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct Shown {
    /// `+0x856`, `orig_weapon`.
    pub attack: DamageAmount,
    /// `+0x858`, `orig_weapon_range`.
    pub range: Distance,
    /// `+0x854`, `orig_armor`.
    pub melee_armor: DamageAmount,
    /// `+0x8b0`, `orig_pierce_armor`.
    pub pierce_armor: DamageAmount,
}

/// A player's masters by id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Masters {
    pub(super) by_id: Vec<Option<Master>>,
}

impl Masters {
    /// The masters, by their ids.
    pub fn new(masters: impl IntoIterator<Item = Master>) -> Self {
        let mut by_id: Vec<Option<Master>> = Vec::new();
        for m in masters {
            let i = m.id.index();
            if let Some(slot) = by_id.get_mut(i) {
                *slot = Some(m);
            } else {
                by_id.resize(i, None);
                by_id.push(Some(m));
            }
        }
        Self { by_id }
    }

    /// The master `id`, if the player has it.
    #[must_use]
    pub fn get(&self, id: MasterId) -> Option<&Master> {
        self.by_id.get(id.index())?.as_ref()
    }

    /// Every master, by id.
    pub fn iter(&self) -> impl Iterator<Item = &Master> {
        self.by_id.iter().flatten()
    }

    /// Every master, by id, to change (a technology's effect).
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Master> {
        self.by_id.iter_mut().flatten()
    }

    /// The master `id` to change, if the player has it.
    pub fn get_mut(&mut self, id: MasterId) -> Option<&mut Master> {
        self.by_id.get_mut(id.index())?.as_mut()
    }
}
