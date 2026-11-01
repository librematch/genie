//! A footprint's spots, occupants and clearing, foreign marks, clear points and obstruction
//! candidates.

use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::map::Rect;
use crate::map::{Tile, TileRect};
use crate::math::fmath::BVector;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::world::slots::CheckedIndex;
use std::collections::BTreeMap;

use crate::GenieError;
use crate::map::spatial::{Placed, QUERY_HITS, RecordId, Spatial, tile_record_id};

impl Spatial {
    /// `FUN_1411566b0`: the tile beside a footprint at `at` of radii `radius` a unit at `from`
    /// with terrain mask `mask` goes to: a flood over passable tiles from the footprint's own
    /// (`(int)(x - rx)` to `(int)(x + rx - 0.001)`, columns first), at most `(int)(rx + ry) + 1`
    /// tiles from its centre; of the tiles found outside the footprint, the first empty one, else
    /// by the 3D distance of its centre (at the unit's height) from the unit the closest, or with
    /// `farthest` (`param_7` 1) the farthest, later ones replacing it in the flood's order once
    /// they beat it (an empty tile found, only empty ones). Returns its centre.
    ///
    /// # Errors
    ///
    /// A tile whose objects the slice does not know.
    pub fn spot_off(
        &mut self,
        from: Position,
        (me, radius_x): (ObjectId, f32),
        mask: crate::map::TerrainMask,
        (at, radius): (Position, Footprint),
        farthest: bool,
    ) -> Result<Option<Point>, GenieError> {
        /// The footprint's last tile stops short of its edge by this (`DAT_142eb1e9c`).
        const EDGE: f32 = crate::math::image::spatial::FOOTPRINT_EDGE;
        /// A tile's centre (`DAT_142e7f384`).
        const CENTRE: f32 = crate::math::image::spatial::TILE_CENTRE;
        /// The closest search's first best (`DAT_14330ebf4`).
        const NO_SPOT: f32 = crate::math::image::spatial::NO_SPOT;
        let (tx, ty) = (at.x(), at.y());
        let (rx, ry) = (radius.x(), radius.y());
        let size = (((ry + rx) as i32) + 2).max(1) * 2;
        let half = size / 2;
        let (cx, cy) = (tx as i32, ty as i32);
        let (fx, fy) = ((cx - half) as f32, (cy - half) as f32);
        self.check_known(Rect::new(fx, fy, fx + size as f32, fy + size as f32))?;
        let (ox, oy) = (half - cx, half - cy);
        let (x0, y0) = ((tx - rx) as i32, (ty - ry) as i32);
        let (x1, y1) = (((rx + tx) - EDGE) as i32, ((ry + ty) - EDGE) as i32);
        let footprint = TileRect::new(x0, y0, x1, y1);
        let mut list: Vec<Tile> = (x0..=x1)
            .flat_map(|x| (y0..=y1).map(move |y| Tile::new(x, y)))
            .collect();
        let mut visited = vec![false; (size * size) as usize];
        let reach = half - 1;
        let mut i = 0;
        while i < list.len() {
            let t = *list.checked(i)?;
            if (cx - t.x()).abs() < reach && (cy - t.y()).abs() < reach {
                for n in t.beside() {
                    let at = ((ox + n.x()) * size + n.y() + oy) as usize;
                    if !*visited.checked(at)?
                        && self.passable(n, mask, me, Distance::new(radius_x))?
                    {
                        *visited.checked_mut(at)? = true;
                        list.push(n);
                    }
                }
            }
            i += 1;
        }
        let mut best = if farthest { 0.0 } else { NO_SPOT };
        let mut spot = None;
        let mut empty = false;
        for &t in &list {
            if footprint.holds(t) {
                continue;
            }
            let (x, y) = (t.x(), t.y());
            let was_empty = empty;
            let count = self.tiles.objects(Tile::new(x, y));
            if empty && count.is_some_and(|n| n != 0) {
                continue;
            }
            if count == Some(0) {
                empty = true;
            }
            // the tile's centre as the game's vector (x, height, y), at the unit's height
            let centre = BVector::new(x as f32 + CENTRE, from.z(), y as f32 + CENTRE);
            let d = BVector::new(
                from.x() - centre.x(),
                from.z() - centre.height(),
                from.y() - centre.y(),
            );
            let distance = d.x() * d.x() + d.height() * d.height() + d.y() * d.y();
            let beats = if farthest {
                best < distance
            } else {
                distance < best
            };
            if (empty && !was_empty) || beats {
                spot = Some(centre.ground());
                best = distance;
            }
        }
        Ok(spot)
    }

    /// The objects whose centre is on `site`'s footprint, edges included, and that the game counts
    /// as occupying it (`FUN_14123bb20`): objects other than the site below state 3, or of a
    /// type other than a building's below state 7 (their state and type `looks`'); for a site
    /// whose master does not obstruct (`FUN_1411205c0`: obstruction type 0 or 12, `obstructs`
    /// false) only those that do not move ([`Placed::moves`], the master's slot 0xe8); a desolid building foundation
    /// (`+0xc60`) only of a player that and the site's count each other allied
    /// (`FUN_141663d90` with the ally flag, both ways); not those the site may be built over
    /// (`can_be_built_on` 1); with all three radii. An object `looks` does not hold (the owner's
    /// sleeping objects, which the unit's view does not hold: of types below 30, alive, in
    /// state 2 or 6) passes the state and foundation tests.
    pub fn footprint_occupants(
        &self,
        site_id: crate::construction::FoundationId,
        site: &Placed,
        obstructs: bool,
        looks: &BTreeMap<ObjectId, crate::construction::build::ClearLook>,
    ) -> Result<Vec<ObjectId>, GenieError> {
        use crate::objects::masters::master_type::BUILDING;
        let (sx, sy) = (site.position.x(), site.position.y());
        let (rx, ry) = (site.radius.x(), site.radius.y());
        self.check_known(Rect::new(sx - rx, sy - ry, sx + rx, sy + ry))?;
        let counts = |id: ObjectId, o: &Placed| {
            looks.get(&id).is_none_or(|l| {
                let alive = l.state < ObjectState::Dying
                    || (l.kind != BUILDING && l.state < ObjectState::Removed);
                let foundation = l.kind == BUILDING && l.desolid;
                alive
                    && (!foundation
                        || (self.allied(site.player, o.player)
                            && self.allied(o.player, site.player)))
            })
        };
        Ok(self
            .objects
            .iter()
            .filter(|&(id, o)| {
                id != site_id.id()
                    && (obstructs || !o.moves)
                    && !o.buildable_on.passed()
                    && o.tall
                    && o.radius.sized()
                    && (o.position.x() - sx).abs() <= rx
                    && (o.position.y() - sy).abs() <= ry
                    && counts(id, o)
            })
            .map(|(id, _)| id)
            .collect())
    }

    /// Whether a static record of another player marks a tile of `site`'s footprint
    /// (`FUN_1411d4b80`).
    /// The edges cast by `CVTTSS2SI` (a NaN or huge edge gives `i32::MIN`: the loops then do
    /// not run), the tile's players the low 32 bits of `word >> 23` against `1 << (player &
    /// 0x1f)` in 32 bits.
    #[must_use]
    pub fn foreign_marks(&self, site: &Placed) -> bool {
        use crate::math::conversions::cvttss2si;
        let (sx, sy) = (site.position.x(), site.position.y());
        let (rx, ry) = (site.radius.x(), site.radius.y());
        let (w, h) = (self.tiles.size.width(), self.tiles.size.height());
        let (x0, y0) = (cvttss2si(sx - rx).max(0), cvttss2si(sy - ry).max(0));
        let x1 = cvttss2si((sx + rx) - 0.001).min(w - 1);
        let y1 = cvttss2si((sy + ry) - 0.001).min(h - 1);
        let own = 1u32 << (site.player.index() as u32 & 0x1f);
        (y0..=y1).any(|y| {
            (x0..=x1).any(|x| {
                let players = (self.tiles.word(Tile::new(x, y)).unwrap_or(0) >> 23) as u32;
                players != 0 && players != own
            })
        })
    }

    /// Slot 0x278 of unit `me` (`FUN_141158720`): whether it could stand at `point`. The point's
    /// tile is on the obstruction manager's map and its terrain class in the unit's `mask` (the
    /// tile's word, `DAT_143f0a088 + 0x98`; a static record's own bits do not count here), and no
    /// obstruction record but the unit's own holds the point within its box grown by `grow` (the
    /// unit's radius): `FUN_141051900` with flags 0x20 in the cells it visits
    /// ([`Spatial::visits`]), whose point test `FUN_141052160` takes records of every kind, edges
    /// included. A grown point beyond the map's edge meets the edge's
    /// records ([`Spatial::edge_records`]) and is never clear.
    pub fn point_clear(
        &mut self,
        me: ObjectId,
        point: Point,
        grow: Distance,
        mask: crate::map::TerrainMask,
    ) -> Result<bool, GenieError> {
        let (x, y) = (point.x(), point.y());
        let (tx, ty) = (x as i32, y as i32);
        let Some(word) = self.tiles.word(Tile::new(tx, ty)) else {
            return Ok(false);
        };
        if word & mask.blocked() != 0 {
            return Ok(false);
        }
        // the unit alone the manager's ignore list (FUN_1410562e0 with DAT_143f49fc8); the
        // buffer it set for the query it resets after (FUN_1410562d0 with 0)
        let own = [RecordId::from(me)];
        self.set_manager_ignore(&own);
        self.set_manager_buffer(None);
        Ok(!self.point_hit(point, grow, &own, false)?)
    }

    /// Slot 0x270 of unit `me` (`FUN_141158940`): whether it could stand at `point` among the
    /// objects of `ignore` (`me` itself for an empty list): the point's tile is on the
    /// obstruction manager's map and its terrain class in the unit's `mask` (its restriction's,
    /// `FUN_141054630`), and no record but the ignored ones holds the point within its box grown
    /// by `grow` (the unit's radius times the caller's factor), the query of
    /// [`Spatial::point_hit`] with 25 hits.
    ///
    /// # Errors
    ///
    /// As [`Spatial::point_hit`].
    pub fn point_clear_among(
        &mut self,
        me: ObjectId,
        point: Point,
        grow: Distance,
        mask: crate::map::TerrainMask,
        ignore: &[ObjectId],
    ) -> Result<bool, GenieError> {
        // CVTTSS2SI: a NaN or huge coordinate is i32::MIN, off the map
        let Some(word) = self.tiles.word(point.tile()) else {
            return Ok(false);
        };
        if word & mask.blocked() != 0 {
            return Ok(false);
        }
        // the objects as the record query ignores them, the unit alone for none
        let ignore: Vec<RecordId> = if ignore.is_empty() {
            vec![RecordId::from(me)]
        } else {
            ignore.iter().map(|&id| RecordId::from(id)).collect()
        };
        self.set_manager_ignore(&ignore);
        // the buffer it set for the query it resets after (FUN_1410562d0 with 0)
        self.set_manager_buffer(None);
        Ok(!self.point_hit(point, grow, &ignore, false)?)
    }

    /// The obstruction manager's point query (`FUN_141051900` with flags 0x20, any player, the
    /// point test `FUN_141052160`) at `point` with the manager's buffer `grow` (`+0x57`, `+0x58`)
    /// and ignore list `ignore` (`+0x2c`, `FUN_1410562e0`): whether a record holds the point
    /// within its box grown by `grow`, edges included, of the records of every kind in the cells
    /// the query visits ([`Spatial::visits`]), the temporary tile records still put among them
    /// ([`Spatial::put_tile_records`]); with `still` (flag 0x40) an object that moves (the
    /// record's `+3` bit 0x10, [`Gait::moving`]) is left out. A grown point beyond the map's edge
    /// meets the edge's records ([`Spatial::edge_records`]).
    ///
    /// # Errors
    ///
    /// A record whose place the slice does not know near the point.
    ///
    /// [`Gait::moving`]: crate::map::spatial::Gait::moving
    pub fn point_hit(
        &self,
        point: Point,
        grow: Distance,
        ignore: &[RecordId],
        still: bool,
    ) -> Result<bool, GenieError> {
        let rect = Rect::between(point, point).grown(grow);
        if rect.leaves_map(self.map) && !self.edge_records(rect, QUERY_HITS).is_empty() {
            return Ok(true);
        }
        let level = self.grid.level(0);
        let shift = self.grid.shift(level);
        let tile = self.tile_records.iter().any(|&(ty, tx)| {
            let bx = Rect::new(tx as f32, ty as f32, tx as f32 + 1.0, ty as f32 + 1.0);
            self.visits_cell(level, tx >> shift, ty >> shift, rect)
                && !ignore.contains(&tile_record_id(Tile::new(tx, ty)))
                && bx.grown(grow).holds(point)
        });
        if tile {
            return Ok(true);
        }
        self.object_holds(point, grow, ignore, still)
    }

    /// The objects' records of [`Spatial::point_hit`]'s query (`FUN_141051900` with flags 0x20,
    /// the point test `FUN_141052160`): whether a record with an object (`+8`), not one of
    /// `ignore`, holds `point` within its box grown by `grow`, edges included (with `still` a
    /// moving object's is left out); the edge's and the tiles' records have no object.
    ///
    /// # Errors
    ///
    /// A record whose place the slice does not know near the point.
    pub fn object_holds(
        &self,
        point: Point,
        grow: Distance,
        ignore: &[RecordId],
        still: bool,
    ) -> Result<bool, GenieError> {
        let rect = Rect::between(point, point).grown(grow);
        self.check_known_records(rect)?;
        Ok(self.records_in(rect).any(|(id, p)| {
            let Some(o) = p
                .obstruction
                .filter(|_| !ignore.contains(&RecordId::from(id)))
            else {
                return false;
            };
            if still && self.gaits.get(id).is_some_and(|g| g.moving) {
                return false;
            }
            if !self.visits(p, o.kind, rect) {
                return false;
            }
            p.bounds().grown(grow).holds(point)
        }))
    }

    /// Whether `FUN_141052b90` over the tile under `point` alone puts a temporary record on it
    /// (`FUN_1410510c0`): the tile is on the map and its word holds neither a terrain class of
    /// `mask` nor a temporary record's bit 0x80 (`mask & 0x7f | 0x80`).
    #[must_use]
    pub fn makes_tile_record(&self, point: Point, mask: crate::map::TerrainMask) -> bool {
        let (x, y) = (point.x() as i32, point.y() as i32);
        self.tiles
            .word(Tile::new(x, y))
            .is_some_and(|w| w & (mask.classes() | 0x80) == 0)
            && !self.tile_records.contains(&(y, x))
    }

    /// The records a move query could report (`FUN_14114cc60` -> `FUN_141051900`): those of a
    /// tested kind in the cells the query visits ([`Spatial::visits`]) whose box, grown by
    /// `grow`, meets `rect` (`x0, y0, x1, y1`, edges included), except the `ignore`d objects'. The game then tests each candidate against the move's
    /// segment and shrinks moving allies' boxes (master `+0x780`, at most 1); every hit is a
    /// candidate, so no candidate means no hit.
    #[must_use]
    pub fn obstruction_candidates(
        &self,
        rect: Rect,
        grow: Distance,
        ignore: &[RecordId],
    ) -> Vec<ObjectId> {
        let query = rect.grown(grow);
        self.records_in(query)
            .filter(|(id, p)| {
                p.obstruction
                    .is_some_and(|o| o.kind.tested() && self.visits(p, o.kind, query))
                    && !ignore.contains(&RecordId::from(*id))
            })
            .filter(|(_, p)| rect.overlaps(p.bounds().grown(grow)))
            .map(|(id, _)| id)
            .collect()
    }
}
