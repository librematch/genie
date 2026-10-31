//! `UpdateDesiredPositions` (`FUN_14103fd00`): the formation type, the walked path it measures
//! along, a slot's place and the point back from the front.

use crate::GenieError;
use crate::groups::FormationKind;
use crate::groups::GroupId;
use crate::groups::formation::EPSILON;
use crate::groups::group_list::NO_FORMATION;
use crate::groups::{Formation, UnitGroup};
use crate::map::Distance;
use crate::map::GridSize;
use crate::math::fmath::{BVector, Rotation, add, sub};
use crate::movement::PathData;
use crate::movement::PathFlags;
use crate::objects::ObjectKey;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::slots::CheckedIndex;

/// `FUN_140f96030`: the walked level's top ancestor (the paths are walked first), none for a
/// level without a parent.
pub(super) const fn top_level(paths: &[PathData]) -> Option<&PathData> {
    if 2 <= paths.len() { paths.last() } else { None }
}

/// `OnLastHighLevelWP` (`FUN_141048180`): the walked level's top ancestor holds a path whose last
/// waypoint (its points less one) the level has reached.
#[must_use]
pub fn on_last_high_level_waypoint(paths: &[PathData]) -> bool {
    top_level(paths)
        .filter(|t| t.has_path)
        .is_some_and(|t| t.points.len() as i32 - 1 <= t.waypoint)
}

/// `FUN_141046500`: the path of the walked level's top ancestor, or, without one, the walked
/// level's own.
pub(crate) fn top_path(paths: &[PathData]) -> Option<&PathData> {
    top_level(paths).map_or_else(
        || paths.first().filter(|w| w.has_path),
        |top| Some(top).filter(|t| t.has_path),
    )
}

/// The path `UpdateDesiredPositions` measures its way along: the top ancestor's (found by its id
/// `+0x20`, `FUN_1415419d0`), else the walked level's own (`+0x18`); a level whose path the
/// game holds by pointer holds its id too (`FUN_140f9b7c0` sets both), so
/// [`PathData::has_path`] stands for either.
pub(super) fn measured_path(paths: &[PathData]) -> Option<&PathData> {
    top_level(paths)
        .filter(|t| t.has_path)
        .or_else(|| paths.first().filter(|w| w.has_path))
}

impl World {
    /// The type `UpdateDesiredPositions` (`FUN_14103fd00`) chooses itself for group
    /// `g` (given none and its lock -1 or 0): a line (2), unless its path (the top level's, else the
    /// walked level's, [`measured_path`]) makes it a column (1): off the last high waypoint
    /// ([`on_last_high_level_waypoint`]) or on a patrol (the top path's flags bit 0,
    /// `FUN_141046500`) always; on the last waypoint, beyond both the owner's minimum column
    /// distance (`+0x6b8`) and its column-to-line distance (`+0x6bc`, plus the target's radius
    /// and range when the group has a target), measured on the ground from the group's place to
    /// the path's last point (the origin for a path of fewer than two points); the game tests a
    /// current column against the one and any other type against the other first, which the
    /// outcome does not depend on. Flags bit 2 (`MinAutoDistanceLock`) make it a line, a group of horde units
    /// only ([`World::horde_units_only`]) a horde (0).
    ///
    /// # Errors
    ///
    /// A member of a master the slice does not have.
    pub fn own_formation_type(&self, g: &UnitGroup) -> Result<FormationKind, GenieError> {
        let current = g
            .current_formation
            .as_ref()
            .map_or(FormationKind::HORDE, |f| f.kind);
        let mut kind = FormationKind::LINE;
        if let Some(path) = measured_path(&g.motion.paths) {
            let end = if 1 < path.points.len() {
                *path.points.checked(path.points.len() - 1)?
            } else {
                BVector::ZERO
            };
            let mut d = sub(end, g.motion.position[0]);
            d = d.with_height(0.0);
            let formations = usize::try_from(g.owner)
                .ok()
                .and_then(|p| self.players.get(p))
                .map(|p| p.formations)
                .unwrap_or_default();
            let mut column_to_line = formations.column_to_line_distance as f32;
            let min_column = formations.min_column_distance as f32;
            if g.current_target.target.key().is_some() {
                column_to_line =
                    column_to_line + g.current_target.target_radius + g.current_target.range.get();
            }
            let remaining =
                crate::math::fmath::sqrt(d.x() * d.x() + d.height() * d.height() + d.y() * d.y());
            let patrol =
                top_path(&g.motion.paths).is_some_and(|p| p.path_flags.contains(PathFlags::LOOPS));
            if on_last_high_level_waypoint(&g.motion.paths) && !patrol {
                if (current != FormationKind::COLUMN || column_to_line <= remaining)
                    && (current == FormationKind::COLUMN || min_column <= remaining)
                    && min_column < remaining
                    && column_to_line < remaining
                {
                    kind = FormationKind::COLUMN;
                }
            } else {
                kind = FormationKind::COLUMN;
            }
        }
        if g.flags & 4 != 0 {
            kind = FormationKind::LINE;
        }
        if self.horde_units_only(g)? {
            kind = FormationKind::HORDE;
        }
        Ok(kind)
    }

    /// The type `UpdateDesiredPositions` (`FUN_14103fd00`) lays group `g` out in for the type
    /// `given`: the given one, else (none, the game's −1) the group's lock
    /// (`mFormationLockedType`), or without a lock (−1) or with a horde's (0) its own choice
    /// ([`World::own_formation_type`]).
    ///
    /// # Errors
    ///
    /// As [`World::own_formation_type`].
    pub fn formation_type(
        &self,
        g: &UnitGroup,
        given: Option<FormationKind>,
    ) -> Result<FormationKind, GenieError> {
        Ok(match (given, g.formation_locked_type) {
            (Some(given), _) => given,
            (None, None | Some(FormationKind::HORDE)) => self.own_formation_type(g)?,
            (None, Some(locked)) => locked,
        })
    }

    /// `UpdateDesiredPositions` (`FUN_14103fd00`) of group `group` with the type `given` (none for
    /// the game's −1): nothing without members. A group without a formation gets an empty one
    /// ([`Formation::new`]); the type is [`World::formation_type`]'s. A type other
    /// than the formation's, or a given one, makes the formation the previous one and a new one
    /// the current. A formation not yet of the type with a slot per member is laid out: a line,
    /// staggered or flanked line or a column by createColumnLine
    /// ([`World::create_column_line`]), a phalanx and a box by their own
    /// ([`World::create_phalanx`], [`World::create_box`]), each with its transform and units; a
    /// horde and a user defined formation from the members' places ([`World::create_horde`],
    /// [`World::create_udf`]) and then transformed ([`World::formation_transform`], `param_4`
    /// 0); a formation already laid out, or of a type the switch does not know (6), is only
    /// transformed.
    ///
    /// The walked path the type's choice measures ([`measured_path`]) is the top level's only by
    /// its id and the walked level's only by its pointer in the game; the slice's
    /// [`PathData::has_path`] stands for either.
    ///
    /// # Errors
    ///
    /// A member of a master the slice does not have, and the
    /// layouts' and the transform's.
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, from a
    /// caller that holds it.
    pub fn update_desired_positions(
        &mut self,
        group: GroupId,
        given: Option<FormationKind>,
    ) -> Result<(), GenieError> {
        let g = self.groups.held(group)?;
        if g.members.is_empty() {
            return Ok(());
        }
        let kind = self.formation_type(g, given)?;
        let current_kind = g
            .current_formation
            .as_ref()
            .map_or(FormationKind::HORDE, |f| f.kind);
        let renew = current_kind != kind || given.is_some();
        let laid_out = !renew
            && g.current_formation
                .as_ref()
                .is_some_and(|f| f.kind == kind && f.slots.len() == g.members.len());
        let g = self.groups.held_mut(group)?;
        if g.current_formation.is_none() {
            g.current_formation = Some(Formation::new());
        }
        if renew {
            g.previous_formation = g.current_formation.take();
            g.current_formation = Some(Formation::new());
        }
        let mut f = g.current_formation.take().ok_or(NO_FORMATION)?;
        let result = match kind {
            FormationKind::LINE
            | FormationKind::STAGGERED
            | FormationKind::FLANKED
            | FormationKind::COLUMN
                if !laid_out =>
            {
                self.create_column_line(
                    &mut f,
                    group,
                    kind == FormationKind::COLUMN,
                    (
                        kind == FormationKind::STAGGERED,
                        kind == FormationKind::FLANKED,
                    ),
                )
                .map(|_| ())
            }
            FormationKind::PHALANX if !laid_out => self.create_phalanx(&mut f, group).map(|_| ()),
            FormationKind::BOX if !laid_out => self.create_box(&mut f, group).map(|_| ()),
            // true: the group has members; the transform after (`LAB_141040abf`)
            FormationKind::HORDE if !laid_out => self
                .groups
                .held(group)
                .and_then(|g| self.create_horde(&mut f, g))
                .and_then(|_| self.formation_transform(&mut f, group, false))
                .map(|_| ()),
            FormationKind::USER_DEFINED if !laid_out => self
                .groups
                .held(group)
                .and_then(|g| self.create_udf(&mut f, g))
                .and_then(|_| self.formation_transform(&mut f, group, false))
                .map(|_| ()),
            // laid out already, or a type the switch does not know: the transform alone
            _ => self.formation_transform(&mut f, group, false).map(|_| ()),
        };
        self.groups.held_mut(group)?.current_formation = Some(f);
        result
    }
}

/// A turn of more than this (`DAT_142eb0c64`, π/2) makes a formation of fewer than two
/// sub-formations take its units anew (`FUN_14103a1f0`).
pub(super) const RIGHT_ANGLE: f32 = crate::math::image::group::RIGHT_ANGLE;
/// A slot's place kept this far inside the map's far edges (`FUN_14100b760`, `DAT_142eb5690`).
pub(super) const MAP_EDGE: f32 = crate::math::image::group::MAP_EDGE;

/// `FUN_14100b760`: a place kept on the map: below 0 it is 0, from the map's width on it is the
/// width less [`MAP_EDGE`], on both axes (the y test against the height, its value the width's).
pub(in crate::groups) fn on_map(mut p: BVector, map: GridSize) -> BVector {
    if p.x() < 0.0 {
        p = p.with_x(0.0);
    }
    if p.y() < 0.0 {
        p = p.with_y(0.0);
    }
    let (w, h) = (map.width() as f32, map.height() as f32);
    if w <= p.x() {
        p = p.with_x(w - MAP_EDGE);
    }
    if h <= p.y() {
        p = p.with_y(w - MAP_EDGE);
    }
    p
}

/// `FUN_14103cfe0`: slot `index` of `formation` placed at `point` plus its offset (turned by
/// `rotation` but for the transform types 2 and 3) as its transform and desired transform
/// positions, kept on the map, with the world's time; once per transform (`+0x44`). Nothing for
/// a slot outside the slots in use or a point at the origin (shorter than 1e-7).
pub(super) fn transform_slot(
    formation: &mut Formation,
    rotation: &Rotation,
    point: BVector,
    index: i32,
    transform_type: i32,
    (world_time, map): (WorldTime, GridSize),
) -> Result<bool, GenieError> {
    if index < 0 || formation.slots.len() as i32 <= index {
        return Ok(false);
    }
    if crate::math::fmath::sqrt(crate::math::fmath::dot(point, point)).abs() < EPSILON {
        return Ok(false);
    }
    let slot = formation.slots.checked_mut(index as usize)?;
    if slot.transformed != 1 {
        let offset = if matches!(transform_type, 2 | 3) {
            slot.offset
        } else {
            crate::math::fmath::rotate(rotation, slot.offset)
        };
        let at = add(point, offset);
        slot.desired_transform_position = on_map(at, map);
        slot.transform_position = on_map(at, map);
        slot.transformed = 1;
        slot.time = world_time;
    }
    Ok(true)
}

/// `FindPointFromFront` (`FUN_141016990`): the point `distance` back along the stored
/// positions `line` from its front (its first point), on the ground between each point and the
/// next (their heights apart ignored, the height the earlier point's), none for a line of fewer
/// than two points or one shorter than the distance; a distance of 0 or less is the front point.
/// The forward and right it also gives (the two points' halfway, normalized, `FUN_141027d00`)
/// only reach the transform's log.
#[must_use]
pub fn find_point_from_front(
    line: &crate::groups::PointLine,
    distance: Distance,
) -> Option<BVector> {
    let distance = distance.get();
    if line.points.len() < 2 {
        return None;
    }
    let ground = |i: usize| {
        let mut d = sub(line.points.get(i + 1)?.point, line.points.get(i)?.point);
        d = d.with_height(0.0);
        Some(d)
    };
    let mut node = 0;
    let mut diff = ground(0)?;
    let mut length = crate::math::fmath::sqrt(
        diff.x() * diff.x() + diff.height() * diff.height() + diff.y() * diff.y(),
    );
    let mut covered = 0.0f32;
    while 0.0 <= length.abs() {
        let total = covered + length;
        if distance <= total {
            let p = line.points.get(node)?.point;
            if distance <= 0.0 {
                return Some(p);
            }
            let k = (distance - covered) / length;
            let along = [k * diff.x(), k * diff.height(), k * diff.y()];
            return Some(BVector::new(
                along[0] + p.x(),
                along[1] + p.height(),
                p.y() + along[2],
            ));
        }
        node += 1;
        if line.points.len() <= node + 1 {
            return None;
        }
        diff = ground(node)?;
        length = crate::math::fmath::sqrt(diff.x() * diff.x() + 0.0 + diff.y() * diff.y());
        covered = total;
    }
    None
}
