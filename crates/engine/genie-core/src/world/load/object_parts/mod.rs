//! A saved object's lifecycle fields and its unit AI.

use crate::garrison::ContainerId;
use crate::groups::FormationKind;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::movement::PathFlags;
use crate::movement::WaypointIndex;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::lifecycle::{Class, Lifecycle};
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::pathing::tile_path::Path;
use crate::players::Amount;
use crate::players::attribute::AttributeId;
use crate::unit_ai::AlertMode;
use crate::unit_ai::OrderPriority;
use crate::unit_ai::best_target::AttackCategory;
use crate::unit_ai::{AiAction, OrderKind};
use crate::unit_ai::{
    Attackers, Home, Notify, OrderRecord, PatrolPath, QueuedOrder, UnitAi, UnitAiKind, UnitStance,
    notify_capacity,
};
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::load::movers::vec3;
use crate::{GenieError, GenieResult};
use genie_import::dat::MasterTable;
use genie_save::generated::world::{
    PlayerObjects, RgeStaticObject, TribeCombatObject, UnitAiModule,
};

/// The object's lifecycle fields. Its sprite after its death is its own master's dying sprite
/// where it has one, else its player's master's in the save, else its current one.
///
/// # Errors
///
/// A saved state past 8.
pub fn lifecycle(
    object: &genie_save::Object,
    (rge_static_object, master_id): (&RgeStaticObject, MasterId),
    objects: &PlayerObjects,
    masters: Option<&MasterTable>,
) -> GenieResult<Lifecycle> {
    let pure_static = object
        .rge_object
        .as_deref()
        .is_some_and(|rge_object| rge_object.rge_static_object.is_some());
    let death_sprite = rge_static_object.own_master().map_or_else(
        || {
            objects
                .master(rge_static_object.master_obj)
                .and_then(|master| master.static_part())
                .and_then(|rge_master_static_object| {
                    SpriteId::of_data(rge_master_static_object.death_sprite_id)
                })
        },
        |own| SpriteId::of_data(own.death_sprite_num),
    );
    // the sprite after the death: the dying one, else the current one
    let after = death_sprite.or_else(|| SpriteId::of_data(rge_static_object.sprite));
    let death_facets = masters
        .zip(after)
        .and_then(|(after_master_table, after)| after_master_table.graphics.get(after))
        .filter(|graphic| {
            pure_static
                && graphic.flags.contains(SpriteFlags::RANDOM_FIRST_FRAME)
                && graphic.facets.several()
        })
        .map(|graphic| graphic.facets);
    Ok(Lifecycle {
        class: if pure_static {
            Class::Static
        } else {
            Class::Animated
        },
        state: ObjectState::of_byte(rge_static_object.object_state)?,
        hp: HitPoints::new(rge_static_object.hp),
        attribute_kind: AttributeId::of_data(rge_static_object.attribute_kind_held),
        amount: Amount::new(rge_static_object.attribute_amount_held),
        death_facets,
        inside: ContainerId::of_field(rge_static_object.inside_obj),
        goto_sleep: rge_static_object.goto_sleep_flag != 0,
        master_id,
    })
}

/// A combat object's unit AI as the save holds it, None without one.
///
/// # Errors
///
/// An order or action value no order or action holds ([`OrderKind::try_from`],
/// [`AiAction::try_from`]), in the current or last one, the history or a queue.
pub fn unit_ai(
    object: &genie_save::Object,
    own: ObjectId,
    players: usize,
) -> GenieResult<Option<UnitAi>> {
    let Some((combat, ai)) = object.combat().and_then(|tribe_combat_object| {
        Some((tribe_combat_object, tribe_combat_object.unit_ai()?))
    }) else {
        return Ok(None);
    };
    let ai_kind = UnitAiKind::try_from(combat.need_unit_ai as u32)?;
    Ok(Some(UnitAi {
        ai_kind,
        current_order: OrderKind::of_field(ai.current_order_value)?,
        order_priority: OrderPriority::of_data(ai.current_order_priority_value),
        current_action: AiAction::of_field(ai.current_action_value)?,
        current_target: ObjectId::of_field(ai.current_target_value),
        target_class: ClassId::of_param(ai.current_target_kind_value)?,
        target_point: Position::new(
            ai.current_target_xvalue,
            ai.current_target_yvalue,
            ai.current_target_zvalue,
        ),
        desired_distance: Distance::new(ai.desired_target_distance_value),
        defend_target: ObjectId::of_field(ai.defend_target_value),
        last_order: OrderKind::of_field(ai.last_order_value)?,
        last_action: AiAction::of_field(ai.last_action_value)?,
        last_target: ObjectId::of_field(ai.last_target_value),
        last_target_class: ClassId::of_param(ai.last_target_kind_value)?,
        idle_timer: Millis::new(ai.idle_timer_value),
        adjusted_idle_timeout: Millis::new(ai.adjusted_idle_timeout_value),
        idle_timeout: Millis::new(ai.idle_timeout_value),
        secondary_timer: Millis::new(ai.secondary_timer_value),
        look_around_timer: Millis::new(ai.look_around_timer_value),
        look_around_timeout: Millis::new(ai.look_around_timeout_value),
        last_retarget_time: WorldTime::from_millis(ai.last_retarget_time),
        retarget_timer: Millis::new(ai.randomized_retarget_timer),
        stance: UnitStance::of_byte(ai.state_value),
        formation_kind: FormationKind::of_byte(ai.formation_kind),
        state_position: Point::new(ai.state_position_xvalue, ai.state_position_yvalue),
        retarget_disabled: ai.disable_retargeting,
        alert_mode: AlertMode::of_data(ai.alert_mode_value),
        time_since_enemy: Seconds::new(ai.time_since_enemy_sighting),
        patrol_path: patrol_path(ai)?,
        patrol_waypoint: WaypointIndex::of_data(ai.patrol_current_waypoint),
        stop_after_kill: ai.stop_after_target_killed_value != 0,
        home: home(combat),
        hunted: None,
        last_update_time: Millis::new(ai.last_update_time_value),
        attackers: Attackers::of_saved(&ai.temp_int),
        order_history: order_history(ai)?,
        retries: ai
            .retarget_entries
            .iter()
            .map(|entry| {
                Ok((
                    ObjectId::try_from(entry.target_id)?,
                    WorldTime::from_millis(entry.timeout),
                ))
            })
            .collect::<GenieResult<_>>()?,
        best_this_update: ObjectId::of_field(ai.best_unit_to_attack_this_update),
        orders: queue(&ai.temp_order, own, players)?,
        building_orders: queue(building_orders(combat), own, players)?,
        notifies: ai
            .temp_notify
            .iter()
            .map(Notify::from_bytes)
            .collect::<Result<_, _>>()?,
        // the load's factory capacity, doubled for what it reads
        notify_capacity: notify_capacity(ai_kind, ai.temp_notify.len()),
        processing: false,
        buffered: Vec::new(),
        ordered_on_target: false,
    }))
}

/// The saved patrol: its path's points in the game's layout, its length and flags.
///
/// # Errors
///
/// A patrol without points (`invalid_input`): the patrol's leash (`FUN_140f95e10`) runs its
/// loop past an empty path's end.
fn patrol_path(ai: &UnitAiModule) -> GenieResult<Option<PatrolPath>> {
    let Some(path) = ai.patrol_path.as_deref() else {
        return Ok(None);
    };
    if path.waypoints.is_empty() {
        return Err(GenieError::invalid_input("a saved patrol without points"));
    }
    Ok(Some(PatrolPath {
        path: Path {
            points: path.waypoints.iter().map(vec3).collect(),
            length: Distance::new(path.path_length_value),
        },
        flags: PathFlags::from_bits_retain(path.flags_bits),
    }))
}

/// An animal's home: its origin and fury (the elephant unit AI's).
fn home(combat: &TribeCombatObject) -> Option<Home> {
    combat
        .tribe_elephant_unit_ai_module
        .as_deref()
        .map(|tribe_elephant_unit_ai_module| Home {
            origin: Position::new(
                tribe_elephant_unit_ai_module.origin_x,
                tribe_elephant_unit_ai_module.origin_y,
                tribe_elephant_unit_ai_module.origin_z,
            ),
            furious: tribe_elephant_unit_ai_module.furious != 0,
            homeward: false,
        })
}

/// The order history in the save's order, the newest first (the load appends each at the tail,
/// `0x140c4efc0`).
///
/// # Errors
///
/// A record's order or action value no order or action holds, or a category past 0 to 11.
fn order_history(ai: &UnitAiModule) -> GenieResult<Vec<OrderRecord>> {
    ai.oh
        .iter()
        .map(|entry| {
            Ok(OrderRecord {
                order: OrderKind::of_field(entry.new_oh_order)?,
                action: AiAction::of_field(entry.new_oh_action)?,
                time: WorldTime::from_millis(entry.new_oh_time),
                // the save's 12 bytes are the game's vector, x, height, y
                position: vec3(&entry.new_oh_position),
                target: ObjectId::of_field(entry.new_oh_target_id),
                category: AttackCategory::of_field(entry.new_oh_target_attack_category)?,
                target_position: vec3(&entry.new_oh_target_position),
            })
        })
        .collect()
}

/// A tower's or another building's saved orders, none for any other object.
fn building_orders(combat: &TribeCombatObject) -> &[[u8; 52]] {
    combat
        .tribe_tower_unit_ai_module
        .as_deref()
        .map(|tribe_tower_unit_ai_module| &tribe_tower_unit_ai_module.saved_order)
        .or_else(|| {
            combat
                .tribe_building_unit_ai_module
                .as_deref()
                .map(|tribe_building_unit_ai_module| &tribe_building_unit_ai_module.saved_order)
        })
        .map_or(&[], Vec::as_slice)
}

/// A saved order queue's records.
///
/// # Errors
///
/// A record's order value no order holds.
fn queue(records: &[[u8; 52]], own: ObjectId, players: usize) -> GenieResult<Vec<QueuedOrder>> {
    records
        .iter()
        .map(|record| QueuedOrder::from_bytes(record, own, players))
        .collect()
}

#[cfg(test)]
mod tests;
