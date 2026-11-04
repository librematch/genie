//! Views of a save's state (a recording's header save, or a save of the game after some updates)
//! and of a recording's checkpoints against the run from its header, as text.

use crate::errors::{no_chapter, no_player};
use genie_core::GenieResult;
use std::fmt::Write;

use crate::MasterTable;
use genie_core::GenieError;
use genie_core::objects::ObjectId;
use genie_core::unit_ai::QueuedOrder;
use genie_core::world::load::groups::unit_groups;
use genie_rec::de::chapters;
use genie_rec::de::{self, Op};
use genie_save::generated::world::RgeActionObject;
use genie_save::generated::world::{Formation, UnitGroup};
use genie_save::generated::world::{Path, RgeMovingObject, RgeStaticObject};
use genie_save::{Header, Object};

fn chapter(data: &[u8]) -> GenieResult<de::Chapter<'_>> {
    chapters(data)?.first().copied().ok_or(no_chapter())
}

/// Three little-endian f32s of a saved vector (`[u8; 12]`).
fn vec3(b: &[u8; 12]) -> [f32; 3] {
    let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    [f(0), f(4), f(8)]
}

/// The world's unit groups (`saveUnitGroups`: `world+0x50`'s list, as genie-save's generated
/// reader of the build holds it): the count and the id counter, then per group its id, owner,
/// state and flags, its units with their specific moves, its formations, centroid, waypoints,
/// current command and target and its queues; with `full`, every saved field as read.
///
/// # Errors
///
/// When the save cannot be read.
pub fn groups(data: &[u8], full: bool) -> GenieResult<String> {
    Ok(groups_of(&chapter(data)?.header()?, full))
}

/// [`groups`] of a save read.
fn groups_of(header: &Header, full: bool) -> String {
    let g = &header.world.base_world.unit_groups;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} unit groups, counter {}",
        g.number_unit_groups, g.unit_group_counter
    );
    if full {
        // the state as genie-core holds it, every vector decoded
        match unit_groups(&header.world.base_world) {
            Ok(groups) => {
                for u in &groups.groups {
                    let _ = writeln!(out, "{u:#?}");
                }
            }
            Err(error) => {
                let _ = writeln!(out, "the groups do not import: {error}");
            }
        }
        return out;
    }
    for group in &g.unit_group {
        group_lines(&mut out, group);
    }
    out
}

/// One saved unit group's lines: its state, its units' specific moves, its formations and
/// its command.
fn group_lines(out: &mut String, group: &UnitGroup) {
    let _ = writeln!(
        out,
        "group {} owner {} state {} flags {:#x} centroid {:?} hibernating {} idle {}",
        group.id,
        group.owner.to_bits(),
        group.state,
        group.group_flags_bits,
        vec3(&group.centroid),
        group.hibernating,
        group.idle_timer
    );
    for member in &group.units {
        let _ = writeln!(
            out,
            "  unit {} move active {} path {} reason {} goal {:?} timer {} failures {} distance {} last {:?} since attack {}",
            member.units,
            member.specific_moves_active,
            member.specific_moves_path_id,
            member.specific_moves_reason,
            vec3(&member.specific_moves_goal),
            member.specific_moves_timer,
            member.specific_moves_consecutive_move_failures,
            member.specific_moves_distance,
            vec3(&member.specific_moves_last_position),
            member.seconds_since_attacking
        );
    }
    for (name, f) in [
        ("previous", group.previous_formation.as_deref()),
        ("current", group.current_formation.as_deref()),
    ] {
        if let Some(formation) = f {
            formation_lines(out, name, formation);
        }
    }
    let waypoints: Vec<[f32; 3]> = group.waypoints.iter().map(vec3).collect();
    let _ = writeln!(
        out,
        "  command {} target {} type {} range {} waypoints {waypoints:?} orders {} commands {} spacing {}",
        group.current_command,
        group.current_target.target,
        group.current_target.kind,
        group.current_target.range,
        group.order_queue.len(),
        group.command_queue.len(),
        group.desired_spacing
    );
}

/// A group's formation `name` (previous or current): its type, slots and transform, then each
/// slot.
fn formation_lines(out: &mut String, name: &str, formation: &Formation) {
    let _ = writeln!(
        out,
        "  {name} formation type {} slots {} lines {} transform {} orientation {:?}",
        formation.kind,
        formation.number_slots,
        formation.number_lines,
        formation.transform_kind,
        vec3(&formation.transform_orientation)
    );
    for slot in &formation.slots {
        let _ = writeln!(
            out,
            "    slot unit {} offset {:?} position {:?} state {} depth {} desired {:?} filled {} category {}/{} active {}",
            slot.slots_unit_id,
            vec3(&slot.slots_offset),
            vec3(&slot.slots_transform_position),
            slot.slots_state,
            slot.slots_depth,
            vec3(&slot.slots_desired_transform_position),
            slot.slots_position_filled,
            slot.slots_desired_category,
            slot.slots_desired_sub_category,
            slot.slots_active
        );
    }
}

/// A player's live objects, one tab-separated row each.
///
/// A row holds the id, master, object state, hit points, position, the top action (type and
/// state) and the unit AI's retarget timer; with `actions_of`, that object's whole action list
/// comes before its row.
///
/// # Errors
///
/// When the save or the player cannot be read.
pub fn objects(data: &[u8], player: usize, actions_of: Option<i32>) -> GenieResult<String> {
    objects_of(&chapter(data)?.header()?, player, actions_of)
}

/// [`objects`] of a save read.
fn objects_of(header: &Header, player: usize, actions_of: Option<i32>) -> GenieResult<String> {
    let players = header.players();
    let section = players.get(player).ok_or_else(|| no_player(player))?;
    let mut out =
        String::from("id\tmaster\tstate\thp\tx\ty\tz\taction\taction_state\tretarget\tsprite\n");
    for object in section.live_objects() {
        let Some(rge_static_object) = object.static_part() else {
            continue;
        };
        if actions_of == Some(rge_static_object.id) {
            let _ = writeln!(
                out,
                "{:#?}",
                action_object(object)
                    .map(|object_rge_action_object| &object_rge_action_object.actions)
            );
        }
        let top = action_object(object)
            .and_then(|rge_action_object| rge_action_object.actions.action_kind_list.first());
        let (action, state) = top.map_or((-1, String::new()), |entry| {
            let debug = format!("{:?}", entry.action);
            let state = debug
                .split("state: ")
                .nth(1)
                .and_then(|text| text.split([',', ' ']).next())
                .unwrap_or("")
                .to_string();
            (entry.action_kind, state)
        });
        let retarget = object
            .combat()
            .and_then(|tribe_combat_object| tribe_combat_object.unit_ai())
            .map_or(String::new(), |ai| ai.randomized_retarget_timer.to_string());
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            rge_static_object.id,
            rge_static_object.master_obj,
            rge_static_object.object_state,
            rge_static_object.hp,
            rge_static_object.tx,
            rge_static_object.ty,
            rge_static_object.tz,
            action,
            state,
            retarget,
            sprite_column(rge_static_object)
        );
    }
    Ok(out)
}

/// The current sprite (`+0x140`, the save's `sprite`) and its node in the sprite list: `a` with
/// the animated node's `animate_flag`, `rge_static_object` for a static node, `-` without a node.
fn sprite_column(rge_static_object: &RgeStaticObject) -> String {
    let nodes = || {
        rge_static_object
            .active_sprite_list
            .iter()
            .flat_map(|active_sprite_list| active_sprite_list.sprite_kind_list.iter())
            .filter_map(|entry| entry.kind.as_deref())
    };
    let node = nodes()
        .filter_map(|n| n.rge_active_animated_sprite.as_deref())
        .find(|a| a.rge_active_sprite.sprite_id as i16 == rge_static_object.sprite)
        .map(|rge_active_animated_sprite| format!("a{}", rge_active_animated_sprite.animate_flag))
        .or_else(|| {
            nodes()
                .filter_map(|active_sprite| active_sprite.rge_active_sprite.as_deref())
                .any(|rge_active_sprite| {
                    rge_active_sprite.sprite_id as i16 == rge_static_object.sprite
                })
                .then(|| "s".to_string())
        })
        .unwrap_or_else(|| "-".to_string());
    format!("{} {node}", rge_static_object.sprite)
}

fn action_object(object: &Object) -> Option<&RgeActionObject> {
    if let Some(tribe_combat_object) = object.combat() {
        return Some(&tribe_combat_object.rge_combat_object.rge_action_object);
    }
    let rge_object = object.rge_object.as_deref()?;
    rge_object.rge_action_object.as_deref().or_else(|| {
        rge_object
            .rge_combat_object
            .as_deref()
            .map(|rge_combat_object| &rge_combat_object.rge_action_object)
    })
}

fn f3(b: &[u8; 12]) -> [f32; 3] {
    let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    [f(0), f(4), f(8)]
}

/// The saved movement state of a player's objects `ids`: position, velocity data, orientation,
/// the path data with the waypoints of each path (from the world's path table), speed fields.
///
/// # Errors
///
/// When the save or the player cannot be read.
pub fn movement(data: &[u8], player: usize, ids: &[i32]) -> GenieResult<String> {
    movement_of(&chapter(data)?.header()?, player, ids)
}

/// [`movement`] of a save read.
fn movement_of(header: &Header, player: usize, ids: &[i32]) -> GenieResult<String> {
    let paths = &header.world.base_world.path;
    let players = header.players();
    let section = players.get(player).ok_or_else(|| no_player(player))?;
    let mut out = String::new();
    for object in section.live_objects() {
        let (Some(rge_static_object), Some(rge_moving_object)) =
            (object.static_part(), object.moving_part())
        else {
            continue;
        };
        if ids.contains(&rge_static_object.id) {
            moving_lines(&mut out, rge_static_object, rge_moving_object, paths);
        }
    }
    Ok(out)
}

/// One moving object's lines: its place, velocity, orientation, speedup and each path level
/// with the world's paths of its id.
fn moving_lines(
    out: &mut String,
    rge_static_object: &RgeStaticObject,
    rge_moving_object: &RgeMovingObject,
    paths: &[Path],
) {
    let _ = writeln!(
        out,
        "{} master {} at ({}, {}, {}) facet {} sprite {}",
        rge_static_object.id,
        rge_static_object.master_obj,
        rge_static_object.tx,
        rge_static_object.ty,
        rge_static_object.tz,
        rge_static_object.facet,
        rge_static_object.sprite
    );
    let _ = writeln!(
        out,
        "  velocity ({}, {}, {}) angle {} turn_towards {} waiting {} on_ground {}",
        rge_moving_object.velocity_x,
        rge_moving_object.velocity_y,
        rge_moving_object.velocity_z,
        rge_moving_object.angle,
        rge_moving_object.turn_towards_time,
        rge_moving_object.waiting_to_move_value,
        rge_moving_object.on_ground
    );
    let _ = writeln!(
        out,
        "  movement {:?} {:?}",
        rge_moving_object.velocity.as_ref().map(f3),
        rge_moving_object.acceleration.as_ref().map(f3)
    );
    let _ = writeln!(
        out,
        "  position {:?} forward {:?} right {:?} last_move {}",
        f3(&rge_moving_object.position_value),
        f3(&rge_moving_object.orientation_forward_value),
        f3(&rge_moving_object.orientation_right_value),
        rge_moving_object.last_move_time_value
    );
    let _ = writeln!(
        out,
        "  speedup building {} value {}",
        rge_moving_object.speedup_building_id, rge_moving_object.speedup_value
    );
    for path_data in rge_moving_object
        .temp_path_data
        .iter()
        .chain(rge_moving_object.future_path_data.as_deref())
    {
        let _ = writeln!(out, "  path data {path_data:?}");
        for p in paths
            .iter()
            .filter(|path| path.id_value == path_data.path_id)
        {
            let points: Vec<[f32; 3]> = p.waypoints.iter().map(f3).collect();
            let _ = writeln!(
                out,
                "    path {} points {points:?} length {} created {} flags {:#x} refs {}",
                p.id_value,
                p.path_length_value,
                p.creation_time_value,
                p.flags_bits,
                p.reference_count
            );
        }
    }
}

/// The saved unit AI of a player's objects `ids`: its order, action and target with their last
/// values, the target's class and point, the timers, a boar's origin, the actions.
///
/// # Errors
///
/// When the save or the player cannot be read.
pub fn unit_ais(data: &[u8], player: usize, ids: &[i32]) -> GenieResult<String> {
    unit_ais_of(&chapter(data)?.header()?, player, ids)
}

/// [`unit_ais`] of a save read.
fn unit_ais_of(header: &Header, player: usize, ids: &[i32]) -> GenieResult<String> {
    let players = header.players();
    let section = players.get(player).ok_or_else(|| no_player(player))?;
    let mut out = String::new();
    for object in section.live_objects() {
        let Some(rge_static_object) = object.static_part() else {
            continue;
        };
        if !ids.contains(&rge_static_object.id) {
            continue;
        }
        let Some(combat) = object.combat() else {
            continue;
        };
        let Some(ai) = combat.unit_ai() else {
            continue;
        };
        let own = ObjectId::try_from(rge_static_object.id)?;
        let _ = writeln!(
            out,
            "{} master {} at ({}, {}) state {} unit-AI type {}",
            rge_static_object.id,
            rge_static_object.master_obj,
            rge_static_object.tx,
            rge_static_object.ty,
            rge_static_object.object_state,
            combat.need_unit_ai
        );
        if let Some(tribe_elephant_unit_ai_module) = combat.tribe_elephant_unit_ai_module.as_deref()
        {
            let _ = writeln!(
                out,
                "  origin ({}, {}, {}) furious {}",
                tribe_elephant_unit_ai_module.origin_x,
                tribe_elephant_unit_ai_module.origin_y,
                tribe_elephant_unit_ai_module.origin_z,
                tribe_elephant_unit_ai_module.furious
            );
        }
        let _ = writeln!(
            out,
            "  order {} action {} target {} (class {}, point ({}, {}, {})); last order {} action {} target {} (class {})",
            ai.current_order_value,
            ai.current_action_value,
            ai.current_target_value,
            ai.current_target_kind_value,
            ai.current_target_xvalue,
            ai.current_target_yvalue,
            ai.current_target_zvalue,
            ai.last_order_value,
            ai.last_action_value,
            ai.last_target_value,
            ai.last_target_kind_value
        );
        let _ = writeln!(
            out,
            "  idle {} / {} (base {}) secondary {} look around {} / {} retarget at {} every {} stance {}",
            ai.idle_timer_value,
            ai.adjusted_idle_timeout_value,
            ai.idle_timeout_value,
            ai.secondary_timer_value,
            ai.look_around_timer_value,
            ai.look_around_timeout_value,
            ai.last_retarget_time,
            ai.randomized_retarget_timer,
            ai.state_value
        );
        let actions: Vec<_> = object
            .actions()
            .map(|action_list| {
                action_list
                    .entries()
                    .map(|(kind, action)| {
                        (kind, action.base().map_or(0, |rge_action| rge_action.state))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  actions {actions:?} notifies {} orders {}",
            ai.number_notifies, ai.number_orders
        );
        for order in &ai.temp_order {
            let _ = writeln!(
                out,
                "    order {:?}",
                QueuedOrder::from_bytes(order, own, players.len())
            );
        }
    }
    Ok(out)
}

/// The map's tile elevations in the window `x` × `y` (inclusive), in both axis orders
/// (`rows[y][x]` and `rows[x][y]`): to check positions' z against the terrain.
///
/// # Errors
///
/// When the save cannot be read.
pub fn tiles(data: &[u8], x: (usize, usize), y: (usize, usize)) -> GenieResult<String> {
    let chapter = chapter(data)?;
    let header = chapter.header()?;
    let map = &header.world.base_world.map;
    let mut out = format!("map {} x {}\n", map.map_width, map.map_height);
    let at = |r: usize, c: usize| {
        map.rows
            .get(r)
            .and_then(|row| row.tiles.get(c))
            .map(|rge_tile| rge_tile.elevation)
    };
    for (name, transposed) in [("rows[y][x]", false), ("rows[x][y]", true)] {
        let _ = writeln!(out, "{name}: y down, x across {}..={}", x.0, x.1);
        for row in y.0..=y.1 {
            let line: Vec<String> = (x.0..=x.1)
                .map(|col| {
                    let (r, c) = if transposed { (col, row) } else { (row, col) };
                    at(r, c).map_or_else(|| "-".into(), |e| e.to_string())
                })
                .collect();
            let _ = writeln!(out, "  y {row:3}: {}", line.join(" "));
        }
    }
    Ok(out)
}

/// The run from a recording's header against the recording's own checkpoints.
///
/// The run goes `updates` updates, nothing forced. Per checkpoint (every 500 updates) it lists each
/// player's words of the 11-word block that differ, as `(word, slice, game)`, or `ok`. The
/// checkpoint after N syncs holds the state after update N - 1.
///
/// # Errors
///
/// When the recording cannot be read or the slice cannot run it.
///
/// # Panics
///
/// When a checkpoint names a player the run does not have.
pub fn checkpoints(rec: &[u8], masters: &MasterTable, updates: usize) -> GenieResult<String> {
    let chapter = chapter(rec)?;
    let players = chapter.header()?.players().len();
    let ids: Vec<u32> = (0..players)
        .map(|p| {
            u32::try_from(p)
                .map_err(|_| GenieError::invalid_input("a save with 2^32 players or more"))
        })
        .collect::<GenieResult<_>>()?;
    let blocks = crate::blocks_per_tick(rec, &ids, updates, Some(masters))?;
    checkpoint_lines(&chapter, &blocks)
}

/// One line per checkpoint of `chapter` that `blocks` (the run's 11-word blocks per tick and
/// player) reaches: each recorded player's differing words, or `ok`.
fn checkpoint_lines(chapter: &de::Chapter<'_>, blocks: &[Vec<[u32; 11]>]) -> GenieResult<String> {
    let mut out = String::new();
    let mut tick = 0usize;
    for op in chapter.ops() {
        match op? {
            Op::Sync { .. } => tick += 1,
            Op::Checkpoint { checkpoint, .. } => {
                if tick > blocks.len() {
                    break;
                }
                let row = &blocks[tick - 1];
                let _ = write!(out, "checkpoint at {tick}:");
                for lane in checkpoint
                    .lanes
                    .iter()
                    .filter(|l| l.iter().any(|&w| w != 0))
                {
                    let p = lane[8] as usize;
                    let differ: Vec<(usize, u32, u32)> = (0..11)
                        .filter(|&w| row[p][w] != lane[w])
                        .map(|w| (w, row[p][w], lane[w]))
                        .collect();
                    let _ = if differ.is_empty() {
                        write!(out, " p{p} ok")
                    } else {
                        write!(out, " p{p} {differ:?}")
                    };
                }
                out.push('\n');
                tick += 1;
            }
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
