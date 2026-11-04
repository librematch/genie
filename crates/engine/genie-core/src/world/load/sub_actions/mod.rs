//! A saved unit's reload, sprite and Attack, its Build and Fly tasks, a building's production
//! and gather point, and the default task an object's first update starts.

use crate::GenieError;
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
use crate::objects::layers::{UnreadDefault, Unstartable};
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
use crate::world::load::top_actions::{first_action, saved_move_to};
use genie_import::dat::dat_id;
use genie_import::dat::{DefaultAction, MasterTable};
use genie_import::save::core_task;
use genie_import::save::parts::combat_master;
use genie_save::generated::world::{Action, OwnTribeMasterCombatObject};
use genie_save::generated::world::{Master, PlayerObjects, RgeStaticObject};

/// A saved unit's reload (the Strike's): its `mReloadStinger` plus its master's
/// `speed_of_attack` less its `mReloadPowerup`; None without a combat master.
#[must_use]
pub fn saved_reload(
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
#[must_use]
pub fn saved_sprite(s: &RgeStaticObject, graphics: &Graphics) -> Option<ActiveSprite> {
    let list = s.active_sprite_list.as_deref()?;
    let graphic = SpriteId::of_data(s.sprite)
        .and_then(|id| graphics.get(id))
        .copied()?;
    let nodes = || {
        list.sprite_kind_list
            .iter()
            .filter_map(|e| e.kind.as_deref())
    };
    let Some(node) = nodes()
        .filter_map(|n| n.rge_active_animated_sprite.as_deref())
        .find(|a| a.rge_active_sprite.sprite_id as i16 == s.sprite)
    else {
        let node = nodes()
            .filter_map(|n| n.rge_active_sprite.as_deref())
            .find(|a| a.sprite_id as i16 == s.sprite)?;
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
pub fn saved_attack(sub: &Action, graphics: Option<&Graphics>) -> Result<Attack, GenieError> {
    use crate::movement::move_to::MOVE_TO;
    let b = sub
        .base()
        .ok_or(GenieError::unsupported("a saved Attack without its base"))?;
    let a = sub
        .rge_action_by_kind
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
        Some((MOVE_TO, s2)) => Some(saved_move_to(s2, Some(graphics), Voice::Quiet)?),
        Some(_) => {
            return Err(GenieError::unsupported(
                "a saved Attack with another sub-action",
            ));
        }
    };
    Ok(Attack {
        target: AttackTargetId::of_field(b.target_id),
        target2: AttackTargetId::of_field(b.target2_id),
        state: AttackState::of_byte(b.state, move_to)?,
        timer: Seconds::new(b.timer),
        range: Distance::new(a.range),
        min_range: Distance::new(a.min_range),
        point: Position::new(b.target_x, b.target_y, b.target_z),
        seen: Position::new(
            a.last_target_position_x,
            a.last_target_position_y,
            a.last_target_position_z,
        ),
        pending: a.need_to_attack != 0,
        swing: Seconds::ZERO,
        rest: Seconds::ZERO,
        arms: Arms {
            move_sprite: g(a.short_val),
            sprite: g(a.short_val_2),
            second_sprite: None,
            idle_sprite: g(a.short_val_3),
            idle_attack_sprite: g(a.short_val_4),
            missile: MasterId::of_data(a.missile_id),
            fire_frame: Frame::of_data(a.frame_delay),
            interrupt_frame: Frame::of_data(a.interrupt_animation_at_frame),
        },
        voice: Voice::of_byte(b.sub_action_value),
        on_target_moved: TargetMoved::try_from(b.target_moved_state)?,
        same_owner: a.was_same_owner != 0,
    })
}

/// A builder's Build task (the game's Build action keeps a pointer to it, in its master's saved
/// task list or, for a master that saves none, in the `.dat`'s): its work range (task `+0x30`)
/// and work value (`+0x28`), with the master's work rate (its action part's `work_rate`).
pub fn build_task(
    objects: &PlayerObjects,
    civ: CivId,
    masters: Option<&MasterTable>,
    master: i16,
) -> Result<BuildTask, GenieError> {
    let m = objects.master(master).ok_or(GenieError::unsupported(
        "a builder whose master is not saved",
    ))?;
    let work_rate = combat_master(m)
        .map(|c| c.rge_master_action_object.work_rate)
        .ok_or(GenieError::unsupported(
            "a builder master without an action part",
        ))?;
    let task = master_build_task(objects, civ, masters, m, master)?;
    Ok(BuildTask::of(&task, WorkRate::new(work_rate)))
}

/// A bird master's Fly task: its saved task list's, else the `.dat`'s.
pub fn fly_task(
    objects: &PlayerObjects,
    civ: CivId,
    masters: Option<&MasterTable>,
    master: i16,
) -> Result<Task, GenieError> {
    let fly = FLY;
    let saved = objects
        .master(master)
        .and_then(|m| m.rge_master.as_deref())
        .and_then(|r| r.rge_master_action_object.as_deref())
        .and_then(|a| a.tasks.as_deref());
    if let Some(tasks) = saved {
        let entry = tasks
            .list
            .iter()
            .find(|e| ActionKind::of_data(e.task.action_kind) == Some(fly))
            .ok_or(GenieError::unsupported(
                "a bird whose master has no Fly task",
            ))?;
        return core_task(&entry.task);
    }
    masters
        .and_then(|t| t.work(civ, dat_id(objects, master)?))
        .and_then(|w| w.action_part().tasks.iter().find(|t| t.action_kind == fly))
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
    m: &Master,
    master: i16,
) -> Result<Task, GenieError> {
    let saved = m.tribe_master_combat_object.as_deref().and_then(|c| {
        c.rge_master_combat_object
            .rge_master_action_object
            .tasks
            .as_deref()
    });
    let build = ActionKind::BUILD;
    if let Some(tasks) = saved {
        let entry = tasks
            .list
            .iter()
            .find(|e| ActionKind::of_data(e.task.action_kind) == Some(build))
            .ok_or(GenieError::unsupported(
                "a builder whose master has no Build task",
            ))?;
        return core_task(&entry.task);
    }
    masters
        .and_then(|t| t.work(civ, dat_id(objects, master)?))
        .and_then(|w| {
            w.action_part()
                .tasks
                .iter()
                .find(|t| t.action_kind == build)
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
pub fn production(object: &genie_save::Object) -> Result<Option<Production>, GenieError> {
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
                tech: TechId::of_data(e.production_queue_tech_id),
                costs: [cost(0)?, cost(1)?, cost(2)?],
                serial: EntrySerial::new(k as i64),
                button: QueueButton::of_data(e.production_queue_button_location_id),
                train_time: WholeSeconds::of_data(e.production_queue_train_time),
            })
        })
        .collect::<Result<Vec<Entry>, GenieError>>()?;
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
            .filter_map(|&t| TechId::of_data(t))
            .collect(),
        researched_techs: researched
            .iter()
            .filter_map(|&t| TechId::of_data(t))
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
pub fn gather_point(object: &genie_save::Object) -> Result<Option<GatherPoint>, GenieError> {
    let Some(b) = object.tribe_building_object.as_deref() else {
        return Ok(None);
    };
    let Some(state) = GatherPointState::of_saved(b.gather_point_exists)? else {
        return Ok(None);
    };
    Ok(Some(GatherPoint {
        state,
        point: Position::new(b.gather_point_x, b.gather_point_y, b.gather_point_z),
        target: ObjectId::of_field(b.gather_point_target),
        target_master: MasterId::of_data(b.gather_point_target_master),
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
/// the table's, from the `.dat`, or the object's own master's. The slice models the Artifact action
/// (type 107, the default task of livestock) and a bird's Fly (type 10); any other default task is
/// a start the slice cannot make ([`Unstartable`]).
pub fn default_task(
    object: &genie_save::Object,
    s: &RgeStaticObject,
    objects: &PlayerObjects,
    civ: CivId,
    masters: Option<&MasterTable>,
) -> Result<Option<DefaultStart>, Unstartable> {
    if !is_action_object(object) {
        return Ok(None);
    }
    let not_in_table = Unstartable::Unread(UnreadDefault::NotInTable);
    let action = match s.own_master() {
        Some(_) => match own_default_task(s) {
            Some(-1) | None => DefaultAction::None,
            Some(_) => return Err(Unstartable::Unread(UnreadDefault::OwnMaster)),
        },
        None => masters
            .ok_or(Unstartable::Unread(UnreadDefault::NoMasterTable))?
            .default_action(civ, dat_id(objects, s.master_obj).ok_or(not_in_table)?)
            .ok_or(not_in_table)?,
    };
    match action {
        DefaultAction::None => Ok(None),
        DefaultAction::Kind(ARTIFACT_KIND) => {
            // the factory gives the action the default task, whose moving sprite its MoveTos take
            let moving = masters
                .and_then(|t| t.work(civ, dat_id(objects, s.master_obj)?))
                .and_then(MastersMaster::default_task)
                .and_then(|t| t.sprites.moving);
            Ok(Some(DefaultStart::Artifact(Box::new(Artifact::new(
                PlayerId::new(s.owner),
                moving,
            )))))
        }
        DefaultAction::Kind(FLY) => Ok(Some(DefaultStart::Fly)),
        DefaultAction::Kind(MAKE) => Ok(Some(DefaultStart::Make)),
        // the action list's factory (`FUN_140aca4b0`, then the RGE one `FUN_140a97e00`) makes
        // no action of the other types: the default task starts nothing (wild animals' Explore,
        // type 6)
        DefaultAction::Kind(t) if !ACTION_FACTORY_KINDS.contains(&t) => Ok(None),
        DefaultAction::Kind(t) => Err(Unstartable::Unported(t)),
        DefaultAction::NoSuchTask => Err(Unstartable::Unread(UnreadDefault::NoSuchTask)),
    }
}

/// Whether the object runs `RGE_Action_Object::update`: types 40 (action), 50 (combat), 60
/// (missile), 70 (tribe combat) and 80 (building).
#[must_use]
pub fn is_action_object(object: &genie_save::Object) -> bool {
    object.combat().is_some()
        || object.rge_object.as_deref().is_some_and(|r| {
            r.rge_action_object.is_some()
                || r.rge_combat_object.is_some()
                || r.rge_missile_object.is_some()
        })
}

/// The default task id of an object's own action master, if it has one.
#[must_use]
pub fn own_default_task(s: &RgeStaticObject) -> Option<i16> {
    let tribe = |c: &OwnTribeMasterCombatObject| {
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
