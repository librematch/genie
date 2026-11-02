//! The spawn point search (`FUN_1411d12d0`): the building's sides in segments, each spot's
//! candidates (`FUN_1411c5b60`) and the corners' wall checks.

use crate::GenieError;
use crate::map::Footprint;
use crate::map::GroundOffset;
use crate::map::Point;
use crate::map::Position;
use crate::map::{Tile, TileOffset};
use crate::production::spawn::{GAP, NONE, SIDE_SHARE, SMALL, Site, Tested};

/// The ways a spot tries its next point: up, down, left and right.
const UP: TileOffset = TileOffset::new(0, 1);
/// See [`UP`].
const DOWN: TileOffset = TileOffset::new(0, -1);
/// See [`UP`].
const LEFT: TileOffset = TileOffset::new(-1, 0);
/// See [`UP`].
const RIGHT: TileOffset = TileOffset::new(1, 0);

/// One spot (`FUN_1411c5b60`'s argument): its candidates `i` in `0..=count` lie at
/// `(x + dir.x * step.x * i, y + dir.y * step.y * i)`.
pub(super) struct Spot {
    pub(super) at: Point,
    pub(super) step: GroundOffset,
    pub(super) dir: TileOffset,
    pub(super) count: i32,
}

/// `FUN_1411c5b60`: the first free candidate; a candidate blocked by an object that stays ends
/// the spot.
pub(super) fn try_spot(
    spot: &Spot,
    test: &mut dyn FnMut(Point) -> Result<Tested, GenieError>,
) -> Result<Option<Point>, GenieError> {
    for i in 0..=spot.count {
        let x = spot.dir.dx() as f32 * spot.step.x() * i as f32 + spot.at.x();
        let y = spot.dir.dy() as f32 * spot.step.y() * i as f32 + spot.at.y();
        match test(Point::new(x, y))? {
            Tested::Free => return Ok(Some(Point::new(x, y))),
            Tested::Blocked { stays: true } => return Ok(None),
            Tested::Blocked { stays: false } => {}
        }
    }
    Ok(None)
}

/// `FUN_1411d12d0`: the spawn point beside `site` for a unit of radii `unit`. The segments of
/// the sides facing the gather point come first (the top side's left part, the left side's top
/// part, the top's right part, the left's lower part; then the right side down; then the bottom
/// side), each only where no wall closes its corner; a second pass tries every open side
/// ([`Frame::segments`]). Without a gather point there is one pass. The search's later rings
/// (cases 11 to 18: the spots on the building's edge and the rings around it, `FUN_1411d0b20`)
/// and its first test under `DAT_143f01833` (`FUN_1411c52f0`, probed 0) are not modelled.
pub fn spawn_point(
    site: &Site,
    unit: Footprint,
    test: &mut dyn FnMut(Point) -> Result<Tested, GenieError>,
) -> Result<Point, GenieError> {
    let frame = Frame::new(site, unit);
    for pass in 1..=2 {
        for segment in frame.segments(pass == 2) {
            // a segment's spots are all made before its first is tried
            let spots: Vec<Spot> = segment.iter().flat_map(|r| frame.spots(r)).collect();
            for spot in &spots {
                if let Some(p) = try_spot(spot, test)? {
                    return Ok(p);
                }
            }
        }
        if frame.no_x {
            break;
        }
    }
    Err(GenieError::unsupported(
        "a spawn point beyond the building's sides (the search's later rings)",
    ))
}

/// Where a run's spots lie and which way each tries its next point: above the building (up),
/// below it (down), left of it or right of it (out).
#[derive(Clone, Copy)]
enum Side {
    Up,
    Down,
    Left,
    Right,
}

/// Which way a run's coordinate goes: up while below its end, or down while above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Course {
    Rising,
    Falling,
}

/// One run of a segment's spots on a side: from `start` by `step` while below `end`
/// ([`Course::Rising`]) or above it, the other coordinate `fixed` (the side's line).
#[derive(Clone, Copy)]
struct Run {
    side: Side,
    start: f32,
    step: f32,
    end: f32,
    course: Course,
    fixed: f32,
}

/// The frame about the building the search walks: the unit's step (its radii, a small unit's
/// doubled plus [`GAP`]), the building's half sizes and centre, the frame's sides, the walled
/// corners (left top, right top, right bottom, left bottom), and the gather point and its
/// distances from the centre (the axes the gather point lacks: [`NONE`]).
struct Frame {
    step: GroundOffset,
    half: Footprint,
    centre: Point,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    walls: [bool; 4],
    gather: Point,
    away: GroundOffset,
    no_x: bool,
    no_y: bool,
}

impl Frame {
    /// The frame beside `site` for a unit of radii `unit`.
    fn new(site: &Site, unit: Footprint) -> Self {
        let (mut sx, mut sy) = (unit.x(), unit.y());
        if sx < SMALL && sy < SMALL {
            sx = sx + sx + GAP;
            sy = sy + sy + GAP;
        }
        let (bx, by) = (site.radius.x(), site.radius.y());
        let (x, y) = (site.position.x(), site.position.y());
        let (gx, gy) = {
            let p = site.gather.unwrap_or(Point::new(NONE, NONE));
            (p.x(), p.y())
        };
        Self {
            step: GroundOffset::new(sx, sy),
            half: site.radius,
            centre: Point::new(x, y),
            left: x - (bx + sx),
            right: x + bx + sx,
            top: y + by + sy,
            bottom: y - (by + sy),
            walls: site.walls,
            gather: Point::new(gx, gy),
            away: GroundOffset::new((x - gx).abs(), (y - gy).abs()),
            no_x: gx == NONE,
            no_y: gy == NONE,
        }
    }

    /// The segments of one pass (the second tries every open side), in their order, each its
    /// runs: the top left corner's four ([`Frame::top_left`]), the right side's two
    /// ([`Frame::right_side`]), the bottom side's two ([`Frame::bottom_side`]); then, on the
    /// second pass with the top right and bottom left corners walled, one segment of the bottom
    /// side from the right (9) and the right side from the bottom (10).
    fn segments(&self, second: bool) -> Vec<Vec<Run>> {
        let [left_top, right_top, right_bottom, left_bottom] = self.walls;
        let (x, y, gx, gy) = (
            self.centre.x(),
            self.centre.y(),
            self.gather.x(),
            self.gather.y(),
        );
        let (dx, dy) = (self.away.x(), self.away.y());
        let mut out = Vec::new();
        if !left_top {
            self.top_left(second, &mut out);
        }
        if !right_top && (self.no_x || second || (x < gx && dy < dx)) {
            self.right_side(second, &mut out);
        }
        let bottom_closed = left_bottom || (!self.no_y && !second && (y <= gy || dy < dx));
        if !bottom_closed {
            self.bottom_side(second, &mut out);
        }
        if !right_bottom && right_top && left_bottom && second {
            let (sx, sy) = (self.step.x(), self.step.y());
            let (bx, by) = (self.half.x(), self.half.y());
            let (right, bottom) = (self.right, self.bottom);
            out.push(vec![
                run(
                    Side::Down,
                    (right, -sx, right - bx),
                    Course::Falling,
                    bottom,
                ),
                run(
                    Side::Right,
                    (bottom, sy, bottom + by),
                    Course::Rising,
                    right,
                ),
            ]);
        }
        out
    }

    /// Below an open top left corner: (1) the top side's left part, (2) the left side's top
    /// part, (3) the top side's right part (without a wall at the top right), (4) the left
    /// side's lower part (without one at the bottom left), each toward the gather point or on
    /// the second pass.
    fn top_left(&self, second: bool, out: &mut Vec<Vec<Run>>) {
        let [_, right_top, _, left_bottom] = self.walls;
        let (x, y, gx, gy) = (
            self.centre.x(),
            self.centre.y(),
            self.gather.x(),
            self.gather.y(),
        );
        let (dx, dy) = (self.away.x(), self.away.y());
        let (sx, sy) = (self.step.x(), self.step.y());
        let (bx, by) = (self.half.x(), self.half.y());
        let (left, top) = (self.left, self.top);
        let (no_x, no_y) = (self.no_x, self.no_y);
        if no_y || second || ((y < gy && dx <= dy) && (no_x || gx < x)) {
            let end = bx * SIDE_SHARE + bx + left;
            out.push(vec![run(Side::Up, (left, sx, end), Course::Rising, top)]);
        }
        if no_x || second || ((gx < x && dy < dx) && (no_y || y < gy)) {
            let end = top - (by * SIDE_SHARE + by);
            out.push(vec![run(
                Side::Left,
                (top, -sy, end),
                Course::Falling,
                left,
            )]);
        }
        if !right_top && (no_y || second || (y < gy && dx <= dy)) {
            let (start, end) = (bx * SIDE_SHARE + bx + left, bx + bx + left + sx + sx);
            out.push(vec![run(Side::Up, (start, sx, end), Course::Rising, top)]);
        }
        if !left_bottom && (no_x || second || (gx < x && dy < dx)) {
            let (start, end) = (top - (by * SIDE_SHARE + by), top - (by + by + sy + sy));
            out.push(vec![run(
                Side::Left,
                (start, -sy, end),
                Course::Falling,
                left,
            )]);
        }
    }

    /// The right side down: (5) its top half (toward a gather point above, or on the second
    /// pass), (6) its lower half without a wall at the bottom right.
    fn right_side(&self, second: bool, out: &mut Vec<Vec<Run>>) {
        let y = self.centre.y();
        let (sy, by) = (self.step.y(), self.half.y());
        let (right, top) = (self.right, self.top);
        if self.no_y || second || y < self.gather.y() {
            out.push(vec![run(
                Side::Right,
                (top, -sy, top - by),
                Course::Falling,
                right,
            )]);
        }
        if !self.walls[2] {
            let (start, end) = (top - by, top - (by + by + sy + sy));
            out.push(vec![run(
                Side::Right,
                (start, -sy, end),
                Course::Falling,
                right,
            )]);
        }
    }

    /// The bottom side: (7) its left half (toward a gather point on the left, or on the second
    /// pass), (8) its right half without a wall at the bottom right.
    fn bottom_side(&self, second: bool, out: &mut Vec<Vec<Run>>) {
        let x = self.centre.x();
        let (sx, bx) = (self.step.x(), self.half.x());
        let (left, bottom) = (self.left, self.bottom);
        if self.no_x || second || self.gather.x() < x {
            out.push(vec![run(
                Side::Down,
                (left, sx, left + bx),
                Course::Rising,
                bottom,
            )]);
        }
        if !self.walls[2] {
            let (start, end) = (left + bx, bx + bx + sx + sx + left);
            out.push(vec![run(
                Side::Down,
                (start, sx, end),
                Course::Rising,
                bottom,
            )]);
        }
    }

    /// A run's spots: from its start by its step while short of its end. A spot of a
    /// horizontal run tries its point and the next one up (or down); a vertical run's, the next
    /// one out.
    fn spots(&self, r: &Run) -> Vec<Spot> {
        let (sx, sy) = (self.step.x(), self.step.y());
        let mut spots = Vec::new();
        let mut c = r.start;
        while match r.course {
            Course::Rising => c < r.end,
            Course::Falling => r.end < c,
        } {
            let (at, step, dir) = match r.side {
                Side::Up => (Point::new(c, r.fixed), GroundOffset::new(0.0, sy), UP),
                Side::Down => (Point::new(c, r.fixed), GroundOffset::new(0.0, sy), DOWN),
                Side::Left => (Point::new(r.fixed, c), GroundOffset::new(sx, 0.0), LEFT),
                Side::Right => (Point::new(r.fixed, c), GroundOffset::new(sx, 0.0), RIGHT),
            };
            spots.push(Spot {
                at,
                step,
                dir,
                count: 1,
            });
            c += r.step;
        }
        spots
    }
}

/// A run on `side` from `start` by `step` to `end` (its `course`), on the line `fixed`.
const fn run(side: Side, (start, step, end): (f32, f32, f32), course: Course, fixed: f32) -> Run {
    Run {
        side,
        start,
        step,
        end,
        course,
        fixed,
    }
}

/// The corners' tile pairs (`FUN_1411d12d0`'s wall checks, left top, right top, right bottom,
/// left bottom) for a unit of radii `unit` beside `site`.
pub(super) fn corner_tiles(
    position: Position,
    radius: Footprint,
    unit: Footprint,
) -> [[Tile; 2]; 4] {
    let (mut sx, mut sy) = (unit.x(), unit.y());
    if sx < SMALL && sy < SMALL {
        sx = sx + sx + GAP;
        sy = sy + sy + GAP;
    }
    let (x, y) = (position.x(), position.y());
    let (bx, by) = (radius.x(), radius.y());
    let left = (x - (bx + sx)) as i32;
    let right = (x + bx + sx) as i32;
    let top = (y + by + sy) as i32;
    let bottom = (y - (by + sy)) as i32;
    [
        [Tile::new(left, top - 1), Tile::new(left + 1, top)],
        [Tile::new(right - 1, top), Tile::new(right, top - 1)],
        [Tile::new(right, bottom + 1), Tile::new(right - 1, bottom)],
        [Tile::new(left + 1, bottom), Tile::new(left, bottom + 1)],
    ]
}
