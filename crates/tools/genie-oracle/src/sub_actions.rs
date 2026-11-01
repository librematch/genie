//! A saved unit's reload, sprite and Attack, its Build and Fly tasks, a building's production
//! and gather point, and the default task an object's first update starts.

use crate::masters::dat_id;
use crate::masters::{DefaultAction, MasterTable};
use crate::top_actions::{first_action, saved_move_to};
use crate::{combat_master, core_task};
use genie_core::GenieError;
use genie_core::map::Distance;
use genie_core::map::Position;
use genie_core::movement::move_to::TargetMoved;
use genie_core::objects::AnimateState;
use genie_core::objects::Frame;
use genie_core::objects::ObjectId;
use genie_core::objects::SpriteId;
use genie_core::objects::WorkRate;
use genie_core::objects::masters::ACTION_FACTORY_TYPES;
use genie_core::objects::masters::{ActionType, MasterId, action_type};
use genie_core::ownership::artifact::ARTIFACT_TYPE;
use genie_core::players::Amount;
use genie_core::players::CivId;
use genie_core::players::attribute::AttributeId;
use genie_core::production::EntrySerial;
use genie_core::production::QueueButton;
use genie_core::production::QueueChanges;
use genie_core::production::QueueLength;
use genie_core::production::QueuedUnits;
use genie_core::world::Seconds;
use genie_core::world::WholeSeconds;
use genie_save::generated::world::{Master, PlayerObjects, RgeStaticObject};

/// A master the table does not have (the save names one the data file lacks).
const NOT_IN_TABLE: GenieError = GenieError::unsupported("a master the table does not have");

/// A saved unit's reload (the Strike's): its `mReloadStinger` plus its master's
/// `speed_of_attack` less its `mReloadPowerup`; None without a combat master.
pub(super) fn saved_reload(
    object: &genie_save::Object,
    s: &RgeStaticObject,
    objects: &PlayerObjects,
) -> Option<f32> {
    let c = object.combat()?;
    let master = objects
        .masters
        .iter()
        .flatten()
        .map(|m| &m.master)
        .find(|m| m.static_part().is_some_and(|ms| ms.id() == s.master_obj))?;
    let rc = &master
        .tribe_master_combat_object
        .as_deref()?
        .rge_master_combat_object;
    Some(c.reload_stinger + (rc.speed_of_attack - c.reload_powerup))
}

/// The unit's current sprite as the save holds it: the node of its sprite list
/// (`temp_load_sprite_list`, `FUN_1413b87c0`) that shows its sprite (`+0x18`), an animated one
/// (type 2) with its frame and clock, a static one (type 1) with its frame; None without one or
/// for a graphic the table lacks.
pub(super) fn saved_sprite(
    s: &RgeStaticObject,
    graphics: &genie_core::objects::Graphics,
) -> Option<genie_core::objects::sprite::ActiveSprite> {
    let list = s.active_sprite_list.as_deref()?;
    let graphic = SpriteId::of_data(s.sprite)
        .and_then(|id| graphics.get(id))
        .copied()?;
    let nodes = || {
        list.sprite_type_list
            .iter()
            .filter_map(|e| e.type_.as_deref())
    };
    let Some(node) = nodes()
        .filter_map(|n| n.rge_active_animated_sprite.as_deref())
        .find(|a| a.rge_active_sprite.sprite_id as i16 == s.sprite)
    else {
        let node = nodes()
            .filter_map(|n| n.rge_active_sprite.as_deref())
            .find(|a| a.sprite_id as i16 == s.sprite)?;
        return Some(genie_core::objects::sprite::ActiveSprite::at_rest(
            graphic,
            Frame::of_data(node.frame),
        ));
    };
    Some(genie_core::objects::sprite::ActiveSprite {
        graphic,
        frame: Frame::of_data(node.rge_active_sprite.frame),
        interval: Seconds::new(node.animate_interval),
        last: Seconds::new(node.animate_last),
        last_frame: Frame::of_data(node.last_frame as i16),
        changed: node.frame_changed != 0,
        looped: node.frame_looped != 0,
        animate: AnimateState::of_data(node.animate_flag),
        speed: genie_core::movement::Speed::new(node.last_speed_value),
    })
}

/// A saved Attack sub-action (type 9, `FUN_140a7c880`): its target, state, timer, ranges, point,
/// the target's last place (`+0xa4`..), a hit still to land (`+0x9e`), its `MoveTo` (in states 5
/// and 11 with that `MoveTo`'s point), and its sprites
/// and frames. The load restores the sprites at `+0x70`, `+0x80` and `+0x88` from the second to
/// fourth saved ids (the first goes to `+0x68`); the second attack sprite (`+0x78`) and the
/// swing's times (`+0xb0`, `+0xb4`) are not saved, and a loaded Attack starts without them.
pub(super) fn saved_attack(
    sub: &genie_save::generated::world::Action,
    graphics: Option<&genie_core::objects::Graphics>,
) -> Result<genie_core::combat::attack::Attack, GenieError> {
    use genie_core::movement::move_to::MOVE_TO;
    let b = sub
        .base()
        .ok_or(GenieError::unsupported("a saved Attack without its base"))?;
    let a = sub
        .rge_action_by_type
        .as_deref()
        .and_then(|t| t.rge_action_attack.as_deref())
        .ok_or(GenieError::unsupported(
            "a saved Attack without its own part",
        ))?;
    let graphics = graphics.ok_or(GenieError::unsupported(
        "a saved Attack without the sprites' table",
    ))?;
    let g = |id: u16| SpriteId::of_data(id as i16).and_then(|id| graphics.get(id).copied());
    // every MoveTo of the Attack is quiet (`FUN_140a7a170` with 1)
    let move_to = match first_action(&b.sub_actions)? {
        None => None,
        Some((MOVE_TO, s2)) => Some(saved_move_to(s2, Some(graphics), true)?),
        Some(_) => {
            return Err(GenieError::unsupported(
                "a saved Attack with another sub-action",
            ));
        }
    };
    Ok(genie_core::combat::attack::Attack {
        target: ObjectId::of_field(b.target_id),
        target2: ObjectId::of_field(b.target2_id),
        state: genie_core::combat::attack::AttackState::of_byte(b.state)?,
        timer: Seconds::new(b.timer),
        range: Distance::new(a.range),
        min_range: Distance::new(a.min_range),
        point: Position::new(b.target_x, b.target_y, b.target_z),
        seen: Position::new(
            a.last_target_position_x,
            a.last_target_position_y,
            a.last_target_position_z,
        ),
        move_to,
        pending: a.need_to_attack != 0,
        swing: Seconds::ZERO,
        rest: Seconds::ZERO,
        arms: genie_core::combat::attack::Arms {
            move_sprite: g(a.short_val),
            sprite: g(a.short_val_2),
            second_sprite: None,
            idle_sprite: g(a.short_val_3),
            idle_attack_sprite: g(a.short_val_4),
            missile: MasterId::of_data(a.missile_id),
            fire_frame: Frame::of_data(a.frame_delay),
            interrupt_frame: Frame::of_data(a.interrupt_animation_at_frame),
        },
        sub_action: b.sub_action_value != 0,
        on_target_moved: TargetMoved::try_from(b.target_moved_state)?,
        same_owner: a.was_same_owner != 0,
    })
}

/// A builder's Build task (the game's Build action keeps a pointer to it, in its master's saved
/// task list or, for a master that saves none, in the `.dat`'s): its work range (task `+0x30`)
/// and work value (`+0x28`), with the master's work rate (its action part's `work_rate`).
pub(super) fn build_task(
    objects: &PlayerObjects,
    civ: CivId,
    masters: Option<&MasterTable>,
    master: i16,
) -> Result<genie_core::construction::build::BuildTask, GenieError> {
    let m = objects.master(master).ok_or(GenieError::unsupported(
        "a builder whose master is not saved",
    ))?;
    let work_rate = combat_master(m)
        .map(|c| c.rge_master_action_object.work_rate)
        .ok_or(GenieError::unsupported(
            "a builder master without an action part",
        ))?;
    let task = master_build_task(objects, civ, masters, m, master)?;
    Ok(genie_core::construction::build::BuildTask::of(
        &task,
        WorkRate::new(work_rate),
    ))
}

/// A bird master's Fly task: its saved task list's, else the `.dat`'s.
pub(super) fn fly_task(
    objects: &PlayerObjects,
    civ: CivId,
    masters: Option<&MasterTable>,
    master: i16,
) -> Result<genie_core::objects::masters::Task, GenieError> {
    let fly = genie_core::movement::fly::FLY;
    let saved = objects
        .master(master)
        .and_then(|m| m.rge_master.as_deref())
        .and_then(|r| r.rge_master_action_object.as_deref())
        .and_then(|a| a.tasks.as_deref());
    if let Some(tasks) = saved {
        let entry = tasks
            .list
            .iter()
            .find(|e| ActionType::of_data(e.task.action_type) == Some(fly))
            .ok_or(GenieError::unsupported(
                "a bird whose master has no Fly task",
            ))?;
        return core_task(&entry.task);
    }
    masters
        .and_then(|t| t.work(civ, dat_id(objects, master)?))
        .and_then(|w| w.tasks.iter().find(|t| t.action_type == fly))
        .copied()
        .ok_or(GenieError::unsupported(
            "a bird whose Fly task is not in the data file",
        ))
}

/// A builder master's Build task: its saved task list's, else the `.dat`'s.
pub(super) fn master_build_task(
    objects: &PlayerObjects,
    civ: CivId,
    masters: Option<&MasterTable>,
    m: &Master,
    master: i16,
) -> Result<genie_core::objects::masters::Task, GenieError> {
    let saved = m.tribe_master_combat_object.as_deref().and_then(|c| {
        c.rge_master_combat_object
            .rge_master_action_object
            .tasks
            .as_deref()
    });
    let build = action_type::BUILD;
    if let Some(tasks) = saved {
        let entry = tasks
            .list
            .iter()
            .find(|e| ActionType::of_data(e.task.action_type) == Some(build))
            .ok_or(GenieError::unsupported(
                "a builder whose master has no Build task",
            ))?;
        return core_task(&entry.task);
    }
    masters
        .and_then(|t| t.work(civ, dat_id(objects, master)?))
        .and_then(|w| w.tasks.iter().find(|t| t.action_type == build))
        .copied()
        .ok_or(GenieError::unsupported(
            "a builder whose Build task is not in the data file",
        ))
}

/// A saved building's production (`TRIBE_Building_Object`'s queue, its production list's action
/// and its local lists, `FUN_1411df160`): every slot of the queue (`mProductionQueueSize` of them,
/// the stale ones past the count too) with its count, total and enabled flag; the load numbers the
/// slots' serials from 0 and leaves the building's next serial (`+0xcd8`) at the slot count; the
/// change counter (`+0xc00`) and `+0xc01` start at 0. The action: a unit's make action or a
/// research, each with its state and `+0x60` (the base's `subActionValue`). `Ok(None)`: a building
/// without slots, action or local lists.
pub(super) fn production(
    object: &genie_save::Object,
) -> Result<Option<genie_core::production::Production>, GenieError> {
    use genie_core::production::{
        Entry, MakeObj, MakeObjState, MakeTech, MakeTechState, Making, Production, Queue,
    };
    let Some(building) = object.tribe_building_object.as_deref() else {
        return Ok(None);
    };
    let slots = building
        .production_queue
        .iter()
        .enumerate()
        .map(|(k, e)| {
            let c = &e.production_queue_costs;
            let word = |k: usize| [c[k], c[k + 1], c[k + 2], c[k + 3]];
            // an entry's attribute (an `int`, −1 for none) and amount
            let cost = |j: usize| -> Result<(Option<AttributeId>, Amount), GenieError> {
                let kind = i32::from_le_bytes(word(8 * j));
                let kind = i16::try_from(kind).map_err(|_| {
                    GenieError::invalid_input("a queue cost's attribute past the attributes")
                        .with("attribute", kind)
                })?;
                Ok((
                    AttributeId::of_field(kind)?,
                    Amount::new(f32::from_le_bytes(word(8 * j + 4))),
                ))
            };
            Ok(Entry {
                master: MasterId::of_data(e.production_queue_master_unit_id),
                count: QueuedUnits::of_data(e.production_queue_unit_count),
                tech: genie_core::technology::TechId::of_data(e.production_queue_tech_id),
                costs: [cost(0)?, cost(1)?, cost(2)?],
                serial: EntrySerial::new(k as i64),
                button: QueueButton::of_data(e.production_queue_button_location_id),
                train_time: WholeSeconds::of_data(e.production_queue_train_time),
            })
        })
        .collect::<Result<Vec<Entry>, GenieError>>()?;
    let action = match first_action(&building.production_queue_actions)? {
        None => None,
        Some((action_type::MAKE_OBJECT, action)) => {
            let make = action
                .tribe_action_make_obj
                .as_deref()
                .ok_or(GenieError::unsupported(
                    "a saved make action without its part",
                ))?;
            let base = action.base().ok_or(GenieError::unsupported(
                "a saved make action without its base",
            ))?;
            Some(Making::Unit(MakeObj {
                state: MakeObjState::of_byte(base.state)?,
                paid: base.sub_action_value != 0,
                master: MasterId::of_data(make.obj_id),
                progress: Seconds::new(make.work_done),
                unique_id: make.unique_id,
                button: QueueButton::of_data(make.button_location_id),
                train_time: WholeSeconds::of_data(make.train_time),
            }))
        }
        Some((action_type::MAKE_TECH, action)) => {
            let tech = action
                .tribe_action_make_tech
                .as_deref()
                .ok_or(GenieError::unsupported(
                    "a saved research action without its part",
                ))?;
            let base = action.base().ok_or(GenieError::unsupported(
                "a saved research action without its base",
            ))?;
            Some(Making::Research(MakeTech {
                state: MakeTechState::of_byte(base.state)?,
                paid: base.sub_action_value != 0,
                tech: genie_core::technology::TechId::try_from(tech.tech_id)?,
                master: tech.unique_id,
                progress: Seconds::new(tech.m_research_progress),
            }))
        }
        Some(_) => {
            return Err(GenieError::unsupported(
                "a saved production action other than making a unit",
            ));
        }
    };
    let (disabled, available, researched) = (&building.value, &building.value_2, &building.value_3);
    if slots.is_empty()
        && action.is_none()
        && disabled.is_empty()
        && available.is_empty()
        && researched.is_empty()
    {
        return Ok(None);
    }
    let next_serial = EntrySerial::new(slots.len() as i64);
    Ok(Some(Production {
        queue: Queue {
            slots,
            count: QueueLength::of_data(building.production_queue_count),
            total: QueuedUnits::of_data(building.production_queue_total_units),
            enabled: building.production_queue_enabled != 0,
            changes: QueueChanges::NONE,
            waiting: false,
            next_serial,
        },
        action,
        disabled_techs: disabled
            .iter()
            .filter_map(|&t| genie_core::technology::TechId::of_data(t))
            .collect(),
        researched_techs: researched
            .iter()
            .filter_map(|&t| genie_core::technology::TechId::of_data(t))
            .collect(),
        available_units: available
            .iter()
            .filter_map(|&m| MasterId::of_data(m))
            .collect(),
    }))
}

/// A building's gather point, if it has one (`gather_point_exists`, its state; 0 is none).
///
/// # Errors
///
/// A saved state past a byte.
pub(super) fn gather_point(
    object: &genie_save::Object,
) -> Result<Option<genie_core::commands::GatherPoint>, GenieError> {
    let Some(b) = object.tribe_building_object.as_deref() else {
        return Ok(None);
    };
    let Some(state) = genie_core::commands::GatherPointState::of_saved(b.gather_point_exists)?
    else {
        return Ok(None);
    };
    Ok(Some(genie_core::commands::GatherPoint {
        state,
        point: Position::new(b.gather_point_x, b.gather_point_y, b.gather_point_z),
        target: ObjectId::of_field(b.gather_point_target),
        target_master: MasterId::of_data(b.gather_point_target_master),
    }))
}

/// A default task's action the slice models.
pub(super) enum DefaultStart {
    /// The Artifact action (type 107, livestock's).
    Artifact(genie_core::ownership::artifact::Artifact),
    /// A bird's Fly (type 10) with its task.
    Fly(genie_core::objects::masters::Task),
    /// A farm's or fish trap's Make (type 21).
    Make,
}

/// What an object's first update starts of its master's default task (`RGE_Action_Object::update`,
/// 0x14106ac90): only action objects (types 40 and up) have one. The default task is game data:
/// the table's, from the `.dat`, or the object's own master's. The slice models the Artifact action
/// (type 107, the default task of livestock) and a bird's Fly (type 10); any other default task is
/// reported.
pub(super) fn default_task(
    object: &genie_save::Object,
    s: &RgeStaticObject,
    objects: &PlayerObjects,
    civ: CivId,
    masters: Option<&MasterTable>,
) -> Result<Option<DefaultStart>, GenieError> {
    if !is_action_object(object) {
        return Ok(None);
    }
    let action = match s.own_master() {
        Some(_) => match own_default_task(s) {
            Some(-1) | None => DefaultAction::None,
            Some(_) => {
                return Err(GenieError::unsupported(
                    "the default task of an object's own master",
                ));
            }
        },
        None => masters
            .ok_or(GenieError::unsupported(
                "no master table: the default task is unknown",
            ))?
            .default_action(civ, dat_id(objects, s.master_obj).ok_or(NOT_IN_TABLE)?)
            .ok_or(NOT_IN_TABLE)?,
    };
    match action {
        DefaultAction::None => Ok(None),
        DefaultAction::Type(ARTIFACT_TYPE) => {
            // the factory gives the action the default task, whose moving sprite its MoveTos take
            let moving = masters
                .and_then(|t| t.work(civ, dat_id(objects, s.master_obj)?))
                .and_then(genie_core::objects::masters::Master::default_task)
                .and_then(|t| t.sprites.moving);
            Ok(Some(DefaultStart::Artifact(
                genie_core::ownership::artifact::Artifact::new(s.owner == 0, moving),
            )))
        }
        DefaultAction::Type(genie_core::movement::fly::FLY) => {
            fly_task(objects, civ, masters, s.master_obj).map(|t| Some(DefaultStart::Fly(t)))
        }
        DefaultAction::Type(genie_core::gathering::make::MAKE) => Ok(Some(DefaultStart::Make)),
        // the action list's factory (`FUN_140aca4b0`, then the RGE one `FUN_140a97e00`) makes
        // no action of the other types: the default task starts nothing (wild animals' Explore,
        // type 6)
        DefaultAction::Type(t) if !ACTION_FACTORY_TYPES.contains(&t) => Ok(None),
        DefaultAction::Type(_) => Err(GenieError::unsupported(
            "a default task other than the Artifact action",
        )),
        DefaultAction::NoSuchTask => Err(GenieError::unsupported(
            "a default task id that names no task",
        )),
    }
}

/// Whether the object runs `RGE_Action_Object::update`: types 40 (action), 50 (combat), 60
/// (missile), 70 (tribe combat) and 80 (building).
pub(super) fn is_action_object(object: &genie_save::Object) -> bool {
    object.combat().is_some()
        || object.rge_object.as_deref().is_some_and(|r| {
            r.rge_action_object.is_some()
                || r.rge_combat_object.is_some()
                || r.rge_missile_object.is_some()
        })
}

/// The default task id of an object's own action master, if it has one.
pub(super) fn own_default_task(s: &RgeStaticObject) -> Option<i16> {
    let tribe = |c: &genie_save::generated::world::OwnTribeMasterCombatObject| {
        c.own_rge_master_combat_object
            .own_rge_master_action_object
            .default_task
    };
    None.or_else(|| {
        s.rge_master_action_object
            .as_deref()
            .map(|a| a.default_task)
    })
    .or_else(|| {
        s.rge_master_missile_object.as_deref().map(|m| {
            m.own_rge_master_combat_object
                .own_rge_master_action_object
                .default_task
        })
    })
    .or_else(|| s.tribe_master_combat_object.as_deref().map(tribe))
    .or_else(|| {
        s.tribe_master_building_object
            .as_deref()
            .map(|b| tribe(&b.own_tribe_master_combat_object))
    })
}

#[cfg(test)]
mod tests;
