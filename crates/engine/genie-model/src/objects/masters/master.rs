//! A master and the players' tables of them: the `.dat`'s unit records as the engine reads them (FD-046).

use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GroundOffset;
use crate::map::RestrictionId;
use crate::map::TerrainId;
use crate::movement::MoveStats;
use crate::movement::Speed;
use crate::objects::Graphics;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::facing::SpriteFacets;
use crate::objects::lifecycle::MasterData;
use crate::objects::masters::ActionPart;
use crate::objects::masters::AdjacentMode;
use crate::objects::masters::AnimatedPart;
use crate::objects::masters::BlastDefense;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::CarryCapacity;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatLevel;
use crate::objects::masters::CombatPart;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::DamageAmount;
use crate::objects::masters::DamageGraphic;
use crate::objects::masters::DamageGraphicEntry;
use crate::objects::masters::FlyMode;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::GarrisonCapacity;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::GhostMode;
use crate::objects::masters::HillMode;
use crate::objects::masters::InteractionMode;
use crate::objects::masters::InterfaceKind;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MinimapMode;
use crate::objects::masters::MissilePart;
use crate::objects::masters::MovingPart;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::Occlusion;
use crate::objects::masters::PlacementTerrains;
use crate::objects::masters::SelectionEffect;
use crate::objects::masters::Task;
use crate::objects::masters::class_id::master_class;
use crate::objects::masters::{AutoGatherMode, Availability, BuiltMode, DisableMode, RecycleMode};
use crate::players::Amount;
use crate::players::attribute::AttributeId;
use crate::production::{HotkeyId, LocationButton};
use crate::technology::TechId;
use crate::world::WholeSeconds;
use genie_diff::Diff;
use serde::Deserialize;
use serde::Serialize;
use std::iter::Flatten;
use std::slice::{Iter as SliceIter, IterMut as SliceIterMut};

impl Master {
    /// Its default task (`FUN_14109e0c0`: the task list's entry at the default task's index,
    /// `+0x788`), if it has one.
    #[must_use]
    pub fn default_task(&self) -> Option<&Task> {
        let action = self.action_part();
        action.tasks.get(action.default_task?.index())
    }

    /// Its first storage's `attribute_amount_held` (`+0x610`): what a gatherer takes of one of
    /// its objects, and what one of its livestock adds to a building's `mLivestockSummation`
    /// inside; 0 for a master without a storage.
    #[must_use]
    pub fn amount_held(&self) -> Amount {
        self.storage.first().map_or(Amount::ZERO, |stored| stored.1)
    }

    /// Whether its objects count as tall on their tiles: its height (`radius_z`, `+0x5e0`) is
    /// above 0.
    #[must_use]
    pub fn is_tall(&self) -> bool {
        Distance::ZERO < self.height
    }

    /// Whether it has damage sprites (`+0x638`..`+0x640`), which a master change checks
    /// (`FUN_1411859a0`).
    #[must_use]
    pub const fn has_damage_sprites(&self) -> bool {
        !self.damage_graphics.is_empty()
    }

    /// Whether its objects move: a moving master (types 30 and up) whose speed (`+0x740`, the
    /// master's slot 0xe8, [`MoveStats::base`]) is above 0 (`FUN_14123bb20`'s test).
    #[must_use]
    pub fn moves(&self) -> bool {
        self.move_stats
            .is_some_and(|move_stats| Speed::ZERO < move_stats.base)
    }

    /// The sprites a unit of this master shows, in the masters' loader's order: its standing
    /// sprites, its moving ones (types 30 and up), its attack sprite (50 and up) and its tasks'
    /// sprites ([`Master::sprite_facets`]).
    #[must_use]
    pub fn shown_sprites(&self) -> Vec<SpriteId> {
        let mut sprites = vec![self.standing_sprite, self.standing_sprite_2];
        if MasterKind::MOVING <= self.kind {
            sprites.extend([
                self.moving_part().move_sprite,
                self.moving_part().run_sprite,
            ]);
        }
        if MasterKind::RGE_COMBAT <= self.kind {
            sprites.push(self.combat_part().attack_sprite);
        }
        for task in &self.action_part().tasks {
            sprites.extend(task.sprites.ids());
        }
        sprites.into_iter().flatten().collect()
    }

    /// The facets the facing routines read of a unit of this master ([`MoveStats::facets`]):
    /// those of its standing sprites, its moving ones (types 30 and up), its attack sprite (50
    /// and up) and its tasks' sprites, when every one the graphics hold has the same count and
    /// bit 1 (as the masters' loader derives them).
    #[must_use]
    pub fn sprite_facets(&self, graphics: &Graphics) -> Option<SpriteFacets> {
        let mut seen = self
            .shown_sprites()
            .into_iter()
            .filter_map(|id| graphics.get(id))
            .map(|graphic| SpriteFacets {
                count: graphic.facets,
                turns: graphic.flags.contains(SpriteFlags::TURNS),
            });
        let first = seen.next()?;
        seen.all(|first_sprite_facets| first_sprite_facets == first)
            .then_some(first)
    }
}

impl<'masters> IntoIterator for &'masters Masters {
    type Item = &'masters Master;
    type IntoIter = Flatten<SliceIter<'masters, Option<Master>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'masters> IntoIterator for &'masters mut Masters {
    type Item = &'masters mut Master;
    type IntoIter = Flatten<SliceIterMut<'masters, Option<Master>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// What a building's completion (`FUN_1411dbb70` with 2) reads of its master, the `.dat`'s type
/// 80 values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BuildingMaster {
    /// `+0x980`, `adjacent_mode`: walls and gates join their neighbours (`FUN_1411ce890`).
    pub adjacent_mode: AdjacentMode,
    /// `+0x984`, `disappears_when_built`.
    pub disappears_when_built: BuiltMode,
    /// `+0x986`, `stack_unit_id`: the building becomes another (−1 for none).
    pub stack_unit: Option<MasterId>,
    /// `+0x988`, `foundation_terrain_id` (the save's `on_build_make_tile`), none for −1.
    pub foundation_terrain: Option<TerrainId>,
    /// `+0x98c`, `tech_id`: the technology its completion researches (−1 for none).
    pub tech: Option<TechId>,
    /// `+0x9bc`, `garrison_type`: the kinds of units it takes in (`FUN_14120eb10`).
    pub garrison_kind: GarrisonKinds,
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
    /// `+0x982`, `building_facet` (the attribute list's "Building Icon Override").
    pub facet: i16,
    /// `+0x960`, the `.dat`'s `destruction_graphic_id` (the save's third building sprite): the
    /// sprite a building that dies before it is built shows (`RGE_Static_Object::update`), −1
    /// for none.
    pub destruction_sprite: Option<SpriteId>,
    /// `+0x950`, the `.dat`'s `construction_graphic_id` (the save's first building sprite): the
    /// sprite a building under construction shows, −1 for none.
    pub construction_sprite: Option<SpriteId>,
}

/// The master values the task finder and the work command read.
#[derive(Clone, Debug, Default, PartialEq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// `+0x410`: −1 for a master without one, which its serde form keeps
    /// ([`crate::objects::masters::class_id::master_class`]).
    #[serde(with = "master_class")]
    #[cfg_attr(feature = "schema", schemars(with = "ClassId"))]
    pub class: ClassId,
    /// The action layer (types 40 and up but 90, `+0x790`..`+0x7b9`): tasks, default task, work
    /// rate, search radius, drop sites; None for other masters.
    pub action: Option<ActionPart>,
    /// `radius_x`, `radius_y` (`+0x5d8`, `+0x5dc`).
    pub radius: Footprint,
    /// `radius_z` (`+0x5e0`): how high a missile may pass over its objects and hit them
    /// (`FUN_14113f740`).
    pub height: Distance,
    /// `line_of_sight` (`+0x5d0`).
    pub line_of_sight: Distance,
    /// `+0x606`, its terrain restriction (the save's `terrain_restriction_mask_id`; −1 for none),
    /// which a placement test reads.
    pub terrain_restriction: Option<RestrictionId>,
    /// `obstruction_type` (`+0x651`) and `can_be_built_on` (`+0x5e9`).
    pub obstruction_kind: ObstructionKind,
    /// `can_be_built_on` (`+0x5e9`): a new building deletes an object of it off its footprint.
    pub buildable_on: BuildableOn,
    /// `obstruction_class` (`+0x652`).
    pub obstruction_class: ObstructionClass,
    /// `object_flags` (`+0x654`): its traits and the trait piece ([`ObjectFlags`]).
    pub object_flags: ObjectFlags,
    /// The `.dat`'s `combat_level` (`+0x62c`).
    pub combat_level: CombatLevel,
    /// The creatable layer (types 70 and 80, `+0x880`..`+0x94f`): costs, train locations, volleys,
    /// charge; None for other masters.
    pub creatable: Option<CreatablePart>,
    /// The hit points (`+0x530`).
    pub hit_points: MasterHitPoints,
    /// The combat layer (types 50 and up but 90, `+0x7f8`..`+0x87f`): attack, armor, blast; None
    /// for other masters.
    pub combat: Option<CombatPart>,
    /// `+0x61c`, `attribute_max_amount`: how much an object of this master carries.
    pub capacity: CarryCapacity,
    /// `+0x688`, `+0x68c`: the `.dat`'s `outline_size` (x, y), an obstruction-type-11 object's
    /// shape radii (`FUN_141054fe0`).
    pub outline: Footprint,
    /// `+0x690`: the `.dat`'s `outline_size` z, the height BUILD's scan takes for an object of
    /// obstruction types 11 to 13 (`FUN_14123aee0`).
    pub outline_height: Distance,
    /// `+0x5fc`, `clearance_size`: a completed building's foundation terrain covers its position
    /// plus and minus this.
    pub clearance: Footprint,
    /// What a building's completion reads (type 80), None for other masters.
    pub building: Option<BuildingMaster>,
    /// The resource storage (`attribute_type_held`, `attribute_amount_held`, `attribute_flag`:
    /// `+0x60a`, `+0x610`, `+0x628`): what an object of this master adds to its player's
    /// attributes, by its flag.
    pub storage: Vec<(Option<AttributeId>, Amount, u8)>,
    /// The `.dat`'s `fly_mode` (`+0x608`): 1 flies above the ground (a bird's Fly lifts it).
    pub fly_mode: FlyMode,
    /// `area_effect_object_level` (`+0x62b`): the blasts that reach it.
    pub blast_defense: BlastDefense,
    /// `+0x5d4`, `obj_max` (the `.dat`'s `garrison_capacity`): a building that garrisons takes a
    /// gather point (`FUN_140e7e9a0`).
    pub garrison: GarrisonCapacity,
    /// `+0x680`, the `.dat`'s `recyclable`: its objects take ids from the negative range
    /// (`FUN_1415301b0`).
    pub recyclable: RecycleMode,
    /// `+0x681`, `enable_auto_gather`: an object of it blocking a spawn point stays
    /// (`FUN_141194170`).
    pub auto_gather: AutoGatherMode,
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
    pub available: Availability,
    /// `+0x5f1`, `disabled`: an enable command leaves `available` as it is.
    pub disabled: DisableMode,
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
    /// Its standing sprite (`+0x470`, −1 for none), which a combat master's attack shows too
    /// ([`CombatPart`]).
    pub standing_sprite: Option<SpriteId>,
    /// Its second standing sprite (`+0x478`, the `.dat`'s `standing_graphic[1]`), whose idle
    /// animation the object swaps in (`0x141071880`); None for none.
    pub standing_sprite_2: Option<SpriteId>,
    /// The moving layer (types 30 and up but 90, `+0x748`..`+0x764`): sprites, turn speed, trail;
    /// None for other masters.
    pub moving: Option<MovingPart>,
    /// The missile layer (type 60, `+0x880`..`+0x888`): projectile type, modes and arc; None for
    /// other masters. Its flight reads [`Master::missile_stats`].
    pub missile: Option<MissilePart>,
    /// What a unit of this master moves with, its speed with the master's walking sprite (the
    /// run sprite `+0x750` if it has one, else the move sprite `+0x748`); None for a master that
    /// does not move.
    pub move_stats: Option<MoveStats>,
    /// The animated layer (types 20 and up but 90, `+0x740`): the top speed; None for other
    /// masters.
    pub animated: Option<AnimatedPart>,
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
    /// [`Master::has_damage_sprites`] reads.
    pub damage_graphics: Vec<DamageGraphic>,
    /// `+0x731`, `mDamageSpriteEntryMod`: the damage sprite the attributes 91, 156 and 157 change.
    pub damage_graphic_entry: DamageGraphicEntry,
}

/// One train location of a combat master (`+0x898`'s 12-byte entries; the save's
/// `train_location`).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff, Serialize, Deserialize)]
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

/// What a combat master shows of its values (`+0x856`, `+0x858`, `+0x854`; the pierce armor it
/// shows is the creatable layer's, [`CreatablePart::pierce_armor_shown`]).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff, Serialize, Deserialize)]
pub struct Shown {
    /// `+0x856`, `orig_weapon`.
    pub attack: DamageAmount,
    /// `+0x858`, `orig_weapon_range`.
    pub range: Distance,
    /// `+0x854`, `orig_armor`.
    pub melee_armor: DamageAmount,
}

impl Shown {
    /// Nothing shown: every value 0.
    pub const ZERO: Self = Self {
        attack: DamageAmount::ZERO,
        range: Distance::ZERO,
        melee_armor: DamageAmount::ZERO,
    };
}

/// A player's masters by id.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Masters {
    pub(super) by_id: Vec<Option<Master>>,
}

impl Masters {
    /// The masters, by their ids.
    pub fn new(masters: impl IntoIterator<Item = Master>) -> Self {
        let mut by_id: Vec<Option<Master>> = Vec::new();
        for master in masters {
            let index = master.id.index();
            if let Some(slot) = by_id.get_mut(index) {
                *slot = Some(master);
            } else {
                by_id.resize(index, None);
                by_id.push(Some(master));
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
    pub fn iter(&self) -> Flatten<SliceIter<'_, Option<Master>>> {
        self.by_id.iter().flatten()
    }

    /// Every master, by id, to change (a technology's effect).
    pub fn iter_mut(&mut self) -> Flatten<SliceIterMut<'_, Option<Master>>> {
        self.by_id.iter_mut().flatten()
    }

    /// The master `id` to change, if the player has it.
    pub fn get_mut(&mut self, id: MasterId) -> Option<&mut Master> {
        self.by_id.get_mut(id.index())?.as_mut()
    }
}

#[cfg(test)]
mod tests {
    use crate::map::Distance;
    use crate::objects::graphics::Graphics;
    use crate::objects::masters::DamageGraphic;
    use crate::objects::masters::Master;
    use crate::objects::masters::MasterKind;
    use crate::objects::masters::{ActionPart, CombatPart, MovingPart, Task};
    use crate::objects::sprite::Graphic;
    use crate::objects::{FacetCount, Frame, SpeedMultiplier, SpriteFlags, SpriteId};
    use crate::players::Amount;
    use crate::world::Seconds;

    /// The default master and its parts name no sprite, so a test's master shows only the
    /// sprites it names: with sprite 0 in the graphics, a combat master of default parts has no
    /// facets, and one that names sprite 0 has its facets.
    #[test]
    fn the_default_master_names_no_sprite() {
        let zero = SpriteId::named(0);
        let graphics = Graphics::from([(
            zero,
            Graphic {
                id: zero,
                frames: Frame::of_data(1),
                facets: FacetCount::of_data(8),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::TURNS,
            },
        )]);
        let combat = Master {
            kind: MasterKind::COMBAT,
            moving: Some(MovingPart::default()),
            combat: Some(CombatPart::default()),
            action: Some(ActionPart {
                tasks: vec![Task::default()],
                ..ActionPart::default()
            }),
            ..Master::default()
        };
        assert_eq!(combat.sprite_facets(&graphics), None);
        let named = Master {
            standing_sprite: Some(zero),
            ..combat.clone()
        };
        assert!(named.sprite_facets(&graphics).is_some());
        assert_eq!(
            (combat.standing_sprite, combat.standing_sprite_2),
            (None, None)
        );
    }

    /// A master is tall while its height is above 0, so an effect on the height (`SIZE_Z`)
    /// changes it with no flag to update; a NaN height is not tall (the import's `0 < z`).
    #[test]
    fn masters_are_tall_while_their_height_is_above_zero() {
        let mut master = Master::default();
        assert!(!master.is_tall());
        master.height = Distance::new(0.5);
        assert!(master.is_tall());
        master.height = Distance::new(f32::NAN);
        assert!(!master.is_tall());
    }

    /// A master has damage sprites while its damage graphics hold one.
    #[test]
    fn masters_have_damage_sprites_while_they_hold_one() {
        let mut master = Master::default();
        assert!(!master.has_damage_sprites());
        master.damage_graphics.push(DamageGraphic::default());
        assert!(master.has_damage_sprites());
    }

    /// A master's amount held is its first storage's amount, 0 without a storage.
    #[test]
    fn amount_held_is_the_first_storage_amount() {
        let mut master = Master::default();
        assert_eq!(master.amount_held(), Amount::ZERO);
        master.storage = vec![(None, Amount::new(-7.5), 2), (None, Amount::new(3.0), 1)];
        assert_eq!(master.amount_held(), Amount::new(-7.5));
    }
}
