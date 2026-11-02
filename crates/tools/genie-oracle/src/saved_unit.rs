//! A saved object as the slice's object ([`object`]: its row, [`unit`], and its parts): its
//! town bell, a building's gate and its construction.

use crate::masters::MasterTable;
use crate::{
    DefaultStart, build_task, default_task, fly_task, gather_point, group_link, lifecycle, mover,
    object_ids, production, saved_actions, saved_sprite, task_at, unit_ai,
};
use genie_core::GenieError;
use genie_core::combat::AttackCount;
use genie_core::combat::AttackPowerup;
use genie_core::combat::Charge;
use genie_core::combat::Firepower;
use genie_core::combat::MissileCount;
use genie_core::combat::damage::ArmorPair;
use genie_core::combat::missile::ShooterId;
use genie_core::construction::Annexes;
use genie_core::construction::BuilderCount;
use genie_core::construction::MainBuildingId;
use genie_core::garrison::RaiderCountdown;
use genie_core::gathering::PastureId;
use genie_core::map::Distance;
use genie_core::map::Point;
use genie_core::map::{Offset, Position};
use genie_core::objects::DamagedTimer;
use genie_core::objects::HitPoints;
use genie_core::objects::HpPowerup;
use genie_core::objects::MasterHitPoints;
use genie_core::objects::ObjectId;
use genie_core::objects::Regeneration;
use genie_core::objects::RelicCount;
use genie_core::objects::Relics;
use genie_core::objects::WorkRate;
use genie_core::objects::WorkerCount;
use genie_core::objects::layers::Building;
use genie_core::objects::lifecycle::Lifecycle;
use genie_core::objects::masters::ActionType;
use genie_core::objects::masters::ClassId;
use genie_core::objects::masters::MasterId;
use genie_core::objects::masters::ObstructionClass;
use genie_core::objects::masters::ObstructionType;
use genie_core::objects::unit::Unit;
use genie_core::orders::town_bell::TownBell;
use genie_core::players::Amount;
use genie_core::players::CivId;
use genie_core::players::attribute::AttributeId;
use genie_core::unit_ai::{ActionKind, UnitAi};
use genie_core::world::Seconds;
use genie_core::world::WholeSeconds;
use genie_core::world::WorldTime;
use genie_core::world::parts::{Object, ObjectParts};
use genie_save::generated::world::{Path, PlayerObjects, RgeStaticObject};

/// The slice's view of one saved object, whole: its row ([`unit`]) and its parts (the gather
/// point, the production lists, the missile's own fields, the town bell, the unit AI; the dead
/// and own masters are the import's gaps, MISSING.md).
pub(super) fn object(
    object: &genie_save::Object,
    objects: &PlayerObjects,
    paths: &[Path],
    civ: CivId,
    masters: Option<&MasterTable>,
    players: usize,
) -> Object {
    let (unit, parts) = row(object, objects, paths, civ, masters, players);
    Object {
        unit,
        parts: ObjectParts {
            dead_master: None,
            gather_point: gather_point(object).ok().flatten(),
            production: production(object).ok().flatten(),
            missile: saved_missile(object),
            own_master: None,
            ..parts
        },
    }
}

/// A saved missile's own fields (`+0x990`..`+0x99c`, its step `+0x7dc`), its object's part:
/// flying (`+0x994`, not saved) while its missile action's distance is past
/// [`genie_core::combat::missile::LANDING`]; a missile without its action (one that met its
/// target, dying) takes the 1 the load's constructor sets. None for an object that is no
/// missile.
pub(super) fn saved_missile(
    object: &genie_save::Object,
) -> Option<genie_core::combat::missile::Missile> {
    let mo = object
        .rge_object
        .as_deref()
        .and_then(|r| r.rge_missile_object.as_deref())?;
    let step = object.moving_part().map_or(Offset::ZERO, |m| {
        Offset::new(m.velocity_x, m.velocity_y, m.velocity_z)
    });
    let distance = object
        .actions()
        .and_then(|a| a.entries().next())
        .filter(|&(kind, _)| {
            ActionType::try_from(kind).ok() == Some(genie_core::combat::missile::MISSILE)
        })
        .and_then(|(_, action)| action.rge_action_by_type.as_deref())
        .and_then(|t| t.rge_action_missile.as_deref())
        .map(|ma| ma.distance);
    Some(genie_core::combat::missile::Missile {
        max_range: Distance::new(mo.max_range),
        fired_from: ObjectId::of_field(mo.fired_from_id).map(ShooterId::new),
        charge: mo.is_charge_projectile != 0,
        flying: distance.is_none_or(|d| genie_core::combat::missile::LANDING < d),
        step,
    })
}

/// The slice's view of one saved object's row: its lifecycle and its actions. A save
/// from the start of a game holds no actions, so an object starts its master's default task in
/// its first update ([`default_task`]); a save from later in the game holds them, and the slice
/// imports the ones it models ([`saved_actions`]). A gather point or production lists the save
/// holds that the slice cannot read are the row's unmodelled part.
#[cfg(test)]
pub(super) fn unit(
    object: &genie_save::Object,
    objects: &PlayerObjects,
    paths: &[Path],
    civ: CivId,
    masters: Option<&MasterTable>,
    players: usize,
) -> Unit {
    row(object, objects, paths, civ, masters, players).0
}

/// [`unit`]'s row with the parts it reads: the unit AI and the town bell its unit AI's read gives
/// (none and a new unit's where the read fails), the building and combat layers.
fn row(
    object: &genie_save::Object,
    objects: &PlayerObjects,
    paths: &[Path],
    civ: CivId,
    masters: Option<&MasterTable>,
    players: usize,
) -> (Unit, ObjectParts) {
    let saved = object.static_part();
    let named = saved.and_then(|s| ObjectId::try_from(s.id).ok());
    let (Some(s), Some(id)) = (saved, named) else {
        let unit = Unit {
            id: ObjectId::UNSAVED,
            actions: genie_core::objects::layers::ActionLayer {
                unmodelled_action: Some(GenieError::unsupported("an object without a static part")),
                ..Default::default()
            },
            ..Unit::default()
        };
        return (unit, ObjectParts::default());
    };
    let Ok(master) = MasterId::try_from(s.master_obj) else {
        let unit = Unit {
            id,
            actions: genie_core::objects::layers::ActionLayer {
                unmodelled_action: Some(GenieError::invalid_input(
                    "an object's master is negative",
                )),
                ..Default::default()
            },
            ..Unit::default()
        };
        return (unit, ObjectParts::default());
    };
    let saved = object
        .actions()
        .is_some_and(|a| a.entries().next().is_some());
    // an order or action value no order or action holds: the object's first unmodelled part
    let ((ai, town_bell), ai_read) = match ai_parts(object, id, players) {
        Ok(parts) => (parts, Ok(())),
        Err(e) => ((None, TownBell::default()), Err(e)),
    };
    let (life, life_read) = match lifecycle(object, (s, master), objects, civ, masters) {
        Ok(life) => (life, Ok(())),
        Err(e) => (Lifecycle::default(), Err(e)),
    };
    let (lists, lists_read) = match building_lists(object) {
        Ok(lists) => (lists, Ok(())),
        Err(e) => (BuildingLists::default(), Err(e)),
    };
    let gather_read = gather_point(object).map(|_| ());
    let building = Building {
        construction: construction(object, s, objects),
        annex_of: object
            .tribe_building_object
            .as_deref()
            .and_then(|b| ObjectId::of_field(b.mupt_linked_object_references_linked_owner_id))
            .map(MainBuildingId::new),
        annexes: object
            .tribe_building_object
            .as_deref()
            .map_or(Annexes::NONE, |b| {
                annexes(&b.mupt_linked_object_references_linked_object_ids)
            }),
        garrison_time: object
            .tribe_building_object
            .as_deref()
            .map_or(WorldTime::ZERO, |b| {
                WorldTime::from_millis(b.last_garrison_time)
            }),
        relics: object
            .tribe_building_object
            .as_deref()
            .map_or(Relics::NONE, |b| {
                Relics::new(
                    RelicCount::of_data(b.relic_count),
                    RelicCount::of_data(b.specific_relic_count),
                )
            }),
        livestock_summation: object
            .tribe_building_object
            .as_deref()
            .map_or(Amount::ZERO, |b| Amount::new(b.livestock_summation)),
        livestock: lists.livestock,
        pasture_workers: lists.pasture_workers,
        attacking_units: lists.attacking_units,
        gate: saved_gate(object),
        // mIsOnMangroveShallows (+0xcd0); build 158041's saves all hold it (its version gate
        // 0x421999 lies before them)
        rice: object
            .tribe_building_object
            .as_deref()
            .and_then(|b| b.is_on_mangrove_shallows)
            .is_some_and(|v| v != 0),
        ..Default::default()
    };
    let combat = genie_core::objects::layers::Combat {
        work_rate_powerup: object
            .combat()
            .map_or(WorkRate::ZERO, |c| WorkRate::new(c.work_rate_powerup)),
        capture_flag: object
            .combat()
            .map_or(genie_core::ownership::CaptureKind::NONE, |c| {
                genie_core::ownership::CaptureKind::of_data(c.rge_combat_object.capture_flag)
            }),
        hp_powerup: object.combat().map_or(HpPowerup::NONE, |c| HpPowerup {
            taken: HitPoints::new(c.hitpoints_powerup),
            max: HitPoints::new(c.hitpoints_max_powerup),
        }),
        regen_timer: object
            .combat()
            .map_or(Seconds::ZERO, |c| Seconds::new(c.berserker_timer)),
        // a building's +0x9e8 is its construction's
        builders: object
            .combat()
            .filter(|_| object.tribe_building_object.is_none())
            .map_or(BuilderCount::NONE, |c| {
                BuilderCount::of_data(c.num_builders)
            }),
        regen_powerup: Regeneration::new(
            object.combat().map_or(0.0, |c| c.regeneration_rate_powerup),
        ),
        regen_stinger: Regeneration::new(object.combat().map_or(0.0, |c| c.regeneration_stinger)),
        next_volley: MissileCount::of_data(object.combat().map_or(0, |c| c.next_volley)),
        raider_countdown: RaiderCountdown::of_data(
            object.combat().map_or(0, |c| c.raider_build_countdown),
        ),
        reload_worldtime: WorldTime::from_millis(
            object
                .combat()
                .and_then(|c| c.rge_combat_object.reload_worldtime)
                .unwrap_or(0),
        ),
        // the load's (FUN_14121ec90): the master's volley fire; the shares of the units inside
        // follow in `garrison_firepower`
        firepower: objects
            .master(s.master_obj)
            .and_then(|m| {
                m.tribe_master_combat_object.as_deref().or_else(|| {
                    m.tribe_master_building_object
                        .as_deref()
                        .map(|b| &b.tribe_master_combat_object)
                })
            })
            .map_or(Firepower::ZERO, |c| Firepower::new(c.volley_fire_amount)),
        attack_timer: object.combat().map_or(Seconds::ZERO, |c| {
            Seconds::new(c.rge_combat_object.attack_timer)
        }),
        // the movement damage timer (+0x9a4) is saved; the attack timer's rate and the terrain
        // look are not (the constructor's 1 and none, until the first update's abilities)
        abilities: genie_core::combat::abilities::Abilities {
            movement_damage_timer: object
                .combat()
                .map_or(Seconds::ZERO, |c| Seconds::new(c.movement_damage_timer)),
            ..genie_core::combat::abilities::Abilities::default()
        },
        reload_stinger: object
            .combat()
            .map_or(Seconds::ZERO, |c| Seconds::new(c.reload_stinger)),
        reload_powerup: object
            .combat()
            .map_or(Seconds::ZERO, |c| Seconds::new(c.reload_powerup)),
        attack_powerup: AttackPowerup::new(object.combat().map_or(0.0, |c| c.attack_powerup)),
        range_powerup: object
            .combat()
            .map_or(Distance::ZERO, |c| Distance::new(c.max_range_powerup)),
        armor_powerups: object.combat().map_or(ArmorPair::ZERO, |c| {
            ArmorPair::new(c.melee_armor_powerup as i16, c.pierce_armor_powerup as i16)
        }),
        armor_removed: object.combat().map_or(ArmorPair::ZERO, |c| {
            ArmorPair::new(c.melee_armor_removed, c.pierce_armor_removed)
        }),
        charge: object
            .combat()
            .map_or(Charge::ZERO, |c| Charge::new(c.charge)),
        charge_hit: object
            .combat()
            .is_some_and(|c| c.rge_combat_object.hit_something_valid_for_charge_attack != 0),
        attack_count: object
            .combat()
            .map(|c| &c.rge_combat_object)
            .or_else(|| {
                object
                    .rge_object
                    .as_deref()
                    .and_then(|r| r.rge_missile_object.as_deref())
                    .map(|m| &m.rge_combat_object)
            })
            .map_or(AttackCount::ZERO, |c| AttackCount::of_data(c.attack_count)),
        // the combat object's localResourceStore (+0xae0), a map by attribute saved in its order
        store: object
            .combat()
            .map(|c| {
                c.local_resource_store
                    .iter()
                    .filter_map(|e| {
                        Some((
                            AttributeId::of_index(usize::from(e.attribute_idx))?,
                            Amount::new(e.value),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    // the animated layer (types 20 and up); a moving object's mover takes it below
    let animated = object.animated_part().map(|a| {
        genie_core::objects::layers::Animated::new(genie_core::movement::Speed::new(
            a.current_speed,
        ))
    });
    let mut unit = Unit {
        id,
        life,
        attackable: s.unit_attackable_flag != 0,
        untargetable: s.unit_targeting_flag == 0,
        under_attack: s.under_attack_value != 0,
        pasture: PastureId::of_field(s.parent_pasture_id),
        my_pasture: PastureId::of_field(s.my_pasture_id),
        undeletable: s.deletion_enabled == 0,
        group: group_link(s),
        workers: WorkerCount::of_data(s.worker_num),
        last_human_order: WorldTime::from_millis(s.last_human_order_time),
        damaged_lately: DamagedTimer::of_data(s.damaged_lately_timer as u8),
        place: genie_core::objects::layers::Static::at(Position::new(s.t_x, s.t_y, s.t_z)),
        // every animated object's current sprite, which slot 0x68 and the sprite list read
        sprite: masters.and_then(|t| saved_sprite(s, &t.graphics)),
        roo_called: s.rooalready_called != 0,
        ..Unit::default()
    };
    let imported = if saved {
        saved_actions(
            object,
            s,
            objects,
            // the Build's task with the builder's powerup (its combat layer's)
            |m| {
                build_task(objects, civ, masters, m).map(|mut t| {
                    t.work_rate += combat.work_rate_powerup;
                    t
                })
            },
            |m| fly_task(objects, civ, masters, m),
            |m, i| task_at(objects, civ, masters, m, i),
            masters.map(|t| &t.graphics),
            &mut unit,
        )
    } else {
        // an object without actions in a save: its default task's action (an Artifact exists
        // from the load on; a Fly starts in its first update)
        default_task(object, s, objects, civ, masters).map(|d| match d {
            Some(DefaultStart::Artifact(a)) => unit.actions.list.set_artifact(Some(*a)),
            Some(DefaultStart::Fly) => unit.actions.default_fly = true,
            Some(DefaultStart::Make) => unit.actions.default_make = true,
            None => {}
        })
    };
    // what the default task starts once the action list empties: nothing, a Fly, the Artifact
    // again or something not modelled
    unit.actions.default_task = match default_task(object, s, objects, civ, masters) {
        Ok(None | Some(DefaultStart::Fly | DefaultStart::Make)) => None,
        Ok(Some(DefaultStart::Artifact(_))) => {
            unit.actions.default_artifact = true;
            None
        }
        Err(e) => Some(e),
    };
    if saved {
        // a saved object's Fly is its saved action's: none starts again (it never ends)
        unit.actions.default_fly = false;
    }
    let production = production(object).map(|_| ());
    let (mover, moving) = match mover(object, s, objects, paths, civ, masters) {
        Ok(m) => (m, Ok(())),
        Err(e) => (None, Err(e)),
    };
    unit.actions.unmodelled_action = life_read
        .and(ai_read)
        .and(lists_read)
        .and(gather_read)
        .and(imported)
        .and(production)
        .and(moving)
        .err();
    (
        unit,
        ObjectParts {
            unit_ai: ai,
            town_bell,
            building,
            combat,
            motion: genie_core::objects::layers::MotionLayer::of(animated, mover),
            ..ObjectParts::default()
        },
    )
}

/// A saved building's lists of objects (none for another object).
#[derive(Default)]
struct BuildingLists {
    /// The livestock it holds.
    livestock: Vec<ObjectId>,
    /// Its pasture's workers.
    pasture_workers: Vec<ObjectId>,
    /// `attackingUnits` (+0xce0): the unallied takers of a foundation with a stack unit.
    attacking_units: Vec<ObjectId>,
}

/// A main building's annex slots (`+0xbc0`..`+0xbd8`) from the save's linked object ids, −1
/// for an empty slot.
fn annexes(ids: &[i32]) -> Annexes {
    let mut slots = [None; Annexes::SLOTS];
    for (slot, &id) in slots.iter_mut().zip(ids) {
        *slot = ObjectId::of_field(id);
    }
    Annexes::new(slots)
}

/// A saved object's [`BuildingLists`].
///
/// # Errors
///
/// An entry of −1, which names no object.
fn building_lists(object: &genie_save::Object) -> Result<BuildingLists, GenieError> {
    let Some(b) = object.tribe_building_object.as_deref() else {
        return Ok(BuildingLists::default());
    };
    let workers: Vec<i32> = b.pasture_worker_id.iter().map(|&w| w as i32).collect();
    Ok(BuildingLists {
        livestock: object_ids(&b.livestock_id)?,
        pasture_workers: object_ids(&workers)?,
        attacking_units: object_ids(&b.unit_id)?,
    })
}

/// The parts of a saved object that hold its unit AI's order and action values: the unit AI
/// and the town bell (none for an object without a combat part).
///
/// # Errors
///
/// A value no order or action holds.
fn ai_parts(
    object: &genie_save::Object,
    own: ObjectId,
    players: usize,
) -> Result<(Option<UnitAi>, TownBell), GenieError> {
    let bell = object.combat().map(town_bell).transpose()?;
    Ok((unit_ai(object, own, players)?, bell.unwrap_or_default()))
}

/// A combat object's town-bell flag and back-to-work records.
///
/// # Errors
///
/// A record's action value no action holds ([`ActionKind::try_from`]).
pub(super) fn town_bell(
    c: &genie_save::generated::world::TribeCombatObject,
) -> Result<TownBell, GenieError> {
    use genie_core::orders::town_bell::BackToWork;
    Ok(TownBell {
        flag: c.town_bell_flag != 0,
        work: BackToWork {
            target: ObjectId::of_field(c.back_to_work1_town_bell_target_id),
            target_class: ClassId::of_param(c.back_to_work1_town_bell_target_type)?,
            action: ActionKind::of_field(c.back_to_work1_town_bell_action)?,
            target2: ObjectId::of_field(c.back_to_work1_town_bell_target_id2),
            point: Point::new(
                c.back_to_work1_town_bell_target_x,
                c.back_to_work1_town_bell_target_y,
            ),
        },
        before_building: BackToWork {
            target: ObjectId::of_field(c.back_to_work2_town_bell_target_id),
            target_class: ClassId::of_param(c.back_to_work2_town_bell_target_type)?,
            action: ActionKind::of_field(c.back_to_work2_town_bell_action)?,
            target2: ObjectId::of_field(c.back_to_work2_town_bell_target_id2),
            point: Point::new(
                c.back_to_work2_town_bell_target_x,
                c.back_to_work2_town_bell_target_y,
            ),
        },
    })
}

/// A saved building's gate state ([`genie_core::world::Gate`]): its lock, first update and close
/// timer, and whether a saved open or shut master or its original owner's would take part in a
/// toggle.
pub(super) fn saved_gate(object: &genie_save::Object) -> Option<genie_core::world::Gate> {
    let b = object.tribe_building_object.as_deref()?;
    Some(genie_core::world::Gate {
        locked: b.gate_locked != 0,
        first_update: b.first_update != 0,
        close_timer: Seconds::new(b.close_timer),
        other_masters: b.has_open_closed_master_object != 0 || b.original_owner != -1,
    })
}

/// A saved Build's or Repair's sub-action other than a `MoveTo`.
pub(super) const OTHER_SUB_ACTION: GenieError =
    GenieError::unsupported("a saved Build or Repair action with another sub-action");

/// A saved building's construction: its `build_pts`, `desolid_flag` and `num_builders`, its
/// master's hit points (the sixth `readTemp`), build time (the first train location's) and
/// obstruction kind and class. None for an object that is not a building.
pub(super) fn construction(
    object: &genie_save::Object,
    s: &RgeStaticObject,
    objects: &PlayerObjects,
) -> Option<genie_core::construction::build::Construction> {
    let b = object.tribe_building_object.as_deref()?;
    let m = objects.master(s.master_obj)?;
    let ms = m.static_part()?;
    let build_time = m
        .tribe_master_building_object
        .as_deref()
        .map(|b| &b.tribe_master_combat_object)
        .or(m.tribe_master_combat_object.as_deref())
        .and_then(|c| c.train_location.first())
        .map_or(WholeSeconds::ZERO, |t| WholeSeconds::of_data(t.train_time));
    Some(genie_core::construction::build::Construction {
        progress: Seconds::new(b.build_pts),
        desolid: b.desolid_flag != 0,
        builders: BuilderCount::of_data(b.tribe_combat_object.num_builders),
        hit_points: MasterHitPoints::of_data(ms.read_temp_6),
        build_time,
        record: genie_core::map::spatial::Obstruction {
            kind: ObstructionType::from(ms.obstruction_type),
            class: ObstructionClass::from(ms.obstruction_class),
        },
        snow: b.snow_flag != 0,
        sprite_override: s.sprite_override_id != -1,
    })
}

#[cfg(test)]
mod tests;
