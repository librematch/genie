//! The searches' walks over the mesh's tiles ([`Ctx`]): a point's holder, the raycast, the
//! axis sweeps and the sight test.

use crate::GenieResult;
use crate::map::Tile;
use crate::pathing::FixedDistance;
use crate::pathing::FixedRect;
use crate::pathing::mesh::ONE;
use crate::pathing::short_path::axis_dir::AxisDir;
use crate::pathing::short_path::{
    Callback, Ctx, FixedPoint, Hit, Meet, RayFraction, Sweep, Token, inside, ray_rect,
    strictly_inside, tile_of,
};
use crate::pathing::{ShapeId, ShapeSide};
use crate::world::slots::CheckedIndexExt;

/// One tile of a raycast ([`Ctx::ray_tile`]'s arguments).
#[derive(Debug)]
pub(super) struct RayTile<'ray> {
    /// The callback over the tile's shapes, its token as the tiles before left it.
    pub(super) callback: &'ray mut Callback,
    /// The tile's shapes.
    pub(super) list: &'ray [ShapeId],
    /// The ray's start.
    pub(super) start: FixedPoint,
    /// The ray's end.
    pub(super) end: FixedPoint,
    /// The ray's fraction so far, which a hit lowers.
    pub(super) fraction: &'ray mut RayFraction,
}

impl Ctx<'_> {
    pub(super) fn ignored(&self, id: ShapeId) -> bool {
        self.ignore.get(id.index()).copied().unwrap_or(false)
    }

    /// The shapes listed on `tile` (its index wraps as the game's does).
    pub(super) fn tile(&self, tile: Tile) -> GenieResult<&[ShapeId]> {
        let index = (tile
            .y()
            .wrapping_mul(self.mesh.width as i32)
            .wrapping_add(tile.x())) as u32 as usize;
        Ok(self.mesh.tiles.checked(index)?)
    }

    /// `FUN_140fa3dc0`: the first shape of `fixed_point`'s tile, not ignored, strictly holding
    /// `fixed_point` (shrunk).
    pub(super) fn holder(&self, fixed_point: FixedPoint) -> GenieResult<Option<ShapeId>> {
        for &id in self.tile(Tile::new(
            tile_of(fixed_point.x()),
            tile_of(fixed_point.y()),
        ))? {
            if !self.ignored(id)
                && strictly_inside(
                    *self.mesh.shapes.checked(id.index())?,
                    fixed_point,
                    self.clearance,
                )
            {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    /// `FUN_140fa2630`: the first shape of `fixed_point`'s tile other than `excl`, not ignored,
    /// holding `fixed_point` with its edges.
    pub(super) fn holder_but(
        &self,
        fixed_point: FixedPoint,
        excl: ShapeId,
        fixed_distance: FixedDistance,
    ) -> GenieResult<Option<ShapeId>> {
        for &id in self.tile(Tile::new(
            tile_of(fixed_point.x()),
            tile_of(fixed_point.y()),
        ))? {
            if id != excl
                && !self.ignored(id)
                && inside(
                    *self.mesh.shapes.checked(id.index())?,
                    fixed_point,
                    fixed_distance,
                )
            {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    /// One tile of the raycast: the callback over the tile's shapes; its last hit (each nearer
    /// than the one before) and the callback's result for it, none without one.
    pub(super) fn ray_tile(&self, tile: RayTile<'_>) -> GenieResult<Option<(Meet, Hit)>> {
        let RayTile {
            callback: cb,
            list,
            start,
            end: fixed_point,
            fraction: ray_fraction,
        } = tile;
        let clearance = self.clearance.get();
        let mut result = None;
        // B and C first test the shape against the ray's box
        let mut bx = [
            if start.x() < fixed_point.x() {
                start.x()
            } else {
                fixed_point.x()
            },
            if start.y() < fixed_point.y() {
                start.y()
            } else {
                fixed_point.y()
            },
            if fixed_point.x() < start.x() {
                start.x()
            } else {
                fixed_point.x()
            },
            if fixed_point.y() < start.y() {
                start.y()
            } else {
                fixed_point.y()
            },
        ];
        if matches!(cb, Callback::Box) {
            if start.x() == fixed_point.x() {
                bx[0] -= 1;
                bx[2] += 1;
            }
            if start.y() == fixed_point.y() {
                bx[1] -= 1;
                bx[3] += 1;
            }
        }
        for &id in list {
            if self.ignored(id) {
                continue;
            }
            let fixed_rect = *self.mesh.shapes.checked(id.index())?;
            let shrunk = FixedRect::new(
                fixed_rect.x0() + clearance,
                fixed_rect.y0() + clearance,
                fixed_rect.x1() - clearance,
                fixed_rect.y1() - clearance,
            );
            match cb {
                Callback::Corner(token) => {
                    let mut tt = *ray_fraction;
                    let Some((meet, side)) = ray_rect(start, fixed_point, shrunk, &mut tt) else {
                        continue;
                    };
                    let meet = if meet == Meet::TouchAtStart {
                        let own = Token::of_shape(id);
                        if Token::of_side(side) == *token || own == *token {
                            *token = own;
                            continue;
                        }
                        Meet::Touch
                    } else {
                        meet
                    };
                    result = Some((meet, Hit { shape: id, side }));
                    *ray_fraction = tt;
                }
                Callback::Box | Callback::Sight => {
                    let clip = [
                        if bx[0] < shrunk.x0() {
                            shrunk.x0()
                        } else {
                            bx[0]
                        },
                        if bx[1] < shrunk.y0() {
                            shrunk.y0()
                        } else {
                            bx[1]
                        },
                        if shrunk.x1() < bx[2] {
                            shrunk.x1()
                        } else {
                            bx[2]
                        },
                        if shrunk.y1() < bx[3] {
                            shrunk.y1()
                        } else {
                            bx[3]
                        },
                    ];
                    if !(clip[0] < clip[2] && clip[1] < clip[3]) {
                        continue;
                    }
                    let mut tt = *ray_fraction;
                    if let Some((met, side)) = ray_rect(start, fixed_point, shrunk, &mut tt) {
                        let meet = if met == Meet::TouchAtStart {
                            Meet::Touch
                        } else {
                            met
                        };
                        result = Some((meet, Hit { shape: id, side }));
                        *ray_fraction = tt;
                    }
                }
            }
        }
        Ok(result)
    }

    /// `FUN_140faa820`: the segment `start`–`fixed_point` walked tile by tile (4-connected, x first
    /// on a corner), the callback per tile; after a hit the walk goes on only to the hit's tile.
    /// Returns the last hit, the nearest (hits only replace nearer ones), with its non-zero
    /// callback result; none for 0.
    pub(super) fn raycast(
        &self,
        start: FixedPoint,
        fixed_point: FixedPoint,
        ray_fraction: &mut RayFraction,
        mut cb: Callback,
    ) -> GenieResult<Option<(Meet, Hit)>> {
        let (x0, y0, x1, y1) = (start.x(), start.y(), fixed_point.x(), fixed_point.y());
        let dx = x1.wrapping_sub(x0);
        let adx = dx.wrapping_abs();
        let ady = y1.wrapping_sub(y0).wrapping_abs();
        let mut tx = tile_of(x0);
        let mut ty = tile_of(y0);
        let mut count: i32 = 1;
        let (step_x, err_x);
        if adx == 0 {
            step_x = 0;
            err_x = 0x7fc0_0000_i32;
        } else {
            let frac;
            if x0 < x1 {
                step_x = 1;
                count = tile_of(x1) - tx;
                frac = (tile_of(x0) + 1).wrapping_mul(ONE).wrapping_sub(x0);
            } else {
                step_x = -1;
                count = tx - tile_of(x1);
                frac = x0.wrapping_sub(tile_of(x0).wrapping_mul(ONE));
            }
            count += 1;
            err_x = ((i64::from(frac) * i64::from(ady)) >> 22) as i32;
        }
        let (step_y, mut err);
        if ady == 0 {
            step_y = 0;
            err = i32::MIN;
        } else {
            let frac = if y0 < y1 {
                step_y = 1;
                count += tile_of(y1) - ty;
                tile_of(y0)
                    .wrapping_mul(ONE)
                    .wrapping_sub(y0)
                    .wrapping_add(ONE)
            } else {
                step_y = -1;
                count += ty - tile_of(y1);
                y0.wrapping_sub(tile_of(y0).wrapping_mul(ONE))
            };
            err = err_x.wrapping_sub(((i64::from(frac) * i64::from(adx)) >> 22) as i32);
        }
        let mut result = None;
        while 0 < count {
            let before = err;
            let list = self.tile(Tile::new(tx, ty))?;
            let ray_tile = self.ray_tile(RayTile {
                callback: &mut cb,
                list,
                start,
                end: fixed_point,
                fraction: ray_fraction,
            })?;
            let mut left = count;
            if ray_tile.is_some() {
                let hy = ((i64::from(y1.wrapping_sub(y0)) * i64::from(ray_fraction.get())) >> 22)
                    as i32
                    + y0;
                let hx = ((i64::from(dx) * i64::from(ray_fraction.get())) >> 22) as i32 + x0;
                let span =
                    (tile_of(hx) - tx).wrapping_abs() + 1 + (tile_of(hy) - ty).wrapping_abs();
                result = ray_tile;
                left = if count < span { count } else { span };
            }
            count = left - 1;
            err = err.wrapping_add(if err < 1 { ady } else { adx.wrapping_neg() });
            if before < 1 {
                tx += step_x;
            } else {
                ty += step_y;
            }
        }
        Ok(result)
    }

    /// `FUN_140fa2d00`: an axis ray from `from` toward `end` in direction `dir` (0 −y, 1 +x, 2 +y,
    /// 3 −x) against the near side of shape `fixed_rect` (shrunk), the ray's line strictly inside
    /// the shape's span; the hit point and side.
    pub(super) fn axis_hit(
        from: FixedPoint,
        end: FixedPoint,
        fixed_rect: FixedRect,
        fixed_distance: FixedDistance,
        dir: AxisDir,
    ) -> Option<(FixedPoint, ShapeSide)> {
        let [x, y] = from.get();
        let fixed_distance = fixed_distance.get();
        let met = match dir {
            AxisDir::PlusX => {
                let edge = fixed_distance + fixed_rect.x0();
                (fixed_rect.y0() + fixed_distance < y
                    && y < fixed_rect.y1() - fixed_distance
                    && edge < end.x()
                    && x <= edge)
                    .then_some(FixedPoint::new(edge, y))
            }
            AxisDir::PlusY => {
                let edge = fixed_distance + fixed_rect.y0();
                (fixed_rect.x0() + fixed_distance < x
                    && x < fixed_rect.x1() - fixed_distance
                    && edge < end.y()
                    && y <= edge)
                    .then_some(FixedPoint::new(x, edge))
            }
            AxisDir::MinusX => {
                let x1 = fixed_rect.x1() - fixed_distance;
                (fixed_rect.y0() + fixed_distance < y
                    && y < fixed_rect.y1() - fixed_distance
                    && end.x() < x1
                    && x1 <= x)
                    .then_some(FixedPoint::new(x1, y))
            }
            AxisDir::MinusY => {
                let y1 = fixed_rect.y1() - fixed_distance;
                (fixed_rect.x0() + fixed_distance < x
                    && x < fixed_rect.x1() - fixed_distance
                    && end.y() < y1
                    && y1 <= y)
                    .then_some(FixedPoint::new(x, y1))
            }
        };
        met.map(|fixed_point| (fixed_point, dir.facing()))
    }

    /// `FUN_140fa8930` (and `FUN_140fa2420` per tile): an axis sweep from `from` to `to` in
    /// direction `dir` over the tiles of its row or column: the nearest shape side it meets
    /// (later hits must be nearer). A hit at `from` itself while the end is still `to` is checked
    /// backward from `to`: landing on `from` again means a shape too thin to count; the game has
    /// already written that hit's point and side by then ([`Sweep`]).
    pub(super) fn sweep(
        &self,
        from: FixedPoint,
        to: FixedPoint,
        dir: AxisDir,
    ) -> GenieResult<Sweep> {
        let fixed_distance = self.clearance;
        let mut end = to;
        let mut out = Sweep::default();
        let visit = |list: &[ShapeId], end: &mut FixedPoint, out: &mut Sweep| -> GenieResult<()> {
            for &id in list {
                if self.ignored(id) {
                    continue;
                }
                let fixed_rect = *self.mesh.shapes.checked(id.index())?;
                let Some((hp, side)) = Self::axis_hit(from, *end, fixed_rect, fixed_distance, dir)
                else {
                    continue;
                };
                out.point = Some(hp);
                out.side = Some(side);
                if from == hp && *end == to {
                    let back =
                        Self::axis_hit(*end, from, fixed_rect, fixed_distance, dir.reverse());
                    if back.is_some_and(|(bp, _)| bp == from) {
                        continue;
                    }
                }
                *end = hp;
                out.shape = Some(id);
                out.found = true;
            }
            Ok(())
        };
        match dir {
            AxisDir::PlusX => {
                let row = tile_of(from.y());
                let mut x = tile_of(from.x()) as u32;
                while x <= tile_of(to.x()) as u32 {
                    visit(self.tile(Tile::new(x as i32, row))?, &mut end, &mut out)?;
                    x = x.wrapping_add(1);
                }
            }
            AxisDir::PlusY => {
                let col = tile_of(from.x());
                let mut y = tile_of(from.y()) as u32;
                while y <= tile_of(to.y()) as u32 {
                    visit(self.tile(Tile::new(col, y as i32))?, &mut end, &mut out)?;
                    y = y.wrapping_add(1);
                }
            }
            AxisDir::MinusX => {
                let row = tile_of(from.y());
                let mut x = tile_of(from.x());
                while tile_of(to.x()) <= x {
                    visit(self.tile(Tile::new(x, row))?, &mut end, &mut out)?;
                    x -= 1;
                }
            }
            AxisDir::MinusY => {
                let col = tile_of(from.x());
                let mut y = tile_of(from.y());
                while tile_of(to.y()) <= y {
                    visit(self.tile(Tile::new(col, y))?, &mut end, &mut out)?;
                    y -= 1;
                }
            }
        }
        Ok(out)
    }

    /// `FUN_140fa8e60`: whether the straight walk from `from` to `to` is blocked (the smoothing's
    /// line of sight): an axis sweep along an axis, else the raycast with callback C.
    pub(super) fn sight_blocked(&self, from: FixedPoint, to: FixedPoint) -> GenieResult<bool> {
        if from.x() == to.x() {
            return Ok(self
                .sweep(from, to, AxisDir::of_dy(to.y() - from.y()))?
                .found);
        }
        if from.y() == to.y() {
            return Ok(self
                .sweep(from, to, AxisDir::of_dx(to.x() - from.x()))?
                .found);
        }
        let mut ray_fraction = RayFraction::START;
        Ok(self
            .raycast(from, to, &mut ray_fraction, Callback::Sight)?
            .is_some())
    }
}
