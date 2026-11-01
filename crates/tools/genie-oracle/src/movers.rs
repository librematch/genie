//! A saved moving object's movement: its masters' animated and moving parts, its path levels,
//! its movement state and its `MoveTo`'s point.

use crate::masters::MasterTable;
use crate::masters::dat_id;
use crate::object_ids;
use crate::{combat_master, master_facets};
use genie_core::GenieError;
use genie_core::map::Distance;
use genie_core::map::{Position, Radii};
use genie_core::math::Angle;
use genie_core::math::fmath::BVector;
use genie_core::movement::LevelFlags;
use genie_core::movement::LinkedPathType;
use genie_core::movement::PathFlags;
use genie_core::movement::PlanTries;
use genie_core::movement::Speed;
use genie_core::movement::SubstituteCount;
use genie_core::movement::WaitDelays;
use genie_core::movement::WaypointIndex;
use genie_core::objects::ObjectId;
use genie_core::objects::SpeedMultiplier;
use genie_core::objects::SpriteId;
use genie_core::players::CivId;
use genie_core::world::Seconds;
use genie_core::world::WorldTime;
use genie_save::generated::world::{
    Master, Path, PathData, PlayerObjects, RgeMasterAnimatedObject, RgeStaticObject,
};

pub(super) fn vec3(b: &[u8; 12]) -> BVector {
    let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    BVector::new(f(0), f(4), f(8))
}

/// The animated part of a player's master, which holds its speed (`mMaxSpeed`, master `+0x740`).
pub(super) fn animated_master(m: &Master) -> Option<&RgeMasterAnimatedObject> {
    let combat = m.tribe_master_combat_object.as_deref().or_else(|| {
        m.tribe_master_building_object
            .as_deref()
            .map(|b| &b.tribe_master_combat_object)
    });
    if let Some(c) = combat {
        let moving = &c
            .rge_master_combat_object
            .rge_master_action_object
            .rge_master_moving_object;
        return Some(&moving.rge_master_animated_object);
    }
    let r = m.rge_master.as_deref()?;
    None.or(r.rge_master_animated_object.as_deref())
        .or_else(|| {
            r.rge_master_moving_object
                .as_deref()
                .map(|m| &m.rge_master_animated_object)
        })
        .or_else(|| {
            r.rge_master_action_object
                .as_deref()
                .map(|a| &a.rge_master_moving_object.rge_master_animated_object)
        })
        .or_else(|| {
            r.rge_master_combat_object.as_deref().map(|c| {
                &c.rge_master_action_object
                    .rge_master_moving_object
                    .rge_master_animated_object
            })
        })
        .or_else(|| {
            r.rge_master_missile_object.as_deref().map(|m| {
                &m.rge_master_combat_object
                    .rge_master_action_object
                    .rge_master_moving_object
                    .rge_master_animated_object
            })
        })
}

/// The moving part of a saved master, when its class has one.
pub(super) fn moving_master(
    m: &Master,
) -> Option<&genie_save::generated::world::RgeMasterMovingObject> {
    let combat = m.tribe_master_combat_object.as_deref().or_else(|| {
        m.tribe_master_building_object
            .as_deref()
            .map(|b| &b.tribe_master_combat_object)
    });
    if let Some(c) = combat {
        return Some(
            &c.rge_master_combat_object
                .rge_master_action_object
                .rge_master_moving_object,
        );
    }
    let r = m.rge_master.as_deref()?;
    None.or(r.rge_master_moving_object.as_deref())
        .or_else(|| {
            r.rge_master_action_object
                .as_deref()
                .map(|a| &a.rge_master_moving_object)
        })
        .or_else(|| {
            r.rge_master_combat_object
                .as_deref()
                .map(|c| &c.rge_master_action_object.rge_master_moving_object)
        })
        .or_else(|| {
            r.rge_master_missile_object.as_deref().map(|m| {
                &m.rge_master_combat_object
                    .rge_master_action_object
                    .rge_master_moving_object
            })
        })
}

/// One saved level of path data, with its path's waypoints from the world's path table.
pub(super) fn path_level(
    d: &PathData,
    paths: &[Path],
) -> Result<genie_core::movement::PathData, GenieError> {
    // a level without a path (id -1) holds none: a bird's top level
    let path = match d.path_id {
        -1 => None,
        id => Some(
            paths
                .iter()
                .find(|p| p.idvalue == id)
                .ok_or(GenieError::unsupported(
                    "path data whose path is not in the world's table",
                ))?,
        ),
    };
    Ok(genie_core::movement::PathData {
        level: genie_core::movement::WaypointLevel::of_byte(d.waypoint_level),
        linked_type: LinkedPathType::of_data(d.linked_path_type),
        points: path.map_or_else(Vec::new, |p| p.m_waypoints.iter().map(vec3).collect()),
        has_path: path.is_some(),
        path_flags: PathFlags::from_bits_retain(path.map_or(0, |p| p.flags_m_bits)),
        waypoint: WaypointIndex::of_data(d.waypoint),
        state: genie_core::movement::PathState::of_int(d.state)?,
        range: Distance::new(d.range),
        target: ObjectId::of_field(d.target_id),
        pause_time: Seconds::new(f32::from_bits(d.pause_time)),
        flags: LevelFlags::from_bits_retain(d.flags_m_bits),
        length: path.map_or(Distance::ZERO, |p| Distance::new(p.path_length_value)),
        created: path.map_or(WorldTime::ZERO, |p| {
            WorldTime::from_millis(p.creation_time_value)
        }),
        continue_counter: PlanTries::of_data(d.continue_counter),
        repath_time: Seconds::new(d.repath_time),
        ignore: path.map_or_else(
            || Ok(Vec::new()),
            |p| {
                p.data
                    .iter()
                    .take(p.number_ignore_units.max(0) as usize)
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
/// (`FUN_14114d1d0`); slot 65's other terms (`+0xa08`, `+0xa2c`, `+0xb38`, a followed object) are
/// not read yet. moveForward's factor `+0x878` is not in the save; every moving object of
/// MarvinBlue/426048731 holds 1.0 at tick 680 (the tick oracle).
pub(super) fn mover(
    object: &genie_save::Object,
    s: &RgeStaticObject,
    objects: &PlayerObjects,
    paths: &[Path],
    civ: CivId,
    masters: Option<&MasterTable>,
) -> Result<Option<genie_core::movement::Mover>, GenieError> {
    use genie_core::movement::{MoveStats, Mover, Velocity};
    let Some(m) = object.moving_part() else {
        return Ok(None);
    };
    if m.future_path_data.is_some() {
        return Err(GenieError::unsupported("saved future path data"));
    }
    // an object with its own master (a copy, say a captured animal's) moves by its own values
    let (radius, restriction, base, turn, flags, sprite_facets) =
        match (s.own_master(), s.own_animated_master()) {
            (Some(own), Some(animated)) => (
                Radii::new(own.radius_x, own.radius_y, own.radius_z),
                own.terrain_restriction_mask_id,
                animated.max_speed,
                s.own_moving_master()
                    .map(|m| (m.turn_speed, m.temp_move_sprite)),
                own.object_flags,
                // an own master copy (a captured animal's) shows its master's sprites
                masters.and_then(|t| {
                    let master = objects.master(s.master_obj)?;
                    master_facets(master, master.static_part()?, t)
                }),
            ),
            (Some(_), None) => {
                return Err(GenieError::unsupported(
                    "a moving object's own master without an animated part",
                ));
            }
            (None, _) => {
                let master = objects.master(s.master_obj).ok_or(GenieError::unsupported(
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
                    moving_master(master).map(|m| (m.turn_speed, m.move_sprite_id)),
                    placement.object_flags,
                    masters.and_then(|t| master_facets(master, placement, t)),
                )
            }
        };
    let table = masters.ok_or(GenieError::unsupported(
        "no master table: turning is unknown",
    ))?;
    // without a sprite (−1: the empty town-centre annex 890, say) no factor
    let factor = if s.sprite == -1 {
        SpeedMultiplier::ZERO
    } else {
        SpriteId::of_data(s.sprite)
            .and_then(|sprite| table.sprite_speed(sprite))
            .ok_or(GenieError::unsupported(
                "a sprite the data file does not have",
            ))?
    };
    let speed = factor.applied_to(Speed::new(base));
    let turning = dat_id(objects, s.master_obj)
        .and_then(|m| table.turning(civ, m))
        .ok_or(GenieError::unsupported(
            "a moving master the data file does not have",
        ))?;
    let (turn_speed, walking) = turn.ok_or(GenieError::unsupported(
        "a moving master without a moving part",
    ))?;
    let facets = SpriteId::of_data(walking)
        .and_then(|w| table.graphics.get(&w))
        .map(|g| g.facets);
    let velocity = (m.have_movement == 1).then(|| Velocity {
        velocity: m.velocity.as_ref().map_or(BVector::ZERO, vec3),
        acceleration: m.acceleration.as_ref().map_or(BVector::ZERO, vec3),
    });
    let mut levels = m
        .temp_path_data
        .iter()
        .rev()
        .map(|d| path_level(d, paths))
        .collect::<Result<Vec<_>, _>>()?;
    let ignore_units = object_ids(&s.temp_int)?;
    loaded_ignored(&mut levels, &ignore_units);
    Ok(Some(Mover {
        animated: genie_core::objects::layers::Animated::new(Speed::new(
            m.rge_animated_object.current_speed,
        )),
        forward: vec3(&m.orientation_forward_value),
        right: vec3(&m.orientation_right_value),
        velocity,
        paths: levels,
        factor: 1.0,
        stats: MoveStats {
            fly_mode: dat_id(objects, s.master_obj)
                .and_then(|m| table.fly_mode(civ, m))
                .unwrap_or_default(),
            speed,
            base: Speed::new(base),
            turn_rate: genie_core::movement::turn_rate(Seconds::new(turn_speed), facets),
            turn_radius: Distance::ZERO,
            turn_radius_speed: Speed::MAX,
            radius,
            restriction: genie_core::map::RestrictionId::of_field(restriction)?
                .ok_or(GenieError::unsupported("a negative terrain restriction"))?,
            restriction_mask: genie_core::map::RestrictionId::of_data(restriction)
                .and_then(|r| table.terrain.masks().get(r.index()))
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
                .master(s.master_obj)
                .and_then(combat_master)
                .map_or(Distance::ZERO, |c| Distance::new(c.weapon_range)),
        },
        waiting: m.waiting_to_move_value,
        wait_delays: WaitDelays::of_data(m.number_wait_delays),
        attack_moving: m.attack_moving != 0,
        last_move: WorldTime::from_millis(m.last_move_time_value),
        // the save's x, height, y
        moved_at: {
            let f = |i: usize| {
                let b = &m.position_value;
                f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
            };
            Position::new(f(0), f(8), f(4))
        },
        substitute_count: SubstituteCount::of_data(m.consecutive_substitute_count),
        ignore_units,
        // the save's userDefinedWaypoints, in the game's layout (x, height, y)
        user_waypoints: m.user_defined_waypoints.iter().map(vec3).collect(),
        last_turn: Seconds::new(m.turn_towards_time),
        angle: Angle::new(m.angle),
        pauses: Vec::new(),
        in_the_way: Vec::new(),
        // the current sprite's, which the unit's update takes from its sprite
        sprite_facets: None,
    }))
}

/// A loaded unit's paths' ignore units (`+0x450`), which the save does not hold: a level below
/// the top whose path was made in the top path's tick took the unit's own (`+0x670`,
/// `FUN_140f9b7a0` after the short pather).
pub(super) fn loaded_ignored(levels: &mut [genie_core::movement::PathData], units: &[ObjectId]) {
    let Some((top, below)) = levels.split_last_mut() else {
        return;
    };
    if top.points.is_empty() {
        return;
    }
    for level in below.iter_mut().filter(|l| l.created == top.created) {
        level.ignored = units.to_vec();
    }
}

#[cfg(test)]
mod tests;
