//! The spawn search's later rings (`FUN_1411d12d0`'s cases 11 to 18): the points about the
//! building's edge, then the points inside its footprint (`FUN_1411d0b20`).

use std::iter::successors;

use crate::GenieResult;
use crate::map::GroundOffset;
use crate::map::Point;
use crate::map::Tile;
use crate::math::conversions::cvttss2si;
use crate::production::spawn::{Site, SpawnTest, Tested};
use crate::world::layer::generated::SpawnTuning;

/// A point of the inner ring kept for want of a free one (`FUN_1411d0b20`'s out-pointers), with
/// the object count of its tile (`*param_7`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Kept {
    /// The point.
    point: Point,
    /// Its tile's object count (the map tile's `+0x1b`).
    objects: u8,
}

/// Cases 11 to 14: the points about the building `site` at its half sizes plus
/// `SpawnTuning::ring_margin`, each tried by the unit master's slot 0x50 on its own
/// ([`SpawnTest::Outer`]): the top side from the left (11), the right side down (12), the
/// bottom side from the right (13), the left side up (14), each from a step past its corner
/// while short of the next (`COMISS`, `JA`: a NaN ends the side). The first free point.
///
/// # Errors
///
/// The test's.
pub(super) fn outer_ring(
    site: &Site,
    test: &mut dyn FnMut(Point, SpawnTest) -> GenieResult<Tested>,
    spawn_tuning: &SpawnTuning,
) -> GenieResult<Option<Point>> {
    let (bx, by) = (site.radius.x(), site.radius.y());
    let (cx, cy) = (site.position.x(), site.position.y());
    let margin = spawn_tuning.ring_margin;
    let (on, back) = (spawn_tuning.ring_step, spawn_tuning.ring_back_step);
    let left = cx - (bx + margin);
    let right = bx + margin + cx;
    let top = by + margin + cy;
    let bottom = cy - (by + margin);
    let top_side = coordinates(left + on, on, move |x| right > x).map(move |x| Point::new(x, top));
    let right_side =
        coordinates(top - on, back, move |y| y > bottom).map(move |y| Point::new(right, y));
    let bottom_side =
        coordinates(right - on, back, move |x| x > left).map(move |x| Point::new(x, bottom));
    let left_side =
        coordinates(bottom + on, on, move |y| top > y).map(move |y| Point::new(left, y));
    for point in top_side
        .chain(right_side)
        .chain(bottom_side)
        .chain(left_side)
    {
        if test(point, SpawnTest::Outer)? == Tested::Free {
            return Ok(Some(point));
        }
    }
    Ok(None)
}

/// Cases 15 to 18: the points inside the building `site` at its half sizes less
/// `SpawnTuning::ring_margin`, each with its three neighbours ([`inside_spot`], the unit's
/// `step`): the top side from the left (15), the left side down from a step below the top (16),
/// the right side the same way (17), the bottom side from a step past the left (18), each
/// while not past its end (`COMISS`, `JNC`: a NaN ends the side). The first free point; else
/// the first point of the fewest objects on its tile, a tile of fewer than 255 (the count
/// starts at the immediate `0xff`, and `0xfe <` the count answers 0); else none.
///
/// # Errors
///
/// The test's and the tile count's.
pub(super) fn inner_ring(
    site: &Site,
    step: GroundOffset,
    test: &mut dyn FnMut(Point, SpawnTest) -> GenieResult<Tested>,
    tile_objects: &dyn Fn(Tile) -> GenieResult<Option<u8>>,
    spawn_tuning: &SpawnTuning,
) -> GenieResult<Option<Point>> {
    let margin = spawn_tuning.ring_margin;
    let (hx, hy) = (site.radius.x() - margin, site.radius.y() - margin);
    let (cx, cy) = (site.position.x(), site.position.y());
    let (on, back) = (spawn_tuning.ring_step, spawn_tuning.ring_back_step);
    let left = cx - hx;
    let right = hx + cx;
    let top = hy + cy;
    let bottom = cy - hy;
    let top_side = coordinates(left, on, move |x| x <= right).map(move |x| Point::new(x, top));
    let left_side =
        coordinates(top - on, back, move |y| y >= bottom).map(move |y| Point::new(left, y));
    let right_side =
        coordinates(top - on, back, move |y| y >= bottom).map(move |y| Point::new(right, y));
    let bottom_side =
        coordinates(left + on, on, move |x| x <= right).map(move |x| Point::new(x, bottom));
    let mut kept = None;
    for point in top_side
        .chain(left_side)
        .chain(right_side)
        .chain(bottom_side)
    {
        if let Some(free) = inside_spot(point, step, &mut kept, test, tile_objects)? {
            return Ok(Some(free));
        }
    }
    Ok(kept.map(|kept| kept.point))
}

/// `FUN_1411d0b20` at `at` for a unit of `step`: the unit master's slot 0x50
/// ([`SpawnTest::Inner`]) at `at`, a step down, a step right of that and a step up again (each
/// coordinate stepped from the last); the first free one. Else, when `at`'s tile
/// (`CVTTSS2SI` of each coordinate) is on the map and holds fewer objects than the point
/// `kept` holds (255 without one), `at` is kept with its count.
fn inside_spot(
    at: Point,
    step: GroundOffset,
    kept: &mut Option<Kept>,
    test: &mut dyn FnMut(Point, SpawnTest) -> GenieResult<Tested>,
    tile_objects: &dyn Fn(Tile) -> GenieResult<Option<u8>>,
) -> GenieResult<Option<Point>> {
    let (x, y) = (at.x(), at.y());
    let below = y - step.y();
    let right = x + step.x();
    let candidates = [
        at,
        Point::new(x, below),
        Point::new(right, below),
        Point::new(right, below + step.y()),
    ];
    for candidate in candidates {
        if test(candidate, SpawnTest::Inner)? == Tested::Free {
            return Ok(Some(candidate));
        }
    }
    let tile = Tile::new(cvttss2si(x), cvttss2si(y));
    let fewest = kept.map_or(u8::MAX, |kept| kept.objects);
    if let Some(objects) = tile_objects(tile)?
        && objects < fewest
    {
        *kept = Some(Kept { point: at, objects });
    }
    Ok(None)
}

/// The coordinates of a ring's side: from `start`, each `step` added to the last (one `ADDSS`
/// each), while `more` holds.
fn coordinates(start: f32, step: f32, more: impl Fn(f32) -> bool) -> impl Iterator<Item = f32> {
    successors(Some(start), move |&at| Some(at + step)).take_while(move |&at| more(at))
}

#[cfg(test)]
mod tests;
