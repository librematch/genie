//! A saved object's own master and its successor master: whole master records the save holds for
//! the object (`has_own_master_object`, the master the object's `+0xa0` names when it is its
//! own: a captured animal's copy, a desolid foundation's; `has_successor_master_obj`, its
//! `+0x6b8`, the death-spawn master an ownership change took). Each record is the master's whole
//! load (`FUN_14112c560` for the static part, then `FUN_1410a9070`, `FUN_1410e3b00`,
//! `FUN_1410a14e0`, `FUN_1410c50b0`, `FUN_1410d6760`, `FUN_14126ae20` and `FUN_141240660` by its
//! class), not a shared master's changes to its `.dat` copy.
//!
//! The master is built as the import builds the others: the `.dat` master its copy id names, with
//! every value the `.dat` import or the shared masters' import reads taken from the record
//! instead ([`crate::dat`], [`crate::save::work_masters`]).

mod layers;
#[cfg(test)]
mod tests;

use crate::dat::MasterTable;
use crate::save::own_master::layers::{
    own_action, own_building, own_combat, own_creatable, own_damage_graphics, own_missile,
    own_move_stats, own_moving, own_storage,
};
use genie_model::map::Distance;
use genie_model::map::Footprint;
use genie_model::map::RestrictionId;
use genie_model::movement::Speed;
use genie_model::objects::MasterHitPoints;
use genie_model::objects::SpriteId;
use genie_model::objects::lifecycle::MasterData;
use genie_model::objects::masters::AnimatedPart;
use genie_model::objects::masters::AutoGatherMode;
use genie_model::objects::masters::Availability;
use genie_model::objects::masters::BlastDefense;
use genie_model::objects::masters::BuildableOn;
use genie_model::objects::masters::CarryCapacity;
use genie_model::objects::masters::ClassId;
use genie_model::objects::masters::CombatLevel;
use genie_model::objects::masters::CombatPart;
use genie_model::objects::masters::DisableMode;
use genie_model::objects::masters::FlyMode;
use genie_model::objects::masters::FogVisibility;
use genie_model::objects::masters::GarrisonCapacity;
use genie_model::objects::masters::GatherGroup;
use genie_model::objects::masters::GhostMode;
use genie_model::objects::masters::HillMode;
use genie_model::objects::masters::Master;
use genie_model::objects::masters::MasterId;
use genie_model::objects::masters::MasterKind;
use genie_model::objects::masters::ObjectFlags;
use genie_model::objects::masters::ObstructionClass;
use genie_model::objects::masters::ObstructionKind;
use genie_model::objects::masters::PlacementTerrains;
use genie_model::objects::masters::RecycleMode;
use genie_model::players::AmountRate;
use genie_model::players::CivId;
use genie_model::{GenieError, GenieResult};
use genie_save::generated::world::RgeStaticObject;
use genie_save::generated::world::{
    OwnRgeMasterActionObject, OwnRgeMasterAnimatedObject, OwnRgeMasterCombatObject,
    OwnRgeMasterMissileObject, OwnRgeMasterMovingObject, OwnRgeMasterStaticObject,
    OwnTribeMasterBuildingObject, OwnTribeMasterCombatObject,
};

/// One saved master record by its layers: the static part every class has and each layer above
/// it that its class holds.
#[derive(Clone, Copy, Debug)]
struct Record<'save> {
    /// The record's class (`master_kind`: 10 static, 20 animated, … 80 building).
    kind: MasterKind,
    static_part: &'save OwnRgeMasterStaticObject,
    animated: Option<&'save OwnRgeMasterAnimatedObject>,
    moving: Option<&'save OwnRgeMasterMovingObject>,
    action: Option<&'save OwnRgeMasterActionObject>,
    combat: Option<&'save OwnRgeMasterCombatObject>,
    missile: Option<&'save OwnRgeMasterMissileObject>,
    creatable: Option<&'save OwnTribeMasterCombatObject>,
    building: Option<&'save OwnTribeMasterBuildingObject>,
}

/// The class records a save holds for one master, the one its class names set.
#[derive(Clone, Copy, Debug, Default)]
struct Saved<'save> {
    static_part: Option<&'save OwnRgeMasterStaticObject>,
    animated: Option<&'save OwnRgeMasterAnimatedObject>,
    moving: Option<&'save OwnRgeMasterMovingObject>,
    action: Option<&'save OwnRgeMasterActionObject>,
    missile: Option<&'save OwnRgeMasterMissileObject>,
    creatable: Option<&'save OwnTribeMasterCombatObject>,
    building: Option<&'save OwnTribeMasterBuildingObject>,
}

impl<'save> Record<'save> {
    /// The record of class `kind` whose top layer `saved` holds, its lower layers taken from
    /// it; None for a record the save does not hold.
    fn of(kind: Option<u8>, saved: Saved<'save>) -> Option<Self> {
        let building = saved.building;
        let creatable = saved.creatable.or_else(|| {
            building.map(|building_object| &building_object.own_tribe_master_combat_object)
        });
        let missile = saved.missile;
        let combat = creatable
            .map(|creatable_object| &creatable_object.own_rge_master_combat_object)
            .or_else(|| missile.map(|missile_object| &missile_object.own_rge_master_combat_object));
        let action = saved
            .action
            .or_else(|| combat.map(|combat_object| &combat_object.own_rge_master_action_object));
        let moving = saved
            .moving
            .or_else(|| action.map(|action_object| &action_object.own_rge_master_moving_object));
        let animated = saved
            .animated
            .or_else(|| moving.map(|moving_object| &moving_object.own_rge_master_animated_object));
        let static_part = saved.static_part.or_else(|| {
            animated.map(|animated_object| &animated_object.own_rge_master_static_object)
        })?;
        Some(Self {
            kind: MasterKind::of_data(kind?),
            static_part,
            animated,
            moving,
            action,
            combat,
            missile,
            creatable,
            building,
        })
    }
}

/// The object's own master (`has_own_master_object`): the master it is an object of, which no
/// player's table holds; None for an object of its player's master.
///
/// # Errors
///
/// A record without its id, or a task or cost slot that holds no type or attribute.
pub fn own_master(
    rge_static_object: &RgeStaticObject,
    civ: CivId,
    table: Option<&MasterTable>,
) -> GenieResult<Option<Master>> {
    let saved = Saved {
        static_part: rge_static_object.rge_master_static_object.as_deref(),
        animated: rge_static_object.rge_master_animated_object.as_deref(),
        moving: rge_static_object.rge_master_moving_object.as_deref(),
        action: rge_static_object.rge_master_action_object.as_deref(),
        missile: rge_static_object.rge_master_missile_object.as_deref(),
        creatable: rge_static_object.tribe_master_combat_object.as_deref(),
        building: rge_static_object.tribe_master_building_object.as_deref(),
    };
    Record::of(rge_static_object.master_kind, saved)
        .map(|record| master(&record, civ, table))
        .transpose()
}

/// The object's successor master (`has_successor_master_obj`, `+0x6b8`): the copy of an old
/// owner's death-spawn master that an ownership change took, which its death spawn makes its dead
/// object of; None for an object without one.
///
/// # Errors
///
/// As [`own_master`].
pub fn successor_master(
    rge_static_object: &RgeStaticObject,
    civ: CivId,
    table: Option<&MasterTable>,
) -> GenieResult<Option<Master>> {
    let saved = Saved {
        static_part: rge_static_object.rge_master_static_object_2.as_deref(),
        animated: rge_static_object.rge_master_animated_object_2.as_deref(),
        moving: rge_static_object.rge_master_moving_object_2.as_deref(),
        action: rge_static_object.rge_master_action_object_2.as_deref(),
        missile: rge_static_object.rge_master_missile_object_2.as_deref(),
        creatable: rge_static_object.tribe_master_combat_object_2.as_deref(),
        building: rge_static_object.tribe_master_building_object_2.as_deref(),
    };
    Record::of(rge_static_object.master_kind_2, saved)
        .map(|record| master(&record, civ, table))
        .transpose()
}

/// The master a saved record holds: the `.dat` master of its copy id (its id where the `.dat`
/// lacks that; none without a table), then each value the record holds; what a unit of it moves
/// with only with the table, which holds the sprites' speeds and the restrictions' masks. The
/// static part's three `readTemp` shorts are its id (`+0x58`), its class (`+0x410`) and its hit
/// points (`+0x530`); the two after its name its copy id (`+0x158`) and its base id (`+0x260`,
/// kept as the record holds it).
///
/// # Errors
///
/// A record without its id, or a task or cost slot that holds no type or attribute.
fn master(record: &Record<'_>, civ: CivId, table: Option<&MasterTable>) -> GenieResult<Master> {
    let static_part = record.static_part;
    let id = MasterId::of_data(static_part.read_temp).ok_or_else(|| {
        GenieError::invalid_input("a saved master record without its id")
            .with("id", static_part.read_temp)
    })?;
    let copy_id = MasterId::of_data(static_part.read_temp_4);
    let mut master = table
        .and_then(|master_table| {
            copy_id
                .and_then(|copy| master_table.work(civ, copy))
                .or_else(|| master_table.work(civ, id))
        })
        .cloned()
        .unwrap_or_default();
    master.id = id;
    master.kind = record.kind;
    master.copy_id = copy_id;
    master.base_id = MasterId::of_data(static_part.read_temp_5);
    master.class = ClassId::of_master(static_part.read_temp_2);
    master.hit_points = MasterHitPoints::of_data(static_part.read_temp_3);
    static_values(&mut master, static_part);
    master.life = Some(MasterData {
        decay: AmountRate::new(static_part.attribute_rot),
        undead: static_part.undead_flag != 0,
    });
    master.animated = record.animated.map(|animated_object| AnimatedPart {
        max_speed: Speed::new(animated_object.max_speed),
    });
    master.moving = record.moving.map(own_moving);
    master.action = record.action.map(own_action).transpose()?;
    // a layer the record lacks goes; one it holds keeps the `.dat` copy's values it does not hold
    let combat = master.combat.take().unwrap_or(CombatPart::ABSENT);
    master.combat = record.combat.map(|saved| own_combat(combat, saved));
    let creatable = master.creatable.take().unwrap_or_default();
    master.creatable = record
        .creatable
        .map(|saved| own_creatable(creatable, saved));
    master.missile = record.missile.map(own_missile);
    let building = master.building.take().unwrap_or_default();
    master.building = record.building.map(|saved| own_building(building, saved));
    master.move_stats = table.and_then(|master_table| own_move_stats(record, master_table));
    Ok(master)
}

/// The static part's values (`FUN_14112c560`'s stores) but the ids, the class and the hit
/// points.
fn static_values(master: &mut Master, static_part: &OwnRgeMasterStaticObject) {
    master.standing_sprite = SpriteId::of_data(static_part.sprite_num);
    master.standing_sprite_2 = SpriteId::of_data(static_part.sprite_num2);
    master.death_sprite = SpriteId::of_data(static_part.death_sprite_num);
    master.undead_sprite = SpriteId::of_data(static_part.undead_sprite_num);
    master.line_of_sight = Distance::new(static_part.los);
    master.garrison = GarrisonCapacity::of_data(static_part.obj_max);
    master.radius = Footprint::new(static_part.radius_x, static_part.radius_y);
    master.height = Distance::new(static_part.radius_z);
    master.death_spawns = [
        MasterId::of_data(static_part.death_spawn_obj_id),
        MasterId::of_data(static_part.death_spawn_obj_id2),
    ];
    master.buildable_on = BuildableOn::of_data(static_part.can_be_built_on);
    master.available = Availability::of_data(static_part.available);
    master.disabled = DisableMode::of_data(static_part.disabled);
    master.placement = PlacementTerrains::of_data([
        static_part.tile_req1,
        static_part.tile_req2,
        static_part.center_tile_req1,
        static_part.center_tile_req2,
    ]);
    master.clearance = Footprint::new(
        static_part.construction_radius_x,
        static_part.construction_radius_y,
    );
    master.hill_mode = HillMode::of_data(static_part.elevation_flag);
    master.fog_visibility = FogVisibility::of_data(static_part.fog_flag);
    master.terrain_restriction = RestrictionId::of_data(static_part.terrain_restriction_mask_id);
    master.fly_mode = FlyMode::of_data(static_part.movement_kind);
    master.capacity = CarryCapacity::of_data(static_part.attribute_max_amount);
    master.blast_defense = BlastDefense::new(static_part.area_effect_object_level);
    master.combat_level = CombatLevel::of_data(static_part.combat_level);
    master.recyclable = RecycleMode::of_data(static_part.recyclable);
    master.auto_gather = AutoGatherMode::of_data(static_part.track_as_resource);
    master.ghost_on_death = GhostMode::of_data(static_part.create_doppleganger_on_death);
    master.gather_group = GatherGroup::from(static_part.resource_group);
    master.obstruction_kind = ObstructionKind::from(static_part.obstruction_kind);
    master.obstruction_class = ObstructionClass::from(static_part.obstruction_class);
    master.object_flags = ObjectFlags::from_bits_retain(static_part.object_flags);
    master.outline = Footprint::new(static_part.outline_radius_x, static_part.outline_radius_y);
    master.outline_height = Distance::new(static_part.outline_radius_z);
    master.storage = own_storage(static_part);
    master.damage_graphics = own_damage_graphics(static_part);
}
