//! The search's goal and area: the goal's query, a goal at the map's edge, an end partway,
//! the area's growth and the entrances' join.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Rect;
use crate::map::Tile;
use crate::map::TileRect;
use crate::map::spatial::RecordId;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered};
use crate::objects::ObjectId;
use crate::pathing::ShortFound;
use crate::pathing::hull_path::{
    EPSILON, EntranceIndex, HullIndex, HullPather, MOST_AREA, PATHS, Point, Target, Waypoints,
};
use crate::world::slots::CheckedIndex;

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
    pub(super) fn goal_record(&self, to: Point) -> Result<Option<Target>, GenieError> {
        let grow = self.buffer.unwrap_or(Distance::ZERO);
        if !self.world.point_hit(to, grow, &self.ignore, false)? {
            return Ok(None);
        }
        let near: Vec<ObjectId> = self
            .world
            .records()
            .filter(|&(id, _, b)| {
                !self.ignore.contains(&RecordId::from(id)) && b.grown(grow).holds(to)
            })
            .map(|(id, _, _)| id)
            .collect();
        // each one the query lists (the cells it visits): the point test with the others ignored
        let mut held = Vec::new();
        for &id in &near {
            let mut others = self.ignore.clone();
            others.extend(
                near.iter()
                    .filter(|&&o| o != id)
                    .map(|&o| RecordId::from(o)),
            );
            if !self.world.point_hit(to, grow, &others, false)? {
                continue;
            }
            let Some(p) = self.world.get(id) else {
                continue;
            };
            if !self.passes(id, p, self.player)? {
                held.push(p);
            }
        }
        match held[..] {
            [] => Ok(None),
            [p] => Ok(Some(Target {
                at: p.position.ground(),
                radius: p.radius,
                speed: p.speed,
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
    ) -> Result<bool, GenieError> {
        if target_id.is_none() {
            return Ok(false);
        }
        let Some(t) = self.world.get_known(target_id)? else {
            return Ok(false);
        };
        if t.position.x() != to.x() || t.position.y() != to.y() {
            return Ok(false);
        }
        let r = self.radius.get();
        let (w, h) = (self.world.map.width(), self.world.map.height());
        self.bounds = Rect::new(0.0, 0.0, w as f32 - 0.0, h as f32 - 0.0);
        let b = self.bounds;
        let (bx0, by0, bx1, by1) = (b.x0(), b.y0(), b.x1(), b.y1());
        let on_map = at_most_or_unordered(bx0, to.x())
            && at_most_or_unordered(to.x(), bx1)
            && at_most_or_unordered(by0, to.y())
            && at_most_or_unordered(to.y(), by1);
        if on_map {
            self.bounds = Rect::new(r, r, w as f32 - r, h as f32 - r);
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
        let d =
            (last.y() - to.y()) * (last.y() - to.y()) + (last.x() - to.x()) * (last.x() - to.x());
        // `COMISS`/`JC` (0x140f876ca): an unordered distance counts as nearer
        let start =
            (from.y() - to.y()) * (from.y() - to.y()) + (from.x() - to.x()) * (from.x() - to.x());
        if below_or_unordered(d, start) {
            if d < range.squared().get() {
                result = ShortFound::Reached;
            }
        } else {
            path.points.clear();
            result = ShortFound::Blocked;
        }
        result
    }

    /// The area grows by two tiles each way on the map (its new tiles' obstructions, then the
    /// cliffs about it); false when it would hold more than [`MOST_AREA`] tiles.
    pub(super) fn grow_area(&mut self) -> Result<bool, GenieError> {
        let (w, h) = (self.world.map.width(), self.world.map.height());
        let x0 = (self.area.x0() - 2).max(0);
        let y0 = (self.area.y0() - 2).max(0);
        let x1 = (self.area.x1() + 2).min(w - 1);
        let y1 = (self.area.y1() + 2).min(h - 1);
        if MOST_AREA < (x1 - x0) * (y1 - y0) {
            return Ok(false);
        }
        let grown = TileRect::new(x0, y0, x1, y1);
        for x in x0..=x1 {
            for y in y0..=y1 {
                self.process_tile(Tile::new(x, y), grown, None)?;
            }
        }
        self.area = grown;
        self.process_cliffs()?;
        Ok(true)
    }

    /// The entrance paths' ends join the goal's hull (`FindShortPath`): each on the hull side it
    /// left by, by side and then by its distance from the side's first point, the later ones
    /// after the earlier; an end on the last side goes last. Each joined point keeps its path.
    pub(super) fn join_entrances(
        &mut self,
        hull: HullIndex,
        count: usize,
        sides: &[usize; PATHS],
    ) -> Result<(), GenieError> {
        if count == 0 {
            return Ok(());
        }
        let mut order: Vec<usize> = (0..count).collect();
        let mut by: Vec<usize> = sides.head(count)?.to_vec();
        let points = &self.hulls.checked(hull.index())?.points;
        let mut d: Vec<f32> = (0..count)
            .map(|k| {
                let last = *self
                    .entrances
                    .checked(k)?
                    .points
                    .last()
                    .unwrap_or(&Point::ORIGIN);
                let i = if *by.checked(k)? == 0 {
                    points.len()
                } else {
                    *by.checked(k)?
                };
                let p = *points.checked(i - 1)?;
                Ok((last.y() - p.y()) * (last.y() - p.y())
                    + (last.x() - p.x()) * (last.x() - p.x()))
            })
            .collect::<Result<_, GenieError>>()?;
        for a in 0..count {
            for b in a + 1..count {
                if *by.checked(b)? < *by.checked(a)?
                    || (*by.checked(a)? == *by.checked(b)? && *d.checked(b)? < *d.checked(a)?)
                {
                    by.swap(a, b);
                    d.swap(a, b);
                    order.swap(a, b);
                }
            }
        }
        let mut joined = 0;
        for k in 0..count {
            let e = *order.checked(k)?;
            let last = *self
                .entrances
                .checked(e)?
                .points
                .last()
                .unwrap_or(&Point::ORIGIN);
            let hull = self.hulls.checked_mut(hull.index())?;
            let at = if joined + *by.checked(k)? != 0 {
                joined + *by.checked(k)?
            } else {
                hull.points.len()
            };
            hull.points.insert(at, last);
            hull.entrances.insert(at, Some(EntranceIndex(e)));
            if *by.checked(k)? != 0 {
                joined += 1;
            }
        }
        Ok(())
    }
}
