//! A saved unit's reload, sprite and Attack, its Build and Fly tasks, a building's production
//! and gather point, and the default task an object's first update starts.

use crate::combat::AttackTargetId;
use crate::combat::attack::{Arms, Attack, AttackState};
use crate::commands::{GatherPoint, GatherPointState};
use crate::construction::build::BuildTask;
use crate::gathering::make::MAKE;
use crate::map::Distance;
use crate::map::Position;
use crate::movement::Speed;
use crate::movement::fly::FLY;
use crate::movement::move_to::TargetMoved;
use crate::objects::AnimateState;
use crate::objects::Frame;
use crate::objects::Graphics;
use crate::objects::ObjectId;
use crate::objects::SpriteId;
use crate::objects::Voice;
use crate::objects::WorkRate;
use crate::objects::layers::{DefaultTask, UnreadDefault, Unstartable};
use crate::objects::masters::ACTION_FACTORY_KINDS;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterId;
use crate::objects::masters::{Master as MastersMaster, Task};
use crate::objects::sprite::ActiveSprite;
use crate::ownership::artifact::ARTIFACT_KIND;
use crate::ownership::artifact::Artifact;
use crate::players::Amount;
use crate::players::CivId;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::production::EntrySerial;
use crate::production::Production;
use crate::production::QueueButton;
use crate::production::QueueChanges;
use crate::production::QueueLength;
use crate::production::QueuedUnits;
use crate::technology::TechId;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::load::top_actions::{first_action, saved_sub_move_to};
use crate::{GenieError, GenieResult};
use genie_import::dat::dat_id;
use genie_import::dat::{DefaultAction, MasterTable};
use genie_import::save::parts::combat_master;
use genie_import::save::tasks::core_task;
use genie_save::generated::world::Action;
use genie_save::generated::world::{Master, PlayerObjects, RgeStaticObject};

/// A saved unit's reload (the Strike's): its `mReloadStinger` plus its master's
/// `speed_of_attack` less its `mReloadPowerup`; None without a combat master.
#[must_use]
pub fn saved_reload(
    object: &genie_save::Object,
    rge_static_object: &RgeStaticObject,
    objects: &PlayerObjects,
) -> Option<f32> {
    let tribe_combat_object = object.combat()?;
    let master = objects
        .masters
        .iter()
        .flatten()
        .map(|player_master| &player_master.master)
        .find(|master| {
            master
                .static_part()
                .is_some_and(|ms| ms.id() == rge_static_object.master_obj)
        })?;
    let rc = &master
        .tribe_master_combat_object
        .as_deref()?
        .rge_master_combat_object;
    Some(
        tribe_combat_object.reload_stinger
            + (rc.speed_of_attack - tribe_combat_object.reload_powerup),
    )
}

/// The unit's current sprite as the save holds it: the node of its sprite list
/// (`temp_load_sprite_list`, `FUN_1413b87c0`) that shows its sprite (`+0x18`), an animated one
/// (type 2) with its frame and clock, a static one (type 1) with its frame; None without one or
/// for a graphic the table lacks.
#[must_use]
pub fn saved_sprite(
    rge_static_object: &RgeStaticObject,
    graphics: &Graphics,
) -> Option<ActiveSprite> {
    let list = rge_static_object.active_sprite_list.as_deref()?;
    let graphic = SpriteId::of_data(rge_static_object.sprite)
        .and_then(|id| graphics.get(id))
        .copied()?;
    let nodes = || {
        list.sprite_kind_list
            .iter()
            .filter_map(|entry| entry.kind.as_deref())
    };
    let Some(node) = nodes()
        .filter_map(|active_sprite| active_sprite.rge_active_animated_sprite.as_deref())
        .find(|rge_active_animated_sprite| {
            rge_active_animated_sprite.rge_active_sprite.sprite_id as i16
                == rge_static_object.sprite
        })
    else {
        let node = nodes()
            .filter_map(|active_sprite| active_sprite.rge_active_sprite.as_deref())
            .find(|rge_active_sprite| {
                rge_active_sprite.sprite_id as i16 == rge_static_object.sprite
            })?;
        return Some(ActiveSprite::at_rest(graphic, Frame::of_data(node.frame)));
    };
    Some(ActiveSprite {
        graphic,
        frame: Frame::of_data(node.rge_active_sprite.frame),
        interval: Seconds::new(node.animate_interval),
        last: Seconds::new(node.animate_last),
        last_frame: Frame::of_data(node.last_frame as i16),
        changed: node.frame_changed != 0,
        looped: node.frame_looped != 0,
        animate: AnimateState::of_data(node.animate_flag),
        speed: Speed::new(node.last_speed_value),
    })
}

/// A saved Attack sub-action (type 9, `FUN_140a7c880`): its target, state, timer, ranges, point,
/// the target's last place (`+0xa4`..), a hit still to land (`+0x9e`), its `MoveTo` (in its walk's
/// state: in states 5 and 11 with that `MoveTo`'s point; in another state the import's error),
/// and its sprites
/// and frames. The load restores the sprites at `+0x70`, `+0x80` and `+0x88` from the second to
/// fourth saved ids (the first goes to `+0x68`); the second attack sprite (`+0x78`) and the
/// swing's times (`+0xb0`, `+0xb4`) are not saved, and a loaded Attack starts without them.
pub fn saved_attack(sub: &Action, graphics: Option<&Graphics>) -> GenieResult<Attack> {
    use crate::movement::move_to::MOVE_TO;
    let rge_action = sub
        .base()
        .ok_or(GenieError::unsupported("a saved Attack without its base"))?;
    let rge_action_attack = sub
        .rge_action_by_kind
        .as_deref()
        .and_then(|rge_action_by_kind| rge_action_by_kind.rge_action_attack.as_deref())
        .ok_or(GenieError::unsupported(
            "a saved Attack without its own part",
        ))?;
    let graphics = graphics.ok_or(GenieError::unsupported(
        "a saved Attack without the sprites' table",
    ))?;
    let graphic_of =
        |id: u16| SpriteId::of_data(id as i16).and_then(|id| graphics.get(id).copied());
    // every MoveTo of the Attack is quiet (`FUN_140a7a170` with 1)
    let move_to = match first_action(&rge_action.sub_actions)? {
        None => None,
        Some((MOVE_TO, s2)) => Some(saved_sub_move_to(s2, Some(graphics))?),
        Some(_) => {
            return Err(GenieError::unsupported(
                "a saved Attack with another sub-action",
            ));
        }
    };
    Ok(Attack {
        target: AttackTargetId::of_field(rge_action.target_id),
        target2: AttackTargetId::of_field(rge_action.target2_id),
        state: AttackState::of_byte(rge_action.state, move_to)?,
        timer: Seconds::new(rge_action.timer),
        range: Distance::new(rge_action_attack.range),
        min_range: Distance::new(rge_action_attack.min_range),
        point: Position::new(
            rge_action.target_x,
            rge_action.target_y,
            rge_action.target_z,
        ),
        seen: Position::new(
            rge_action_attack.last_target_position_x,
            rge_action_attack.last_target_position_y,
            rge_action_attack.last_target_position_z,
        ),
        pending: rge_action_attack.need_to_attack != 0,
        swing: Seconds::ZERO,
        rest: Seconds::ZERO,
        arms: Arms {
            move_sprite: graphic_of(rge_action_attack.short_val),
            sprite: graphic_of(rge_action_attack.short_val_2),
            second_sprite: None,
            idle_sprite: graphic_of(rge_action_attack.short_val_3),
            idle_attack_sprite: graphic_of(rge_action_attack.short_val_4),
            missile: MasterId::of_data(rge_action_attack.missile_id),
            fire_frame: Frame::of_data(rge_action_attack.frame_delay),
            interrupt_frame: Frame::of_data(rge_action_attack.interrupt_animation_at_frame),
        },
        voice: Voice::of_byte(rge_action.sub_action_value),
        on_target_moved: TargetMoved::try_from(rge_action.target_moved_state)?,
        same_owner: rge_action_attack.was_same_owner != 0,
    })
}

/// A bird whose master's task list holds no Fly task.
const NO_FLY_TASK: GenieError = GenieError::unsupported("a bird whose master has no Fly task");

/// A builder whose master's task list holds no Build task.
const NO_BUILD_TASK: GenieError =
    GenieError::unsupported("a builder whose master has no Build task");

/// The first task of type `kind` of an object's own master `own` (the action reads it through
/// the object's master pointer, `+0xa0`), else `missing`.
///
/// # Errors
///
/// `missing`, for a master without such a task.
fn own_task_of_kind(
    own: &MastersMaster,
    kind: ActionKind,
    missing: GenieError,
) -> GenieResult<Task> {
    own.action_part()
        .tasks
        .iter()
        .find(|task| task.action_kind == kind)
        .copied()
        .ok_or(missing)
}

/// [`fly_task`] of a bird of its own master `own`: that master's Fly task.
///
/// # Errors
///
/// A master without a Fly task.
pub fn own_fly_task(own: &MastersMaster) -> GenieResult<Task> {
    own_task_of_kind(own, FLY, NO_FLY_TASK)
}

/// [`build_task`] of a builder of its own master `own`: that master's Build task with its work
/// rate.
///
/// # Errors
///
/// A master without a Build task.
pub fn own_build_task(own: &MastersMaster) -> GenieResult<BuildTask> {
    let task = own_task_of_kind(own, ActionKind::BUILD, NO_BUILD_TASK)?;
    Ok(BuildTask::of(&task, own.action_part().work_rate))
}

/// A builder's Build task (the game's Build action keeps a pointer to it, in its master's saved
/// task list or, for a master that saves none, in the `.dat`'s): its work range (task `+0x30`)
/// and work value (`+0x28`), with the master's work rate (its action part's `work_rate`).
pub fn build_task(
    objects: &PlayerObjects,
    civ: CivId,
    masters: Option<&MasterTable>,
    master: i16,
) -> GenieResult<BuildTask> {
    let builder = objects.master(master).ok_or(GenieError::unsupported(
        "a builder whose master is not saved",
    ))?;
    let work_rate = combat_master(builder)
        .map(|rge_master_combat_object| rge_master_combat_object.rge_master_action_object.work_rate)
        .ok_or(GenieError::unsupported(
            "a builder master without an action part",
        ))?;
    let task = master_build_task(objects, civ, masters, builder, master)?;
    Ok(BuildTask::of(&task, WorkRate::new(work_rate)))
}

/// A bird master's Fly task: its saved task list's, else the `.dat`'s.
pub fn fly_task(
    objects: &PlayerObjects,
    civ: CivId,
    masters: Option<&MasterTable>,
    master: i16,
) -> GenieResult<Task> {
    let fly = FLY;
    let saved = objects
        .master(master)
        .and_then(|master| master.rge_master.as_deref())
        .and_then(|rge_master| rge_master.rge_master_action_object.as_deref())
        .and_then(|rge_master_action_object| rge_master_action_object.tasks.as_deref());
    if let Some(tasks) = saved {
        let entry = tasks
            .list
            .iter()
            .find(|entry| ActionKind::of_data(entry.task.action_kind) == Some(fly))
            .ok_or(NO_FLY_TASK)?;
        return core_task(&entry.task);
    }
    masters
        .and_then(|master_table| master_table.work(civ, dat_id(objects, master)?))
        .and_then(|master| {
            master
                .action_part()
                .tasks
                .iter()
                .find(|task| task.action_kind == fly)
        })
        .copied()
        .ok_or(GenieError::unsupported(
            "a bird whose Fly task is not in the data file",
        ))
}

/// A builder master's Build task: its saved task list's, else the `.dat`'s.
pub fn master_build_task(
    objects: &PlayerObjects,
    civ: CivId,
    masters: Option<&MasterTable>,
    builder: &Master,
    master: i16,
) -> GenieResult<Task> {
    let saved =
        builder
            .tribe_master_combat_object
            .as_deref()
            .and_then(|tribe_master_combat_object| {
                tribe_master_combat_object
                    .rge_master_combat_object
                    .rge_master_action_object
                    .tasks
                    .as_deref()
            });
    let build = ActionKind::BUILD;
    if let Some(tasks) = saved {
        let entry = tasks
            .list
            .iter()
            .find(|entry| ActionKind::of_data(entry.task.action_kind) == Some(build))
            .ok_or(NO_BUILD_TASK)?;
        return core_task(&entry.task);
    }
    masters
        .and_then(|master_table| master_table.work(civ, dat_id(objects, master)?))
        .and_then(|master| {
            master
                .action_part()
                .tasks
                .iter()
                .find(|task| task.action_kind == build)
        })
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
pub fn production(object: &genie_save::Object) -> GenieResult<Option<Production>> {
    use crate::production::{
        Entry, MakeObj, MakeObjState, MakeTech, MakeTechState, Making, Production, Queue,
    };
    let Some(building) = object.tribe_building_object.as_deref() else {
        return Ok(None);
    };
    let slots = building
        .production_queue
        .iter()
        .enumerate()
        .map(|(entry_index, entry)| {
            let production_queue_costs = &entry.production_queue_costs;
            let word = |offset: usize| {
                [
                    production_queue_costs[offset],
                    production_queue_costs[offset + 1],
                    production_queue_costs[offset + 2],
                    production_queue_costs[offset + 3],
                ]
            };
            // an entry's attribute (an `int`, −1 for none) and amount
            let cost = |index: usize| -> GenieResult<(Option<AttributeId>, Amount)> {
                let kind = i32::from_le_bytes(word(8 * index));
                let kind = i16::try_from(kind).map_err(|_| {
                    GenieError::invalid_input("a queue cost's attribute past the attributes")
                        .with("attribute", kind)
                })?;
                Ok((
                    AttributeId::of_field(kind)?,
                    Amount::new(f32::from_le_bytes(word(8 * index + 4))),
                ))
            };
            Ok(Entry {
                master: MasterId::of_data(entry.production_queue_master_unit_id),
                count: QueuedUnits::of_data(entry.production_queue_unit_count),
                tech: TechId::of_data(entry.production_queue_tech_id),
                costs: [cost(0)?, cost(1)?, cost(2)?],
                serial: EntrySerial::new(entry_index as i64),
                button: QueueButton::of_data(entry.production_queue_button_location_id),
                train_time: WholeSeconds::of_data(entry.production_queue_train_time),
            })
        })
        .collect::<GenieResult<Vec<Entry>>>()?;
    let action = match first_action(&building.production_queue_actions)? {
        None => None,
        Some((ActionKind::MAKE_OBJECT, action)) => {
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
        Some((ActionKind::MAKE_TECH, action)) => {
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
                tech: TechId::try_from(tech.tech_id)?,
                master: tech.unique_id,
                progress: Seconds::new(tech.research_progress),
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
    // the queue's entries are its first `count` slots (`Queue::entries`), which the game reads
    // unchecked
    let count = QueueLength::of_data(building.production_queue_count);
    if slots.len() < count.slots() {
        return Err(GenieError::invalid_input(
            "a saved production queue that counts more entries than its slots",
        )
        .with("count", count.slots())
        .with("slots", slots.len()));
    }
    Ok(Some(Production {
        queue: Queue {
            slots,
            count,
            total: QueuedUnits::of_data(building.production_queue_total_units),
            enabled: building.production_queue_enabled != 0,
            changes: QueueChanges::NONE,
            waiting: false,
            next_serial,
        },
        action,
        disabled_techs: disabled
            .iter()
            .filter_map(|&tech| TechId::of_data(tech))
            .collect(),
        researched_techs: researched
            .iter()
            .filter_map(|&tech| TechId::of_data(tech))
            .collect(),
        available_units: available
            .iter()
            .filter_map(|&master| MasterId::of_data(master))
            .collect(),
    }))
}

/// A building's gather point, if it has one (`gather_point_exists`, its state; 0 is none).
///
/// # Errors
///
/// A saved state past a byte.
pub fn gather_point(object: &genie_save::Object) -> GenieResult<Option<GatherPoint>> {
    let Some(tribe_building_object) = object.tribe_building_object.as_deref() else {
        return Ok(None);
    };
    let Some(state) = GatherPointState::of_saved(tribe_building_object.gather_point_exists)? else {
        return Ok(None);
    };
    Ok(Some(GatherPoint {
        state,
        point: Position::new(
            tribe_building_object.gather_point_x,
            tribe_building_object.gather_point_y,
            tribe_building_object.gather_point_z,
        ),
        target: ObjectId::of_field(tribe_building_object.gather_point_target),
        target_master: MasterId::of_data(tribe_building_object.gather_point_target_master),
    }))
}

/// A default task's action the slice models.
#[derive(Debug)]
pub enum DefaultStart {
    /// The Artifact action (type 107, livestock's).
    Artifact(Box<Artifact>),
    /// A bird's Fly (type 10), with its master's default task as it starts.
    Fly,
    /// A farm's or fish trap's Make (type 21).
    Make,
}

/// What an object's first update starts of its master's default task (`RGE_Action_Object::update`,
/// 0x14106ac90): only action objects (types 40 and up) have one. The default task is game data:
/// the table's, from the `.dat`, or the object's own master's (`own`, as the import read the
/// record the object holds; none for a record it could not read). The slice models the Artifact
/// action (type 107, the default task of livestock) and a bird's Fly (type 10); any other default
/// task is a start the slice cannot make ([`Unstartable`]).
pub fn default_task(
    object: &genie_save::Object,
    rge_static_object: &RgeStaticObject,
    objects: &PlayerObjects,
    civ: CivId,
    masters: Option<&MasterTable>,
    own: Option<&MastersMaster>,
) -> Result<Option<DefaultStart>, Unstartable> {
    if !is_action_object(object) {
        return Ok(None);
    }
    let not_in_table = Unstartable::Unread(UnreadDefault::NotInTable);
    let action = match (rge_static_object.own_master(), own) {
        (Some(_), Some(own_master)) => own_default_action(own_master),
        (Some(_), None) => return Err(Unstartable::Unread(UnreadDefault::OwnMaster)),
        (None, _) => masters
            .ok_or(Unstartable::Unread(UnreadDefault::NoMasterTable))?
            .default_action(
                civ,
                dat_id(objects, rge_static_object.master_obj).ok_or(not_in_table)?,
            )
            .ok_or(not_in_table)?,
    };
    match action {
        DefaultAction::None => Ok(None),
        DefaultAction::Kind(ARTIFACT_KIND) => {
            // the factory gives the action the default task
            let task = own
                .or_else(|| {
                    masters.and_then(|master_table| {
                        master_table.work(civ, dat_id(objects, rge_static_object.master_obj)?)
                    })
                })
                .and_then(MastersMaster::default_task)
                .copied();
            Ok(Some(DefaultStart::Artifact(Box::new(Artifact::new(
                PlayerId::new(rge_static_object.owner),
                task,
            )))))
        }
        DefaultAction::Kind(FLY) => Ok(Some(DefaultStart::Fly)),
        DefaultAction::Kind(MAKE) => Ok(Some(DefaultStart::Make)),
        // the action list's factory (`FUN_140aca4b0`, then the RGE one `FUN_140a97e00`) makes
        // no action of the other types: the default task starts nothing (wild animals' Explore,
        // type 6)
        DefaultAction::Kind(kind) if !ACTION_FACTORY_KINDS.contains(&kind) => Ok(None),
        DefaultAction::Kind(action_kind) => Err(Unstartable::Unported(action_kind)),
        DefaultAction::NoSuchTask => Err(Unstartable::Unread(UnreadDefault::NoSuchTask)),
    }
}

/// Whether an object's save holds its actions: an entry in its action list, or none (every
/// object of a save from a game's start, an idle one later).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveHolds {
    /// Its action list holds an entry.
    Actions,
    /// It has no action list, or an empty one.
    NoActions,
}

impl SaveHolds {
    /// What `object`'s save holds.
    pub(crate) fn of(object: &genie_save::Object) -> Self {
        if object
            .actions()
            .is_some_and(|action_list| action_list.entries().next().is_some())
        {
            Self::Actions
        } else {
            Self::NoActions
        }
    }
}

/// What the default task's start `start` of an object starts once its action list empties, by
/// what its save `holds`: nothing, a Fly or a Make before its first update, or the Artifact
/// again. A saved object's Fly is its saved action's: none starts again (it never ends); a saved
/// Make started. A default task the slice cannot start is the caller's.
pub(crate) fn loaded_default_task(start: Option<&DefaultStart>, holds: SaveHolds) -> DefaultTask {
    match start {
        None => DefaultTask::Nothing,
        Some(DefaultStart::Fly | DefaultStart::Make) if holds == SaveHolds::Actions => {
            DefaultTask::Nothing
        }
        Some(DefaultStart::Fly) => DefaultTask::Fly,
        Some(DefaultStart::Make) => DefaultTask::Make,
        Some(DefaultStart::Artifact(_)) => DefaultTask::Artifact,
    }
}

/// The default action of an object's own master, as the table holds a shared master's: none
/// without a default task, else the type of the task at its index (`FUN_14109e0c0`).
fn own_default_action(own: &MastersMaster) -> DefaultAction {
    match (own.action_part().default_task, own.default_task()) {
        (None, _) => DefaultAction::None,
        (Some(_), None) => DefaultAction::NoSuchTask,
        (Some(_), Some(task)) => DefaultAction::Kind(task.action_kind),
    }
}

/// Whether the object runs `RGE_Action_Object::update`: types 40 (action), 50 (combat), 60
/// (missile), 70 (tribe combat) and 80 (building).
#[must_use]
pub fn is_action_object(object: &genie_save::Object) -> bool {
    object.combat().is_some()
        || object.rge_object.as_deref().is_some_and(|rge_object| {
            rge_object.rge_action_object.is_some()
                || rge_object.rge_combat_object.is_some()
                || rge_object.rge_missile_object.is_some()
        })
}

#[cfg(test)]
mod tests;
