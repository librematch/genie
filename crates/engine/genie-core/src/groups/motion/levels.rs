//! The walked level's flags, its reset, a looping path's next waypoint and the waypoints'
//! points and ground lengths.

use crate::GenieError;
use crate::groups::motion::NEAR;
use crate::map::GridSize;
use crate::map::Point;
use crate::math::fmath;
use crate::math::fmath::BVector;
use crate::movement::LevelFlags;
use crate::movement::PathData;
use crate::world::slots::CheckedIndex;

/// `FUN_140f9b4d0(level, bit, on)` on the walked level: its flag `flag` set or cleared;
/// [`LevelFlags::MET_TARGET`] (bit 2) is then set on every parent too.
pub(super) fn set_flag(paths: &mut [PathData], flag: LevelFlags, on: bool) {
    let Some((level, parents)) = paths.split_first_mut() else {
        return;
    };
    level.flags.set(flag, on);
    if flag == LevelFlags::MET_TARGET {
        for p in parents {
            p.flags |= LevelFlags::MET_TARGET;
        }
    }
}

/// `FUN_140f98e80` on the walked level: it and its parents go, the level holds the defaults
/// (its plans back to 10, flags clear; its repath time stays).
pub(super) fn reset_level(paths: &mut Vec<PathData>) -> Result<(), GenieError> {
    let repath_time = paths.first().map_or(0.0, |l| l.repath_time);
    paths.truncate(1);
    let level = PathData {
        repath_time,
        ..PathData::without_path()
    };
    if paths.is_empty() {
        paths.push(level);
    } else {
        *paths.checked_mut(0)? = level;
    }
    Ok(())
}

/// The waypoint at `index` (`FUN_140f9bcd0`), the game's static zero point outside the path.
pub(super) fn point(level: &PathData, index: i32) -> BVector {
    usize::try_from(index)
        .ok()
        .and_then(|i| level.points.get(i))
        .copied()
        .unwrap_or(BVector::ZERO)
}

/// The length on the ground from `a` to `b` (`FUN_14087e560`, the height zeroed).
pub(super) fn ground(a: BVector, b: BVector) -> f32 {
    let mut d = fmath::sub(a, b);
    d = d.with_height(0.0);
    fmath::sqrt(d.x() * d.x() + 0.0 + d.y() * d.y())
}

/// Where a step toward a waypoint ends against it (`FUN_141046130`'s answer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::groups) enum Passing {
    /// 0: short of it (`COMISS` then `JBE`: a NaN step or distance is short).
    Short = 0,
    /// 1: across it, ending no farther beyond than it was ahead.
    Reaches = 1,
    /// 2: past it, or a step under 0.1.
    Passes = 2,
}

/// `FUN_141046130` for a walk from `at` toward `waypoint` that would end at `next`.
pub(in crate::groups) fn passes_waypoint(waypoint: BVector, at: BVector, next: BVector) -> Passing {
    let mut a = fmath::sub(waypoint, at);
    let mut b = fmath::sub(waypoint, next);
    let c = fmath::sub(next, at);
    a = a.with_height(0.0);
    b = b.with_height(0.0);
    let to_waypoint = fmath::sqrt(a.x() * a.x() + 0.0 + a.y() * a.y());
    let beyond = fmath::sqrt(b.x() * b.x() + b.height() * b.height() + b.y() * b.y());
    let step = fmath::sqrt(c.x() * c.x() + c.height() * c.height() + c.y() * c.y());
    if fmath::at_most_or_unordered(step, to_waypoint) {
        Passing::Short
    } else if to_waypoint < beyond || step < NEAR {
        Passing::Passes
    } else {
        Passing::Reaches
    }
}

/// `FUN_1411570c0`'s ends held inside the map: below 0 at 0, from the map's size on at the size
/// less 0.1, per axis.
pub(in crate::groups) fn hold(p: BVector, map: GridSize) -> Point {
    let axis = |v: f32, size: i32| {
        if v < 0.0 {
            0.0
        } else if size as f32 <= v {
            size as f32 - NEAR
        } else {
            v
        }
    };
    Point::new(axis(p.x(), map.width()), axis(p.y(), map.height()))
}
