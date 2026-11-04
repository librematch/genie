//! A saved object as the slice's object ([`object`]: its row, [`unit`], and its parts): its
//! town bell, a building's gate and its construction.

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
use crate::garrison::RaiderCountdown;
use crate::gathering::PastureId;
use crate::groups::{GroupId, GroupLink};
use crate::map::Distance;
use crate::map::Point;
use crate::map::{Offset, Position};
use crate::movement::Speed;
use crate::objects::DamagedTimer;
use crate::objects::HitPoints;
use crate::objects::HpPowerup;
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
use crate::objects::unit::Unit;
use crate::objects::unit::UnitFlags;
use crate::orders::town_bell::{BellShelter, TownBell};
use crate::ownership::CaptureKind;
use crate::players::Amount;
use crate::players::CivId;
use crate::players::attribute::AttributeId;
use crate::unit_ai::{AiAction, UnitAi};
use crate::world::Seconds;
use crate::world::Unread;
use crate::world::WorldTime;
use crate::world::layer::generated::MissileTuning;
use crate::world::load::movers::mover;
use crate::world::load::object::building::BuildingLists;
use crate::world::load::object::building::annexes;
use crate::world::load::object::building::building_lists;
use crate::world::load::object::building::construction;
use crate::world::load::object::building::saved_gate;
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
use crate::{GenieError, GenieResult};
use genie_import::dat::MasterTable;
use genie_import::save::task_at;
use genie_save::generated::world::TribeCombatObject;
use genie_save::generated::world::{Path, PlayerObjects, RgeMissileObject, RgeStaticObject};

pub mod building;

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
        .and_then(|rge_object| rge_object.rge_missile_object.as_deref())
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
    let step = object
        .moving_part()
        .map_or(Offset::ZERO, |rge_moving_object| {
            Offset::new(
                rge_moving_object.velocity_x,
                rge_moving_object.velocity_y,
                rge_moving_object.velocity_z,
            )
        });
    let distance = object
        .actions()
        .and_then(|action_list| action_list.entries().next())
        .filter(|&(kind, _)| ActionKind::try_from(kind).ok() == Some(MISSILE))
        .and_then(|(_, action)| action.rge_action_by_kind.as_deref())
        .and_then(|rge_action_by_kind| rge_action_by_kind.rge_action_missile.as_deref())
        .map(|ma| ma.distance);
    Some(Missile {
        max_range: Distance::new(mo.max_range),
        fired_from: ObjectId::of_field(mo.fired_from_id).map(ShooterId::new),
        charge: mo.is_charge_projectile != 0,
        flying: distance.is_none_or(|travelled| missile_tuning.landing < travelled),
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
    let named = saved.and_then(|rge_static_object| ObjectId::try_from(rge_static_object.id).ok());
    let (Some(rge_static_object), Some(id)) = (saved, named) else {
        let unit = Unit {
            id: ObjectId::UNSAVED,
            ..Unit::default()
        };
        return (
            unit,
            unmodelled(GenieError::unsupported("an object without a static part")),
        );
    };
    let Ok(master) = MasterId::try_from(rge_static_object.master_obj) else {
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
        .is_some_and(|action_list| action_list.entries().next().is_some());
    // an order or action value no order or action holds: the object's first unmodelled part
    let ((ai, town_bell), ai_read) = match ai_parts(object, id, players) {
        Ok(parts) => (parts, Ok(())),
        Err(error) => ((None, TownBell::default()), Err(error)),
    };
    let (life, life_read) =
        match lifecycle(object, (rge_static_object, master), objects, civ, masters) {
            Ok(life) => (life, Ok(())),
            Err(error) => (Lifecycle::default(), Err(error)),
        };
    let (lists, lists_read) = match building_lists(object) {
        Ok(lists) => (lists, Ok(())),
        Err(error) => (BuildingLists::default(), Err(error)),
    };
    let gather_read = gather_point(object).map(|_| ());
    // a capture flag no store of the game writes: the object's unmodelled part
    let capture = object
        .combat()
        .map_or(Ok(CaptureKind::None), |tribe_combat_object| {
            CaptureKind::try_from(tribe_combat_object.rge_combat_object.capture_flag)
        });
    let (capture_flag, capture_read) = match capture {
        Ok(capture_kind) => (capture_kind, Ok(())),
        Err(error) => (CaptureKind::None, Err(error)),
    };
    let (construction, construction_read) = match construction(object, rge_static_object, objects) {
        Ok(construction) => (construction, Ok(())),
        Err(error) => (None, Err(error)),
    };
    let building = Building {
        construction,
        annex_of: object
            .tribe_building_object
            .as_deref()
            .and_then(|tribe_building_object| {
                ObjectId::of_field(
                    tribe_building_object.mupt_linked_object_references_linked_owner_id,
                )
            })
            .map(MainBuildingId::new),
        annexes: object.tribe_building_object.as_deref().map_or(
            Annexes::NONE,
            |tribe_building_object| {
                annexes(&tribe_building_object.mupt_linked_object_references_linked_object_ids)
            },
        ),
        garrison_time: object.tribe_building_object.as_deref().map_or(
            WorldTime::ZERO,
            |tribe_building_object| {
                WorldTime::from_millis(tribe_building_object.last_garrison_time)
            },
        ),
        relics: object.tribe_building_object.as_deref().map_or(
            Relics::NONE,
            |tribe_building_object| {
                Relics::new(
                    RelicCount::of_data(tribe_building_object.relic_count),
                    RelicCount::of_data(tribe_building_object.specific_relic_count),
                )
            },
        ),
        livestock_summation: object
            .tribe_building_object
            .as_deref()
            .map_or(Amount::ZERO, |tribe_building_object| {
                Amount::new(tribe_building_object.livestock_summation)
            }),
        livestock: lists.livestock,
        pasture_workers: lists.pasture_workers,
        attacking_units: lists.attacking_units,
        gate: saved_gate(object),
        // mIsOnMangroveShallows (+0xcd0); build 158041's saves all hold it (its version gate
        // 0x421999 lies before them)
        rice: object
            .tribe_building_object
            .as_deref()
            .and_then(|tribe_building_object| tribe_building_object.is_on_mangrove_shallows)
            .is_some_and(|byte| byte != 0),
        ..Default::default()
    };
    let combat = Combat {
        work_rate_powerup: object
            .combat()
            .map_or(WorkRate::ZERO, |tribe_combat_object| {
                WorkRate::new(tribe_combat_object.work_rate_powerup)
            }),
        capture_flag,
        hp_powerup: object
            .combat()
            .map_or(HpPowerup::NONE, |tribe_combat_object| HpPowerup {
                taken: HitPoints::new(tribe_combat_object.hitpoints_powerup),
                max: HitPoints::new(tribe_combat_object.hitpoints_max_powerup),
            }),
        regen_timer: object
            .combat()
            .map_or(Seconds::ZERO, |tribe_combat_object| {
                Seconds::new(tribe_combat_object.berserker_timer)
            }),
        // a building's +0x9e8 is its construction's
        builders: object
            .combat()
            .filter(|_| object.tribe_building_object.is_none())
            .map_or(BuilderCount::NONE, |tribe_combat_object| {
                BuilderCount::of_data(tribe_combat_object.num_builders)
            }),
        regen_powerup: Regeneration::new(object.combat().map_or(0.0, |tribe_combat_object| {
            tribe_combat_object.regeneration_rate_powerup
        })),
        regen_stinger: Regeneration::new(object.combat().map_or(0.0, |tribe_combat_object| {
            tribe_combat_object.regeneration_stinger
        })),
        next_volley: MissileCount::of_data(
            object
                .combat()
                .map_or(0, |tribe_combat_object| tribe_combat_object.next_volley),
        ),
        raider_countdown: RaiderCountdown::of_data(
            object.combat().map_or(0, |tribe_combat_object| {
                tribe_combat_object.raider_build_countdown
            }),
        ),
        reload_worldtime: WorldTime::from_millis(
            object
                .combat()
                .and_then(|tribe_combat_object| {
                    tribe_combat_object.rge_combat_object.reload_worldtime
                })
                .unwrap_or(0),
        ),
        // the load's (FUN_14121ec90): the master's volley fire; the shares of the units inside
        // follow in `garrison_firepower`
        firepower: objects
            .master(rge_static_object.master_obj)
            .and_then(|master| {
                master.tribe_master_combat_object.as_deref().or_else(|| {
                    master.tribe_master_building_object.as_deref().map(
                        |tribe_master_building_object| {
                            &tribe_master_building_object.tribe_master_combat_object
                        },
                    )
                })
            })
            .map_or(Firepower::ZERO, |tribe_master_combat_object| {
                Firepower::new(tribe_master_combat_object.volley_fire_amount)
            }),
        attack_timer: object
            .combat()
            .map_or(Seconds::ZERO, |tribe_combat_object| {
                Seconds::new(tribe_combat_object.rge_combat_object.attack_timer)
            }),
        // the movement damage timer (+0x9a4) is saved; the attack timer's rate and the terrain
        // look are not (the constructor's 1 and none, until the first update's abilities)
        abilities: Abilities {
            movement_damage_timer: object
                .combat()
                .map_or(Seconds::ZERO, |tribe_combat_object| {
                    Seconds::new(tribe_combat_object.movement_damage_timer)
                }),
            ..Abilities::default()
        },
        reload_stinger: object
            .combat()
            .map_or(Seconds::ZERO, |tribe_combat_object| {
                Seconds::new(tribe_combat_object.reload_stinger)
            }),
        reload_powerup: object
            .combat()
            .map_or(Seconds::ZERO, |tribe_combat_object| {
                Seconds::new(tribe_combat_object.reload_powerup)
            }),
        attack_powerup: AttackPowerup::new(object.combat().map_or(0.0, |tribe_combat_object| {
            tribe_combat_object.attack_powerup
        })),
        range_powerup: object
            .combat()
            .map_or(Distance::ZERO, |tribe_combat_object| {
                Distance::new(tribe_combat_object.max_range_powerup)
            }),
        armor_powerups: object
            .combat()
            .map_or(ArmorPair::ZERO, |tribe_combat_object| {
                ArmorPair::new(
                    tribe_combat_object.melee_armor_powerup as i16,
                    tribe_combat_object.pierce_armor_powerup as i16,
                )
            }),
        armor_removed: object
            .combat()
            .map_or(ArmorPair::ZERO, |tribe_combat_object| {
                ArmorPair::new(
                    tribe_combat_object.melee_armor_removed,
                    tribe_combat_object.pierce_armor_removed,
                )
            }),
        charge: object.combat().map_or(Charge::ZERO, |tribe_combat_object| {
            Charge::new(tribe_combat_object.charge)
        }),
        charge_hit: object.combat().is_some_and(|tribe_combat_object| {
            tribe_combat_object
                .rge_combat_object
                .hit_something_valid_for_charge_attack
                != 0
        }),
        attack_count: object
            .combat()
            .map(|tribe_combat_object| &tribe_combat_object.rge_combat_object)
            .or_else(|| {
                object
                    .rge_object
                    .as_deref()
                    .and_then(|rge_object| rge_object.rge_missile_object.as_deref())
                    .map(|rge_missile_object| &rge_missile_object.rge_combat_object)
            })
            .map_or(AttackCount::ZERO, |rge_combat_object| {
                AttackCount::of_data(rge_combat_object.attack_count)
            }),
        // the combat object's localResourceStore (+0xae0), a map by attribute saved in its order
        store: object
            .combat()
            .map(|tribe_combat_object| {
                tribe_combat_object
                    .local_resource_store
                    .iter()
                    .filter_map(|entry| {
                        Some((
                            AttributeId::of_index(usize::from(entry.attribute_idx))?,
                            Amount::new(entry.value),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    // the animated layer (types 20 and up); a moving object's mover takes it below
    let animated = object
        .animated_part()
        .map(|rge_animated_object| Animated::new(Speed::new(rge_animated_object.current_speed)));
    let unit = Unit {
        id,
        life,
        flags: {
            let mut flags = UnitFlags::empty();
            flags.set(
                UnitFlags::ATTACKABLE,
                rge_static_object.unit_attackable_flag != 0,
            );
            flags.set(
                UnitFlags::UNTARGETABLE,
                rge_static_object.unit_targeting_flag == 0,
            );
            flags.set(
                UnitFlags::UNDER_ATTACK,
                rge_static_object.under_attack_value != 0,
            );
            flags.set(
                UnitFlags::UNDELETABLE,
                rge_static_object.deletion_enabled == 0,
            );
            flags.set(
                UnitFlags::ROO_CALLED,
                rge_static_object.roo_already_called != 0,
            );
            flags
        },
        pasture: PastureId::of_field(rge_static_object.parent_pasture_id),
        my_pasture: PastureId::of_field(rge_static_object.my_pasture_id),
        group: group_link(rge_static_object),
        workers: WorkerCount::of_data(rge_static_object.worker_num),
        last_human_order: WorldTime::from_millis(rge_static_object.last_human_order_time),
        damaged_lately: DamagedTimer::of_data(rge_static_object.damaged_lately_timer as u8),
        place: Static::at(Position::new(
            rge_static_object.tx,
            rge_static_object.ty,
            rge_static_object.tz,
        )),
        // every animated object's current sprite, which slot 0x68 and the sprite list read
        sprite: masters
            .and_then(|master_table| saved_sprite(rge_static_object, &master_table.graphics)),
        ..Unit::default()
    };
    let mut actions = Action::default();
    let default = default_task(object, rge_static_object, objects, civ, masters);
    let imported = if saved {
        saved_actions(
            object,
            rge_static_object,
            objects,
            // the Build's task with the builder's powerup (its combat layer's)
            |master| {
                build_task(objects, civ, masters, master).map(|mut build_task| {
                    build_task.work_rate += combat.work_rate_powerup;
                    build_task
                })
            },
            |master| fly_task(objects, civ, masters, master),
            |master, index| task_at(objects, civ, masters, master, index),
            masters.map(|master_table| &master_table.graphics),
            &mut actions,
        )
    } else {
        // an object without actions in a save: its default task's action (an Artifact exists
        // from the load on; a Fly starts in its first update)
        match &default {
            Ok(Some(DefaultStart::Artifact(artifact))) => {
                actions.list.set_artifact(Some(**artifact));
                Ok(())
            }
            Ok(_) => Ok(()),
            Err(unstartable) => Err(unstartable.error()),
        }
    };
    actions.default_task = default_start(default, saved);
    let production = production(object).map(|_| ());
    let (mover, moving) = match mover(object, rge_static_object, objects, paths, civ, masters) {
        Ok(mover) => (mover, Ok(())),
        Err(error) => (None, Err(error)),
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
        Err(unstartable) => DefaultTask::Fails(unstartable),
    }
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
) -> GenieResult<(Option<UnitAi>, TownBell)> {
    let bell = object.combat().map(town_bell).transpose()?;
    Ok((unit_ai(object, own, players)?, bell.unwrap_or_default()))
}

/// A combat object's town-bell flag and back-to-work records.
///
/// # Errors
///
/// A record's action value no action holds ([`AiAction::try_from`]).
pub fn town_bell(tribe_combat_object: &TribeCombatObject) -> GenieResult<TownBell> {
    use crate::orders::town_bell::BackToWork;
    Ok(TownBell {
        flag: BellShelter::of_byte(tribe_combat_object.town_bell_flag),
        work: BackToWork {
            target: ObjectId::of_field(tribe_combat_object.back_to_work1_town_bell_target_id),
            target_class: ClassId::of_param(
                tribe_combat_object.back_to_work1_town_bell_target_kind,
            )?,
            action: AiAction::of_field(tribe_combat_object.back_to_work1_town_bell_action)?,
            target2: ObjectId::of_field(tribe_combat_object.back_to_work1_town_bell_target_id2),
            point: Point::new(
                tribe_combat_object.back_to_work1_town_bell_target_x,
                tribe_combat_object.back_to_work1_town_bell_target_y,
            ),
        },
        before_building: BackToWork {
            target: ObjectId::of_field(tribe_combat_object.back_to_work2_town_bell_target_id),
            target_class: ClassId::of_param(
                tribe_combat_object.back_to_work2_town_bell_target_kind,
            )?,
            action: AiAction::of_field(tribe_combat_object.back_to_work2_town_bell_action)?,
            target2: ObjectId::of_field(tribe_combat_object.back_to_work2_town_bell_target_id2),
            point: Point::new(
                tribe_combat_object.back_to_work2_town_bell_target_x,
                tribe_combat_object.back_to_work2_town_bell_target_y,
            ),
        },
    })
}

/// A saved Build's or Repair's sub-action other than a `MoveTo`.
pub const OTHER_SUB_ACTION: GenieError =
    GenieError::unsupported("a saved Build or Repair action with another sub-action");

/// An object's unit group link as saved (`mGroupID`, `mGroupFlag`).
#[must_use]
pub const fn group_link(rge_static_object: &RgeStaticObject) -> GroupLink {
    GroupLink {
        id: GroupId::of_field(rge_static_object.group_id),
        flag: rge_static_object.group_flag,
    }
}

#[cfg(test)]
mod tests;
