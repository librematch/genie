//! The objects on the map: their tiles and records, their freezing and the known checks, their
//! moves inside and out, and the segment tests.

use crate::GenieError;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::Rect;
use crate::map::TerrainMask;
use crate::map::Tile;
use crate::map::spatial::{
    Grid, Obstruction, Placed, RecordCells, RecordId, SearchCache, Spatial, Tiles, WalkTest,
    tile_record_id,
};
use crate::movement::Speed;
use crate::objects::IdTable;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::ObstructionKind;
use crate::objects::{ObjectId, ObjectKey};
use crate::pathing::tile_path::PlanRect;
use crate::players::ByPlayer;
use crate::players::PlayerId;
use crate::vision::VisionBits;
use crate::world::Seconds;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::mem::take;

impl Spatial {
    /// A map without terrain: every tile's class is 0.
    #[must_use]
    pub fn new(map: GridSize) -> Self {
        let n = (map.width().max(0) * map.height().max(0)) as usize;
        Self::with_tiles(map, Tiles::new(map, vec![0; n]))
    }

    /// A map with the given tiles.
    #[must_use]
    pub fn with_tiles(map: GridSize, tiles: Tiles) -> Self {
        Self {
            map,
            manager_ignore: Vec::new(),
            tile_records: BTreeSet::new(),
            manager_buffer: None,
            objects: IdTable::new(),
            records: RecordCells::default(),
            inside: IdTable::new(),
            tiles,
            changed: Vec::new(),
            frozen_for: IdTable::new(),
            fastest: 0.0,
            reach: Distance::ZERO,
            scout_maps: BTreeMap::new(),
            relations: ByPlayer::default(),
            unit_diplomacy: ByPlayer::default(),
            task_0x99: Vec::new(),
            sight: None,
            vision: ByPlayer::default(),
            terrain: None,
            search_cache: SearchCache::default(),
            grid: Grid::new(map),
        }
    }

    /// What the game keeps per tile.
    #[must_use]
    pub const fn tiles(&self) -> &Tiles {
        &self.tiles
    }

    /// The tiles, for a test's marks.
    #[cfg(test)]
    pub(crate) const fn tiles_mut_for_test(&mut self) -> &mut Tiles {
        &mut self.tiles
    }

    /// An object gets its obstruction record (`FUN_141056460` for a started foundation): a static
    /// one marks its tiles.
    pub fn add_record(&mut self, id: ObjectId, o: Obstruction) {
        let Some(p) = self.objects.get_mut(id) else {
            return;
        };
        p.obstruction = Some(o);
        if o.kind.marks_tiles() {
            self.tiles.mark(p, o);
            self.changed.push(self.tiles.covered_rect(p));
        }
        self.refile(id);
    }

    /// Whether an object on `tile` (its centre there) satisfies `pred`.
    pub fn object_on_tile(&self, tile: Tile, pred: impl Fn(&Placed) -> bool) -> bool {
        self.objects
            .values()
            .any(|o| Tile::new(o.position.x() as i32, o.position.y() as i32) == tile && pred(o))
    }

    /// Object `id`'s place in its tile's object list ([`Spatial::objects_on`]'s order, ghosts
    /// counted); 0 for an object the world does not hold.
    #[must_use]
    pub fn tile_place(&self, id: ObjectId) -> usize {
        self.objects.get(id).map_or(0, |o| {
            let list = self
                .tiles
                .list(Tile::new(o.position.x() as i32, o.position.y() as i32));
            list.iter().position(|&t| t == id).unwrap_or(0)
        })
    }

    /// The map tile's object list (`+0x1b` count, `+0x28` array) of `tile` in its order, the
    /// ghosts' ids among them; none off the map.
    #[must_use]
    pub fn tile_list(&self, tile: Tile) -> &[ObjectId] {
        self.tiles.list(tile)
    }

    /// The objects whose centre is on `tile`: the map tile's object list (`+0x1b` count, `+0x28`
    /// array), in its order, without the ghosts.
    pub fn objects_on(&self, tile: Tile) -> impl Iterator<Item = (ObjectId, &Placed)> + '_ {
        self.tiles
            .list(tile)
            .iter()
            .filter_map(|id| self.objects.get(*id).map(|o| (*id, o)))
    }

    /// The object's obstruction record goes (`FUN_141055b40`'s record part), by its master's
    /// obstruction type (`+0x651`) whether or not it has its record yet: types 0 and 12 have
    /// none (false); types 0, 1, 2, 11, 13 and 10 (as 2) clear their tiles' marks
    /// ([`Spatial::unmark`]), after which type 1 keeps its record filed (true); any other type
    /// drops its record (true), false without one. The mesh shape's removal is the caller's
    /// (`Pathing::removed`).
    pub fn dropped_record(&mut self, id: ObjectId) -> bool {
        let Some(p) = self.objects.get(id).copied() else {
            return false;
        };
        let kind = p.obstruction_kind;
        if !kind.recorded() {
            return false;
        }
        if kind.marks_tiles() {
            self.unmark(&p, id);
            if kind == ObstructionKind::KIND_1 {
                return true;
            }
        }
        match self.objects.get_mut(id) {
            Some(o) if o.obstruction.is_some() => {
                o.obstruction = None;
                self.refile(id);
                true
            }
            _ => false,
        }
    }

    /// [`Spatial::dropped_record`] for a caller that does not read its answer.
    pub fn drop_record(&mut self, id: ObjectId) {
        let _ = self.dropped_record(id);
    }

    /// `FUN_141055980` for object `id`'s static record ([`Tiles::unmark`]): the planners hear of
    /// the covered tiles unless another object's static record on one of them stopped it.
    pub(super) fn unmark(&mut self, p: &Placed, id: ObjectId) {
        let objects = &self.objects;
        let static_record = |o: ObjectId| {
            objects
                .get(o)
                .and_then(|q| q.obstruction)
                .is_some_and(|r| r.kind.marks_tiles())
        };
        if self.tiles.unmark(p, id, static_record) {
            self.changed.push(self.tiles.covered_rect(p));
        }
    }

    /// The object's master's height radius changed: it counts as tall or not from now on (a
    /// dying unit's own master copy has none, `FUN_14121cc40`); the tiles' counts are not
    /// redone, as the game does not redo them.
    pub fn set_tall(&mut self, id: ObjectId, tall: bool) {
        if let Some(o) = self.objects.get_mut(id) {
            o.tall = tall;
        }
    }

    /// The objects whose place lies within `x0`..`x1` and `y0`..`y1`, by id.
    pub fn objects_within(&self, area: Rect) -> impl Iterator<Item = (ObjectId, &Placed)> + '_ {
        let (x0, y0, x1, y1) = (area.x0(), area.y0(), area.x1(), area.y1());
        self.objects.iter().filter(move |(_, o)| {
            let (x, y) = (o.position.x(), o.position.y());
            x0 <= x && x <= x1 && y0 <= y && y <= y1
        })
    }

    /// The tile rectangles whose static marks changed since the last call.
    pub fn take_changes(&mut self) -> Vec<PlanRect> {
        take(&mut self.changed)
    }

    /// Player `player`'s masks both meet the unified map's bits of `tile`: what it sees
    /// (`+0x2a8`) and what it explored (`+0x2a0`). False off the map and without the players'
    /// sight.
    #[must_use]
    pub fn sees_and_explored(&self, player: PlayerId, tile: Tile) -> bool {
        let Some(v) = self.sight.as_ref() else {
            return false;
        };
        let Some(&unified) = v.size.index(tile).and_then(|i| v.unified.get(i)) else {
            return false;
        };
        let word = unified & VisionBits::OBJECT_MASK;
        let masks = self.vision(player);
        word.intersects(masks.visible) && word.intersects(masks.explored)
    }

    /// The object `id`'s place, if the world holds it.
    #[must_use]
    pub fn get(&self, id: impl ObjectKey) -> Option<&Placed> {
        let id = id.key()?;
        self.objects.get(id).or_else(|| self.inside.get(id))
    }

    /// An object whose place the slice knows: an object of a player the slice stopped is known
    /// while it cannot have moved.
    pub fn get_known(&self, id: impl ObjectKey) -> Result<Option<&Placed>, GenieError> {
        match self.get(id) {
            Some(p) if p.unknown.is_some_and(|u| 0.0 < u) => Err(GenieError::unsupported(
                "an object of a stopped player (its place is unknown)",
            )
            .with("object", id.key().map_or(-1, ObjectId::get))
            .with("player", p.player)),
            found => Ok(found),
        }
    }

    /// The slice stops updating the object: from now on its place is known only within its speed
    /// times the time since.
    pub fn freeze(&mut self, id: ObjectId) {
        if let Some(p) = self.objects.get_mut(id) {
            let _ = p.unknown.get_or_insert(0.0);
            if !self.frozen_for.contains_key(id) {
                let _ = self.frozen_for.insert(id, Seconds::ZERO);
            }
        }
    }

    /// The object's place is the game's again (the oracle's forced replay): it is known.
    pub fn unfreeze(&mut self, id: ObjectId) {
        if let Some(p) = self.objects.get_mut(id) {
            p.unknown = None;
        }
        let _ = self.frozen_for.remove(id);
    }

    /// A tick of `dt` seconds passed: every frozen object that moves may have moved its master's
    /// speed (`speed` of its player and master) times dt.
    pub fn grow_unknown(&mut self, dt: Seconds, speed: impl Fn(PlayerId, MasterId) -> Speed) {
        for p in self.objects.values_mut() {
            if let Some(u) = p.unknown.as_mut()
                && p.moves
            {
                *u += (speed(p.player, p.master) * dt).get();
            }
        }
        for t in self.frozen_for.values_mut() {
            *t += dt;
        }
    }

    /// Fails when a frozen object may be within `rect` (`x0, y0, x1, y1`) or may have changed
    /// there: the answer of a query over it depends on where that object really is. A frozen
    /// object that does not move stays where it was; it can only have changed when a stopped
    /// player's unit (Gaia's animals do not work on objects) may have reached it: a frozen unit
    /// within what it may have moved, or a unit a stopped player's building may have made since,
    /// at the fastest speed of the players' units, each plus the farthest reach.
    pub fn check_known(&self, rect: Rect) -> Result<(), GenieError> {
        self.check_known_of(rect, false, None)
    }

    /// [`Spatial::check_known`] for a query over obstruction records and the mesh (the motion
    /// step's and the short pather's): a frozen object without a record (a bird) has no say.
    pub fn check_known_records(&self, rect: Rect) -> Result<(), GenieError> {
        self.check_known_of(rect, true, None)
    }

    /// [`Spatial::check_known`] for a query over the registry's lists of `classes`: a frozen
    /// object of another class has no say.
    pub fn check_known_classes(&self, rect: Rect, classes: &[ClassId]) -> Result<(), GenieError> {
        self.check_known_of(rect, false, Some(classes))
    }

    pub(super) fn check_known_of(
        &self,
        rect: Rect,
        records: bool,
        classes: Option<&[ClassId]>,
    ) -> Result<(), GenieError> {
        // only a frozen object has an unknown place, and each is in `frozen_for`
        let near = self.frozen().find(|&(id, p)| {
            if records && p.obstruction.is_none() {
                return false;
            }
            if classes.is_some_and(|c| !c.contains(&p.class)) {
                return false;
            }
            p.unknown.is_some_and(|u| {
                let bounds = p.bounds();
                rect.overlaps(bounds.grown(Distance::new(u)))
                    && (0.0 < u || self.touchable(id, bounds))
            })
        });
        if let Some((id, p)) = near {
            return Err(
                GenieError::unsupported("an object of a stopped player may be near")
                    .with("object", id)
                    .with("player", p.player)
                    .with(
                        "query",
                        format_args!(
                            "({}, {})..({}, {})",
                            rect.x0(),
                            rect.y0(),
                            rect.x1(),
                            rect.y1()
                        ),
                    ),
            );
        }
        Ok(())
    }

    /// Whether a stopped player's unit may have reached the frozen object `id` within `rect`.
    pub(super) fn touchable(&self, id: ObjectId, rect: Rect) -> bool {
        self.frozen().any(|(other, q)| {
            let Some(u) = q.unknown.filter(|_| other != id) else {
                return false;
            };
            if q.player == PlayerId::GAIA {
                // Gaia's units (animals) do not work on objects
                return false;
            }
            let moved = if q.moves {
                u
            } else {
                self.fastest * self.frozen_for.get(other).copied().unwrap_or(Seconds::ZERO)
            };
            let grow = moved + self.reach.get();
            rect.overlaps(q.bounds().grown(Distance::new(grow)))
        })
    }

    /// The frozen objects (those with an unknown place), in id order.
    pub(super) fn frozen(&self) -> impl Iterator<Item = (ObjectId, &Placed)> + '_ {
        self.frozen_for
            .keys()
            .filter_map(|id| self.objects.get(id).map(|p| (id, p)))
            .filter(|(_, p)| p.unknown.is_some())
    }

    /// A new object, in the order the game adds them: a static record marks only the tiles no
    /// earlier record marked.
    pub fn insert(&mut self, id: ObjectId, placed: Placed) {
        self.remove(id);
        if placed.unknown.is_some() && !self.frozen_for.contains_key(id) {
            let _ = self.frozen_for.insert(id, Seconds::ZERO);
        }
        if let Some(o) = placed.obstruction.filter(|o| o.kind.marks_tiles()) {
            self.tiles.mark(&placed, o);
            self.changed.push(self.tiles.covered_rect(&placed));
        }
        let _ = self.objects.insert(id, placed);
        self.refile(id);
        if let Some(i) = self.tiles.tile_of(placed.position) {
            let objects = &self.objects;
            self.tiles.append(i, id, |o| Self::is_tall(objects, o));
        }
    }

    /// Whether listed object `id` counts as tall (its master's height radius above 0, which
    /// `FUN_140f5f0a0` and `FUN_140f5f560` read): a ghost, whose master the slice does not hold,
    /// counts as not tall.
    pub(super) fn is_tall(objects: &IdTable<Placed>, id: ObjectId) -> bool {
        objects.get(id).is_some_and(|p| p.tall)
    }

    /// The object's master changed (`FUN_1411a6a70`): it takes the new master's radii, class,
    /// obstruction and speed (`placed`, at its place), its static record's marks follow, and it
    /// keeps its slot in its tile's list and the tile's tall count, which the game does not redo.
    pub fn change_master(&mut self, id: ObjectId, placed: Placed) {
        let Some(old) = self.objects.get(id).copied() else {
            return self.insert(id, placed);
        };
        if self.tiles.tile_of(old.position) != self.tiles.tile_of(placed.position) {
            return self.insert(id, placed);
        }
        if old.obstruction.is_some_and(|o| o.kind.marks_tiles()) {
            self.unmark(&old, id);
        }
        if let Some(o) = placed.obstruction.filter(|o| o.kind.marks_tiles()) {
            self.tiles.mark(&placed, o);
            self.changed.push(self.tiles.covered_rect(&placed));
        }
        let _ = self.objects.insert(id, placed);
        self.refile(id);
    }

    /// The object leaves the world: its place, its sight's state and its records in the registry
    /// (its destructor, `FUN_141073060`) go.
    pub fn leave(&mut self, id: ObjectId) -> Result<(), GenieError> {
        if let (Some(v), Some(p)) = (self.sight.as_mut(), self.objects.get(id)) {
            v.registry_leave(id, p.player, p.position, p.radius, p.class)?;
            v.remove_seer(id);
        }
        self.remove(id);
        Ok(())
    }

    /// The object goes inside another at `at` (`FUN_1411921d0`): it leaves its tile's list
    /// (`FUN_140f5f560`) and the obstruction manager (`FUN_141055b40`), and takes its container's
    /// x and y (`FUN_14118c890`).
    pub fn go_inside(&mut self, id: ObjectId, at: Position) {
        let Some(mut p) = self.objects.get(id).copied() else {
            return;
        };
        self.remove(id);
        p.position = at.with_z(p.position.z());
        p.obstruction = None;
        let _ = self.inside.insert(id, p);
    }

    /// The record of object `id` inside another ([`Spatial::go_inside`]) becomes `placed` (a master
    /// change inside, which touches no tile, list nor record); nothing for an object not inside.
    pub fn set_inside_record(&mut self, id: ObjectId, placed: Placed) {
        if let Some(p) = self.inside.get_mut(id) {
            *p = placed;
        }
    }

    /// The record of object `id` inside another takes the place `at` (its object's copy of the
    /// container's place); nothing for an object not inside.
    pub fn set_inside_position(&mut self, id: ObjectId, at: Position) {
        if let Some(p) = self.inside.get_mut(id) {
            p.position = at;
        }
    }

    /// The object `id` comes out of the object it was inside ([`Spatial::go_inside`]): its record,
    /// which the caller puts on the map again.
    pub fn come_out(&mut self, id: ObjectId) -> Option<Placed> {
        self.inside.remove(id)
    }

    /// The object `id` leaves the world.
    pub fn remove(&mut self, id: ObjectId) {
        let _ = self.inside.remove(id);
        if let Some(gone) = self.objects.remove(id) {
            self.refile(id);
            if let Some(i) = self.tiles.tile_of(gone.position) {
                let objects = &self.objects;
                self.tiles.take_off(i, id, |o| Self::is_tall(objects, o));
            }
            if gone.obstruction.is_some_and(|o| o.kind.marks_tiles()) {
                self.unmark(&gone, id);
            }
        }
    }

    /// Whether the straight walk from `a` to `b` (x, y) of a unit `grow` wide meets an obstruction,
    /// as the routines that test one walk ask (`FUN_141052b90`, `FUN_141052350` with flags `0x100`,
    /// then `FUN_1410561f0`): the temporary records of the tiles its terrain mask lacks
    /// ([`Spatial::put_tile_records`]), tested ([`Spatial::segment_meets`]) and removed again.
    pub fn segment_blocked(
        &mut self,
        a: Point,
        b: Point,
        grow: Distance,
        mask: TerrainMask,
        ignore: &[RecordId],
    ) -> bool {
        self.put_tile_records(a, b, grow, mask);
        let blocked = self.segment_meets(a, b, grow, ignore);
        self.clear_tile_records();
        blocked
    }

    /// `FUN_141052350` with flags `0x100`: whether the straight walk from `a` to `b` (x, y) of a
    /// unit `grow` wide (the manager's buffer) meets a record. A walk whose grown box leaves the
    /// map does. Else the records of the cells the query visits ([`Spatial::visits`], the walk's
    /// box grown by `grow`) whose id is not in `ignore` count when their box, grown by `grow`,
    /// meets the walk's box and segment (`FUN_141052e10`): the temporary tile records
    /// ([`Spatial::put_tile_records`], kind 3) and the objects' static records (kinds 2, 11 and
    /// 13; flag `0x100` passes the kinds of the mask `0x2804` and the records of no object).
    #[must_use]
    pub fn segment_meets(&self, a: Point, b: Point, grow: Distance, ignore: &[RecordId]) -> bool {
        let Some(test) = WalkTest::new(a, b, grow, self.map) else {
            return true;
        };
        self.tile_records_meet(&test, ignore)
            || self.records_in(test.query).any(|(id, p)| {
                // kind 10 is filed as 2
                let kind = p.obstruction.map(|o| o.kind.filed());
                matches!(
                    kind,
                    Some(
                        ObstructionKind::KIND_2
                            | ObstructionKind::KIND_11
                            | ObstructionKind::KIND_13
                    )
                ) && !ignore.contains(&RecordId::from(id))
                    && test.meets(p.bounds())
                    && self.visits(p, kind.unwrap_or(ObstructionKind::NONE), test.query)
            })
    }

    /// Whether a temporary tile record ([`Spatial::put_tile_records`]) of a cell the walk's
    /// query visits, not in `ignore`, meets the walk ([`WalkTest::meets`]). The records are filed
    /// on the level of the table's first entry (`+0x1bc`), in the cell of their tile.
    pub(super) fn tile_records_meet(&self, test: &WalkTest, ignore: &[RecordId]) -> bool {
        let level = self.grid.level(0);
        let shift = self.grid.shift(level);
        self.tile_records.iter().any(|&(y, x)| {
            self.visits_cell(level, x >> shift, y >> shift, test.query)
                && !ignore.contains(&tile_record_id(Tile::new(x, y)))
                && test.meets(Rect::new(
                    x as f32,
                    y as f32,
                    x as f32 + 1.0,
                    y as f32 + 1.0,
                ))
        })
    }

    /// Whether a tile its terrain `mask` lacks lies in the way of a straight walk from `a` to
    /// `b` of a unit `grow` wide, as the walk's query asks (`FUN_14114cc60`: `FUN_141052b90`,
    /// its query, then `FUN_1410561f0`): its temporary tile records
    /// ([`Spatial::put_tile_records`]) tested as [`Spatial::segment_meets`] tests them, and
    /// removed again. A walk whose grown box leaves the map counts as blocked.
    pub fn tiles_block(&mut self, a: Point, b: Point, grow: Distance, mask: TerrainMask) -> bool {
        self.put_tile_records(a, b, grow, mask);
        let blocked =
            WalkTest::new(a, b, grow, self.map).is_none_or(|t| self.tile_records_meet(&t, &[]));
        self.clear_tile_records();
        blocked
    }

    /// `FUN_141052b90`: a temporary record (`FUN_1410510c0`) on each tile of the rectangle of
    /// `a` and `b` (x, y), grown by `grow` (the manager's buffer) and held on the map, whose word
    /// holds neither a terrain class of `mask` nor a temporary record (`mask & 0x7f | 0x80`), row
    /// by row. The records stay until [`Spatial::clear_tile_records`].
    pub fn put_tile_records(&mut self, a: Point, b: Point, grow: Distance, mask: TerrainMask) {
        use crate::math::conversions::cvttss2si;
        let (lo_x, hi_x) = (
            if b.x() <= a.x() { b.x() } else { a.x() },
            if a.x() <= b.x() { b.x() } else { a.x() },
        );
        let (lo_y, hi_y) = (
            if b.y() <= a.y() { b.y() } else { a.y() },
            if a.y() <= b.y() { b.y() } else { a.y() },
        );
        let (w, h) = (self.map.width(), self.map.height());
        let (lo_x, lo_y, hi_x, hi_y) = (
            cvttss2si(lo_x - grow.get()),
            cvttss2si(lo_y - grow.get()),
            cvttss2si(hi_x + grow.get()),
            cvttss2si(hi_y + grow.get()),
        );
        let (x0, y0) = (lo_x.max(0), lo_y.max(0));
        let x1 = if hi_x < w { hi_x } else { w - 1 };
        let y1 = if hi_y < h { hi_y } else { h - 1 };
        let impassable = mask.classes();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let Some(word) = self.tiles.word(Tile::new(x, y)) else {
                    continue;
                };
                if word & (impassable | 0x80) == 0 {
                    let _ = self.tile_records.insert((y, x));
                }
            }
        }
    }

    /// `FUN_1410561f0`: every temporary tile record ([`Spatial::put_tile_records`]) goes.
    pub fn clear_tile_records(&mut self) {
        self.tile_records.clear();
    }

    /// The temporary tile records ([`Spatial::put_tile_records`]) as (x, y), row by row.
    #[must_use]
    pub fn tile_records(&self) -> Vec<Tile> {
        self.tile_records
            .iter()
            .map(|&(y, x)| Tile::new(x, y))
            .collect()
    }
}
