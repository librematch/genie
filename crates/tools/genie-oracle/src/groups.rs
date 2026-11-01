//! The world's unit groups from a save (FD-036): genie-save's generated reader of the build into
//! genie-core's [`genie_core::groups`] state, and the field-by-field differences of two such
//! states, which the forced replay reports.
//!
//! The mapping is the only place that knows the build's save layout of a group: a later build
//! with another layout maps into the same state here.

use genie_core::GenieError;
use genie_core::groups::GroupFlags;
use genie_core::groups::MotionFlags;
use genie_core::groups::{
    Formation, FormationKind, FormationLine, GroupCommand, GroupCommandKind, GroupId, GroupOrder,
    GroupTarget, LinePoint, Member, Motion, PointLine, Slot, SlotState, SpecificMove, TargetData,
    UnitGroup, UnitGroups, UnitTypeStatistic,
};
use genie_core::map::Distance;
use genie_core::math::fmath::BVector;
use genie_core::movement::Speed;
use genie_core::movement::TurnRate;
use genie_core::objects::ObjectId;
use genie_core::unit_ai::UnitStance;
use genie_core::world::Millis;
use genie_core::world::Seconds;
use genie_core::world::WorldTime;
use genie_save::generated::world as saved;

use crate::{object_ids, path_level, vec3};
use genie_core::objects::masters::ActionType;

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

fn motion(m: &saved::Motion, paths: &[saved::Path]) -> Result<Motion, GenieError> {
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
        flags: MotionFlags::from_bits_retain(m.motion_flags_m_bits),
        maximum_velocity_forward: Speed::new(m.maximum_velocity_forward),
        number_wait_delays: m.number_wait_delays,
        // not saved: the constructor's
        lead: None,
    })
}

fn formation(f: &saved::Formation) -> Result<Formation, GenieError> {
    Ok(Formation {
        kind: FormationKind::from(f.type_),
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
                    desired_category: s.slots_desired_category,
                    active: s.slots_active,
                    desired_sub_category: s.slots_desired_sub_category,
                    transformed: 0,
                })
            })
            .collect::<Result<_, GenieError>>()?,
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
        transform_type: f.transform_type,
        number_lines: f.number_lines,
        transform_orientation: vec3(&f.transform_orientation),
        // a load's array holds the saved slots alone
        spare_slots: Vec::new(),
    })
}

fn group(g: &saved::UnitGroup, paths: &[saved::Path]) -> Result<UnitGroup, GenieError> {
    let t = &g.current_target;
    Ok(UnitGroup {
        motion: motion(&g.motion, paths)?,
        id: GroupId::try_from(g.id)?,
        owner: g.owner.to_bits(),
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
                        path_id: u.specific_moves_path_id,
                        reason: u.specific_moves_reason,
                        goal: vec3(&u.specific_moves_goal),
                        timer: Seconds::new(u.specific_moves_timer),
                        consecutive_move_failures: u.specific_moves_consecutive_move_failures,
                        distance: Distance::new(u.specific_moves_distance),
                        last_position: vec3(&u.specific_moves_last_position),
                    },
                    seconds_since_attacking: Seconds::new(u.seconds_since_attacking),
                })
            })
            .collect::<Result<_, GenieError>>()?,
        previous_formation: g.previous_formation.as_deref().map(formation).transpose()?,
        current_formation: g.current_formation.as_deref().map(formation).transpose()?,
        garbage: g.garbage,
        formation_locked_type: FormationKind::of_field(g.formation_locked_type),
        flags: GroupFlags::from_bits_retain(g.group_flags_m_bits),
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
        unit_types: g
            .unit_types
            .unit_type_statistic
            .iter()
            .map(|u| {
                Ok(UnitTypeStatistic {
                    formation_category: u.formation_category,
                    units: object_ids(&u.units)?,
                    used: u.used,
                    attack_range: Distance::new(u.attack_range),
                    desired_spacing: Distance::new(u.desired_spacing),
                    number_unit_types: u.number_unit_types,
                    unit_types: words(&u.unit_types),
                    unit_type_counts: words(&u.unit_type_counts),
                })
            })
            .collect::<Result<_, GenieError>>()?,
        waypoints: g.waypoints.iter().map(vec3).collect(),
        current_command: GroupCommandKind::named(g.current_command),
        current_target: TargetData {
            target: GroupTarget::of_saved(t.target, t.type_),
            original_position: vec3(&t.original_position),
            range: Distance::new(t.range),
            target_radius: Distance::new(t.target_radius),
            disable_retargeting: t.disable_retargeting,
            certain_action: ActionType::of_data(t.certain_action),
        },
        attack_plan_type: g.attack_plan_type,
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
        differentiator_id: g.differentiator_id,
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
/// dump gives the game's ([`globals_from_dump`]).
///
/// # Errors
///
/// When a group's path data names a path the world's table does not hold.
pub fn unit_groups(world: &saved::BaseWorld) -> Result<UnitGroups, GenieError> {
    let g = &world.unit_groups;
    let groups: Vec<UnitGroup> = g
        .unit_group
        .iter()
        .map(|u| group(u, &world.path))
        .collect::<Result<_, _>>()?;
    let mut commands = 0;
    for u in &groups {
        if commands < u.differentiator_id {
            commands = u.differentiator_id + 1;
        }
    }
    Ok(UnitGroups {
        groups,
        counter: GroupId::try_from(g.unit_group_counter)?,
        commands,
        unit_types_scratch: Vec::new(),
    })
}

/// The tick oracle's `globals` dump (one line `NAME WORD…` per global of the build data):
/// `group_commands` and `unit_types_scratch` into the world's groups; other lines are left.
pub fn globals_from_dump(text: &str, groups: &mut UnitGroups) {
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let name = words.next();
        let values: Vec<i32> = words.filter_map(|w| w.parse().ok()).collect();
        match name {
            Some("group_commands") => {
                if let Some(&v) = values.first() {
                    groups.commands = v;
                }
            }
            Some("unit_types_scratch") => {
                groups.unit_types_scratch = values.iter().map(|&v| v as u32).collect();
            }
            _ => {}
        }
    }
}

/// The differences of the slice's unit groups from the game's, one line per field that differs:
/// `group ID: FIELD slice A game B`, every field named by its path in the group's state
/// (`motion.velocity[1]`, `members[2].specific_move.timer`; a list's length is its `len`), each
/// float in full; a group only one side holds shows in the list of ids. With `globals` (the
/// game's process globals were dumped) the command counter and the scratch array count too.
#[must_use]
pub fn group_differences(slice: &UnitGroups, game: &UnitGroups, globals: bool) -> Vec<String> {
    let mut out = Vec::new();
    if globals {
        if slice.commands != game.commands {
            out.push(format!(
                "group commands slice {} game {}",
                slice.commands, game.commands
            ));
        }
        let words = |g: &UnitGroups| {
            let mut w = g.unit_types_scratch.clone();
            w.resize(50, 0);
            w
        };
        if words(slice) != words(game) {
            out.push(format!(
                "group unit types scratch slice {:?} game {:?}",
                words(slice),
                words(game)
            ));
        }
    }
    if slice.counter != game.counter {
        out.push(format!(
            "unit group counter slice {} game {}",
            slice.counter, game.counter
        ));
    }
    let ids = |g: &UnitGroups| g.groups.iter().map(|u| u.id.get()).collect::<Vec<_>>();
    if ids(slice) != ids(game) {
        out.push(format!(
            "unit groups slice {:?} game {:?}",
            ids(slice),
            ids(game)
        ));
    }
    for g in &game.groups {
        let Some(s) = slice.groups.iter().find(|s| s.id == g.id) else {
            continue;
        };
        let (mut a, mut b) = (Vec::new(), Vec::new());
        s.flat("", &mut a);
        g.flat("", &mut b);
        if a == b {
            continue;
        }
        let theirs: std::collections::BTreeMap<&str, &str> =
            b.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let ours: std::collections::BTreeMap<&str, &str> =
            a.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        for (k, v) in &a {
            match theirs.get(k.as_str()) {
                Some(w) if *w == v.as_str() => {}
                Some(w) => out.push(format!("group {}: {k} slice {v} game {w}", g.id)),
                None => out.push(format!("group {}: {k} slice {v} game none", g.id)),
            }
        }
        for (k, w) in &b {
            if !ours.contains_key(k.as_str()) {
                out.push(format!("group {}: {k} slice none game {w}", g.id));
            }
        }
    }
    out
}

/// A value as its fields' paths and values (`Debug`, floats in full), for
/// [`group_differences`].
trait Flat {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>);
}

/// `path.field`, or `field` at the top.
fn join(path: &str, field: &str) -> String {
    if path.is_empty() {
        field.to_owned()
    } else {
        format!("{path}.{field}")
    }
}

macro_rules! flat_value {
    ($($t:ty),*) => {$(
        impl Flat for $t {
            fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
                out.push((path.to_owned(), format!("{self:?}")));
            }
        }
    )*};
}

flat_value!(
    f32,
    i32,
    u32,
    i16,
    u16,
    u8,
    genie_core::movement::PathData,
    ObjectId,
    GroupId,
    GroupTarget,
    ActionType,
    FormationKind,
    SlotState,
    UnitStance
);

impl<T: Flat, const N: usize> Flat for [T; N] {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        for (i, v) in self.iter().enumerate() {
            v.flat(&format!("{path}[{i}]"), out);
        }
    }
}

/// A moment of the world's clock flattens as its milliseconds, as the save holds them.
impl Flat for WorldTime {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.millis().flat(path, out);
    }
}

/// A span of the world's clock flattens as its milliseconds, as the save holds them.
impl Flat for Millis {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.get().flat(path, out);
    }
}

/// A span of seconds flattens as its f32, as the save holds it.
impl Flat for Seconds {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.get().flat(path, out);
    }
}

/// A distance flattens as its f32, as the save holds it.
impl Flat for Distance {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.get().flat(path, out);
    }
}

/// A speed flattens as its f32, as the save holds it.
impl Flat for Speed {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.get().flat(path, out);
    }
}

/// A turn rate flattens as its f32, as the save holds it.
impl Flat for TurnRate {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.get().flat(path, out);
    }
}

/// A vector flattens as its three components in the game's order, as the save holds them.
impl Flat for BVector {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.get().flat(path, out);
    }
}

/// A group command's kind flattens as its value, as the save holds it.
impl Flat for GroupCommandKind {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.get().flat(path, out);
    }
}

/// A group's flags flatten as their bits, as the save holds them.
impl Flat for GroupFlags {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.bits().flat(path, out);
    }
}

/// A motion's flags flatten as their bits, as the save holds them.
impl Flat for MotionFlags {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.bits().flat(path, out);
    }
}

impl<T: Flat> Flat for Vec<T> {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        out.push((join(path, "len"), self.len().to_string()));
        for (i, v) in self.iter().enumerate() {
            v.flat(&format!("{path}[{i}]"), out);
        }
    }
}

impl<T: Flat> Flat for Option<T> {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        match self {
            None => out.push((path.to_owned(), "None".to_owned())),
            Some(v) => v.flat(path, out),
        }
    }
}

macro_rules! flat_struct {
    ($($t:ty { $($f:ident),* $(,)? })*) => {$(
        impl Flat for $t {
            fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
                $( self.$f.flat(&join(path, stringify!($f)), out); )*
            }
        }
    )*};
}

flat_struct! {
    Motion {
        position, new_position, velocity, acceleration, angular_velocity, angular_acceleration,
        turn_radius, turn_radius_speed, maximum_yaw_per_second_moving,
        stationary_yaw_revolution_time, maximum_yaw_per_second_stationary, paths, flags,
        maximum_velocity_forward, number_wait_delays,
    }
    SpecificMove {
        active, path_id, reason, goal, timer, consecutive_move_failures, distance, last_position,
    }
    Member { id, specific_move, seconds_since_attacking }
    Slot {
        unit_id, offset, transform_position, time, state, depth, desired_transform_position,
        position_filled, desired_category, active, desired_sub_category,
    }
    FormationLine { depth, indices, attack_range, distance_back }
    Formation {
        kind, slots, number_transformed_slots, maximum_width, maximum_depth, about_faced, lines,
        transform_type, number_lines, transform_orientation,
    }
    LinePoint { point, forward, up, right }
    PointLine { points, length, desired_length }
    UnitTypeStatistic {
        formation_category, units, used, attack_range, desired_spacing, number_unit_types,
        unit_types, unit_type_counts,
    }
    TargetData {
        target, original_position, range, target_radius, disable_retargeting,
        certain_action,
    }
    GroupOrder { command, target_id, parm, point, execute_time }
    GroupCommand {
        command, target_id, range, point, approved, disable_retargeting, approved_2,
    }
    UnitGroup {
        motion, id, owner, stance, members, previous_formation, current_formation, garbage,
        formation_locked_type, flags, centroid, desired_height, largest_unit_radius,
        smallest_unit_radius, desired_spacing, smallest_unit_range, smallest_ranged_unit_range,
        largest_unit_range, largest_ranged_unit_range, stored_positions, unit_types, waypoints,
        current_command, current_target, attack_plan_type, idle_timer, order_queue, hibernating,
        command_queue, objects, differentiator_id, seconds_not_under_attack,
        differentiator_groups, total_spacing, non_retarget_units,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_ids::{group_id, object_id};
    use genie_core::world::Seconds;

    fn world(groups: Vec<UnitGroup>) -> UnitGroups {
        UnitGroups {
            groups,
            counter: group_id(7),
            ..UnitGroups::default()
        }
    }

    fn group(id: i32) -> UnitGroup {
        UnitGroup {
            id: group_id(id),
            owner: 1,
            members: vec![Member {
                id: object_id(31000),
                ..Member::default()
            }],
            ..UnitGroup::default()
        }
    }

    /// A formation slot's saved state reads as its state (3, in place); 4 names none and the
    /// formation does not import.
    #[test]
    fn a_slot_takes_its_saved_state() {
        let saved = |state: i32| saved::Formation {
            slots: vec![saved::FormationEntry1 {
                slots_state: state,
                ..saved::FormationEntry1::default()
            }],
            ..saved::Formation::default()
        };
        let f = formation(&saved(3)).unwrap();
        assert_eq!(f.slots[0].state, SlotState::InPlace);
        assert!(formation(&saved(4)).is_err());
    }

    #[test]
    fn equal_groups_have_no_differences() {
        let d = group_differences(&world(vec![group(3)]), &world(vec![group(3)]), false);
        assert_eq!(d, Vec::<String>::new());
    }

    #[test]
    fn a_field_that_differs_is_named_by_its_path_with_both_values_in_full() {
        let mut theirs = group(3);
        theirs.motion.velocity = theirs.motion.velocity.with_height(0.1);
        theirs.members[0].specific_move.timer = Seconds::new(-0.0);
        let d = group_differences(&world(vec![group(3)]), &world(vec![theirs]), false);
        assert_eq!(
            d,
            [
                "group 3: motion.velocity[1] slice 0.0 game 0.1",
                "group 3: members[0].specific_move.timer slice 0.0 game -0.0",
            ]
        );
    }

    #[test]
    fn a_list_of_another_length_shows_its_length_and_the_entries_one_side_holds() {
        let mut theirs = group(3);
        theirs.waypoints.push(BVector::new(1.5, 2.5, 0.0));
        let d = group_differences(&world(vec![group(3)]), &world(vec![theirs]), false);
        assert_eq!(
            d,
            [
                "group 3: waypoints.len slice 0 game 1",
                "group 3: waypoints[0][0] slice none game 1.5",
                "group 3: waypoints[0][1] slice none game 2.5",
                "group 3: waypoints[0][2] slice none game 0.0",
            ]
        );
    }

    /// The globals dump sets the group command counter and the unit types' scratch words;
    /// other lines and a counter line without a value leave the groups.
    #[test]
    fn the_globals_dump_sets_the_command_counter_and_the_scratch() {
        let mut groups = world(vec![]);
        globals_from_dump(
            "group_commands 12\nunit_types_scratch 1 2 -1\nother 5\n",
            &mut groups,
        );
        assert_eq!(groups.commands, 12);
        assert_eq!(groups.unit_types_scratch, [1, 2, u32::MAX]);
        globals_from_dump("group_commands\n\n", &mut groups);
        assert_eq!(groups.commands, 12);
    }

    #[test]
    fn groups_only_one_side_holds_and_the_counter_show_in_the_world_lines() {
        let mut theirs = world(vec![group(3), group(4)]);
        theirs.counter = group_id(8);
        let d = group_differences(&world(vec![group(3)]), &theirs, false);
        assert_eq!(
            d,
            [
                "unit group counter slice 7 game 8",
                "unit groups slice [3] game [3, 4]",
            ]
        );
    }
}
