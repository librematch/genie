//! A saved object as the slice's object ([`object`]: its row, [`unit`], and its parts): its
//! town bell, a building's gate and its construction.

use crate::GenieError;
use crate::combat::AttackCount;
use crate::combat::AttackPowerup;
use crate::combat::Charge;
use crate::combat::Firepower;
use crate::combat::MissileCount;
use crate::combat::abilities::Abilities;
use crate::combat::damage::ArmorPair;
use crate::combat::missile::ShooterId;
use crate::combat::missile::{MISSILE, Missile};
use crate::construction::Annexes;
use crate::construction::BuilderCount;
use crate::construction::MainBuildingId;
use crate::construction::build::Construction;
use crate::garrison::RaiderCountdown;
use crate::gathering::PastureId;
use crate::groups::{GroupId, GroupLink};
use crate::map::Distance;
use crate::map::Point;
use crate::map::Snow;
use crate::map::spatial::Obstruction;
use crate::map::{Offset, Position};
use crate::movement::Speed;
use crate::objects::DamagedTimer;
use crate::objects::HitPoints;
use crate::objects::HpPowerup;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::Regeneration;
use crate::objects::RelicCount;
use crate::objects::Relics;
use crate::objects::WorkRate;
use crate::objects::WorkerCount;
use crate::objects::layers::Building;
use crate::objects::layers::{Action, Animated, Combat, DefaultTask, Motion, Static, Unstartable};
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionKind;
use crate::objects::unit::Unit;
use crate::objects::unit::UnitFlags;
use crate::orders::town_bell::{BellShelter, TownBell};
use crate::ownership::CaptureKind;
use crate::players::Amount;
use crate::players::CivId;
use crate::players::attribute::AttributeId;
use crate::unit_ai::{AiAction, UnitAi};
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::WorldTime;
use crate::world::layer::generated::MissileTuning;
use crate::world::load::movers::mover;
use crate::world::load::object_ids;
use crate::world::load::object_parts::lifecycle;
use crate::world::load::object_parts::unit_ai;
use crate::world::load::sub_actions::DefaultStart;
use crate::world::load::sub_actions::build_task;
use crate::world::load::sub_actions::default_task;
use crate::world::load::sub_actions::fly_task;
use crate::world::load::sub_actions::gather_point;
use crate::world::load::sub_actions::production;
use crate::world::load::sub_actions::saved_sprite;
use crate::world::load::top_actions::saved_actions;
use crate::world::parts::{Object, ObjectParts};
use crate::world::{Gate, Unread};
use genie_import::dat::MasterTable;
use genie_import::save::task_at;
use genie_save::generated::world::TribeCombatObject;
use genie_save::generated::world::{Path, PlayerObjects, RgeMissileObject, RgeStaticObject};

/// The slice's view of one saved object, whole: its row ([`unit`]) and its parts (the gather
/// point, the production lists, the missile's own fields, the town bell, the unit AI; the dead
/// and own masters are the import's gaps, MISSING.md).
#[must_use]
pub fn object(
    object: &genie_save::Object,
    objects: &PlayerObjects,
    paths: &[Path],
    civ: CivId,
    masters: Option<&MasterTable>,
    players: usize,
    missile_tuning: &MissileTuning,
) -> Object {
    let (unit, parts) = row(object, objects, paths, civ, masters, players);
    Object {
        unit,
        parts: ObjectParts {
            dead_master: None,
            gather_point: gather_point(object).ok().flatten(),
            production: production(object).ok().flatten(),
            missile: saved_missile(object, missile_tuning),
            own_master: None,
            ..parts
        },
    }
}

/// A saved object's missile part (`RGE_Missile_Object`): None for an object that is no missile.
#[must_use]
pub fn missile_part(object: &genie_save::Object) -> Option<&RgeMissileObject> {
    object
        .rge_object
        .as_deref()
        .and_then(|r| r.rge_missile_object.as_deref())
}

/// A saved missile's own fields (`+0x990`..`+0x99c`, its step `+0x7dc`), its object's part:
/// flying (`+0x994`, not saved) while its missile action's distance is past
/// the build's landing distance (`missile` tuning's `landing`); a missile without its action (one
/// that met its target, dying) takes the 1 the load's constructor sets. None for an object that
/// is no missile.
pub fn saved_missile(
    object: &genie_save::Object,
    missile_tuning: &MissileTuning,
) -> Option<Missile> {
    let mo = missile_part(object)?;
    let step = object.moving_part().map_or(Offset::ZERO, |m| {
        Offset::new(m.velocity_x, m.velocity_y, m.velocity_z)
    });
    let distance = object
        .actions()
        .and_then(|a| a.entries().next())
        .filter(|&(kind, _)| ActionKind::try_from(kind).ok() == Some(MISSILE))
        .and_then(|(_, action)| action.rge_action_by_kind.as_deref())
        .and_then(|t| t.rge_action_missile.as_deref())
        .map(|ma| ma.distance);
    Some(Missile {
        max_range: Distance::new(mo.max_range),
        fired_from: ObjectId::of_field(mo.fired_from_id).map(ShooterId::new),
        charge: mo.is_charge_projectile != 0,
        flying: distance.is_none_or(|d| missile_tuning.landing < d),
        step,
    })
}

/// The parts of an object the slice cannot read at all: none, the load's reason `why` its only
/// entry ([`crate::world::Unread`]).
fn unmodelled(why: GenieError) -> ObjectParts {
    ObjectParts {
        unread: Some(Unread::new(why)),
        ..ObjectParts::default()
    }
}

/// The slice's view of one saved object: its row (its lifecycle) and the parts it reads, the
/// unit AI and the town bell its unit AI's read gives (none and a new unit's where the read
/// fails), the building, combat and motion layers and the action layer. A save from the start
/// of a game holds no actions, so an object starts its master's default task in its first
/// update ([`default_task`]); a save from later in the game holds them, and the slice imports
/// the ones it models into the action layer ([`saved_actions`]). A gather point or production
/// lists the save holds that the slice cannot read are the object's unmodelled part.
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
            ..Unit::default()
        };
        return (
            unit,
            unmodelled(GenieError::unsupported("an object without a static part")),
        );
    };
    let Ok(master) = MasterId::try_from(s.master_obj) else {
        let unit = Unit {
            id,
            ..Unit::default()
        };
        return (
            unit,
            unmodelled(GenieError::invalid_input("an object's master is negative")),
        );
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
    // a capture flag no store of the game writes: the object's unmodelled part
    let capture = object.combat().map_or(Ok(CaptureKind::None), |c| {
        CaptureKind::try_from(c.rge_combat_object.capture_flag)
    });
    let (capture_flag, capture_read) = match capture {
        Ok(k) => (k, Ok(())),
        Err(e) => (CaptureKind::None, Err(e)),
    };
    let (construction, construction_read) = match construction(object, s, objects) {
        Ok(c) => (c, Ok(())),
        Err(e) => (None, Err(e)),
    };
    let building = Building {
        construction,
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
    let combat = Combat {
        work_rate_powerup: object
            .combat()
            .map_or(WorkRate::ZERO, |c| WorkRate::new(c.work_rate_powerup)),
        capture_flag,
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
        abilities: Abilities {
            movement_damage_timer: object
                .combat()
                .map_or(Seconds::ZERO, |c| Seconds::new(c.movement_damage_timer)),
            ..Abilities::default()
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
    let animated = object
        .animated_part()
        .map(|a| Animated::new(Speed::new(a.current_speed)));
    let unit = Unit {
        id,
        life,
        flags: {
            let mut flags = UnitFlags::empty();
            flags.set(UnitFlags::ATTACKABLE, s.unit_attackable_flag != 0);
            flags.set(UnitFlags::UNTARGETABLE, s.unit_targeting_flag == 0);
            flags.set(UnitFlags::UNDER_ATTACK, s.under_attack_value != 0);
            flags.set(UnitFlags::UNDELETABLE, s.deletion_enabled == 0);
            flags.set(UnitFlags::ROO_CALLED, s.roo_already_called != 0);
            flags
        },
        pasture: PastureId::of_field(s.parent_pasture_id),
        my_pasture: PastureId::of_field(s.my_pasture_id),
        group: group_link(s),
        workers: WorkerCount::of_data(s.worker_num),
        last_human_order: WorldTime::from_millis(s.last_human_order_time),
        damaged_lately: DamagedTimer::of_data(s.damaged_lately_timer as u8),
        place: Static::at(Position::new(s.tx, s.ty, s.tz)),
        // every animated object's current sprite, which slot 0x68 and the sprite list read
        sprite: masters.and_then(|t| saved_sprite(s, &t.graphics)),
        ..Unit::default()
    };
    let mut actions = Action::default();
    let default = default_task(object, s, objects, civ, masters);
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
            &mut actions,
        )
    } else {
        // an object without actions in a save: its default task's action (an Artifact exists
        // from the load on; a Fly starts in its first update)
        match &default {
            Ok(Some(DefaultStart::Artifact(a))) => {
                actions.list.set_artifact(Some(**a));
                Ok(())
            }
            Ok(_) => Ok(()),
            Err(f) => Err(f.error()),
        }
    };
    actions.default_task = default_start(default, saved);
    let production = production(object).map(|_| ());
    let (mover, moving) = match mover(object, s, objects, paths, civ, masters) {
        Ok(m) => (m, Ok(())),
        Err(e) => (None, Err(e)),
    };
    let unread = life_read
        .and(ai_read)
        .and(lists_read)
        .and(construction_read)
        .and(gather_read)
        .and(capture_read)
        .and(imported)
        .and(production)
        .and(moving)
        .err()
        .map(Unread::new);
    (
        unit,
        ObjectParts {
            unit_ai: ai,
            town_bell,
            building,
            combat,
            motion: Motion::of(animated, mover),
            actions,
            unread,
            ..ObjectParts::default()
        },
    )
}

/// What the default task `default` of an object (`saved`: its save holds its actions) starts
/// once its action list empties: nothing, a Fly or a Make before its first update, the Artifact
/// again, or a start the slice cannot make. A saved object's Fly is its saved action's: none
/// starts again (it never ends); a saved Make started.
fn default_start(default: Result<Option<DefaultStart>, Unstartable>, saved: bool) -> DefaultTask {
    use crate::objects::layers::DefaultTask;
    match default {
        Ok(None) => DefaultTask::Nothing,
        Ok(Some(DefaultStart::Fly | DefaultStart::Make)) if saved => DefaultTask::Nothing,
        Ok(Some(DefaultStart::Fly)) => DefaultTask::Fly,
        Ok(Some(DefaultStart::Make)) => DefaultTask::Make,
        Ok(Some(DefaultStart::Artifact(_))) => DefaultTask::Artifact,
        Err(f) => DefaultTask::Fails(f),
    }
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
/// A record's action value no action holds ([`AiAction::try_from`]).
pub fn town_bell(c: &TribeCombatObject) -> Result<TownBell, GenieError> {
    use crate::orders::town_bell::BackToWork;
    Ok(TownBell {
        flag: BellShelter::of_byte(c.town_bell_flag),
        work: BackToWork {
            target: ObjectId::of_field(c.back_to_work1_town_bell_target_id),
            target_class: ClassId::of_param(c.back_to_work1_town_bell_target_kind)?,
            action: AiAction::of_field(c.back_to_work1_town_bell_action)?,
            target2: ObjectId::of_field(c.back_to_work1_town_bell_target_id2),
            point: Point::new(
                c.back_to_work1_town_bell_target_x,
                c.back_to_work1_town_bell_target_y,
            ),
        },
        before_building: BackToWork {
            target: ObjectId::of_field(c.back_to_work2_town_bell_target_id),
            target_class: ClassId::of_param(c.back_to_work2_town_bell_target_kind)?,
            action: AiAction::of_field(c.back_to_work2_town_bell_action)?,
            target2: ObjectId::of_field(c.back_to_work2_town_bell_target_id2),
            point: Point::new(
                c.back_to_work2_town_bell_target_x,
                c.back_to_work2_town_bell_target_y,
            ),
        },
    })
}

/// A saved building's gate state ([`crate::world::Gate`]): its lock, first update and close
/// timer, and whether a saved open or shut master or its original owner's would take part in a
/// toggle.
#[must_use]
pub fn saved_gate(object: &genie_save::Object) -> Option<Gate> {
    let b = object.tribe_building_object.as_deref()?;
    Some(Gate {
        locked: b.gate_locked != 0,
        first_update: b.first_update != 0,
        close_timer: Seconds::new(b.close_timer),
        other_masters: b.has_open_closed_master_object != 0 || b.original_owner != -1,
    })
}

/// A saved Build's or Repair's sub-action other than a `MoveTo`.
pub const OTHER_SUB_ACTION: GenieError =
    GenieError::unsupported("a saved Build or Repair action with another sub-action");

/// A saved building's construction: its `build_pts`, `desolid_flag` and `num_builders`, its
/// master's hit points (the sixth `readTemp`), build time (the first train location's) and
/// obstruction kind and class. None for an object that is not a building.
///
/// # Errors
///
/// A snow flag that is not the game's (above 2).
pub fn construction(
    object: &genie_save::Object,
    s: &RgeStaticObject,
    objects: &PlayerObjects,
) -> Result<Option<Construction>, GenieError> {
    let Some(b) = object.tribe_building_object.as_deref() else {
        return Ok(None);
    };
    let Some(m) = objects.master(s.master_obj) else {
        return Ok(None);
    };
    let Some(ms) = m.static_part() else {
        return Ok(None);
    };
    let build_time = m
        .tribe_master_building_object
        .as_deref()
        .map(|b| &b.tribe_master_combat_object)
        .or(m.tribe_master_combat_object.as_deref())
        .and_then(|c| c.train_location.first())
        .map_or(WholeSeconds::ZERO, |t| WholeSeconds::of_data(t.train_time));
    Ok(Some(Construction {
        progress: Seconds::new(b.build_pts),
        desolid: b.desolid_flag != 0,
        builders: BuilderCount::of_data(b.tribe_combat_object.num_builders),
        hit_points: MasterHitPoints::of_data(ms.read_temp_6),
        build_time,
        record: Obstruction {
            kind: ObstructionKind::from(ms.obstruction_kind),
            class: ObstructionClass::from(ms.obstruction_class),
        },
        snow: Snow::of_byte(b.snow_flag)?,
        sprite_override: s.sprite_override_id != -1,
    }))
}

/// An object's unit group link as saved (`mGroupID`, `mGroupFlag`).
#[must_use]
pub const fn group_link(s: &RgeStaticObject) -> GroupLink {
    GroupLink {
        id: GroupId::of_field(s.group_id),
        flag: s.group_flag,
    }
}

#[cfg(test)]
mod tests;
