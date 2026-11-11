//! The spawn search's first test under the world's feature flags (`FUN_1411c52f0`): the free
//! point nearest its goal ([`closest_goal`]) on the square about the building.

use crate::GenieResult;
use crate::commands::GatherPoint;
use crate::map::Footprint;
use crate::map::Point;
use crate::math::image::spawn::SIDE_END;
use crate::math::msvc::sort;
use crate::objects::ObjectId;
use crate::production::spawn::{NONE, Site, SpawnTest, Tested};
use crate::world::layer::generated::SpawnTuning;

/// The least step of the walk along a side: the immediate `0x3dcccccd` that `std::max` keeps
/// where the unit's radius is not above [`SpawnTuning::gap`] (the store to `[RSP + 0x70]` at
/// 0x1411c56d8; the compare reads the gap, `DAT_142eb5690`, at 0x1411c56e9).
const LEAST_STEP: f32 = 0.1;

/// A point of the square and its squared distance from the goal: the routine's element of 12
/// bytes (x, y, the distance).
#[derive(Clone, Copy, Debug, PartialEq)]
struct EdgePoint {
    /// The point.
    at: Point,
    /// Its squared distance from the goal.
    distance: f32,
}

/// `FUN_1411c52f0`'s goal (`+0xc30`, `+0xc40`, then its slots 0x218 and 0x220 of the unit AI):
/// none without a gather point or with one on the building itself (`building`); else each
/// coordinate of the caller's point `from`, or for one that is [`NONE`] (`UCOMISS`, so a NaN is
/// not) the first location of the building's unit AI's command queue (`queue_first`: the
/// building unit AI's `FUN_140c16d70` and `FUN_140c16830` at index 0, or the tower unit AI's
/// twins `FUN_140c16da0` and `FUN_140c168a0`), or without a queue the gather point's (`+0xc34`,
/// `+0xc38`).
pub(super) fn closest_goal(
    from: Point,
    gather: Option<GatherPoint>,
    building: ObjectId,
    queue_first: Option<Point>,
) -> Option<Point> {
    let gather_point = gather.filter(|gather_point| gather_point.target != Some(building))?;
    let fallback = queue_first.unwrap_or_else(|| gather_point.point.ground());
    let x = if from.x() == NONE {
        fallback.x()
    } else {
        from.x()
    };
    let y = if from.y() == NONE {
        fallback.y()
    } else {
        from.y()
    };
    Some(Point::new(x, y))
}

/// `FUN_1411c52f0`: the first free point (the master's slot 0x50, [`SpawnTest::Closest`]) of
/// the square about `site` at its x radius plus the unit's (`unit`) plus [`SpawnTuning::gap`]
/// from its centre, the square's points ([`edge_points`]) sorted by their squared distance from
/// `goal` (`std::sort`, `FUN_1411ab710`: [`sort`]). None when every point is blocked: the game's
/// 0, its out x and y unwritten.
///
/// # Errors
///
/// The test's, and the sort's unreachable bound.
pub(super) fn closest_spot(
    site: &Site,
    unit: Footprint,
    goal: Point,
    test: &mut dyn FnMut(Point, SpawnTest) -> GenieResult<Tested>,
    spawn_tuning: &SpawnTuning,
) -> GenieResult<Option<Point>> {
    let points = {
        let mut points = edge_points(site, unit, goal, spawn_tuning);
        sort(&mut points, &|first: EdgePoint, second: EdgePoint| {
            first.distance < second.distance
        })?;
        points
    };
    for point in points {
        if test(point.at, SpawnTest::Closest)? == Tested::Free {
            return Ok(Some(point.at));
        }
    }
    Ok(None)
}

/// The square's points in the routine's order: the bottom side from the left, the right side
/// from the top, the top side from the right, the left side from the bottom, each from its first
/// corner by shares of the step (the unit's x radius, at least [`LEAST_STEP`]: `COMISS` and
/// `CMOVA`, so a NaN radius takes it) while the share is at most [`SIDE_END`] (`COMISS` and
/// `JNC` at 0x1411c5971), each with its squared distance from `goal`.
fn edge_points(
    site: &Site,
    unit: Footprint,
    goal: Point,
    spawn_tuning: &SpawnTuning,
) -> Vec<EdgePoint> {
    let (x, y) = (site.position.x(), site.position.y());
    let reach = site.radius.x() + unit.x() + spawn_tuning.gap;
    let low_left = Point::new(x - reach, y - reach);
    let low_right = Point::new(x + reach, y - reach);
    let high_right = Point::new(x + reach, y + reach);
    let high_left = Point::new(x - reach, y + reach);
    let step = if unit.x() > spawn_tuning.gap {
        unit.x()
    } else {
        LEAST_STEP
    };
    let sides = [
        (low_left, low_right),
        (high_right, low_right),
        (high_right, high_left),
        (low_left, high_left),
    ];
    let mut points = Vec::new();
    for (from, to) in sides {
        let mut share = 0.0;
        let mut more = true;
        while more {
            let at = Point::new(
                (to.x() - from.x()) * share + from.x(),
                (to.y() - from.y()) * share + from.y(),
            );
            let (dx, dy) = ((at.x() - goal.x()).abs(), (at.y() - goal.y()).abs());
            // the top side adds `dx * dx` first: a sum's bits are the same either way, and the
            // distance only feeds the sort's compares, which no NaN's bits change
            let distance = dy * dy + dx * dx;
            points.push(EdgePoint { at, distance });
            share += step;
            more = share <= SIDE_END;
        }
    }
    points
}
