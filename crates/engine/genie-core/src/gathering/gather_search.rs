//! The searches for a resource to gather: a builder's search for more work around the building it
//! completed (`FUN_1415cc4e0`, over the tiles `FUN_1415cc1a0` reaches) and the unit AI's
//! `ClosestResourceObject` (`FUN_140c49760`, slot 0x80), which the builder's Build asks next
//! (`0x140ab0370` case 0xdf).

use crate::GenieError;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::map::TileOffset;
use crate::math::msvc;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::MasterId;
use crate::objects::masters::class;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::unit_ai::task_finder::DropSite;
use crate::vision::SightTiles;

/// What the searches read of an object in a map tile's list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    /// `+8`.
    pub id: ObjectId,
    /// The owner.
    pub player: PlayerId,
    /// Its master's class (`+0x410`, decoded).
    pub class: ClassId,
    /// Its master's id.
    pub master: MasterId,
    /// `+0x640`.
    pub state: ObjectState,
    /// `+0x644`, `worker_num`.
    pub workers: crate::objects::WorkerCount,
    /// x, y, z.
    pub position: Position,
    /// `+0x790`, `parent_pasture_id`.
    pub pasture: Option<crate::gathering::PastureId>,
    /// `+0x689` is 0 ([`crate::objects::unit::Unit::untargetable`]).
    pub untargetable: bool,
    /// `+0x308`, `attribute_type_held`, none for −1.
    pub attribute: Option<AttributeId>,
    /// `+0x310`, `attribute_amount_held`.
    pub amount: Amount,
    /// Its master's `enable_auto_gather` (`+0x681`).
    pub auto_gather: bool,
    /// Its master's `resource_gather_group` (`+0x683`).
    pub gather_group: GatherGroup,
}

/// What a builder's search reads of the builder and the building it completed.
#[derive(Clone, Copy, Debug)]
pub struct Builder {
    /// The builder's place.
    pub position: Position,
    /// Its master's x radius (`+0x5d8`).
    pub radius: Distance,
    /// The building's place.
    pub building: Position,
    /// The search's reach in tiles: the building's line of sight held to 4 to 20 (`DAT_142eb0f48`,
    /// `DAT_142e7f38c`), truncated.
    pub range: i16,
}

/// `FUN_1415cc4e0` with `param_5` and `param_6` 0: the object the builder works next, from the
/// drop site's entry for the building, the builder's class and its owner's civ (`entry`,
/// [`crate::unit_ai::task_finder::drop_site`]; none, or one without `auto_work`, finds nothing). The search
/// covers the tiles within the reach plus the builder's radius (rounded) of the building's tile,
/// cut to the map; of those only the tiles the builder reaches count ([`reached`], with
/// `blocked` the builder's restriction's planner, `FUN_140f7d470`; None without a planner, when
/// every tile counts). `objects` lists a tile's objects in the tile's order.
///
/// The tiles go by columns (x outer, y inner), a tile's objects in its list's order. An object
/// below state 6 whose class the entry takes ([`DropSite::resource_of`]) joins its resource's
/// group (gold 3, stone 2, food 0 and 15 to 17, wood 1), by its distance from the builder
/// truncated to whole tiles (`CVTTSS2SI`): the group keeps the first nearest or, at no lesser
/// distance, the one with fewer workers (its workers less one, at least 0), and its least
/// distance of those kept. Gold comes first, then stone, food and wood, each taking over only at
/// a lesser least distance. A farm (class 49) joins only through the entry's farm target, which
/// is not modelled.
///
/// # Errors
///
/// A farm on a tile for an entry with a farm target.
pub fn more_work(
    entry: Option<&DropSite>,
    builder: &Builder,
    map: GridSize,
    blocked: Option<&dyn Fn(Tile) -> bool>,
    objects: &dyn Fn(Tile) -> Result<Vec<Candidate>, GenieError>,
) -> Result<Option<Candidate>, GenieError> {
    let Some(entry) = entry.filter(|e| e.auto_work) else {
        return Ok(None);
    };
    // the box: (short)(int) of the building's place, ± the reach plus the builder's radius
    let r = builder
        .range
        .wrapping_add((builder.radius + Distance::new(0.5)).get() as i32 as i16);
    let (bx, by) = (
        builder.building.x() as i32 as i16,
        builder.building.y() as i32 as i16,
    );
    let x0 = bx.wrapping_sub(r).max(0);
    let y0 = by.wrapping_sub(r).max(0);
    let mut x1 = bx.wrapping_add(r);
    let mut y1 = by.wrapping_add(r);
    if map.width() <= i32::from(x1) {
        x1 = map.width() as i16 - 1;
    }
    if map.height() <= i32::from(y1) {
        y1 = map.height() as i16 - 1;
    }
    let (w, h) = (i32::from(x1 - x0) + 1, i32::from(y1 - y0) + 1);
    let corner = TileOffset::new(i32::from(x0), i32::from(y0));
    let start =
        Tile::new(builder.position.x() as i32, builder.position.y() as i32).stepped_back(corner);
    let grid = blocked.and_then(|b| reached(start, GridSize::new(w, h), &|t| b(t.stepped(corner))));
    // gold, stone, food, wood: (the object, its workers, the group's least distance)
    let mut groups: [Option<(Candidate, crate::objects::WorkerCount)>; 4] = [None; 4];
    let mut least = [90.0f32; 4];
    for x in x0..=x1 {
        for y in y0..=y1 {
            let (gx, gy) = (i32::from(x - x0), i32::from(y - y0));
            if grid.as_ref().is_some_and(|g| !g[(gx * h + gy) as usize]) {
                continue;
            }
            for o in objects(Tile::new(i32::from(x), i32::from(y)))? {
                // The farm class (49), which both searches treat apart.
                if o.class == class::FARM {
                    // its own tests (owner, pasture, the targeting flag, masters 1893 and 1897)
                    // come before the entry's, which takes no farm without a farm target
                    if entry.targets.iter().any(|t| t.class == class::FARM) {
                        return Err(GenieError::unsupported(
                            "a farm in a builder's search for more work",
                        ));
                    }
                    continue;
                }
                if ObjectState::Undead <= o.state {
                    continue;
                }
                let Some(resource) = entry.resource_of(o.class, o.state, o.workers) else {
                    continue;
                };
                let g = match resource {
                    attribute::GOLD => 0,
                    attribute::STONE => 1,
                    food if food.is_food_load() => 2,
                    attribute::WOOD => 3,
                    _ => continue,
                };
                let dx = o.position.x() - builder.position.x();
                let dy = o.position.y() - builder.position.y();
                let d = crate::math::conversions::cvttss2si(crate::math::fmath::sqrt(
                    dy * dy + dx * dx,
                )) as f32;
                let workers = o.workers.plus(-1).max(crate::objects::WorkerCount::ZERO);
                let take = match groups[g] {
                    Some((_, kept)) if least[g] <= d => workers < kept,
                    _ => true,
                };
                if take {
                    groups[g] = Some((o, workers));
                    if d < least[g] {
                        least[g] = d;
                    }
                }
            }
        }
    }
    // gold, then stone, food and wood, each at a lesser least distance
    let mut best: Option<(Candidate, f32)> = None;
    for g in 0..4 {
        if let Some((o, _)) = groups[g]
            && best.is_none_or(|(_, d)| least[g] < d)
        {
            best = Some((o, least[g]));
        }
    }
    Ok(best.map(|(o, _)| o))
}

/// `FUN_1415cc1a0`: the cells of a box of `size` a flood from `start` reaches, by
/// columns (`[x * height + y]`): every cell next to (left, right, above, below) a cell it
/// entered and not `blocked`, `start` entered whatever it holds. None when `start` lies outside
/// the box.
#[must_use]
pub fn reached(start: Tile, size: GridSize, blocked: &dyn Fn(Tile) -> bool) -> Option<Vec<bool>> {
    if !size.holds(start) {
        return None;
    }
    // by columns, unlike the grid's own rows
    let cell = |t: Tile| (t.x() * size.height() + t.y()) as usize;
    let mut seen = vec![false; (size.width() * size.height()) as usize];
    *seen.get_mut(cell(start))? = true;
    let mut todo = vec![start];
    let mut first = true;
    while let Some(t) = todo.pop() {
        if !std::mem::take(&mut first) && blocked(t) {
            continue;
        }
        for n in t.beside() {
            if size.holds(n)
                && let Some(s) = seen.get_mut(cell(n))
                && !*s
            {
                *s = true;
                todo.push(n);
            }
        }
    }
    Some(seen)
}

/// What `ClosestResourceObject` reads of the unit that searches.
#[derive(Clone, Copy, Debug)]
pub struct Seeker {
    /// The owner.
    pub player: PlayerId,
    /// The unit's place.
    pub position: Position,
    /// Its master's line of sight (`+0x5d0`).
    pub line_of_sight: Distance,
    /// `+0x640`.
    pub state: ObjectState,
    /// Its unit AI's current target (`+0x48`), which the search passes over.
    pub current_target: Option<ObjectId>,
    /// `+0x794`, `my_pasture_id`.
    pub my_pasture: Option<crate::gathering::PastureId>,
    /// `param_6`: below this state the unit finds nothing (2 for the unit AIs' searches, 0 for
    /// a mule cart's).
    pub min_state: ObjectState,
}

/// One cell of the search's queue: a tile and its steps from the start.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Cell {
    tile: Tile,
    steps: i32,
}

/// The most objects whose reach a search tests before it settles for what it has (`0x1e`).
const TESTED: i32 = 30;

/// The drop site a resource search starts from (`ClosestResourceObject`'s `param_3`).
#[derive(Clone, Copy, Debug)]
pub struct SearchSite {
    /// The site's place.
    pub position: Position,
    /// Its master's radii (`+0x5d8`, `+0x5dc`).
    pub radius: Footprint,
    /// Its master's line of sight (`+0x5d0`).
    pub line_of_sight: Distance,
    /// The search starts from the site's footprint: the site's tile lies in the unit's zone (its
    /// restriction's zone of the tile, `FUN_1411958c0`, is the unit's own, `FUN_14118eea0`), or
    /// the site's master is 45 or 2172 (the docks). Else it starts from the site's tile alone.
    pub footprint: bool,
}

/// The tiles a pass of the search starts from and how far it goes.
struct Pass {
    /// The tile the steps' distance counts from (the search's point, truncated).
    origin: Tile,
    /// The farthest distance a tile may lie from the origin.
    max_steps: i32,
    /// The queue's first cells, sorted.
    seeds: Vec<Cell>,
}

/// The search's reads of its world `W` (the spatial index): a tile the player has explored, a tile
/// the unit can stand on, the candidates on a tile, and the check whether the unit reaches an
/// object, which writes the obstruction manager's scratch in `W` and so takes it by `&mut`.
pub type Explored<'a, W> = dyn Fn(&W, Tile) -> Result<bool, GenieError> + 'a;
/// See [`Explored`].
pub type Passable<'a, W> = dyn Fn(&W, Tile) -> bool + 'a;
/// See [`Explored`].
pub type ObjectsOn<'a, W> = dyn Fn(&W, Tile) -> Result<Vec<Candidate>, GenieError> + 'a;
/// See [`Explored`].
pub type Reaches<'a, W> = dyn FnMut(&mut W, ObjectId) -> Result<bool, GenieError> + 'a;

/// What [`closest_resource`] and [`closest_resource_at`] read of the world `W` (`FUN_140c49760`'s
/// arguments past the unit, the site and the kind).
pub struct ResourceWorld<'a, W> {
    /// The map's size.
    pub map: GridSize,
    /// The spatial index the reads take, which the reachability check writes scratch in.
    pub world: &'a mut W,
    /// Whether the player has explored a tile.
    pub explored: &'a Explored<'a, W>,
    /// Whether the unit can stand on a tile (`FUN_141185690`).
    pub passable: &'a Passable<'a, W>,
    /// The objects on a tile.
    pub objects: &'a ObjectsOn<'a, W>,
    /// Whether the unit reaches an object (slot 0x3d0 with the range 0).
    pub reach: &'a mut Reaches<'a, W>,
}

impl<W> std::fmt::Debug for ResourceWorld<'_, W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceWorld")
            .field("map", &self.map)
            .finish_non_exhaustive()
    }
}

/// `ClosestResourceObject` (`FUN_140c49760`) with no point (`param_7`, `param_8` −1: the unit's
/// place), no drop site (`param_3` −1) and `param_6` 2: the closest object of gather group `kind`
/// that the unit reaches (`reach`, slot 0x3d0 with the range 0: `FUN_14152b6e0` holds in the
/// games the slice runs), −1 for none. A unit below state 2 is not modelled.
///
/// A breadth-first walk over the tiles from the unit's tile, its queue after the tile in hand kept
/// sorted by the distance of the tiles' centres from the unit ([`msvc::sort`]): a tile the player
/// has explored (`explored`), within the line of sight plus 5 of the unit (`1.4` per diagonal
/// step, `+ 0.61`, truncated) and that the unit can stand on (`passable`, `FUN_141185690`) adds
/// its four neighbours not seen yet that lie on the map. On each such tile the first object in a
/// pasture or targetable whose master gathers by itself (`enable_auto_gather` 1) as `kind`, with
/// some amount held, and that is not the unit AI's current target, is tested: with fewer workers
/// than `max_workers` (−1 for any) a reach makes it the answer; else a reach makes it the
/// fallback when it is fewer steps away than the last. After 30 tests, or the queue's end, the
/// fallback is the answer. A tile whose last object fails the first test counts it as found
/// (the game's loop leaves it in hand).
///
/// # Errors
///
/// A unit with a pasture of its own, or a tile whose order of objects decides (two candidates,
/// or another object beside one that fails the first test).
pub fn closest_resource<W>(
    seeker: &Seeker,
    kind: GatherGroup,
    max_workers: crate::objects::WorkerCount,
    reads: ResourceWorld<'_, W>,
) -> Result<Option<ObjectId>, GenieError> {
    closest_resource_at(seeker, None, kind, max_workers, reads).map(|(id, _)| id)
}

/// [`closest_resource`] from a drop site (`param_3`, None for −1 or a site not found), with what
/// its out argument (`param_5`) takes: the distance of the answer's tile from the pass's origin
/// (the search's 1.4-per-diagonal measure, truncated), None for no answer (the argument keeps its
/// value).
///
/// With a site the search's point is the site's place, and the queue sorts by the distance from
/// a point a quarter of the way from the site to the unit (`0.75` of the site's place plus `0.25`
/// of the unit's). From the site's footprint ([`SearchSite::footprint`]) the first pass starts
/// from every tile of the footprint the unit can stand on (its x from the site's x less its
/// radius, truncated, at least 1, less 1, to the site's x plus its radius, truncated, at most the
/// map's last column; y alike; row by row), reaches the site's line of sight plus 10 and walks a
/// box of the larger line of sight of the two; when it finds nothing a second pass starts from
/// the unit's tile as the search without a site does, its tests counted on from the first pass
/// unless that one stopped after 30, its queue still sorted by the point between. Otherwise the
/// one pass starts from the site's tile and reaches the unit's line of sight plus 5.
///
/// # Errors
///
/// As [`closest_resource`].
pub fn closest_resource_at<W>(
    seeker: &Seeker,
    site: Option<&SearchSite>,
    kind: GatherGroup,
    max_workers: crate::objects::WorkerCount,
    reads: ResourceWorld<'_, W>,
) -> Result<(Option<ObjectId>, Option<i32>), GenieError> {
    /// The fallback's steps before it has one (`1000000`).
    const NO_STEPS: i32 = 1_000_000;
    const TILE_CENTRE: f32 = crate::math::image::gather_search::TILE_CENTRE;
    let ResourceWorld {
        map,
        world,
        explored,
        passable,
        objects,
        reach,
    } = reads;
    if seeker.state < seeker.min_state {
        return Ok((None, None));
    }
    if seeker.my_pasture.is_some() {
        return Err(GenieError::unsupported(
            "a resource search by a unit with a pasture of its own",
        ));
    }
    let (ux, uy) = (seeker.position.x(), seeker.position.y());
    let los = SightTiles::of(seeker.line_of_sight);
    let unit_tile = Tile::new(ux as i32, uy as i32);
    // the sort's point, and the box's side (fixed for both passes)
    let (sort_at, side) = sort_point_and_side(Point::new(ux, uy), los, site);
    let half = side / 2;
    let nearer = |a: Cell, b: Cell| {
        let d = |c: Cell| {
            let dx = sort_at.0 - (c.tile.x() as f32 + TILE_CENTRE);
            let dy = sort_at.1 - (c.tile.y() as f32 + TILE_CENTRE);
            dy * dy + dx * dx
        };
        d(a) < d(b)
    };
    let passes = search_passes(site, map, &|t| passable(world, t), unit_tile, los, &nearer);
    let cap = if max_workers == crate::objects::WorkerCount::NO_CAP {
        crate::objects::WorkerCount::of_data(i32::MAX)
    } else {
        max_workers
    };
    let (mut fallback, mut fallback_steps, mut fallback_d, mut tested) = (None, NO_STEPS, 0, 0);
    for pass in passes {
        let (ox, oy) = (half - pass.origin.x(), half - pass.origin.y());
        let mut seen = vec![false; (side * side) as usize];
        let at = |t: Tile| ((t.y() + oy) * side + t.x() + ox) as usize;
        for c in &pass.seeds {
            seen[at(c.tile)] = true;
        }
        let mut queue = pass.seeds;
        let mut i = 0;
        while i < queue.len() {
            let Cell { tile, steps } = queue[i];
            i += 1;
            if !explored(world, tile)? {
                continue;
            }
            let d = pass_distance(pass.origin, tile);
            if pass.max_steps < d || !map.holds(tile) {
                continue;
            }
            if passable(world, tile) {
                push_neighbours(&mut queue, &mut seen, &at, Cell { tile, steps }, map);
                msvc::sort(&mut queue[i..], &nearer);
            }
            let on_tile = objects(world, tile)?;
            let Some(o) = first_resource(&on_tile, kind)? else {
                continue;
            };
            tested += 1;
            if Some(o.id) == seeker.current_target {
                continue;
            }
            if o.workers < cap {
                if reach(world, o.id)? {
                    return Ok((Some(o.id), Some(d)));
                }
            } else if steps < fallback_steps && reach(world, o.id)? {
                fallback = Some(o.id);
                fallback_steps = steps;
                fallback_d = d;
            }
            if TESTED < tested {
                tested = 0;
                break;
            }
        }
        if fallback.is_some() {
            return Ok((fallback, Some(fallback_d)));
        }
    }
    Ok((None, None))
}

/// [`closest_resource_at`]'s passes: from a footprint site its footprint's tiles the unit can
/// stand on ([`footprint_seeds`], sorted by `nearer`, within the site's line of sight plus 10 of
/// its tile), then the unit's tile alone; from another site its tile alone; without one the
/// unit's tile alone (a single tile's pass within the line of sight `los` plus 5).
fn search_passes(
    site: Option<&SearchSite>,
    map: GridSize,
    passable: &dyn Fn(Tile) -> bool,
    unit_tile: Tile,
    los: SightTiles,
    nearer: &impl Fn(Cell, Cell) -> bool,
) -> Vec<Pass> {
    let single = |at: Tile| Pass {
        origin: at,
        max_steps: los.get().wrapping_add(5),
        seeds: vec![Cell { tile: at, steps: 0 }],
    };
    let mut passes = Vec::with_capacity(2);
    match site {
        Some(s) if s.footprint => {
            let mut seeds = footprint_seeds(s, map, passable);
            msvc::sort(&mut seeds, nearer);
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
    passes
}

/// [`closest_resource_at`]'s spread from a passable `cell`: its neighbours x−1, x+1, y−1, y+1
/// ([`Tile::beside`]) not seen yet become seen (`at` gives a tile's index in `seen`), and those
/// on the map join the `queue` a step farther.
fn push_neighbours(
    queue: &mut Vec<Cell>,
    seen: &mut [bool],
    at: &impl Fn(Tile) -> usize,
    Cell { tile, steps }: Cell,
    map: GridSize,
) {
    for n in tile.beside() {
        let c = at(n);
        if !seen[c] {
            seen[c] = true;
            if map.holds(n) {
                queue.push(Cell {
                    tile: n,
                    steps: steps + 1,
                });
            }
        }
    }
}

/// [`closest_resource_at`]'s sort point and box side: without a site the unit's place and
/// three times its line of sight plus 10; with one the point a quarter of the way from the
/// site's place to the unit's, and for a footprint search the larger line of sight of the two.
fn sort_point_and_side(at: Point, los: SightTiles, site: Option<&SearchSite>) -> ((f32, f32), i32) {
    const SITE_SHARE: f32 = crate::math::image::gather_search::SITE_SHARE;
    const UNIT_SHARE: f32 = crate::math::image::gather_search::UNIT_SHARE;
    let (ux, uy) = (at.x(), at.y());
    let box_side = |los: i32| los.wrapping_add(10).wrapping_mul(3);
    let los = los.get();
    site.map_or_else(
        || ((ux, uy), box_side(los)),
        |s| {
            let (sx, sy) = (s.position.x(), s.position.y());
            let at = (
                sx * SITE_SHARE + ux * UNIT_SHARE,
                sy * SITE_SHARE + uy * UNIT_SHARE,
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
fn footprint_seeds(s: &SearchSite, map: GridSize, passable: &dyn Fn(Tile) -> bool) -> Vec<Cell> {
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
/// times 1.4 rounded down, less the shorter one (`DIAGONAL` plus `ROUNDING`, truncated).
fn pass_distance(origin: Tile, tile: Tile) -> i32 {
    const DIAGONAL: f32 = crate::math::image::gather_search::DIAGONAL;
    const ROUNDING: f32 = crate::math::image::gather_search::ROUNDING;
    let (dx, dy) = ((origin.x() - tile.x()).abs(), (origin.y() - tile.y()).abs());
    if dy < dx {
        ((dy as f32 * DIAGONAL + ROUNDING) as i32 - dy) + dx
    } else {
        ((dx as f32 * DIAGONAL + ROUNDING) as i32 - dx) + dy
    }
}

/// The object of a tile's list `ClosestResourceObject` takes: the first in a pasture or
/// targetable whose master gathers by itself as `kind` with some amount held; with none, the
/// last object when it is neither in a pasture nor targetable.
fn first_resource(
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

/// `FUN_140fdd470` then `FUN_140fdd5f0`: the gather group of a resource held, and the most workers
/// the Build's search for more work lets an object of it have (`0x140ab0c57`): food (0, 15 to 17)
/// forage (1) and 6, wood (0) and 2, stone (3) and 3, gold (4) and 3, the rest fish (2) and 2.
#[must_use]
pub fn gather_group(attribute: Option<AttributeId>) -> (GatherGroup, crate::objects::WorkerCount) {
    let kind = match attribute.and_then(AttributeId::load_resource) {
        Some(attribute::FOOD) => GatherGroup::FORAGE,
        Some(attribute::WOOD) => GatherGroup::WOOD,
        Some(attribute::STONE) => GatherGroup::STONE,
        Some(attribute::GOLD) => GatherGroup::GOLD,
        _ => GatherGroup::FISH,
    };
    let workers = crate::objects::WorkerCount::of_data(match kind {
        GatherGroup::FORAGE => 6,
        GatherGroup::STONE | GatherGroup::GOLD => 3,
        _ => 2,
    });
    (kind, workers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gathering::PastureId;
    use crate::players::attribute;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::unit_ai::task_finder::DropTarget;

    fn tree(id: i32, x: f32, y: f32) -> Candidate {
        Candidate {
            id: object_id(id),
            player: PlayerId::new(0),
            class: class::TREE,
            master: master_id(349),
            state: ObjectState::Alive,
            workers: crate::objects::WorkerCount::of_data(0),
            position: Position::new(x, y, 0.0),
            pasture: None,
            untargetable: false,
            attribute: Some(attribute::WOOD),
            amount: Amount::new(100.0),
            auto_gather: true,
            gather_group: GatherGroup::WOOD,
        }
    }

    fn lumber_camp() -> DropSite {
        DropSite {
            building: master_id(562),
            worker_class: Some(class::VILLAGER),
            workers: Vec::new(),
            civs: Vec::new(),
            auto_work: true,
            targets: vec![DropTarget {
                class: class::TREE,
                workers: crate::objects::WorkerCount::of_data(-1),
                state: None,
                attribute: Some(attribute::WOOD),
            }],
            livestock: false,
            resource_types: vec![attribute::WOOD],
        }
    }

    #[test]
    fn a_flood_enters_the_blocked_cells_next_to_what_it_reaches_but_not_past_them() {
        // a wall at x = 2 in a 5 x 3 box: the flood from (0, 1) reaches x 0..=1 and enters the
        // wall's cells, never x 3 or 4
        let seen = reached(Tile::new(0, 1), GridSize::new(5, 3), &|t| t.x() == 2).unwrap();
        let col = |x: i32| {
            (0..3)
                .map(|y| seen[(x * 3 + y) as usize])
                .collect::<Vec<_>>()
        };
        assert_eq!(col(1), [true; 3]);
        assert_eq!(col(2), [true; 3]);
        assert_eq!(col(3), [false; 3]);
        assert!(reached(Tile::new(5, 0), GridSize::new(5, 3), &|_| false).is_none());
    }

    #[test]
    fn more_work_takes_the_first_nearest_tree_then_fewer_workers_at_no_lesser_distance() {
        let entry = lumber_camp();
        let b = Builder {
            position: Position::new(10.5, 10.5, 0.0),
            radius: Distance::new(0.2),
            building: Position::new(12.0, 10.0, 0.0),
            range: 6,
        };
        // 3 and 4 both 3 tiles away (truncated): x outer, so 3 (x 7) comes first; 4 is as far
        // with fewer workers than 3's kept 1 and takes over
        let mut trees = [tree(3, 7.5, 10.5), tree(4, 13.5, 10.5)];
        trees[0].workers = crate::objects::WorkerCount::of_data(2);
        let objects = |tile: Tile| {
            Ok(trees
                .iter()
                .filter(|t| t.position.tile() == tile)
                .copied()
                .collect())
        };
        let found = more_work(Some(&entry), &b, GridSize::new(40, 40), None, &objects).unwrap();
        assert_eq!(found.map(|o| o.id), Some(object_id(4)));
        let mut off = lumber_camp();
        off.auto_work = false;
        assert_eq!(
            more_work(Some(&off), &b, GridSize::new(40, 40), None, &objects).unwrap(),
            None
        );
    }

    #[test]
    fn gold_wins_over_nearer_wood_unless_the_wood_is_nearer_than_the_gold() {
        let mut entry = lumber_camp();
        entry.targets.push(DropTarget {
            class: class::CLASS_32,
            workers: crate::objects::WorkerCount::of_data(-1),
            state: None,
            attribute: Some(attribute::GOLD),
        });
        let b = Builder {
            position: Position::new(10.5, 10.5, 0.0),
            radius: Distance::new(0.2),
            building: Position::new(10.0, 10.0, 0.0),
            range: 6,
        };
        let mut gold = tree(7, 14.5, 10.5);
        gold.class = class::CLASS_32;
        let wood = tree(8, 12.5, 10.5);
        let all = [gold, wood];
        let objects = |tile: Tile| {
            Ok(all
                .iter()
                .filter(|t| t.position.tile() == tile)
                .copied()
                .collect())
        };
        // the wood (2 tiles) is nearer than the gold (4 tiles): it takes over
        let found = more_work(Some(&entry), &b, GridSize::new(40, 40), None, &objects).unwrap();
        assert_eq!(found.map(|o| o.id), Some(object_id(8)));
    }

    /// `FUN_1415cc4e0` walks a tile's objects in its list's order (`+0x28` up to the count
    /// `+0x1b`): of two trees on one tile at one distance with as many workers, the first listed
    /// stays the group's.
    #[test]
    fn more_work_takes_the_first_listed_of_two_trees_on_one_tile() {
        let entry = lumber_camp();
        let b = Builder {
            position: Position::new(10.5, 10.5, 0.0),
            radius: Distance::new(0.2),
            building: Position::new(12.0, 10.0, 0.0),
            range: 6,
        };
        let listed = |first: i32, second: i32| {
            let trees = [tree(first, 13.2, 10.5), tree(second, 13.6, 10.5)];
            let objects = |tile: Tile| {
                Ok(trees
                    .iter()
                    .filter(|t| t.position.tile() == tile)
                    .copied()
                    .collect())
            };
            more_work(Some(&entry), &b, GridSize::new(40, 40), None, &objects)
                .unwrap()
                .map(|o| o.id)
        };
        assert_eq!(listed(9, 3), Some(object_id(9)));
        assert_eq!(listed(3, 9), Some(object_id(3)));
    }

    #[test]
    fn the_resource_search_walks_out_from_the_unit_and_takes_the_first_it_reaches() {
        let seeker = Seeker {
            player: PlayerId::new(1),
            position: Position::new(10.2, 10.7, 0.0),
            line_of_sight: Distance::new(4.0),
            state: ObjectState::Alive,
            current_target: None,
            my_pasture: None,
            min_state: ObjectState::Alive,
        };
        // trees at (12, 10) and (8, 11): the second is nearer the unit's place
        let trees = [tree(1, 12.5, 10.5), tree(2, 8.5, 11.5)];
        let objects = |(): &(), tile: Tile| {
            Ok(trees
                .iter()
                .filter(|t| t.position.tile() == tile)
                .copied()
                .collect())
        };
        let tree_at = |tile: Tile| trees.iter().any(|t| t.position.tile() == tile);
        let mut asked = Vec::new();
        let found = closest_resource(
            &seeker,
            GatherGroup::WOOD,
            crate::objects::WorkerCount::of_data(2),
            ResourceWorld {
                map: GridSize::new(40, 40),
                world: &mut (),
                explored: &|&(), _| Ok(true),
                passable: &|&(), t| !tree_at(t),
                objects: &objects,
                reach: &mut |&mut (), id| {
                    asked.push(id);
                    Ok(true)
                },
            },
        )
        .unwrap();
        assert_eq!((found, asked), (Some(object_id(2)), vec![object_id(2)]));
        // with the tree as the unit AI's target, the other one
        let busy = Seeker {
            current_target: Some(object_id(2)),
            ..seeker
        };
        let found = closest_resource(
            &busy,
            GatherGroup::WOOD,
            crate::objects::WorkerCount::of_data(2),
            ResourceWorld {
                map: GridSize::new(40, 40),
                world: &mut (),
                explored: &|&(), _| Ok(true),
                passable: &|&(), t| !tree_at(t),
                objects: &objects,
                reach: &mut |&mut (), _| Ok(true),
            },
        )
        .unwrap();
        assert_eq!(found, Some(object_id(1)));
    }

    #[test]
    fn a_search_from_a_drop_site_starts_from_its_footprint_then_from_the_unit() {
        let seeker = Seeker {
            player: PlayerId::new(1),
            position: Position::new(10.2, 10.7, 0.0),
            line_of_sight: Distance::new(4.0),
            state: ObjectState::Alive,
            current_target: None,
            my_pasture: None,
            min_state: ObjectState::Alive,
        };
        // a 2x2 site at (14, 10): its own tiles are blocked
        let site = SearchSite {
            position: Position::new(14.0, 10.0, 0.0),
            radius: Footprint::new(1.0, 1.0),
            line_of_sight: Distance::new(6.0),
            footprint: true,
        };
        let search = |trees: &[Candidate], site: Option<&SearchSite>| {
            let objects = |(): &(), tile: Tile| {
                Ok(trees
                    .iter()
                    .filter(|t| t.position.tile() == tile)
                    .copied()
                    .collect())
            };
            let blocked = |tile: Tile| {
                (13..=14).contains(&tile.x()) && (9..=10).contains(&tile.y())
                    || trees.iter().any(|t| t.position.tile() == tile)
            };
            closest_resource_at(
                &seeker,
                site,
                GatherGroup::WOOD,
                crate::objects::WorkerCount::of_data(2),
                ResourceWorld {
                    map: GridSize::new(40, 40),
                    world: &mut (),
                    explored: &|&(), _| Ok(true),
                    passable: &|&(), t| !blocked(t),
                    objects: &objects,
                    reach: &mut |&mut (), _| Ok(true),
                },
            )
            .unwrap()
        };
        // a tree by the unit and one by the site: without the site the unit's, with it the site's
        // (3 from the site's tile)
        let trees = [tree(1, 8.5, 11.5), tree(2, 17.5, 10.5)];
        assert_eq!(search(&trees, None).0, Some(object_id(1)));
        assert_eq!(search(&trees, Some(&site)), (Some(object_id(2)), Some(3)));
        // the site's tile outside the unit's zone: one pass from the site's tile alone, which is
        // blocked, so nothing
        let apart = SearchSite {
            footprint: false,
            ..site
        };
        assert_eq!(search(&trees, Some(&apart)), (None, None));
        // nothing within the site's line of sight plus 10 of it: the second pass from the unit
        // finds the tree 8 columns and a row off (its distance from the unit's tile)
        let blind = SearchSite {
            line_of_sight: Distance::ZERO,
            ..site
        };
        assert_eq!(
            search(&[tree(3, 2.5, 11.5)], Some(&blind)),
            (Some(object_id(3)), Some(9))
        );
    }

    /// The searching unit of `closest_resource_at`'s tests: at (10.2, 10.7), line of sight 4
    /// (its distance at most 9), in state 2.
    const SEEKER: Seeker = Seeker {
        player: PlayerId::new(1),
        position: Position::new(10.2, 10.7, 0.0),
        line_of_sight: Distance::new(4.0),
        state: ObjectState::Alive,
        current_target: None,
        my_pasture: None,
        min_state: ObjectState::Alive,
    };

    /// `closest_resource_at` from [`SEEKER`] over a 40-tile map without a site: the tiles
    /// `explored` gives, every tile passable, `trees` on their tiles, and `reach` true for the
    /// ids it lists.
    fn search_trees(
        seeker: &Seeker,
        trees: &[Candidate],
        max_workers: crate::objects::WorkerCount,
        explored: &Explored<'_, ()>,
        reach: &[ObjectId],
    ) -> Result<(Option<ObjectId>, Option<i32>), GenieError> {
        let objects = |(): &(), tile: Tile| {
            Ok(trees
                .iter()
                .filter(|t| t.position.tile() == tile)
                .copied()
                .collect())
        };
        closest_resource_at(
            seeker,
            None,
            GatherGroup::WOOD,
            max_workers,
            ResourceWorld {
                map: GridSize::new(40, 40),
                world: &mut (),
                explored,
                passable: &|&(), _| true,
                objects: &objects,
                reach: &mut |&mut (), id| Ok(reach.contains(&id)),
            },
        )
    }

    #[test]
    fn a_unit_below_its_least_state_finds_nothing_and_one_with_a_pasture_stops() {
        let trees = [tree(1, 12.5, 10.5)];
        let low = Seeker {
            state: ObjectState::Unfinished,
            ..SEEKER
        };
        assert_eq!(
            search_trees(
                &low,
                &trees,
                crate::objects::WorkerCount::of_data(2),
                &|&(), _| Ok(true),
                &[object_id(1)]
            ),
            Ok((None, None))
        );
        let tending = Seeker {
            my_pasture: Some(PastureId::new(object_id(50))),
            ..SEEKER
        };
        assert_eq!(
            search_trees(
                &tending,
                &trees,
                crate::objects::WorkerCount::of_data(2),
                &|&(), _| Ok(true),
                &[object_id(1)]
            ),
            Err(GenieError::unsupported(
                "a resource search by a unit with a pasture of its own"
            ))
        );
    }

    #[test]
    fn a_full_resource_is_the_fallback_and_minus_1_workers_has_no_cap() {
        // a full tree (3 workers) 2 east, a free one 5 east
        let full = Candidate {
            workers: crate::objects::WorkerCount::of_data(3),
            ..tree(1, 12.5, 10.5)
        };
        let both = [full, tree(2, 15.5, 10.5)];
        let all = |&(): &(), _| Ok(true);
        // at most 2 workers: past the full one to the free one
        assert_eq!(
            search_trees(
                &SEEKER,
                &both,
                crate::objects::WorkerCount::of_data(2),
                &all,
                &[object_id(1), object_id(2)]
            ),
            Ok((Some(object_id(2)), Some(5)))
        );
        // no cap: the full one first
        assert_eq!(
            search_trees(
                &SEEKER,
                &both,
                crate::objects::WorkerCount::of_data(-1),
                &all,
                &[object_id(1), object_id(2)]
            ),
            Ok((Some(object_id(1)), Some(2)))
        );
        // only the full one: the pass ends with it as the fallback
        assert_eq!(
            search_trees(
                &SEEKER,
                &[full],
                crate::objects::WorkerCount::of_data(2),
                &all,
                &[object_id(1)]
            ),
            Ok((Some(object_id(1)), Some(2)))
        );
        // a fallback must be reachable
        assert_eq!(
            search_trees(
                &SEEKER,
                &[full],
                crate::objects::WorkerCount::of_data(2),
                &all,
                &[]
            ),
            Ok((None, None))
        );
    }

    #[test]
    fn an_unexplored_tile_s_resource_is_not_tested() {
        let both = [tree(1, 12.5, 10.5), tree(2, 15.5, 10.5)];
        let dark = |&(): &(), t: Tile| Ok(t != Tile::new(12, 10));
        assert_eq!(
            search_trees(
                &SEEKER,
                &both,
                crate::objects::WorkerCount::of_data(2),
                &dark,
                &[object_id(1), object_id(2)]
            ),
            Ok((Some(object_id(2)), Some(5)))
        );
    }

    #[test]
    fn the_search_settles_after_30_resources_tested() {
        // unreachable trees on the tiles nearest the unit, a reachable one 8 north of it
        let near: Vec<Candidate> = (7..=13)
            .flat_map(|x| (8..=12).map(move |y| (x, y)))
            .enumerate()
            .map(|(i, (x, y))| tree(i as i32 + 1, x as f32 + 0.5, y as f32 + 0.5))
            .collect();
        let far = tree(99, 10.5, 18.5);
        let with = |n: usize| {
            let mut trees: Vec<Candidate> = near[..n].to_vec();
            trees.push(far);
            search_trees(
                &SEEKER,
                &trees,
                crate::objects::WorkerCount::of_data(2),
                &|&(), _| Ok(true),
                &[object_id(99)],
            )
        };
        // 30 tested: the 31st is the far one, taken
        assert_eq!(with(30), Ok((Some(object_id(99)), Some(8))));
        // 31 tested: the pass ends without one
        assert_eq!(with(31), Ok((None, None)));
    }

    #[test]
    fn a_held_resource_names_its_gather_group_and_the_workers_the_build_lets_it_have() {
        assert_eq!(
            gather_group(Some(attribute::WOOD)),
            (GatherGroup::WOOD, crate::objects::WorkerCount::of_data(2))
        );
        assert_eq!(
            gather_group(Some(attribute::FARM_FOOD)),
            (GatherGroup::FORAGE, crate::objects::WorkerCount::of_data(6))
        );
        assert_eq!(
            gather_group(Some(attribute::STONE)),
            (GatherGroup::STONE, crate::objects::WorkerCount::of_data(3))
        );
        assert_eq!(
            gather_group(Some(attribute::GOLD)),
            (GatherGroup::GOLD, crate::objects::WorkerCount::of_data(3))
        );
        assert_eq!(
            gather_group(None),
            (GatherGroup::FISH, crate::objects::WorkerCount::of_data(2))
        );
    }
}
