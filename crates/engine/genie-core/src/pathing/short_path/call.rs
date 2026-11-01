//! The short pather's call ([`Call`], [`Query`]) and its state between searches
//! ([`Pather`]): the terrain's shapes, the start test and the core.

use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::math::fmath::BVector;
use crate::objects::ObjectId;
use crate::pathing::FixedRect;
use crate::pathing::ShapeId;
use crate::pathing::mesh::{Mesh, ONE};
use crate::pathing::short_path::{
    Ctx, FixedPoint, Goal, NONE, Search, TO_FIXED, TO_FLOAT, clip, tile_of,
};
use crate::world::slots::CheckedIndex;
use crate::{GenieError, math::fmath};

/// The short pather's arguments (`FUN_140f9d3a0`'s block, which `FUN_140f9e540` and
/// `FUN_140fa0110` fill): the start and the goal in fixed point (the callers truncate them), the
/// unit's radius, the target's half sizes (when the goal is the target's own position) and the
/// range as floats, and the terrain classes the unit cannot cross (`mask ^ 0xff`). The search
/// ([`Pather::search`]) and the start test ([`Pather::start_blocked`]) truncate the floats.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Query {
    /// The start.
    pub from: FixedPoint,
    /// The goal.
    pub to: FixedPoint,
    /// The unit's radius (`+0xc`).
    pub radius: Distance,
    /// The target's half sizes, when the goal is its own position (`+0x10`, `+0x14`).
    pub target_size: Footprint,
    /// The range (`+0x18`).
    pub range: Distance,
    /// The terrain classes the unit cannot cross, as a tile mask (`+0x1c`).
    pub blocked: u64,
}

impl Query {
    /// `FUN_140f9d300`'s and `FUN_140f9d3e0`'s casts: each float times 2^22 by `CVTTSS2SI` into
    /// a 64-bit register, its low 32 bits kept (the radius through `uint`, the same bits).
    pub(super) fn fixed(&self) -> Fixed {
        let fixed =
            |f: Distance| crate::math::conversions::cvttss2si_64((f * TO_FIXED).get()) as i32;
        Fixed {
            from: self.from,
            to: self.to,
            radius: fixed(self.radius),
            target_size: FixedPoint::new(
                fixed(Distance::new(self.target_size.x())),
                fixed(Distance::new(self.target_size.y())),
            ),
            range: fixed(self.range),
            blocked: self.blocked,
        }
    }
}

/// A [`Query`] in fixed point, as `FUN_140fa53d0` and `FUN_140faa170` take it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Fixed {
    pub(super) from: FixedPoint,
    pub(super) to: FixedPoint,
    pub(super) radius: i32,
    /// The target's half sizes, x and y.
    pub(super) target_size: FixedPoint,
    pub(super) range: i32,
    pub(super) blocked: u64,
}

/// What a search returns (`FUN_140fa53d0`'s result) with the path's points from the start on, in
/// fixed point (the game appends them as floats, `{x, 0, y}`, through the path's own dedup).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortPath {
    /// 0 already in range, 1 reached, 2 a partial path, 3 the start blocked, 4 invalid.
    pub result: u8,
    /// The path's points.
    pub points: Vec<FixedPoint>,
}

/// The target of a short path (`FUN_140f9e540`'s argument 5): its position and its master's half
/// sizes (`+0x5d8`, `+0x5dc`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    /// The target.
    pub id: ObjectId,
    /// Its place.
    pub position: Point,
    /// Its master's half sizes (`+0x5d8`, `+0x5dc`).
    pub half_size: Footprint,
}

/// An object the call is asked to ignore (arguments 6 and 7): its id and its shape in the mesh
/// (`+0x64c`), if it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Listed {
    /// The object.
    pub id: ObjectId,
    /// Its shape in the mesh (`+0x64c`), if it has one.
    pub shape: Option<ShapeId>,
}

/// A call of the short pather in the game's own terms (`FUN_140f9e540`, the short branch): the
/// start and the goal on the map (x, y), the unit's radius and range, the target, the objects to
/// ignore, the map's size and the unit's terrain mask.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    /// The start.
    pub from: Point,
    /// The goal.
    pub to: Point,
    /// The unit's radius.
    pub radius: Distance,
    /// Its range.
    pub range: Distance,
    /// The target, when the goal is an object.
    pub target: Option<Target>,
    /// The objects it ignores.
    pub listed: Vec<Listed>,
    /// The map's size in tiles.
    pub map: GridSize,
    /// The terrain classes the unit can cross (its restriction's mask).
    pub mask: crate::map::TerrainMask,
}

/// What the entry makes of a call.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// 0: the start lies within range of the target (`FUN_14118faf0` less the radius): the path is
    /// cleared.
    InRange,
    /// 4: the start or the goal lies off the map.
    Invalid,
    /// A search with the shapes it ignores.
    Search {
        /// The query.
        query: Query,
        /// The shapes it ignores.
        ignore: Vec<ShapeId>,
    },
}

impl Call {
    /// `FUN_140f9e540` up to the search: the target's range test, the map test, fixed point by
    /// truncation (`(int)(longlong)(f * 2^22)`); the listed objects' shapes are ignored, except the
    /// target's when the goal is the target's own position, whose half sizes then count. The
    /// target's gap (`FUN_14118faf0`) clamps each axis by `MAXSS` (an unordered one is 0) and is
    /// in range by `COMISS`/`JC` (0x140f9e5f5: an unordered one is not); an end lies off the map
    /// below 0 (`COMISS`/`JA`) or at the size (`JNC`), so an unordered coordinate is on it.
    #[must_use]
    pub fn entry(&self) -> Entry {
        if let Some(t) = self.target {
            let fx = fmath::maxss(
                (self.from.x() - t.position.x()).abs() - t.half_size.x(),
                0.0,
            );
            let fy = fmath::maxss(
                (self.from.y() - t.position.y()).abs() - t.half_size.y(),
                0.0,
            );
            if fmath::sqrt(fy * fy + fx * fx) - self.radius.get() <= self.range.get() {
                return Entry::InRange;
            }
        }
        let on_map = |p: Point| {
            let (x, y) = (p.x(), p.y());
            fmath::at_most_or_unordered(0.0, x)
                && fmath::at_most_or_unordered(0.0, y)
                && fmath::below_or_unordered(x, self.map.width() as f32)
                && fmath::below_or_unordered(y, self.map.height() as f32)
        };
        if !on_map(self.from) || !on_map(self.to) {
            return Entry::Invalid;
        }
        // CVTTSS2SI into a 64-bit register, its low 32 bits kept
        let fixed = |f: f32| crate::math::conversions::cvttss2si_64(f * TO_FIXED) as i32;
        let to = FixedPoint::new(fixed(self.to.x()), fixed(self.to.y()));
        let at_target = self
            .target
            .filter(|t| to == FixedPoint::new(fixed(t.position.x()), fixed(t.position.y())));
        let ignore = self
            .listed
            .iter()
            .filter(|l| at_target.is_none_or(|t| l.id != t.id))
            .filter_map(|l| l.shape)
            .collect();
        let size = at_target.map_or(Footprint::ZERO, |t| t.half_size);
        Entry::Search {
            query: Query {
                from: FixedPoint::new(fixed(self.from.x()), fixed(self.from.y())),
                to,
                radius: self.radius,
                target_size: size,
                range: self.range,
                blocked: self.mask.blocked(),
            },
            ignore,
        }
    }
}

/// A path's fixed-point points as the game appends them (`{x * 2^-22, 0, y * 2^-22}` through the
/// path's dedup, `FUN_140f95a10`).
#[must_use]
pub fn to_path(points: &[FixedPoint]) -> crate::pathing::tile_path::Path {
    let mut path = crate::pathing::tile_path::Path::default();
    for p in points {
        let [x, y] = p.get();
        let _ = path.push(BVector::new(x as f32 * TO_FLOAT, 0.0, y as f32 * TO_FLOAT));
    }
    path
}

/// The pather's state between searches: its temporary shapes (`pather + 0x98`, removed at the end
/// of each call by `FUN_140f9d2b0`) and the flag per ignored shape (`+0xd8`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pather {
    pub(super) temp: Vec<ShapeId>,
    pub(super) ignore: Vec<bool>,
}

impl Pather {
    /// `FUN_140fa8650`: the runs of tiles in the bounds whose terrain the unit cannot cross, as
    /// temporary shapes (each tile ±half a tile).
    pub(super) fn add_terrain(
        &mut self,
        mesh: &mut Mesh,
        words: &dyn Fn(u32, u32) -> u64,
        blocked: u64,
        b: [i32; 4],
    ) {
        let mut ty = tile_of(b[1]) as u32;
        let mut y1 = (ty as i32).wrapping_mul(ONE).wrapping_add(0x60_0000);
        while ty <= tile_of(b[3]) as u32 {
            let mut run: Option<u32> = None;
            let mut tx = tile_of(b[0]) as u32;
            while tx <= tile_of(b[2]) as u32 {
                if words(tx, ty) & blocked == 0 {
                    if let Some(a) = run.take() {
                        let r = FixedRect::new(
                            (a as i32).wrapping_mul(ONE) - 0x20_0000,
                            (ty as i32).wrapping_mul(ONE) - 0x20_0000,
                            (tx as i32).wrapping_mul(ONE) + 0x20_0000,
                            y1,
                        );
                        self.temp.push(mesh.add(r));
                    }
                } else if run.is_none() {
                    run = Some(tx);
                }
                tx += 1;
            }
            if let Some(a) = run {
                let r = FixedRect::new(
                    (a as i32).wrapping_mul(ONE) - 0x20_0000,
                    (ty as i32).wrapping_mul(ONE) - 0x20_0000,
                    (tx as i32).wrapping_mul(ONE) + 0x20_0000,
                    y1,
                );
                self.temp.push(mesh.add(r));
            }
            ty += 1;
            y1 = y1.wrapping_add(ONE);
        }
        if self.ignore.len() < mesh.shapes.len() {
            self.ignore.resize(mesh.shapes.len(), false);
        }
    }

    /// `FUN_140fa3b60`: the temporary shapes leave the mesh, in the order they came.
    pub(super) fn remove_terrain(&mut self, mesh: &mut Mesh) {
        for id in self.temp.drain(..) {
            mesh.remove(id);
        }
    }

    /// One call of the short pather (`FUN_140f9d2f0` the ignored shapes, `FUN_140f9d300` the
    /// search with the query's floats in fixed point, `FUN_140f9d2b0` the cleanup): `ignore` the
    /// shape ids of the units it ignores, `words` the obstruction manager's tile words.
    pub fn search(
        &mut self,
        mesh: &mut Mesh,
        words: &dyn Fn(u32, u32) -> u64,
        q: &Query,
        ignore: &[ShapeId],
    ) -> Result<ShortPath, GenieError> {
        if self.ignore.len() < mesh.shapes.len() {
            self.ignore.resize(mesh.shapes.len(), false);
        }
        for &id in ignore {
            *self.ignore.checked_mut(id.index())? = true;
        }
        let result = self.core(mesh, words, &q.fixed());
        for &id in ignore {
            if let Some(f) = self.ignore.get_mut(id.index()) {
                *f = false;
            }
        }
        self.remove_terrain(mesh);
        result
    }

    /// `FUN_140faa170` (from `FUN_140fa0110`, the drop-off walker's start test): the search's
    /// prologue as [`Pather::search`]'s, without ignored shapes, then only the start
    /// (`FUN_140fa4410`): true when it is out of the goal's range and blocked with fewer than
    /// two escape candidates (its 2). The temporary terrain shapes leave the mesh after it
    /// (`FUN_140f9d2b0`).
    pub fn start_blocked(
        &mut self,
        mesh: &mut Mesh,
        words: &dyn Fn(u32, u32) -> u64,
        q: &Query,
    ) -> Result<bool, GenieError> {
        let q = &q.fixed();
        if self.ignore.len() < mesh.shapes.len() {
            self.ignore.resize(mesh.shapes.len(), false);
        }
        let r = q.radius;
        let d = 0x20_0000 - r;
        let w = mesh.width as i32;
        let cap = w.wrapping_mul(ONE) - 1 - r;
        let [sx, sy] = q.from.get();
        let [gx, gy] = q.to.get();
        let hold_high = |v: i32| {
            if v.wrapping_add(0x100_0000) < cap {
                v.wrapping_add(0x100_0000)
            } else {
                cap
            }
        };
        let hold_low = |v: i32| {
            if r < v.wrapping_sub(0x100_0000) {
                v.wrapping_sub(0x100_0000)
            } else {
                r
            }
        };
        let bounds = [
            hold_low(if sx < gx { sx } else { gx }),
            hold_low(if sy < gy { sy } else { gy }),
            hold_high(if gx < sx { sx } else { gx }),
            hold_high(if gy < sy { sy } else { gy }),
        ];
        self.remove_terrain(mesh);
        self.add_terrain(mesh, words, q.blocked, bounds);
        let goal = Goal {
            at: FixedPoint::new(gx, gy),
            r,
            rx: q.target_size.x(),
            ry: q.target_size.y(),
            rrx: i64::from(r.wrapping_add(q.target_size.x())),
            rry: i64::from(r.wrapping_add(q.target_size.y())),
            range: q.range,
            range2: (i64::from(q.range) * i64::from(q.range)) >> 22,
        };
        let ignore = self.ignore.clone();
        let mut s = Search {
            ctx: Ctx {
                mesh,
                ignore: &ignore,
                d,
            },
            goal,
            bounds,
            open: Vec::new(),
            arena: Vec::new(),
            polys: Vec::new(),
            goal_node: NONE,
            best: 0,
        };
        let started = s.start(q.from);
        self.remove_terrain(mesh);
        Ok(started? == 2)
    }

    /// `FUN_140fa53d0`. Its call counter (`DAT_143f0f364 += 1` at entry) has no reader (its only
    /// xref is that read-write) and is not held.
    pub(super) fn core(
        &mut self,
        mesh: &mut Mesh,
        words: &dyn Fn(u32, u32) -> u64,
        q: &Fixed,
    ) -> Result<ShortPath, GenieError> {
        let r = q.radius;
        let d = 0x20_0000 - r;
        let w = mesh.width as i32;
        let cap = w.wrapping_mul(ONE) - 1 - r;
        let [sx, sy] = q.from.get();
        let [mut gx, mut gy] = q.to.get();
        let hold_high = |v: i32| {
            if v.wrapping_add(0x100_0000) < cap {
                v.wrapping_add(0x100_0000)
            } else {
                cap
            }
        };
        let hold_low = |v: i32| {
            if r < v.wrapping_sub(0x100_0000) {
                v.wrapping_sub(0x100_0000)
            } else {
                r
            }
        };
        let bounds = [
            hold_low(if sx < gx { sx } else { gx }),
            hold_low(if sy < gy { sy } else { gy }),
            hold_high(if gx < sx { sx } else { gx }),
            hold_high(if gy < sy { sy } else { gy }),
        ];
        let invalid = ShortPath {
            result: 4,
            points: Vec::new(),
        };
        if sx < bounds[0] || sy < bounds[1] || bounds[2] < sx || bounds[3] < sy {
            return Ok(invalid);
        }
        if gx < bounds[0] || gy < bounds[1] || bounds[2] < gx || bounds[3] < gy {
            match clip(q.from, FixedPoint::new(gx, gy), bounds) {
                Some((_, end)) => [gx, gy] = end.get(),
                None => return Ok(invalid),
            }
        }
        self.remove_terrain(mesh);
        self.add_terrain(mesh, words, q.blocked, bounds);
        let goal = Goal {
            at: FixedPoint::new(gx, gy),
            r,
            rx: q.target_size.x(),
            ry: q.target_size.y(),
            rrx: i64::from(r.wrapping_add(q.target_size.x())),
            rry: i64::from(r.wrapping_add(q.target_size.y())),
            range: q.range,
            range2: (i64::from(q.range) * i64::from(q.range)) >> 22,
        };
        let ignore = self.ignore.clone();
        let mut s = Search {
            ctx: Ctx {
                mesh,
                ignore: &ignore,
                d,
            },
            goal,
            bounds,
            open: Vec::new(),
            arena: Vec::new(),
            polys: Vec::new(),
            goal_node: NONE,
            best: 0,
        };
        s.run(q.from)
    }
}
