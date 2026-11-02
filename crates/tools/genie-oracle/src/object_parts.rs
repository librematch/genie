//! A saved object's lifecycle fields and its unit AI.

use crate::masters::MasterTable;
use crate::masters::dat_id;
use crate::vec3;
use genie_core::GenieError;
use genie_core::garrison::ContainerId;
use genie_core::map::Distance;
use genie_core::map::Point;
use genie_core::map::Position;
use genie_core::movement::PathFlags;
use genie_core::movement::WaypointIndex;
use genie_core::objects::HitPoints;
use genie_core::objects::ObjectId;
use genie_core::objects::SpriteFlags;
use genie_core::objects::SpriteId;
use genie_core::objects::lifecycle::{Class, Lifecycle, MasterData};
use genie_core::objects::masters::ClassId;
use genie_core::objects::masters::MasterId;
use genie_core::players::Amount;
use genie_core::players::AmountRate;
use genie_core::players::CivId;
use genie_core::players::attribute::AttributeId;
use genie_core::unit_ai::OrderPriority;
use genie_core::unit_ai::best_target::AttackCategory;
use genie_core::unit_ai::{ActionKind, OrderKind};
use genie_core::world::Millis;
use genie_core::world::Seconds;
use genie_core::world::WorldTime;
use genie_save::generated::world::{
    PlayerObjects, RgeStaticObject, TribeCombatObject, UnitAiModule,
};

/// The object's lifecycle fields. An object with its own master takes every master value from it;
/// otherwise the death sprite and spawns come from its player's master in the save, and the decay
/// and undead flag from the table, which the save lacks.
///
/// # Errors
///
/// A saved state past 8.
pub(super) fn lifecycle(
    object: &genie_save::Object,
    (s, master_id): (&RgeStaticObject, MasterId),
    objects: &PlayerObjects,
    civ: CivId,
    masters: Option<&MasterTable>,
) -> Result<Lifecycle, GenieError> {
    let pure_static = object
        .rge_object
        .as_deref()
        .is_some_and(|r| r.rge_static_object.is_some());
    let death_sprite = s.own_master().map_or_else(
        || {
            objects
                .master(s.master_obj)
                .and_then(|m| m.static_part())
                .and_then(|m| SpriteId::of_data(m.death_sprite_id))
        },
        |own| SpriteId::of_data(own.death_sprite_num),
    );
    // the sprite after the death: the dying one, else the current one
    let after = death_sprite.or_else(|| SpriteId::of_data(s.sprite));
    let death_facets = masters
        .zip(after)
        .and_then(|(t, after)| t.graphics.get(after))
        .filter(|g| {
            pure_static && g.flags.contains(SpriteFlags::RANDOM_FIRST_FRAME) && g.facets.several()
        })
        .map(|g| g.facets);
    let (dying_sprite, death_spawn, master) = s.own_master().map_or_else(
        || {
            let player = objects.master(s.master_obj).and_then(|m| m.static_part());
            (
                player.and_then(|m| SpriteId::of_data(m.death_sprite_id)),
                player.map_or(-1, |m| m.death_spawn_obj_id),
                masters.and_then(|t| t.get(civ, dat_id(objects, s.master_obj)?)),
            )
        },
        |own| {
            (
                SpriteId::of_data(own.death_sprite_num),
                own.death_spawn_obj_id,
                Some(MasterData {
                    decay: AmountRate::new(own.attribute_rot),
                    undead: own.undead_flag != 0,
                }),
            )
        },
    );
    Ok(Lifecycle {
        class: if pure_static {
            Class::Static
        } else {
            Class::Animated
        },
        state: genie_core::objects::ObjectState::of_byte(s.object_state)?,
        hp: HitPoints::new(s.hp),
        attribute_type: AttributeId::of_data(s.attribute_type_held),
        amount: Amount::new(s.attribute_amount_held),
        dying_sprite,
        death_spawn: death_spawn >= 0,
        death_facets,
        inside: ContainerId::of_field(s.inside_obj),
        goto_sleep: s.goto_sleep_flag != 0,
        master_id,
        master,
    })
}

/// A combat object's unit AI as the save holds it, None without one.
///
/// # Errors
///
/// An order or action value no order or action holds ([`OrderKind::try_from`],
/// [`ActionKind::try_from`]), in the current or last one, the history or a queue.
pub(super) fn unit_ai(
    object: &genie_save::Object,
    own: ObjectId,
    players: usize,
) -> Result<Option<genie_core::unit_ai::UnitAi>, GenieError> {
    let Some((combat, ai)) = object.combat().and_then(|c| Some((c, c.unit_ai()?))) else {
        return Ok(None);
    };
    Ok(Some(genie_core::unit_ai::UnitAi {
        ai_type: genie_core::unit_ai::UnitAiType::of_value(combat.need_unit_ai as u32),
        current_order: OrderKind::of_field(ai.current_order_value)?,
        order_priority: OrderPriority::of_data(ai.current_order_priority_value),
        current_action: ActionKind::of_field(ai.current_action_value)?,
        current_target: ObjectId::of_field(ai.current_target_value),
        target_class: ClassId::of_param(ai.current_target_type_value)?,
        target_point: Position::new(
            ai.current_target_xvalue,
            ai.current_target_yvalue,
            ai.current_target_zvalue,
        ),
        desired_distance: Distance::new(ai.desired_target_distance_value),
        defend_target: ObjectId::of_field(ai.defend_target_value),
        last_order: OrderKind::of_field(ai.last_order_value)?,
        last_action: ActionKind::of_field(ai.last_action_value)?,
        last_target: ObjectId::of_field(ai.last_target_value),
        last_target_class: ClassId::of_param(ai.last_target_type_value)?,
        idle_timer: Millis::new(ai.idle_timer_value),
        adjusted_idle_timeout: Millis::new(ai.adjusted_idle_timeout_value),
        idle_timeout: Millis::new(ai.idle_timeout_value),
        secondary_timer: Millis::new(ai.secondary_timer_value),
        look_around_timer: Millis::new(ai.look_around_timer_value),
        look_around_timeout: Millis::new(ai.look_around_timeout_value),
        last_retarget_time: WorldTime::from_millis(ai.last_retarget_time),
        retarget_timer: Millis::new(ai.randomized_retarget_timer),
        stance: genie_core::unit_ai::UnitStance::of_byte(ai.state_value),
        formation_type: genie_core::groups::FormationKind::of_byte(ai.formation_type),
        state_position: Point::new(ai.state_position_xvalue, ai.state_position_yvalue),
        retarget_disabled: ai.disable_retargeting,
        alert_mode: ai.alert_mode_value != 0,
        time_since_enemy: Seconds::new(ai.time_since_enemy_sighting),
        patrol_path: patrol_path(ai),
        patrol_waypoint: WaypointIndex::of_data(ai.patrol_current_waypoint),
        stop_after_kill: ai.stop_after_target_killed_value != 0,
        home: home(combat),
        hunted: None,
        last_update_time: Millis::new(ai.last_update_time_value),
        attackers: genie_core::unit_ai::Attackers::of_saved(&ai.temp_int),
        order_history: order_history(ai)?,
        retries: ai
            .retarget_entries
            .iter()
            .map(|e| {
                Ok((
                    ObjectId::try_from(e.target_id)?,
                    WorldTime::from_millis(e.timeout),
                ))
            })
            .collect::<Result<_, GenieError>>()?,
        best_this_update: ObjectId::of_field(ai.best_unit_to_attack_this_update),
        orders: queue(&ai.temp_order, own, players)?,
        building_orders: queue(building_orders(combat), own, players)?,
        notifies: ai
            .temp_notify
            .iter()
            .map(genie_core::unit_ai::Notify::from_bytes)
            .collect::<Result<_, _>>()?,
        // the load's factory capacity, doubled for what it reads
        notify_capacity: genie_core::unit_ai::notify_capacity(
            genie_core::unit_ai::UnitAiType::of_value(combat.need_unit_ai as u32),
            ai.temp_notify.len(),
        ),
        processing: false,
        buffered: Vec::new(),
        ordered_on_target: false,
    }))
}

/// The saved patrol: its path's points in the game's layout, its length and flags.
fn patrol_path(ai: &UnitAiModule) -> Option<genie_core::unit_ai::PatrolPath> {
    ai.patrol_path
        .as_deref()
        .map(|p| genie_core::unit_ai::PatrolPath {
            path: genie_core::pathing::tile_path::Path {
                points: p.waypoints.iter().map(vec3).collect(),
                length: Distance::new(p.path_length_value),
            },
            flags: PathFlags::from_bits_retain(p.flags_bits),
        })
}

/// An animal's home: its origin and fury (the elephant unit AI's).
fn home(combat: &TribeCombatObject) -> Option<genie_core::unit_ai::Home> {
    combat
        .tribe_elephant_unit_ai_module
        .as_deref()
        .map(|e| genie_core::unit_ai::Home {
            origin: Position::new(e.origin_x, e.origin_y, e.origin_z),
            furious: e.furious != 0,
            go_home: false,
        })
}

/// The order history in the save's order, the newest first (the load appends each at the tail,
/// `0x140c4efc0`).
///
/// # Errors
///
/// A record's order or action value no order or action holds, or a category past 0 to 11.
fn order_history(ai: &UnitAiModule) -> Result<Vec<genie_core::unit_ai::OrderRecord>, GenieError> {
    ai.oh
        .iter()
        .map(|e| {
            Ok(genie_core::unit_ai::OrderRecord {
                order: OrderKind::of_field(e.new_oh_order)?,
                action: ActionKind::of_field(e.new_oh_action)?,
                time: WorldTime::from_millis(e.new_oh_time),
                // the save's 12 bytes are the game's vector, x, height, y
                position: vec3(&e.new_oh_position),
                target: ObjectId::of_field(e.new_oh_target_id),
                category: AttackCategory::of_field(e.new_oh_target_attack_category)?,
                target_position: vec3(&e.new_oh_target_position),
            })
        })
        .collect()
}

/// A tower's or another building's saved orders, none for any other object.
fn building_orders(combat: &TribeCombatObject) -> &[[u8; 52]] {
    combat
        .tribe_tower_unit_ai_module
        .as_deref()
        .map(|t| &t.saved_order)
        .or_else(|| {
            combat
                .tribe_building_unit_ai_module
                .as_deref()
                .map(|b| &b.saved_order)
        })
        .map_or(&[], Vec::as_slice)
}

/// A saved order queue's records.
///
/// # Errors
///
/// A record's order value no order holds.
fn queue(
    records: &[[u8; 52]],
    own: ObjectId,
    players: usize,
) -> Result<Vec<genie_core::unit_ai::QueuedOrder>, GenieError> {
    records
        .iter()
        .map(|r| genie_core::unit_ai::QueuedOrder::from_bytes(r, own, players))
        .collect()
}

#[cfg(test)]
mod tests;
