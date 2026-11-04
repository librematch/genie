//! The search's goal and area: the goal's query, a goal at the map's edge, an end partway,
//! the area's growth and the entrances' join.

use crate::map::Distance;
use crate::map::Rect;
use crate::map::Tile;
use crate::map::TileRect;
use crate::map::spatial::Movers;
use crate::map::spatial::RecordId;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered};
use crate::objects::ObjectId;
use crate::pathing::ShortFound;
use crate::pathing::hull_path::{
    EPSILON, EntranceIndex, HullIndex, HullPather, MOST_AREA, PATHS, Point, Target, Waypoints,
};
use crate::world::slots::CheckedIndexExt;
use crate::{GenieError, GenieResult};

/// Whether the hull search's area grew ([`HullPather::grow_area`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub(super) enum AreaGrowth {
    /// It grew by two tiles each way.
    Grown,
    /// It would hold more than [`MOST_AREA`] tiles: the search ends.
    TooLarge,
}

impl HullPather<'_> {
    /// `FindShortPath`'s goal query when no target stands on the goal (`FUN_141051900` with
    /// flags 0x20 at `to`, the manager's buffer and ignore list the pather's, its player for the
    /// gates): the records whose box, grown by the buffer, holds the goal, edges included (the
    /// point test `FUN_141052160`, moving ones too), but the ignored ones and a gate the player
    /// may pass (slot 0x678); the first with an object is the target. The goal lies on the map
    /// less the radius, so the grown point meets no edge record, and the temporary tile records
    /// have no object. The records come in the grid's order and each cell's list in the order
    /// the records joined it, which the slice does not hold: the answer is exact for one object
    /// holding the goal or none.
    pub(super) fn goal_record(&self, to: Point) -> GenieResult<Option<Target>> {
        let grow = self.buffer.unwrap_or(Distance::ZERO);
        if !self
            .world
            .point_hit(to, grow, &self.ignore, Movers::Counted, self.by_id)?
        {
            return Ok(None);
        }
        let near: Vec<ObjectId> = self
            .world
            .records()
            .filter(|&(id, _, rect)| {
                !self.ignore.contains(&RecordId::from(id)) && rect.grown(grow).holds(to)
            })
            .map(|(id, _, _)| id)
            .collect();
        // each one the query lists (the cells it visits): the point test with the others ignored
        let mut held = Vec::new();
        for &id in &near {
            let mut others = self.ignore.clone();
            others.extend(
                near.iter()
                    .filter(|&&object_id| object_id != id)
                    .map(|&object_id| RecordId::from(object_id)),
            );
            if !self
                .world
                .point_hit(to, grow, &others, Movers::Counted, self.by_id)?
            {
                continue;
            }
            let Some(placed) = self.world.get(id) else {
                continue;
            };
            if !self.passes(id, placed, self.player)? {
                held.push(placed);
            }
        }
        match held[..] {
            [] => Ok(None),
            [placed] => Ok(Some(Target {
                at: placed.position.ground(),
                radius: placed.radius,
                moves: placed.moves,
            })),
            _ => Err(GenieError::unsupported(
                "the old short pather's goal held by two objects or more (the grid's order)",
            )),
        }
    }

    /// `FindShortPath` for a goal off the map less the radius: the target (none: 4) must stand
    /// on it (`UCOMISS`, an unordered coordinate does not), and the goal lie on the map itself
    /// (`FUN_140f89d80` with the bounds the map's: `COMISS`/`JA` each way, an unordered
    /// coordinate on it); then the bounds go back to the map less the radius.
    pub(super) fn edge_goal(
        &mut self,
        to: Point,
        target_id: Option<ObjectId>,
    ) -> GenieResult<bool> {
        if target_id.is_none() {
            return Ok(false);
        }
        let Some(placed) = self.world.get_known(target_id)? else {
            return Ok(false);
        };
        if placed.position.x() != to.x() || placed.position.y() != to.y() {
            return Ok(false);
        }
        let radius = self.radius.get();
        let (width, height) = (self.world.map.width(), self.world.map.height());
        self.bounds = Rect::new(0.0, 0.0, width as f32 - 0.0, height as f32 - 0.0);
        let rect = self.bounds;
        let (bx0, by0, bx1, by1) = (rect.x0(), rect.y0(), rect.x1(), rect.y1());
        let on_map = at_most_or_unordered(bx0, to.x())
            && at_most_or_unordered(to.x(), bx1)
            && at_most_or_unordered(by0, to.y())
            && at_most_or_unordered(to.y(), by1);
        if on_map {
            self.bounds = Rect::new(
                radius,
                radius,
                width as f32 - radius,
                height as f32 - radius,
            );
        }
        Ok(on_map)
    }

    /// `FindShortPath`'s end for a path partway (2): none when it ends at the start (3); within
    /// range of the goal it is a path (1); one that ends no nearer the goal than the start is none.
    pub(super) fn partway(
        &self,
        path: &mut Waypoints,
        from: Point,
        to: Point,
        range: Distance,
    ) -> ShortFound {
        let mut result = ShortFound::Partial;
        let last = *path
            .points
            .last()
            .unwrap_or(&Point::new(f32::NAN, f32::NAN));
        if (last.x() - self.start.x()).abs() < EPSILON
            && (last.y() - self.start.y()).abs() < EPSILON
        {
            path.points.clear();
            result = ShortFound::Blocked;
        }
        let squared =
            (last.y() - to.y()) * (last.y() - to.y()) + (last.x() - to.x()) * (last.x() - to.x());
        // `COMISS`/`JC` (0x140f876ca): an unordered distance counts as nearer
        let start =
            (from.y() - to.y()) * (from.y() - to.y()) + (from.x() - to.x()) * (from.x() - to.x());
        if below_or_unordered(squared, start) {
            if squared < range.squared().get() {
                result = ShortFound::Reached;
            }
        } else {
            path.points.clear();
            result = ShortFound::Blocked;
        }
        result
    }

    /// The area grows by two tiles each way on the map (its new tiles' obstructions, then the
    /// cliffs about it); [`AreaGrowth::TooLarge`] when it would hold more than [`MOST_AREA`]
    /// tiles.
    pub(super) fn grow_area(&mut self) -> GenieResult<AreaGrowth> {
        let (width, height) = (self.world.map.width(), self.world.map.height());
        let x0 = (self.area.x0() - 2).max(0);
        let y0 = (self.area.y0() - 2).max(0);
        let x1 = (self.area.x1() + 2).min(width - 1);
        let y1 = (self.area.y1() + 2).min(height - 1);
        if MOST_AREA < (x1 - x0) * (y1 - y0) {
            return Ok(AreaGrowth::TooLarge);
        }
        let grown = TileRect::new(x0, y0, x1, y1);
        for x in x0..=x1 {
            for y in y0..=y1 {
                self.process_tile(Tile::new(x, y), grown, None)?;
            }
        }
        self.area = grown;
        self.process_cliffs()?;
        Ok(AreaGrowth::Grown)
    }

    /// The entrance paths' ends join the goal's hull (`FindShortPath`): each on the hull side it
    /// left by, by side and then by its distance from the side's first point, the later ones
    /// after the earlier; an end on the last side goes last. Each joined point keeps its path.
    pub(super) fn join_entrances(
        &mut self,
        hull: HullIndex,
        count: usize,
        sides: &[usize; PATHS],
    ) -> GenieResult<()> {
        if count == 0 {
            return Ok(());
        }
        let mut order: Vec<usize> = (0..count).collect();
        let mut by: Vec<usize> = sides.head(count)?.to_vec();
        let points = &self.hulls.checked(hull.index())?.points;
        let mut distances: Vec<f32> = (0..count)
            .map(|index| {
                let last = *self
                    .entrances
                    .checked(index)?
                    .points
                    .last()
                    .unwrap_or(&Point::ORIGIN);
                let vertex = if *by.checked(index)? == 0 {
                    points.len()
                } else {
                    *by.checked(index)?
                };
                let point = *points.checked(vertex - 1)?;
                Ok((last.y() - point.y()) * (last.y() - point.y())
                    + (last.x() - point.x()) * (last.x() - point.x()))
            })
            .collect::<GenieResult<_>>()?;
        for index in 0..count {
            for other in index + 1..count {
                if *by.checked(other)? < *by.checked(index)?
                    || (*by.checked(index)? == *by.checked(other)?
                        && *distances.checked(other)? < *distances.checked(index)?)
                {
                    by.swap(index, other);
                    distances.swap(index, other);
                    order.swap(index, other);
                }
            }
        }
        let mut joined = 0;
        for index in 0..count {
            let checked = *order.checked(index)?;
            let last = *self
                .entrances
                .checked(checked)?
                .points
                .last()
                .unwrap_or(&Point::ORIGIN);
            let hull = self.hulls.checked_mut(hull.index())?;
            let at = if joined + *by.checked(index)? != 0 {
                joined + *by.checked(index)?
            } else {
                hull.points.len()
            };
            hull.points.insert(at, last);
            hull.entrances.insert(at, Some(EntranceIndex(checked)));
            if *by.checked(index)? != 0 {
                joined += 1;
            }
        }
        Ok(())
    }
}
