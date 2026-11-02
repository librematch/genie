//! Views of a save's state (a recording's header save, or a save of the game after some updates)
//! and of a recording's checkpoints against the run from its header, as text.

use std::fmt::Write;

use crate::{Error, MasterTable};
use genie_core::GenieError;
use genie_rec::de::{self, Op};
use genie_save::generated::world::RgeActionObject;
use genie_save::generated::world::{Formation, UnitGroup};
use genie_save::{Header, Object};

fn chapter(data: &[u8]) -> Result<de::Chapter<'_>, Error> {
    de::chapters(data)?.first().copied().ok_or(Error::NoChapter)
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
pub fn groups(data: &[u8], full: bool) -> Result<String, Error> {
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
        match crate::groups::unit_groups(&header.world.base_world) {
            Ok(groups) => {
                for u in &groups.groups {
                    let _ = writeln!(out, "{u:#?}");
                }
            }
            Err(e) => {
                let _ = writeln!(out, "the groups do not import: {e}");
            }
        }
        return out;
    }
    for u in &g.unit_group {
        group_lines(&mut out, u);
    }
    out
}

/// One saved unit group's lines: its state, its units' specific moves, its formations and
/// its command.
fn group_lines(out: &mut String, u: &UnitGroup) {
    let _ = writeln!(
        out,
        "group {} owner {} state {} flags {:#x} centroid {:?} hibernating {} idle {}",
        u.id,
        u.owner.to_bits(),
        u.state,
        u.group_flags_bits,
        vec3(&u.centroid),
        u.hibernating,
        u.idle_timer
    );
    for e in &u.units {
        let _ = writeln!(
            out,
            "  unit {} move active {} path {} reason {} goal {:?} timer {} failures {} distance {} last {:?} since attack {}",
            e.units,
            e.specific_moves_active,
            e.specific_moves_path_id,
            e.specific_moves_reason,
            vec3(&e.specific_moves_goal),
            e.specific_moves_timer,
            e.specific_moves_consecutive_move_failures,
            e.specific_moves_distance,
            vec3(&e.specific_moves_last_position),
            e.seconds_since_attacking
        );
    }
    for (name, f) in [
        ("previous", u.previous_formation.as_deref()),
        ("current", u.current_formation.as_deref()),
    ] {
        if let Some(f) = f {
            formation_lines(out, name, f);
        }
    }
    let waypoints: Vec<[f32; 3]> = u.waypoints.iter().map(vec3).collect();
    let _ = writeln!(
        out,
        "  command {} target {} type {} range {} waypoints {waypoints:?} orders {} commands {} spacing {}",
        u.current_command,
        u.current_target.target,
        u.current_target.type_,
        u.current_target.range,
        u.order_queue.len(),
        u.command_queue.len(),
        u.desired_spacing
    );
}

/// A group's formation `name` (previous or current): its type, slots and transform, then each
/// slot.
fn formation_lines(out: &mut String, name: &str, f: &Formation) {
    let _ = writeln!(
        out,
        "  {name} formation type {} slots {} lines {} transform {} orientation {:?}",
        f.type_,
        f.number_slots,
        f.number_lines,
        f.transform_type,
        vec3(&f.transform_orientation)
    );
    for slot in &f.slots {
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
pub fn objects(data: &[u8], player: usize, actions_of: Option<i32>) -> Result<String, Error> {
    objects_of(&chapter(data)?.header()?, player, actions_of)
}

/// [`objects`] of a save read.
fn objects_of(header: &Header, player: usize, actions_of: Option<i32>) -> Result<String, Error> {
    let players = header.players();
    let section = players.get(player).ok_or(Error::NoPlayer(player))?;
    let mut out =
        String::from("id\tmaster\tstate\thp\tx\ty\tz\taction\taction_state\tretarget\tsprite\n");
    for object in section.live_objects() {
        let Some(s) = object.static_part() else {
            continue;
        };
        if actions_of == Some(s.id) {
            let _ = writeln!(out, "{:#?}", action_object(object).map(|a| &a.actions));
        }
        let top = action_object(object).and_then(|a| a.actions.action_type_list.first());
        let (action, state) = top.map_or((-1, String::new()), |e| {
            let debug = format!("{:?}", e.action);
            let state = debug
                .split("state: ")
                .nth(1)
                .and_then(|r| r.split([',', ' ']).next())
                .unwrap_or("")
                .to_string();
            (e.action_type, state)
        });
        let retarget = object
            .combat()
            .and_then(|c| c.unit_ai())
            .map_or(String::new(), |ai| ai.randomized_retarget_timer.to_string());
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            s.id,
            s.master_obj,
            s.object_state,
            s.hp,
            s.tx,
            s.ty,
            s.tz,
            action,
            state,
            retarget,
            sprite_column(s)
        );
    }
    Ok(out)
}

/// The current sprite (`+0x140`, the save's `sprite`) and its node in the sprite list: `a` with
/// the animated node's `animate_flag`, `s` for a static node, `-` without a node.
fn sprite_column(s: &genie_save::generated::world::RgeStaticObject) -> String {
    let nodes = || {
        s.active_sprite_list
            .iter()
            .flat_map(|l| l.sprite_type_list.iter())
            .filter_map(|e| e.type_.as_deref())
    };
    let node = nodes()
        .filter_map(|n| n.rge_active_animated_sprite.as_deref())
        .find(|a| a.rge_active_sprite.sprite_id as i16 == s.sprite)
        .map(|a| format!("a{}", a.animate_flag))
        .or_else(|| {
            nodes()
                .filter_map(|n| n.rge_active_sprite.as_deref())
                .any(|a| a.sprite_id as i16 == s.sprite)
                .then(|| "s".to_string())
        })
        .unwrap_or_else(|| "-".to_string());
    format!("{} {node}", s.sprite)
}

fn action_object(o: &Object) -> Option<&RgeActionObject> {
    if let Some(c) = o.combat() {
        return Some(&c.rge_combat_object.rge_action_object);
    }
    let r = o.rge_object.as_deref()?;
    r.rge_action_object
        .as_deref()
        .or_else(|| r.rge_combat_object.as_deref().map(|c| &c.rge_action_object))
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
pub fn movement(data: &[u8], player: usize, ids: &[i32]) -> Result<String, Error> {
    movement_of(&chapter(data)?.header()?, player, ids)
}

/// [`movement`] of a save read.
fn movement_of(header: &Header, player: usize, ids: &[i32]) -> Result<String, Error> {
    let paths = &header.world.base_world.path;
    let players = header.players();
    let section = players.get(player).ok_or(Error::NoPlayer(player))?;
    let mut out = String::new();
    for object in section.live_objects() {
        let (Some(s), Some(m)) = (object.static_part(), object.moving_part()) else {
            continue;
        };
        if ids.contains(&s.id) {
            moving_lines(&mut out, s, m, paths);
        }
    }
    Ok(out)
}

/// One moving object's lines: its place, velocity, orientation, speedup and each path level
/// with the world's paths of its id.
fn moving_lines(
    out: &mut String,
    s: &genie_save::generated::world::RgeStaticObject,
    m: &genie_save::generated::world::RgeMovingObject,
    paths: &[genie_save::generated::world::Path],
) {
    let _ = writeln!(
        out,
        "{} master {} at ({}, {}, {}) facet {} sprite {}",
        s.id, s.master_obj, s.tx, s.ty, s.tz, s.facet, s.sprite
    );
    let _ = writeln!(
        out,
        "  velocity ({}, {}, {}) angle {} turn_towards {} waiting {} on_ground {}",
        m.velocity_x,
        m.velocity_y,
        m.velocity_z,
        m.angle,
        m.turn_towards_time,
        m.waiting_to_move_value,
        m.on_ground
    );
    let _ = writeln!(
        out,
        "  movement {:?} {:?}",
        m.velocity.as_ref().map(f3),
        m.acceleration.as_ref().map(f3)
    );
    let _ = writeln!(
        out,
        "  position {:?} forward {:?} right {:?} last_move {}",
        f3(&m.position_value),
        f3(&m.orientation_forward_value),
        f3(&m.orientation_right_value),
        m.last_move_time_value
    );
    let _ = writeln!(
        out,
        "  speedup building {} value {}",
        m.speedup_building_id, m.speedup_value
    );
    for d in m.temp_path_data.iter().chain(m.future_path_data.as_deref()) {
        let _ = writeln!(out, "  path data {d:?}");
        for p in paths.iter().filter(|p| p.id_value == d.path_id) {
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
pub fn unit_ais(data: &[u8], player: usize, ids: &[i32]) -> Result<String, Error> {
    unit_ais_of(&chapter(data)?.header()?, player, ids)
}

/// [`unit_ais`] of a save read.
fn unit_ais_of(header: &Header, player: usize, ids: &[i32]) -> Result<String, Error> {
    let players = header.players();
    let section = players.get(player).ok_or(Error::NoPlayer(player))?;
    let mut out = String::new();
    for object in section.live_objects() {
        let Some(s) = object.static_part() else {
            continue;
        };
        if !ids.contains(&s.id) {
            continue;
        }
        let Some(combat) = object.combat() else {
            continue;
        };
        let Some(ai) = combat.unit_ai() else {
            continue;
        };
        let own = genie_core::objects::ObjectId::try_from(s.id).map_err(Error::Unmodelled)?;
        let _ = writeln!(
            out,
            "{} master {} at ({}, {}) state {} unit-AI type {}",
            s.id, s.master_obj, s.tx, s.ty, s.object_state, combat.need_unit_ai
        );
        if let Some(e) = combat.tribe_elephant_unit_ai_module.as_deref() {
            let _ = writeln!(
                out,
                "  origin ({}, {}, {}) furious {}",
                e.origin_x, e.origin_y, e.origin_z, e.furious
            );
        }
        let _ = writeln!(
            out,
            "  order {} action {} target {} (class {}, point ({}, {}, {})); last order {} action {} target {} (class {})",
            ai.current_order_value,
            ai.current_action_value,
            ai.current_target_value,
            ai.current_target_type_value,
            ai.current_target_xvalue,
            ai.current_target_yvalue,
            ai.current_target_zvalue,
            ai.last_order_value,
            ai.last_action_value,
            ai.last_target_value,
            ai.last_target_type_value
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
            .map(|a| {
                a.entries()
                    .map(|(kind, a)| (kind, a.base().map_or(0, |b| b.state)))
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
                genie_core::unit_ai::QueuedOrder::from_bytes(order, own, players.len())
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
pub fn tiles(data: &[u8], x: (usize, usize), y: (usize, usize)) -> Result<String, Error> {
    let chapter = chapter(data)?;
    let header = chapter.header()?;
    let map = &header.world.base_world.map;
    let mut out = format!("map {} x {}\n", map.map_width, map.map_height);
    let at = |r: usize, c: usize| {
        map.rows
            .get(r)
            .and_then(|row| row.tiles.get(c))
            .map(|t| t.elevation)
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
pub fn checkpoints(rec: &[u8], masters: &MasterTable, updates: usize) -> Result<String, Error> {
    let chapter = chapter(rec)?;
    let players = chapter.header()?.players().len();
    let ids: Vec<u32> = (0..players)
        .map(|p| {
            u32::try_from(p)
                .map_err(|_| GenieError::invalid_input("a save with 2^32 players or more"))
        })
        .collect::<Result<_, GenieError>>()?;
    let blocks = crate::blocks_per_tick(rec, &ids, updates, Some(masters))?;
    checkpoint_lines(&chapter, &blocks)
}

/// One line per checkpoint of `chapter` that `blocks` (the run's 11-word blocks per tick and
/// player) reaches: each recorded player's differing words, or `ok`.
fn checkpoint_lines(chapter: &de::Chapter<'_>, blocks: &[Vec<[u32; 11]>]) -> Result<String, Error> {
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
