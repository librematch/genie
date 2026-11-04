//! The hulls' building from the area's obstructions: the tiles' boxes, the runs the unit
//! cannot cross, the cliffs; a point's hull and the area's bounds.

use crate::map::Distance;
use crate::map::Rect;
use crate::map::Tile;
use crate::map::TileRect;
use crate::map::spatial::RecordId;
use crate::map::spatial::{Placed, tile_record_id};
use crate::math::fmath::at_most_or_unordered;
use crate::objects::ObjectId;
use crate::pathing::hull_path::convex::AtStart;
use crate::pathing::hull_path::{HALF, Hull, HullIndex, HullPather, Point, Shape};
use crate::players::PlayerId;
use crate::world::layer::ClassRoles;
use crate::world::layer::generated::HullPathTuning;
use crate::world::slots::CheckedIndex;
use crate::{GenieError, GenieResult};

impl HullPather<'_> {
    // The hulls.

    /// `MShortPather::addShapeToHulls` (`FUN_140f8af60`): the box `min`..`max` into the hulls:
    /// into the first it overlaps (its corners outside it, or all four when it only touches),
    /// which then takes in each hull it overlaps in turn; a box inside a hull changes nothing;
    /// else a new hull. Returns the hull's index.
    pub(super) fn add_shape_to(
        hulls: &mut Vec<Hull>,
        min: Point,
        max: Point,
        hull_path_tuning: &HullPathTuning,
    ) -> GenieResult<HullIndex> {
        let mut index = 0;
        let mut hull: Option<HullIndex> = None;
        while index < hulls.len() {
            let (overlap, outside) =
                hulls
                    .checked(index)?
                    .shape_overlap(min, max, hull_path_tuning);
            let corners = [
                min,
                Point::new(max.x(), min.y()),
                max,
                Point::new(min.x(), max.y()),
            ];
            match overlap {
                1 => {
                    hulls.checked_mut(index)?.merge(&corners)?;
                    hull = Some(HullIndex::new(index));
                }
                2 => {
                    let some: Vec<Point> = corners
                        .iter()
                        .zip(outside)
                        .filter(|&(_, outside)| outside)
                        .map(|(corner, _)| *corner)
                        .collect();
                    hulls.checked_mut(index)?.merge(&some)?;
                    hull = Some(HullIndex::new(index));
                }
                3 => return Ok(HullIndex::new(index)),
                _ => {
                    index += 1;
                    continue;
                }
            }
            break;
        }
        let Some(mut mine) = hull else {
            hulls.push(Hull {
                points: vec![
                    max,
                    Point::new(max.x(), min.y()),
                    min,
                    Point::new(min.x(), max.y()),
                ],
                entrances: vec![None; 4],
                min,
                max,
                objects: Vec::new(),
            });
            return Ok(HullIndex::new(hulls.len() - 1));
        };
        'merging: loop {
            for j in 0..hulls.len() {
                if j == mine.index() {
                    continue;
                }
                let (h, o) = (hulls.checked(mine.index())?, hulls.checked(j)?);
                if !(h.min.x() <= o.max.x()
                    && h.min.y() <= o.max.y()
                    && o.min.x() <= h.max.x()
                    && o.min.y() <= h.max.y())
                {
                    continue;
                }
                let count = o.points.len();
                let overlaps = h.points.iter().any(|p| o.inside(*p))
                    || o.points.iter().any(|p| h.inside(*p))
                    // the other's sides k = 0..n-1 (`cmp edi, esi; jl` at 0x140f8b2c4, esi = n - 1)
                    || (0..count.saturating_sub(1)).any(|k| {
                        let prev = if k == 0 { count - 1 } else { k - 1 };
                        o.points
                            .get(k)
                            .zip(o.points.get(prev))
                            .is_some_and(|(a, b)| h.intersection(AtStart::Counts, *a, *b, hull_path_tuning).is_some())
                    });
                if !overlaps {
                    continue;
                }
                mine = Self::absorb(hulls, mine, j)?;
                continue 'merging;
            }
            return Ok(mine);
        }
    }

    /// Hull `j` merges into hull `mine`: its boxes the other lacks and its points join, and its
    /// slot goes to the last hull. Returns where `mine` now is.
    fn absorb(hulls: &mut Vec<Hull>, mine: HullIndex, j: usize) -> GenieResult<HullIndex> {
        let other = hulls.checked(j)?.clone();
        for s in &other.objects {
            let mine_objects = &mut hulls.checked_mut(mine.index())?.objects;
            if !mine_objects.iter().any(|shape| shape.id == s.id) {
                mine_objects.push(*s);
            }
        }
        hulls.checked_mut(mine.index())?.merge(&other.points)?;
        let last = hulls.len() - 1;
        hulls.swap(j, last);
        let _ = hulls.pop();
        Ok(if mine.index() == last {
            HullIndex::new(j)
        } else {
            mine
        })
    }

    /// `MShortPather::addTileObjects` (`FUN_140f8b460`): the tall objects of `tile`'s list (master `+0x5e0` above 0), in its order, but gates `player` may pass (slot 0x678) and,
    /// unless included, those ignored or moving (slot 0x5c8): each box, grown by the radius, into
    /// `hull` (when given and the object is at most a tile wide) or its own place in the hulls,
    /// once per hull.
    pub(super) fn add_tile_objects(
        &mut self,
        tile: Tile,
        hull: Option<HullIndex>,
        player: Option<PlayerId>,
    ) -> GenieResult<()> {
        let hull_path_tuning = &self.by_id.tuning().hull_path;
        let world = &*self.world;
        for &id in world.tiles().list(tile) {
            let Some(placed) = world.get(id) else {
                continue;
            };
            if !placed.tall || self.passes(id, placed, player)? {
                continue;
            }
            // slot 0x5c8 is asked only of an object not ignored
            let left_out = self.ignore.contains(&RecordId::from(id)) || self.by_id.moving(id);
            if left_out && !self.include.contains(&id) {
                continue;
            }
            Self::add_box(
                &mut self.hulls,
                self.radius,
                id,
                placed,
                hull,
                hull_path_tuning,
            )?;
        }
        Ok(())
    }

    /// Slot 0x678 of object `id` for `player` (the pather's, or none: -1): only a gate's
    /// (`FUN_1411d4a30`) answers yes, one of the gates the caller gave
    /// ([`HullPather::with_gates`]), and never for no player (-1, `processCliffs`'): its ally test
    /// `FUN_141663dc0` takes no number at or above the player count, unsigned.
    pub(super) fn passes(
        &self,
        id: ObjectId,
        placed: &Placed,
        player: Option<PlayerId>,
    ) -> GenieResult<bool> {
        let roles = self.by_id.roles();
        if !roles.of_class(placed.class).contains(ClassRoles::GATE) || player.is_none() {
            return Ok(false);
        }
        let gates = self.gates.ok_or(GenieError::unsupported(
            "a gate in the old short pather's area (its players' passage, slot 0x678)",
        ))?;
        Ok(gates.contains(&id))
    }

    /// One object's box into the hulls ([`HullPather::add_tile_objects`]).
    pub(super) fn add_box(
        hulls: &mut Vec<Hull>,
        distance: Distance,
        id: ObjectId,
        placed: &Placed,
        hull: Option<HullIndex>,
        hull_path_tuning: &HullPathTuning,
    ) -> GenieResult<()> {
        let (x, y) = (placed.position.x(), placed.position.y());
        let (rx, ry) = (placed.radius.x(), placed.radius.y());
        let bounds = Rect::new(
            (x - rx) - distance.get(),
            (y - ry) - distance.get(),
            x + rx + distance.get(),
            y + ry + distance.get(),
        );
        // `COMISS`/`JA` (0x140f8b772, 0x140f8b7a4): an unordered radius takes the given hull
        let h = match hull {
            Some(hull_index)
                if at_most_or_unordered(rx, HALF) && at_most_or_unordered(ry, HALF) =>
            {
                hull_index
            }
            _ => Self::add_shape_to(hulls, bounds.corner0(), bounds.corner1(), hull_path_tuning)?,
        };
        let objects = &mut hulls.checked_mut(h.index())?.objects;
        let id = RecordId::from(id);
        if !objects.iter().any(|shape| shape.id == id) {
            objects.push(Shape { bounds, id });
        }
        Ok(())
    }

    /// [`HullPather::add_shape_to`] on the pather's hulls.
    pub(super) fn add_shape(&mut self, min: Point, max: Point) -> GenieResult<HullIndex> {
        let hull_path_tuning = &self.by_id.tuning().hull_path;
        Self::add_shape_to(&mut self.hulls, min, max, hull_path_tuning)
    }

    /// `MShortPather::processTileObstructions` (`FUN_140f8e1d0`): `tile`'s box when its terrain is not the unit's (unless ignored), into `hull` or its own place, then its objects,
    /// and those of its building's anchor tile when that lies outside the `area`.
    pub(super) fn process_tile(
        &mut self,
        tile: Tile,
        area: TileRect,
        hull: Option<HullIndex>,
    ) -> GenieResult<()> {
        if !self.world.map.holds(tile) {
            return Ok(());
        }
        let (x, y) = (tile.x(), tile.y());
        let word = self.world.tiles().word(tile).unwrap_or(0);
        if word & self.mask.blocked() != 0 {
            let id = tile_record_id(tile);
            if !self.ignore.contains(&id) {
                let distance = self.radius;
                let bounds = Rect::new(
                    x as f32 - distance.get(),
                    y as f32 - distance.get(),
                    (x + 1) as f32 + distance.get(),
                    (y + 1) as f32 + distance.get(),
                );
                let k = match hull {
                    Some(hull_index) => hull_index,
                    None => self.add_shape(bounds.corner0(), bounds.corner1())?,
                };
                let objects = &mut self.hulls.checked_mut(k.index())?.objects;
                if !objects.iter().any(|shape| shape.id == id) {
                    objects.push(Shape { bounds, id });
                }
            }
        }
        self.add_tile_objects(tile, hull, self.player)?;
        let sight = self.world.sight.as_ref().ok_or(GenieError::unsupported(
            "the old short pather without the map's building links",
        ))?;
        if sight.link(tile)?.0 & 0x60 != 0
            && let Some(anchor) = sight.anchor_of(tile)
            && !area.holds(anchor)
        {
            self.add_tile_objects(anchor, hull, self.player)?;
        }
        Ok(())
    }

    /// `MShortPather::buildHulls` (`FUN_140f8b8f0`): the area's tiles row by row: a run of tiles
    /// the unit cannot cross (its terrain, or a static record's mark but a class 5 one) is one box
    /// whose tiles' boxes and small objects go into its hull; each other tile's objects go each to
    /// its own place. Then the cliffs about the area.
    pub(super) fn build_hulls(&mut self) -> GenieResult<()> {
        self.hulls.clear();
        let (w, h) = (self.world.map.width(), self.world.map.height());
        let [x0, y0, x1, y1] = self.area.get();
        self.area = TileRect::new(x0.max(0), y0.max(0), x1.min(w - 1), y1.min(h - 1));
        let distance = self.radius;
        let not_mask = self.mask.blocked();
        for y in self.area.y0()..=self.area.y1() {
            let mut run: Option<i32> = None;
            let mut x = self.area.x0();
            while x <= self.area.x1() {
                let word = self.world.tiles().word(Tile::new(x, y)).unwrap_or(0);
                let open =
                    word & not_mask == 0 && ((word >> 14) & 1 == 0 || word & 0x3f00 == 0x500);
                if open {
                    if let Some(x0) = run.take() {
                        self.add_run(x0, x, y, distance)?;
                    }
                    self.process_tile(Tile::new(x, y), self.area, None)?;
                } else if run.is_none() {
                    run = Some(x);
                }
                x += 1;
            }
            if let Some(x0) = run {
                self.add_run(x0, x, y, distance)?;
            }
        }
        self.process_cliffs()
    }

    /// A run of tiles `x0`..`x1` of row `y` the unit cannot cross: one box, then its tiles.
    pub(super) fn add_run(
        &mut self,
        x0: i32,
        x1: i32,
        y: i32,
        distance: Distance,
    ) -> GenieResult<()> {
        let min = Point::new(x0 as f32 - distance.get(), y as f32 - distance.get());
        let max = Point::new(x1 as f32 + distance.get(), (y + 1) as f32 + distance.get());
        let hull = self.add_shape(min, max)?;
        for index in x0..x1 {
            self.process_tile(Tile::new(index, y), self.area, Some(hull))?;
        }
        Ok(())
    }

    /// `MShortPather::processCliffs` (`FUN_140f8e020`): the objects of the cliff tiles (a static
    /// record of class 6) within two tiles about the area.
    pub(super) fn process_cliffs(&mut self) -> GenieResult<()> {
        let (w, h) = (self.world.map.width(), self.world.map.height());
        let [x0, y0, x1, y1] = self.area.get();
        let (ex0, ey0) = ((x0 - 2).max(0), (y0 - 2).max(0));
        let (ex1, ey1) = ((x1 + 2).min(w - 1), (y1 + 2).min(h - 1));
        for y in ey0..=ey1 {
            for x in (ex0..x0).chain(x1 + 1..=ex1) {
                self.cliff(Tile::new(x, y))?;
            }
        }
        for y in (ey0..y0).chain(y1 + 1..=ey1) {
            for x in x0..=x1 {
                self.cliff(Tile::new(x, y))?;
            }
        }
        Ok(())
    }

    /// `FUN_140f8a890`: a cliff tile's objects (its word's static class 6), each to its own place
    /// ([`HullPather::add_tile_objects`] with no hull and no player, the game's -1: no gate passes).
    pub(super) fn cliff(&mut self, tile: Tile) -> GenieResult<()> {
        let word = self.world.tiles().word(tile).unwrap_or(0);
        if word & 0x3f00 == 0x600 {
            self.add_tile_objects(tile, None, None)?;
        }
        Ok(())
    }

    /// `MShortPather::checkPointInsideHull` (`FUN_140f8be30`): the first hull holding `point`.
    pub(super) fn hull_of(&self, point: Point) -> Option<HullIndex> {
        self.hulls
            .iter()
            .position(|hull| !hull.points.is_empty() && hull.inside(point))
            .map(HullIndex::new)
    }

    /// Whether `point` lies within the area's tiles, `distance` from their edge. Each bound skips
    /// by `COMISS`/`JA` (0x140f86a44, 0x140f8d942, 0x140f899d3): an unordered coordinate is inside.
    pub(super) fn in_area(&self, point: Point) -> bool {
        let distance = self.radius;
        let [x0, y0, x1, y1] = self.area.get();
        at_most_or_unordered(x0 as f32 + distance.get(), point.x())
            && at_most_or_unordered(point.x(), (x1 + 1) as f32 - distance.get())
            && at_most_or_unordered(y0 as f32 + distance.get(), point.y())
            && at_most_or_unordered(point.y(), (y1 + 1) as f32 - distance.get())
    }

    /// Whether `point` lies on the map, `distance` from its edge.
    pub(super) fn on_map(&self, point: Point) -> bool {
        let distance = self.radius;
        let (w, h) = (self.world.map.width(), self.world.map.height());
        !(point.x() < distance.get()
            || w as f32 - distance.get() < point.x()
            || point.y() < distance.get()
            || h as f32 - distance.get() < point.y())
    }
}
