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
fn frame(b: &[u8; 48]) -> [BVector; 4] {
    let v = |i: usize| {
        let mut w = [0; 12];
        w.copy_from_slice(&b[i * 12..i * 12 + 12]);
        vec3(&w)
    };
    [v(0), v(1), v(2), v(3)]
}

/// Fifty words of a saved 200-byte array.
fn words(b: &[u8; 200]) -> Vec<u32> {
    b.as_chunks::<4>()
        .0
        .iter()
        .map(|w| u32::from_le_bytes(*w))
        .collect()
}

/// The 200 bytes of a statistic's unit types (`mUnitTypes`) as 50 unit types.
fn unit_kinds(b: &[u8; 200]) -> Vec<UnitKind> {
    words(b).into_iter().map(UnitKind::of_word).collect()
}

fn motion(m: &saved::Motion, paths: &[saved::Path]) -> GenieResult<Motion> {
    Ok(Motion {
        position: frame(&m.position),
        new_position: frame(&m.new_position),
        velocity: vec3(&m.velocity),
        acceleration: vec3(&m.acceleration),
        angular_velocity: vec3(&m.angular_velocity),
        angular_acceleration: vec3(&m.angular_acceleration),
        turn_radius: Distance::new(m.turn_radius),
        turn_radius_speed: Speed::new(m.turn_radius_speed),
        maximum_yaw_per_second_moving: TurnRate::new(m.maximum_yaw_per_second_moving),
        stationary_yaw_revolution_time: m.stationary_yaw_revolution_time,
        maximum_yaw_per_second_stationary: TurnRate::new(m.maximum_yaw_per_second_stationary),
        // the save writes the top level first; genie-core keeps the walked level first, as a
        // unit's (the unit mapping's reverse)
        paths: m
            .temp_path_data
            .iter()
            .rev()
            .map(|d| path_level(d, paths))
            .collect::<Result<_, _>>()?,
        flags: MotionFlags::from_bits_retain(m.motion_flags_bits),
        maximum_velocity_forward: Speed::new(m.maximum_velocity_forward),
        number_wait_delays: DelayCount::of_data(m.number_wait_delays),
        // not saved: the constructor's
        lead: None,
    })
}

fn formation(f: &saved::Formation) -> GenieResult<Formation> {
    Ok(Formation {
        kind: FormationKind::from(f.kind),
        slots: f
            .slots
            .iter()
            .map(|s| {
                Ok(Slot {
                    unit_id: ObjectId::of_field(s.slots_unit_id),
                    offset: vec3(&s.slots_offset),
                    transform_position: vec3(&s.slots_transform_position),
                    time: WorldTime::from_millis(s.slots_time),
                    state: SlotState::of_int(s.slots_state)?,
                    depth: s.slots_depth,
                    desired_transform_position: vec3(&s.slots_desired_transform_position),
                    position_filled: s.slots_position_filled,
                    desired_category: FormationCategory::named(s.slots_desired_category),
                    active: s.slots_active,
                    desired_sub_category: UnitKind::of_word(
                        s.slots_desired_sub_category.cast_unsigned(),
                    ),
                    transformed: 0,
                })
            })
            .collect::<GenieResult<_>>()?,
        number_transformed_slots: f.number_transformed_slots,
        maximum_width: f.maximum_width,
        maximum_depth: f.maximum_depth,
        about_faced: f.about_faced,
        lines: f
            .lines
            .iter()
            .map(|l| FormationLine {
                depth: l.temp_depth,
                indices: l.line_indices.clone(),
                attack_range: Distance::new(l.attack_range),
                distance_back: Distance::new(l.distance_back),
            })
            .collect(),
        transform_kind: TransformKind::of_data(f.transform_kind),
        number_lines: f.number_lines,
        transform_orientation: vec3(&f.transform_orientation),
        // a load's array holds the saved slots alone
        spare_slots: Vec::new(),
    })
}

fn group(g: &saved::UnitGroup, paths: &[saved::Path]) -> GenieResult<UnitGroup> {
    let t = &g.current_target;
    Ok(UnitGroup {
        motion: motion(&g.motion, paths)?,
        id: GroupId::try_from(g.id)?,
        // the 32 bits of −1 (the constructor's) are none
        owner: match g.owner.to_bits() {
            u32::MAX => None,
            bits => Some(u8::try_from(bits).map(PlayerId::from).map_err(|_| {
                GenieError::invalid_input("a group's owner past a byte").with("owner", bits)
            })?),
        },
        // −1 (the constructor's) is none
        stance: match g.state {
            -1 => None,
            s => Some(UnitStance::of_byte(u8::try_from(s).map_err(|_| {
                GenieError::invalid_input("a group's stance past a byte").with("stance", s)
            })?)),
        },
        members: g
            .units
            .iter()
            .map(|u| {
                Ok(Member {
                    id: ObjectId::try_from(u.units)?,
                    specific_move: SpecificMove {
                        active: u.specific_moves_active,
                        path_id: MovePath::of_data(u.specific_moves_path_id),
                        reason: MoveReason::of_data(u.specific_moves_reason),
                        goal: vec3(&u.specific_moves_goal),
                        timer: Seconds::new(u.specific_moves_timer),
                        consecutive_move_failures: u.specific_moves_consecutive_move_failures,
                        distance: Distance::new(u.specific_moves_distance),
                        last_position: vec3(&u.specific_moves_last_position),
                    },
                    seconds_since_attacking: Seconds::new(u.seconds_since_attacking),
                })
            })
            .collect::<GenieResult<_>>()?,
        previous_formation: g.previous_formation.as_deref().map(formation).transpose()?,
        current_formation: g.current_formation.as_deref().map(formation).transpose()?,
        garbage: g.garbage,
        formation_locked_kind: FormationKind::of_field(g.formation_locked_kind),
        flags: GroupFlags::from_bits_retain(g.group_flags_bits),
        centroid: vec3(&g.centroid),
        desired_height: g.desired_height,
        largest_unit_radius: Distance::new(g.largest_unit_radius),
        smallest_unit_radius: Distance::new(g.smallest_unit_radius),
        desired_spacing: Distance::new(g.desired_spacing),
        smallest_unit_range: Distance::new(g.smallest_unit_range),
        smallest_ranged_unit_range: Distance::new(g.smallest_ranged_unit_range),
        largest_unit_range: Distance::new(g.largest_unit_range),
        largest_ranged_unit_range: Distance::new(g.largest_ranged_unit_range),
        stored_positions: PointLine {
            points: g
                .stored_positions
                .points
                .iter()
                .map(|p| LinePoint {
                    point: vec3(&p.ve_point),
                    forward: vec3(&p.ve_forward),
                    up: vec3(&p.ve_up),
                    right: vec3(&p.ve_right),
                })
                .collect(),
            length: Distance::new(g.stored_positions.length),
            desired_length: Distance::new(g.stored_positions.desired_length),
        },
        unit_kinds: g
            .unit_kinds
            .unit_kind_statistic
            .iter()
            .map(|u| {
                Ok(UnitKindStatistic {
                    formation_category: FormationCategory::named(u.formation_category),
                    units: object_ids(&u.units)?,
                    used: u.used,
                    attack_range: Distance::new(u.attack_range),
                    desired_spacing: Distance::new(u.desired_spacing),
                    number_unit_kinds: u.number_unit_kinds,
                    unit_kinds: unit_kinds(&u.unit_kinds),
                    unit_kind_counts: words(&u.unit_kind_counts),
                })
            })
            .collect::<GenieResult<_>>()?,
        waypoints: g.waypoints.iter().map(vec3).collect(),
        current_command: GroupCommandKind::named(g.current_command),
        current_target: TargetData {
            target: GroupTarget::of_saved(t.target, t.kind),
            original_position: vec3(&t.original_position),
            range: Distance::new(t.range),
            target_radius: Distance::new(t.target_radius),
            disable_retargeting: t.disable_retargeting,
            certain_action: ActionKind::of_data(t.certain_action),
        },
        attack_plan_kind: g.attack_plan_kind,
        idle_timer: Millis::new(g.idle_timer),
        order_queue: g
            .order_queue
            .iter()
            .map(|o| GroupOrder {
                command: GroupCommandKind::named(o.new_order_command),
                target_id: ObjectId::of_field(o.new_order_target_id),
                parm: o.new_order_parm,
                point: vec3(&o.new_order_point),
                execute_time: WorldTime::from_millis(o.new_order_execute_time),
            })
            .collect(),
        hibernating: g.hibernating,
        command_queue: g
            .command_queue
            .iter()
            .map(|c| GroupCommand {
                command: GroupCommandKind::named(c.new_command_command),
                target_id: ObjectId::of_field(c.new_command_target_id),
                range: Distance::new(c.new_command_range),
                point: vec3(&c.new_command_point),
                approved: c.new_command_approved,
                disable_retargeting: c.new_command_disable_retargeting,
                approved_2: c.new_command_approved_2,
            })
            .collect(),
        objects: object_ids(&g.obj_id)?,
        // unsaved: the next statistics fill it
        sighted: Vec::new(),
        differentiator_id: Differentiator::of_data(g.differentiator_id),
        seconds_not_under_attack: Seconds::new(g.seconds_not_under_attack),
        differentiator_groups: g
            .differentiator_groups
            .iter()
            .map(|&w| GroupId::try_from(w))
            .collect::<Result<_, _>>()?,
        total_spacing: Distance::new(g.total_spacing),
        non_retarget_units: g.non_retarget_units.clone(),
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
    let g = &world.unit_groups;
    let groups: Vec<UnitGroup> = g
        .unit_group
        .iter()
        .map(|u| group(u, &world.path))
        .collect::<Result<_, _>>()?;
    let mut commands = Differentiator::default();
    for u in &groups {
        if commands < u.differentiator_id {
            commands = u.differentiator_id.next();
        }
    }
    Ok(UnitGroups {
        groups,
        counter: GroupId::try_from(g.unit_group_counter)?,
        commands,
        unit_kinds_scratch: Vec::new(),
    })
}

#[cfg(test)]
mod tests;
