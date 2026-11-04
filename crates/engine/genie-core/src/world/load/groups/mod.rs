//! The world's unit groups from a save (FD-036): genie-save's generated reader of the build into
//! the [`crate::groups`] state.
//!
//! The mapping is the only place that knows the build's save layout of a group: a later build
//! with another layout maps into the same state here.

use crate::groups::GroupFlags;
use crate::groups::MotionFlags;
use crate::groups::{
    Differentiator, Formation, FormationCategory, FormationKind, FormationLine, GroupCommand,
    GroupCommandKind, GroupId, GroupOrder, GroupTarget, LinePoint, Member, Motion, MovePath,
    MoveReason, PointLine, Slot, SlotState, SpecificMove, TargetData, TransformKind, UnitGroup,
    UnitGroups, UnitKind, UnitKindStatistic,
};
use crate::map::Distance;
use crate::math::fmath::BVector;
use crate::movement::Speed;
use crate::movement::TurnRate;
use crate::objects::ObjectId;
use crate::players::PlayerId;
use crate::unit_ai::UnitStance;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::{GenieError, GenieResult};
use genie_save::generated::world as saved;

use crate::objects::masters::ActionKind;
use crate::pathing::DelayCount;
use crate::world::load::movers::path_level;
use crate::world::load::movers::vec3;
use crate::world::load::object_ids;

/// Four vectors of a saved 48-byte position (point, forward, up, right).
fn frame(bytes: &[u8; 48]) -> [BVector; 4] {
    let vector_at = |index: usize| {
        let mut chunk = [0; 12];
        chunk.copy_from_slice(&bytes[index * 12..index * 12 + 12]);
        vec3(&chunk)
    };
    [vector_at(0), vector_at(1), vector_at(2), vector_at(3)]
}

/// Fifty words of a saved 200-byte array.
fn words(bytes: &[u8; 200]) -> Vec<u32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_le_bytes(*word))
        .collect()
}

/// The 200 bytes of a statistic's unit types (`mUnitTypes`) as 50 unit types.
fn unit_kinds(bytes: &[u8; 200]) -> Vec<UnitKind> {
    words(bytes).into_iter().map(UnitKind::of_word).collect()
}

fn motion(saved: &saved::Motion, paths: &[saved::Path]) -> GenieResult<Motion> {
    Ok(Motion {
        position: frame(&saved.position),
        new_position: frame(&saved.new_position),
        velocity: vec3(&saved.velocity),
        acceleration: vec3(&saved.acceleration),
        angular_velocity: vec3(&saved.angular_velocity),
        angular_acceleration: vec3(&saved.angular_acceleration),
        turn_radius: Distance::new(saved.turn_radius),
        turn_radius_speed: Speed::new(saved.turn_radius_speed),
        maximum_yaw_per_second_moving: TurnRate::new(saved.maximum_yaw_per_second_moving),
        stationary_yaw_revolution_time: saved.stationary_yaw_revolution_time,
        maximum_yaw_per_second_stationary: TurnRate::new(saved.maximum_yaw_per_second_stationary),
        // the save writes the top level first; genie-core keeps the walked level first, as a
        // unit's (the unit mapping's reverse)
        paths: saved
            .temp_path_data
            .iter()
            .rev()
            .map(|path_data| path_level(path_data, paths))
            .collect::<Result<_, _>>()?,
        flags: MotionFlags::from_bits_retain(saved.motion_flags_bits),
        maximum_velocity_forward: Speed::new(saved.maximum_velocity_forward),
        number_wait_delays: DelayCount::of_data(saved.number_wait_delays),
        // not saved: the constructor's
        lead: None,
    })
}

fn formation(saved: &saved::Formation) -> GenieResult<Formation> {
    Ok(Formation {
        kind: FormationKind::from(saved.kind),
        slots: saved
            .slots
            .iter()
            .map(|slot| {
                Ok(Slot {
                    unit_id: ObjectId::of_field(slot.slots_unit_id),
                    offset: vec3(&slot.slots_offset),
                    transform_position: vec3(&slot.slots_transform_position),
                    time: WorldTime::from_millis(slot.slots_time),
                    state: SlotState::of_int(slot.slots_state)?,
                    depth: slot.slots_depth,
                    desired_transform_position: vec3(&slot.slots_desired_transform_position),
                    position_filled: slot.slots_position_filled,
                    desired_category: FormationCategory::named(slot.slots_desired_category),
                    active: slot.slots_active,
                    desired_sub_category: UnitKind::of_word(
                        slot.slots_desired_sub_category.cast_unsigned(),
                    ),
                    transformed: 0,
                })
            })
            .collect::<GenieResult<_>>()?,
        number_transformed_slots: saved.number_transformed_slots,
        maximum_width: saved.maximum_width,
        maximum_depth: saved.maximum_depth,
        about_faced: saved.about_faced,
        lines: saved
            .lines
            .iter()
            .map(|formation_line| FormationLine {
                depth: formation_line.temp_depth,
                indices: formation_line.line_indices.clone(),
                attack_range: Distance::new(formation_line.attack_range),
                distance_back: Distance::new(formation_line.distance_back),
            })
            .collect(),
        transform_kind: TransformKind::of_data(saved.transform_kind),
        number_lines: saved.number_lines,
        transform_orientation: vec3(&saved.transform_orientation),
        // a load's array holds the saved slots alone
        spare_slots: Vec::new(),
    })
}

fn group(saved: &saved::UnitGroup, paths: &[saved::Path]) -> GenieResult<UnitGroup> {
    let target_data = &saved.current_target;
    Ok(UnitGroup {
        motion: motion(&saved.motion, paths)?,
        id: GroupId::try_from(saved.id)?,
        // the 32 bits of −1 (the constructor's) are none
        owner: match saved.owner.to_bits() {
            u32::MAX => None,
            bits => Some(u8::try_from(bits).map(PlayerId::from).map_err(|_| {
                GenieError::invalid_input("a group's owner past a byte").with("owner", bits)
            })?),
        },
        // −1 (the constructor's) is none
        stance: match saved.state {
            -1 => None,
            value => Some(UnitStance::of_byte(u8::try_from(value).map_err(|_| {
                GenieError::invalid_input("a group's stance past a byte").with("stance", value)
            })?)),
        },
        members: saved
            .units
            .iter()
            .map(|member| {
                Ok(Member {
                    id: ObjectId::try_from(member.units)?,
                    specific_move: SpecificMove {
                        active: member.specific_moves_active,
                        path_id: MovePath::of_data(member.specific_moves_path_id),
                        reason: MoveReason::of_data(member.specific_moves_reason),
                        goal: vec3(&member.specific_moves_goal),
                        timer: Seconds::new(member.specific_moves_timer),
                        consecutive_move_failures: member.specific_moves_consecutive_move_failures,
                        distance: Distance::new(member.specific_moves_distance),
                        last_position: vec3(&member.specific_moves_last_position),
                    },
                    seconds_since_attacking: Seconds::new(member.seconds_since_attacking),
                })
            })
            .collect::<GenieResult<_>>()?,
        previous_formation: saved
            .previous_formation
            .as_deref()
            .map(formation)
            .transpose()?,
        current_formation: saved
            .current_formation
            .as_deref()
            .map(formation)
            .transpose()?,
        garbage: saved.garbage,
        formation_locked_kind: FormationKind::of_field(saved.formation_locked_kind),
        flags: GroupFlags::from_bits_retain(saved.group_flags_bits),
        centroid: vec3(&saved.centroid),
        desired_height: saved.desired_height,
        largest_unit_radius: Distance::new(saved.largest_unit_radius),
        smallest_unit_radius: Distance::new(saved.smallest_unit_radius),
        desired_spacing: Distance::new(saved.desired_spacing),
        smallest_unit_range: Distance::new(saved.smallest_unit_range),
        smallest_ranged_unit_range: Distance::new(saved.smallest_ranged_unit_range),
        largest_unit_range: Distance::new(saved.largest_unit_range),
        largest_ranged_unit_range: Distance::new(saved.largest_ranged_unit_range),
        stored_positions: PointLine {
            points: saved
                .stored_positions
                .points
                .iter()
                .map(|line_point| LinePoint {
                    point: vec3(&line_point.ve_point),
                    forward: vec3(&line_point.ve_forward),
                    up: vec3(&line_point.ve_up),
                    right: vec3(&line_point.ve_right),
                })
                .collect(),
            length: Distance::new(saved.stored_positions.length),
            desired_length: Distance::new(saved.stored_positions.desired_length),
        },
        unit_kinds: saved
            .unit_kinds
            .unit_kind_statistic
            .iter()
            .map(|unit_kind_statistic| {
                Ok(UnitKindStatistic {
                    formation_category: FormationCategory::named(
                        unit_kind_statistic.formation_category,
                    ),
                    units: object_ids(&unit_kind_statistic.units)?,
                    used: unit_kind_statistic.used,
                    attack_range: Distance::new(unit_kind_statistic.attack_range),
                    desired_spacing: Distance::new(unit_kind_statistic.desired_spacing),
                    number_unit_kinds: unit_kind_statistic.number_unit_kinds,
                    unit_kinds: unit_kinds(&unit_kind_statistic.unit_kinds),
                    unit_kind_counts: words(&unit_kind_statistic.unit_kind_counts),
                })
            })
            .collect::<GenieResult<_>>()?,
        waypoints: saved.waypoints.iter().map(vec3).collect(),
        current_command: GroupCommandKind::named(saved.current_command),
        current_target: TargetData {
            target: GroupTarget::of_saved(target_data.target, target_data.kind),
            original_position: vec3(&target_data.original_position),
            range: Distance::new(target_data.range),
            target_radius: Distance::new(target_data.target_radius),
            disable_retargeting: target_data.disable_retargeting,
            certain_action: ActionKind::of_data(target_data.certain_action),
        },
        attack_plan_kind: saved.attack_plan_kind,
        idle_timer: Millis::new(saved.idle_timer),
        order_queue: saved
            .order_queue
            .iter()
            .map(|order| GroupOrder {
                command: GroupCommandKind::named(order.new_order_command),
                target_id: ObjectId::of_field(order.new_order_target_id),
                parm: order.new_order_parm,
                point: vec3(&order.new_order_point),
                execute_time: WorldTime::from_millis(order.new_order_execute_time),
            })
            .collect(),
        hibernating: saved.hibernating,
        command_queue: saved
            .command_queue
            .iter()
            .map(|entry| GroupCommand {
                command: GroupCommandKind::named(entry.new_command_command),
                target_id: ObjectId::of_field(entry.new_command_target_id),
                range: Distance::new(entry.new_command_range),
                point: vec3(&entry.new_command_point),
                approved: entry.new_command_approved,
                disable_retargeting: entry.new_command_disable_retargeting,
                approved_2: entry.new_command_approved_2,
            })
            .collect(),
        objects: object_ids(&saved.obj_id)?,
        // unsaved: the next statistics fill it
        sighted: Vec::new(),
        differentiator_id: Differentiator::of_data(saved.differentiator_id),
        seconds_not_under_attack: Seconds::new(saved.seconds_not_under_attack),
        differentiator_groups: saved
            .differentiator_groups
            .iter()
            .map(|&word| GroupId::try_from(word))
            .collect::<Result<_, _>>()?,
        total_spacing: Distance::new(saved.total_spacing),
        non_retarget_units: saved.non_retarget_units.clone(),
        // a load's arrays hold the members alone
        spare_seconds: Vec::new(),
        // the constructor's empty list (+0x488 is 0), which no save writes
        special_units: Vec::new(),
        // the constructor's, which the load keeps
        replaces_queue: 1,
    })
}

/// The world's unit groups of a save (`saveUnitGroups`), each group's paths from the world's
/// path table. The process globals are a load's: the command counter past every group's
/// `mDifferentiatorID` (`FUN_14101d350`), the scratch array zero; the tick oracle's `globals`
/// dump gives the game's.
///
/// # Errors
///
/// When a group's path data names a path the world's table does not hold.
pub fn unit_groups(world: &saved::BaseWorld) -> GenieResult<UnitGroups> {
    let unit_groups = &world.unit_groups;
    let groups: Vec<UnitGroup> = unit_groups
        .unit_group
        .iter()
        .map(|saved| group(saved, &world.path))
        .collect::<Result<_, _>>()?;
    let mut commands = Differentiator::default();
    for group in &groups {
        if commands < group.differentiator_id {
            commands = group.differentiator_id.next();
        }
    }
    Ok(UnitGroups {
        groups,
        counter: GroupId::try_from(unit_groups.unit_group_counter)?,
        commands,
        unit_kinds_scratch: Vec::new(),
    })
}

#[cfg(test)]
mod tests;
