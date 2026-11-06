//! The world's unit groups against the game's (FD-036): the tick oracle's `globals` dump into
//! the groups, and the field-by-field differences of two groups' states, which the forced
//! replay reports.

use genie_core::groups::GroupFlags;
use genie_core::groups::MotionFlags;
use genie_core::groups::{
    Differentiator, Formation, FormationCategory, FormationKind, FormationLine, GroupCommand,
    GroupCommandKind, GroupId, GroupOrder, GroupTarget, LinePoint, Member, Motion, MovePath,
    MoveReason, PointLine, Slot, SlotState, SpecificMove, TargetData, TransformKind, UnitGroup,
    UnitGroups, UnitKind, UnitKindStatistic,
};
use genie_core::map::Distance;
use genie_core::math::fmath::BVector;
use genie_core::movement::PathData;
use genie_core::movement::Speed;
use genie_core::movement::TurnRate;
use genie_core::objects::ObjectId;
use genie_core::objects::masters::ActionKind;
use genie_core::pathing::DelayCount;
use genie_core::players::PlayerId;
use genie_core::unit_ai::UnitStance;
use genie_core::world::Millis;
use genie_core::world::Seconds;
use genie_core::world::WorldTime;
use std::collections::BTreeMap;

/// The tick oracle's `globals` dump (one line `NAME WORD…` per global of the build data):
/// `group_commands` and `unit_types_scratch` into the world's groups; other lines are left.
pub fn globals_from_dump(text: &str, groups: &mut UnitGroups) {
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let name = words.next();
        let values: Vec<i32> = words.filter_map(|text| text.parse().ok()).collect();
        match name {
            Some("group_commands") => {
                if let Some(&value) = values.first() {
                    groups.commands = Differentiator::of_data(value);
                }
            }
            Some("unit_types_scratch") => {
                groups.unit_kinds_scratch = values
                    .iter()
                    .map(|&value| UnitKind::of_word(value.cast_unsigned()))
                    .collect();
            }
            _ => {}
        }
    }
}

/// Whether a comparison of unit groups holds the game's process globals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupGlobals {
    /// The globals were dumped: the command counter and the unit types' scratch array count.
    Compared,
    /// None were dumped: the comparison leaves them out.
    Skipped,
}

/// The differences of the slice's unit groups from the game's, one line per field that differs:
/// `group ID: FIELD slice A game B`, every field named by its path in the group's state
/// (`motion.velocity[1]`, `members[2].specific_move.timer`; a list's length is its `len`), each
/// float in full; a group only one side holds shows in the list of ids. With the `globals`
/// [`GroupGlobals::Compared`] the command counter and the scratch array count too.
#[must_use]
pub fn group_differences(
    slice: &UnitGroups,
    game: &UnitGroups,
    globals: GroupGlobals,
) -> Vec<String> {
    let mut out = Vec::new();
    if globals == GroupGlobals::Compared {
        if slice.commands != game.commands {
            out.push(format!(
                "group commands slice {} game {}",
                slice.commands.get(),
                game.commands.get()
            ));
        }
        let words = |groups: &UnitGroups| {
            let mut unit_kinds = groups.unit_kinds_scratch.clone();
            unit_kinds.resize(50, UnitKind::CLEARED);
            unit_kinds
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
    let ids = |groups: &UnitGroups| {
        groups
            .groups
            .iter()
            .map(|group| group.id.get())
            .collect::<Vec<_>>()
    };
    if ids(slice) != ids(game) {
        out.push(format!(
            "unit groups slice {:?} game {:?}",
            ids(slice),
            ids(game)
        ));
    }
    for game_group in &game.groups {
        let Some(group) = slice.groups.iter().find(|group| group.id == game_group.id) else {
            continue;
        };
        let (mut slice_fields, mut game_fields) = (Vec::new(), Vec::new());
        group.flat("", &mut slice_fields);
        game_group.flat("", &mut game_fields);
        if slice_fields == game_fields {
            continue;
        }
        let theirs: BTreeMap<&str, &str> = game_fields
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let ours: BTreeMap<&str, &str> = slice_fields
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        for (key, value) in &slice_fields {
            match theirs.get(key.as_str()) {
                Some(game_value) if *game_value == value.as_str() => {}
                Some(text) => out.push(format!(
                    "group {}: {key} slice {value} game {text}",
                    game_group.id
                )),
                None => out.push(format!(
                    "group {}: {key} slice {value} game none",
                    game_group.id
                )),
            }
        }
        for (key, game_value) in &game_fields {
            if !ours.contains_key(key.as_str()) {
                out.push(format!(
                    "group {}: {key} slice none game {game_value}",
                    game_group.id
                ));
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
    PathData,
    ObjectId,
    GroupId,
    GroupTarget,
    ActionKind,
    FormationKind,
    SlotState,
    UnitStance
);

impl<T: Flat, const N: usize> Flat for [T; N] {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        for (index, element) in self.iter().enumerate() {
            element.flat(&format!("{path}[{index}]"), out);
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

/// A wait count flattens as its int, as the save holds it.
impl Flat for DelayCount {
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

/// A player flattens as its number, as the save holds it.
impl Flat for PlayerId {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.get().flat(path, out);
    }
}

/// A member's own move's path and reason flatten as their values, as the save holds them.
impl Flat for MovePath {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.get().flat(path, out);
    }
}

impl Flat for MoveReason {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.get().flat(path, out);
    }
}

/// A formation's transform type flattens as its value, as the save holds it.
impl Flat for TransformKind {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.get().flat(path, out);
    }
}

/// A differentiator flattens as its value, as the save holds it.
impl Flat for Differentiator {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.get().flat(path, out);
    }
}

/// A unit type flattens as its signed word.
impl Flat for UnitKind {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        self.get().flat(path, out);
    }
}

/// A formation category flattens as its value, as the save holds it.
impl Flat for FormationCategory {
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
        for (index, element) in self.iter().enumerate() {
            element.flat(&format!("{path}[{index}]"), out);
        }
    }
}

impl<T: Flat> Flat for Option<T> {
    fn flat(&self, path: &str, out: &mut Vec<(String, String)>) {
        match self {
            None => out.push((path.to_owned(), "None".to_owned())),
            Some(inner) => inner.flat(path, out),
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
        transform_kind, number_lines, transform_orientation,
    }
    LinePoint { point, forward, up, right }
    PointLine { points, length, desired_length }
    UnitKindStatistic {
        formation_category, units, used, attack_range, desired_spacing, number_unit_kinds,
        unit_kinds, unit_kind_counts,
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
        formation_locked_kind, flags, centroid, desired_height, largest_unit_radius,
        smallest_unit_radius, desired_spacing, smallest_unit_range, smallest_ranged_unit_range,
        largest_unit_range, largest_ranged_unit_range, stored_positions, unit_kinds, waypoints,
        current_command, current_target, attack_plan_kind, idle_timer, order_queue, hibernating,
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
            owner: Some(PlayerId::new(1)),
            members: vec![Member {
                id: object_id(31000),
                ..Member::default()
            }],
            ..UnitGroup::default()
        }
    }

    #[test]
    fn equal_groups_have_no_differences() {
        let texts = group_differences(
            &world(vec![group(3)]),
            &world(vec![group(3)]),
            GroupGlobals::Skipped,
        );
        assert_eq!(texts, Vec::<String>::new());
    }

    #[test]
    fn field_that_differs_is_named_by_its_path_with_both_values_in_full() {
        let mut theirs = group(3);
        theirs.motion.velocity = theirs.motion.velocity.with_height(0.1);
        theirs.members[0].specific_move.timer = Seconds::new(-0.0);
        let texts = group_differences(
            &world(vec![group(3)]),
            &world(vec![theirs]),
            GroupGlobals::Skipped,
        );
        assert_eq!(
            texts,
            [
                "group 3: motion.velocity[1] slice 0.0 game 0.1",
                "group 3: members[0].specific_move.timer slice 0.0 game -0.0",
            ]
        );
    }

    #[test]
    fn list_of_another_length_shows_its_length_and_the_entries_one_side_holds() {
        let mut theirs = group(3);
        theirs.waypoints.push(BVector::new(1.5, 2.5, 0.0));
        let texts = group_differences(
            &world(vec![group(3)]),
            &world(vec![theirs]),
            GroupGlobals::Skipped,
        );
        assert_eq!(
            texts,
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
        assert_eq!(groups.commands, Differentiator::of_data(12));
        assert_eq!(
            groups.unit_kinds_scratch,
            [UnitKind::of_word(1), UnitKind::of_word(2), UnitKind::NONE]
        );
        globals_from_dump("group_commands\n\n", &mut groups);
        assert_eq!(groups.commands, Differentiator::of_data(12));
    }

    #[test]
    fn groups_only_one_side_holds_and_the_counter_show_in_the_world_lines() {
        let mut theirs = world(vec![group(3), group(4)]);
        theirs.counter = group_id(8);
        let texts = group_differences(&world(vec![group(3)]), &theirs, GroupGlobals::Skipped);
        assert_eq!(
            texts,
            [
                "unit group counter slice 7 game 8",
                "unit groups slice [3] game [3, 4]",
            ]
        );
    }
}
