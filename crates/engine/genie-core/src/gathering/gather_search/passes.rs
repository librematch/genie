//! The resource search's passes: the tiles it starts from, the flood's next tiles and its order, and the first resource a tile holds.

use crate::GenieError;
use crate::gathering::gather_search::Candidate;
use crate::gathering::gather_search::Cell;
use crate::gathering::gather_search::Pass;
use crate::gathering::gather_search::SearchSite;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Tile;
use crate::math::image::gather_search::ROUNDING as GATHER_SEARCH_ROUNDING;
use crate::math::msvc;
use crate::objects::masters::GatherGroup;
use crate::players::Amount;
use crate::vision::SightTiles;
use crate::world::layer::generated::GatherSearchTuning;
use crate::world::slots::CheckedIndex;

/// [`closest_resource_at`]'s passes: from a footprint site its footprint's tiles the unit can
/// stand on ([`footprint_seeds`], sorted by `nearer`, within the site's line of sight plus 10 of
/// its tile), then the unit's tile alone; from another site its tile alone; without one the
/// unit's tile alone (a single tile's pass within the line of sight `los` plus 5).
///
/// # Errors
///
/// What the sort of the footprint's tiles fails on.
pub(super) fn search_passes(
    site: Option<&SearchSite>,
    map: GridSize,
    passable: &dyn Fn(Tile) -> bool,
    unit_tile: Tile,
    los: SightTiles,
    nearer: &impl Fn(Cell, Cell) -> bool,
) -> Result<Vec<Pass>, GenieError> {
    let single = |at: Tile| Pass {
        origin: at,
        max_steps: los.get().wrapping_add(5),
        seeds: vec![Cell { tile: at, steps: 0 }],
    };
    let mut passes = Vec::with_capacity(2);
    match site {
        Some(s) if s.footprint => {
            let mut seeds = footprint_seeds(s, map, passable);
            msvc::sort(&mut seeds, nearer)?;
            passes.push(Pass {
                origin: Tile::new(s.position.x() as i32, s.position.y() as i32),
                max_steps: SightTiles::of(s.line_of_sight).get().wrapping_add(10),
                seeds,
            });
            passes.push(single(unit_tile));
        }
        Some(s) => passes.push(single(Tile::new(
            s.position.x() as i32,
            s.position.y() as i32,
        ))),
        None => passes.push(single(unit_tile)),
    }
    Ok(passes)
}

/// [`closest_resource_at`]'s spread from a passable `cell`: its neighbours x−1, x+1, y−1, y+1
/// ([`Tile::beside`]) not seen yet become seen (`at` gives a tile's index in `seen`), and those
/// on the map join the `queue` a step farther.
///
/// # Errors
///
/// A neighbour outside `seen`.
pub(super) fn push_neighbours(
    queue: &mut Vec<Cell>,
    seen: &mut [bool],
    at: &impl Fn(Tile) -> usize,
    Cell { tile, steps }: Cell,
    map: GridSize,
) -> Result<(), GenieError> {
    for n in tile.beside() {
        let c = seen.checked_mut(at(n))?;
        if !*c {
            *c = true;
            if map.holds(n) {
                queue.push(Cell {
                    tile: n,
                    steps: steps + 1,
                });
            }
        }
    }
    Ok(())
}

/// [`closest_resource_at`]'s sort point and box side: without a site the unit's place and
/// three times its line of sight plus 10; with one the point a quarter of the way from the
/// site's place to the unit's, and for a footprint search the larger line of sight of the two.
pub(super) fn sort_point_and_side(
    at: Point,
    los: SightTiles,
    site: Option<&SearchSite>,
    gather_search_tuning: &GatherSearchTuning,
) -> ((f32, f32), i32) {
    let (ux, uy) = (at.x(), at.y());
    let box_side = |los: i32| los.wrapping_add(10).wrapping_mul(3);
    let los = los.get();
    site.map_or_else(
        || ((ux, uy), box_side(los)),
        |s| {
            let (sx, sy) = (s.position.x(), s.position.y());
            let at = (
                sx * gather_search_tuning.site_share + ux * gather_search_tuning.unit_share,
                sy * gather_search_tuning.site_share + uy * gather_search_tuning.unit_share,
            );
            let site_los = SightTiles::of(s.line_of_sight).get();
            let wide = if site_los.wrapping_add(10) <= los.wrapping_add(10) {
                los
            } else {
                site_los
            };
            (
                at,
                if s.footprint {
                    box_side(wide)
                } else {
                    box_side(los)
                },
            )
        },
    )
}

/// [`closest_resource_at`]'s first pass from a site's footprint: every tile of the footprint
/// the unit can stand on (x from the site's x less its radius, truncated, at least 1, less 1, to
/// the site's x plus its radius, truncated, at most the map's last column; y alike), row by row.
pub(super) fn footprint_seeds(
    s: &SearchSite,
    map: GridSize,
    passable: &dyn Fn(Tile) -> bool,
) -> Vec<Cell> {
    let (sx, sy) = (s.position.x(), s.position.y());
    let (rx, ry) = (s.radius.x(), s.radius.y());
    let x0 = ((sx - rx) as i32).max(1) - 1;
    let x1 = ((sx + rx) as i32).min(map.width() - 1);
    let y0 = ((sy - ry) as i32).max(1) - 1;
    let y1 = ((sy + ry) as i32).min(map.height() - 1);
    let mut seeds = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let tile = Tile::new(x, y);
            if passable(tile) {
                seeds.push(Cell { tile, steps: 0 });
            }
        }
    }
    seeds
}

/// The search's distance of `tile` from a pass's origin: the longer axis plus the shorter one
/// times 1.4 rounded down, less the shorter one (`GatherSearchTuning::diagonal` plus `ROUNDING`, truncated).
pub(super) fn pass_distance(
    origin: Tile,
    tile: Tile,
    gather_search_tuning: &GatherSearchTuning,
) -> i32 {
    const ROUNDING: f32 = GATHER_SEARCH_ROUNDING;
    let (dx, dy) = ((origin.x() - tile.x()).abs(), (origin.y() - tile.y()).abs());
    if dy < dx {
        ((dy as f32 * gather_search_tuning.diagonal + ROUNDING) as i32 - dy) + dx
    } else {
        ((dx as f32 * gather_search_tuning.diagonal + ROUNDING) as i32 - dx) + dy
    }
}

/// The object of a tile's list `ClosestResourceObject` takes: the first in a pasture or
/// targetable whose master gathers by itself as `kind` with some amount held; with none, the
/// last object when it is neither in a pasture nor targetable.
pub(super) fn first_resource(
    on_tile: &[Candidate],
    kind: GatherGroup,
) -> Result<Option<Candidate>, GenieError> {
    let open = |o: &Candidate| o.pasture.is_some() || !o.untargetable;
    let found: Vec<&Candidate> = on_tile
        .iter()
        .filter(|o| open(o) && o.auto_gather && o.gather_group == kind && Amount::ZERO < o.amount)
        .collect();
    match found.as_slice() {
        [one] => Ok(Some(**one)),
        [] if on_tile.iter().all(open) => Ok(None),
        [] if on_tile.len() == 1 => Ok(on_tile.first().copied()),
        _ => Err(GenieError::unsupported(
            "a tile whose order of objects decides a resource search",
        )),
    }
}
