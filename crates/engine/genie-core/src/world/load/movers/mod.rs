//! A saved moving object's movement: its masters' animated and moving parts, its path levels,
//! its movement state and its `MoveTo`'s point.

use crate::map::Distance;
use crate::map::RestrictionId;
use crate::map::{Position, Radii};
use crate::math::Angle;
use crate::math::fmath::BVector;
use crate::movement::LevelFlags;
use crate::movement::LinkedPathKind;
use crate::movement::PathFlags;
use crate::movement::PlanTries;
use crate::movement::Speed;
use crate::movement::SubstituteCount;
use crate::movement::WaitDelays;
use crate::movement::WaypointIndex;
use crate::movement::{Mover, PathData as MovementPathData, PathState, WaypointLevel, turn_rate};
use crate::objects::ObjectId;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteId;
use crate::objects::layers::Animated;
use crate::objects::layers::{Speedup, SpeedupValue};
use crate::objects::sprite::NO_SPRITE;
use crate::players::CivId;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::load::object_ids;
use crate::{GenieError, GenieResult};
use genie_import::dat::MasterTable;
use genie_import::dat::dat_id;
use genie_import::save::master_facets;
use genie_import::save::parts::combat_master;
use genie_import::save::parts::{animated_master, moving_master};
use genie_save::generated::world::{Path, PathData, PlayerObjects, RgeStaticObject};

/// A saved vector's three `f32`s, as the save holds them (little-endian).
#[must_use]
pub fn vec3(bytes: &[u8; 12]) -> BVector {
    let float_at = |offset: usize| {
        f32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    BVector::new(float_at(0), float_at(4), float_at(8))
}

/// One saved level of path data, with its path's waypoints from the world's path table.
pub fn path_level(path_data: &PathData, paths: &[Path]) -> GenieResult<MovementPathData> {
    // a level without a path (id -1) holds none: a bird's top level
    let path = match path_data.path_id {
        -1 => None,
        id => Some(paths.iter().find(|path| path.id_value == id).ok_or(
            GenieError::unsupported("path data whose path is not in the world's table"),
        )?),
    };
    Ok(MovementPathData {
        level: WaypointLevel::of_byte(path_data.waypoint_level),
        linked_kind: LinkedPathKind::of_data(path_data.linked_path_kind),
        points: path.map_or_else(Vec::new, |vec3_path| {
            vec3_path.waypoints.iter().map(vec3).collect()
        }),
        has_path: path.is_some(),
        path_flags: PathFlags::from_bits_retain(path.map_or(0, |path| path.flags_bits)),
        waypoint: WaypointIndex::of_data(path_data.waypoint),
        state: PathState::of_int(path_data.state)?,
        range: Distance::new(path_data.range),
        target: ObjectId::of_field(path_data.target_id),
        pause_time: Seconds::new(f32::from_bits(path_data.pause_time)),
        flags: LevelFlags::from_bits_retain(path_data.flags_bits),
        length: path.map_or(Distance::ZERO, |path| Distance::new(path.path_length_value)),
        created: path.map_or(WorldTime::ZERO, |path| {
            WorldTime::from_millis(path.creation_time_value)
        }),
        continue_counter: PlanTries::of_data(path_data.continue_counter),
        repath_time: Seconds::new(path_data.repath_time),
        ignore: path.map_or_else(
            || Ok(Vec::new()),
            |path| {
                path.data
                    .iter()
                    .take(path.number_ignore_units.max(0) as usize)
                    .map(|&id| ObjectId::try_from(id as i32))
                    .collect()
            },
        )?,
        ignored: Vec::new(),
    })
}

/// A moving object's movement state from the save: orientation, velocity data and path data (the
/// save lists the levels top first; the game walks the last), and from its master and the `.dat`
/// its speed, turning and radii. `None` for an object that does not move (types below 30).
///
/// The top speed is the master's speed times the saved sprite's speed factor when that is above 0
/// (`FUN_14114d1d0`'s first term); its speed-up (`+0x808`, `+0x80c`) is the save's. moveForward's
/// factor `+0x878` is not in the save; every moving object of MarvinBlue/426048731 holds 1.0 at
/// tick 680 (the tick oracle).
pub fn mover(
    object: &genie_save::Object,
    rge_static_object: &RgeStaticObject,
    objects: &PlayerObjects,
    paths: &[Path],
    civ: CivId,
    masters: Option<&MasterTable>,
) -> GenieResult<Option<Mover>> {
    use crate::movement::{MoveStats, Mover, Velocity};
    let Some(rge_moving_object) = object.moving_part() else {
        return Ok(None);
    };
    if rge_moving_object.future_path_data.is_some() {
        return Err(GenieError::unsupported("saved future path data"));
    }
    // an object with its own master (a copy, say a captured animal's) moves by its own values
    let (radius, restriction, base, turn, flags, sprite_facets) = match (
        rge_static_object.own_master(),
        rge_static_object.own_animated_master(),
    ) {
        (Some(own), Some(animated)) => (
            Radii::new(own.radius_x, own.radius_y, own.radius_z),
            own.terrain_restriction_mask_id,
            animated.max_speed,
            rge_static_object
                .own_moving_master()
                .map(|own_rge_master_moving_object| {
                    (
                        own_rge_master_moving_object.turn_speed,
                        own_rge_master_moving_object.temp_move_sprite,
                    )
                }),
            own.object_flags,
            // an own master copy (a captured animal's) shows its master's sprites
            masters.and_then(|master_table| {
                let master = objects.master(rge_static_object.master_obj)?;
                master_facets(master, master.static_part()?, master_table)
            }),
        ),
        (Some(_), None) => {
            return Err(GenieError::unsupported(
                "a moving object's own master without an animated part",
            ));
        }
        (None, _) => {
            let master =
                objects
                    .master(rge_static_object.master_obj)
                    .ok_or(GenieError::unsupported(
                        "a moving object whose master is not saved",
                    ))?;
            let placement = master.static_part().ok_or(GenieError::unsupported(
                "a moving master without a static part",
            ))?;
            let base = animated_master(master)
                .ok_or(GenieError::unsupported(
                    "a moving master without an animated part",
                ))?
                .max_speed;
            (
                Radii::new(placement.radius_x, placement.radius_y, placement.radius_z),
                placement.terrain_restriction_mask_id,
                base,
                moving_master(master).map(|rge_master_moving_object| {
                    (
                        rge_master_moving_object.turn_speed,
                        rge_master_moving_object.move_sprite_id,
                    )
                }),
                placement.object_flags,
                masters.and_then(|master_table| master_facets(master, placement, master_table)),
            )
        }
    };
    let table = masters.ok_or(GenieError::unsupported(
        "no master table: turning is unknown",
    ))?;
    // without a sprite (−1: the empty town-centre annex 890, say) no factor
    let factor = if rge_static_object.sprite == -1 {
        SpeedMultiplier::ZERO
    } else {
        SpriteId::of_data(rge_static_object.sprite)
            .and_then(|sprite| table.sprite_speed(sprite))
            .ok_or(NO_SPRITE)?
    };
    let speed = factor.applied_to(Speed::new(base));
    let turning = dat_id(objects, rge_static_object.master_obj)
        .and_then(|master_id| table.turning(civ, master_id))
        .ok_or(GenieError::unsupported(
            "a moving master the data file does not have",
        ))?;
    let (turn_speed, walking) = turn.ok_or(GenieError::unsupported(
        "a moving master without a moving part",
    ))?;
    let facets = SpriteId::of_data(walking)
        .and_then(|sprite_id| table.graphics.get(sprite_id))
        .map(|graphic| graphic.facets);
    let velocity = (rge_moving_object.have_movement == 1).then(|| Velocity {
        velocity: rge_moving_object
            .velocity
            .as_ref()
            .map_or(BVector::ZERO, vec3),
        acceleration: rge_moving_object
            .acceleration
            .as_ref()
            .map_or(BVector::ZERO, vec3),
    });
    let mut levels = rge_moving_object
        .temp_path_data
        .iter()
        .rev()
        .map(|path_data| path_level(path_data, paths))
        .collect::<Result<Vec<_>, _>>()?;
    let ignore_units = object_ids(&rge_static_object.temp_int)?;
    loaded_ignored(&mut levels, &ignore_units);
    Ok(Some(Mover {
        animated: Animated::new(Speed::new(
            rge_moving_object.rge_animated_object.current_speed,
        )),
        forward: vec3(&rge_moving_object.orientation_forward_value),
        right: vec3(&rge_moving_object.orientation_right_value),
        velocity,
        paths: levels,
        factor: 1.0,
        stats: MoveStats {
            fly_mode: dat_id(objects, rge_static_object.master_obj)
                .and_then(|master_id| table.fly_mode(civ, master_id))
                .unwrap_or_default(),
            speed,
            base: Speed::new(base),
            turn_rate: turn_rate(Seconds::new(turn_speed), facets),
            turn_radius: Distance::ZERO,
            turn_radius_speed: Speed::MAX,
            radius,
            restriction: RestrictionId::of_field(restriction)?
                .ok_or(GenieError::unsupported("a negative terrain restriction"))?,
            restriction_mask: RestrictionId::of_data(restriction)
                .and_then(|restriction_id| table.terrain.masks().get(restriction_id.index()))
                .copied()
                .ok_or(GenieError::unsupported(
                    "a terrain restriction the data file does not have",
                ))?,
            turn_time: Seconds::new(turn_speed),
            move_algorithm: turning.move_algorithm,
            min_collision: turning.min_collision_size_multiplier,
            fast_turn: flags & 2 != 0,
            facets: sprite_facets,
            weapon_range: objects
                .master(rge_static_object.master_obj)
                .and_then(combat_master)
                .map_or(Distance::ZERO, |rge_master_combat_object| {
                    Distance::new(rge_master_combat_object.weapon_range)
                }),
        },
        waiting: rge_moving_object.waiting_to_move_value,
        wait_delays: WaitDelays::of_data(rge_moving_object.number_wait_delays),
        attack_moving: rge_moving_object.attack_moving != 0,
        last_move: WorldTime::from_millis(rge_moving_object.last_move_time_value),
        // the save's x, height, y
        moved_at: {
            let float_at = |index: usize| {
                let position_value = &rge_moving_object.position_value;
                f32::from_le_bytes([
                    position_value[index],
                    position_value[index + 1],
                    position_value[index + 2],
                    position_value[index + 3],
                ])
            };
            Position::new(float_at(0), float_at(8), float_at(4))
        },
        substitute_count: SubstituteCount::of_data(rge_moving_object.consecutive_substitute_count),
        speedup: Speedup {
            building: ObjectId::of_field(rge_moving_object.speedup_building_id),
            value: SpeedupValue::of_data(rge_moving_object.speedup_value),
        },
        ignore_units,
        // the save's userDefinedWaypoints, in the game's layout (x, height, y)
        user_waypoints: rge_moving_object
            .user_defined_waypoints
            .iter()
            .map(vec3)
            .collect(),
        last_turn: Seconds::new(rge_moving_object.turn_towards_time),
        angle: Angle::new(rge_moving_object.angle),
        pauses: Vec::new(),
        in_the_way: Vec::new(),
        // the current sprite's, which the unit's update takes from its sprite
        sprite_facets: None,
    }))
}

/// A loaded unit's paths' ignore units (`+0x450`), which the save does not hold: a level below
/// the top whose path was made in the top path's tick took the unit's own (`+0x670`,
/// `FUN_140f9b7a0` after the short pather).
pub fn loaded_ignored(levels: &mut [MovementPathData], units: &[ObjectId]) {
    let Some((top, below)) = levels.split_last_mut() else {
        return;
    };
    if top.points.is_empty() {
        return;
    }
    for level in below
        .iter_mut()
        .filter(|path_data| path_data.created == top.created)
    {
        level.ignored = units.to_vec();
    }
}

#[cfg(test)]
mod tests;
