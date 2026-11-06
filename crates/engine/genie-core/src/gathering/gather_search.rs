//! The searches for a resource to gather: a builder's search for more work around the building it
//! completed (`FUN_1415cc4e0`, over the tiles `FUN_1415cc1a0` reaches) and the unit AI's
//! `ClosestResourceObject` (`FUN_140c49760`, slot 0x80), which the builder's Build asks next
//! (`0x140ab0370` case 0xdf).

use crate::gathering::PastureId;
use crate::gathering::gather_search::passes::first_resource;
use crate::gathering::gather_search::passes::pass_distance;
use crate::gathering::gather_search::passes::push_neighbours;
use crate::gathering::gather_search::passes::search_passes;
use crate::gathering::gather_search::passes::sort_point_and_side;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::TerrainMask;
use crate::map::Tile;
use crate::map::TileOffset;
use crate::map::spatial::Spatial;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::sqrt;
use crate::math::image::gather_search::TILE_CENTRE as GATHER_SEARCH_TILE_CENTRE;
use crate::math::msvc::sort;
use crate::movement::UnitReach;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::masters::ClassId;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::unit::Unit;
use crate::objects::unit::UnitFlags;
use crate::pathing::tile_path::Planner;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::unit_ai::task_finder::DropSite;
use crate::vision::SightTiles;
use crate::world::layer::AttributeRoles;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::GatherSearchTuning;
use crate::world::objects_ref::ObjectsRef;
use crate::world::slots::CheckedIndexExt;
use crate::{GenieError, GenieResult};
use std::fmt::Debug;
use std::mem::take;

/// An object on a tile's list that the slice does not hold.
pub(crate) const TILE_OBJECT_NOT_HELD: GenieError =
    GenieError::unsupported("an object on a tile the slice does not hold");

mod passes;

/// What the searches read of an object in a map tile's list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    /// `+8`.
    pub(crate) id: ObjectId,
    /// The owner.
    pub(crate) player: PlayerId,
    /// Its master's class (`+0x410`, decoded).
    pub(crate) class: ClassId,
    /// Its master's id.
    pub(crate) master: MasterId,
    /// `+0x640`.
    pub(crate) state: ObjectState,
    /// `+0x644`, `worker_num`.
    pub(crate) workers: WorkerCount,
    /// x, y, z.
    pub(crate) position: Position,
    /// `+0x790`, `parent_pasture_id`.
    pub(crate) pasture: Option<PastureId>,
    /// `+0x689` is 0 ([`crate::objects::unit::UnitFlags::UNTARGETABLE`]).
    pub(crate) untargetable: bool,
    /// `+0x308`, `attribute_type_held`, none for −1.
    pub(crate) attribute: Option<AttributeId>,
    /// `+0x310`, `attribute_amount_held`.
    pub(crate) amount: Amount,
    /// Its master's `enable_auto_gather` (`+0x681`).
    pub(crate) auto_gather: bool,
    /// Its master's `resource_gather_group` (`+0x683`).
    pub(crate) gather_group: GatherGroup,
}

impl Candidate {
    /// Object `id` of `owner` as a search reads it: `unit` of `master` at `position`.
    #[must_use]
    pub const fn of(
        id: ObjectId,
        owner: PlayerId,
        unit: &Unit,
        master: &Master,
        position: Position,
    ) -> Self {
        Self {
            id,
            player: owner,
            class: master.class,
            master: master.id,
            state: unit.life.state,
            workers: unit.workers,
            position,
            pasture: unit.pasture,
            untargetable: unit.flags.contains(UnitFlags::UNTARGETABLE),
            attribute: unit.life.attribute_kind,
            amount: unit.life.amount,
            auto_gather: master.auto_gather.is_on(),
            gather_group: master.gather_group,
        }
    }
}

/// The objects a search finds on a tile ([`Candidate`]): the spatial index's objects on it in
/// their order, each read from the world's objects by id; the searching unit, out of its slot
/// while it updates, as it stands (`me`).
#[derive(Clone, Copy, Debug)]
pub struct Candidates<'world> {
    /// The world's objects by id.
    pub(crate) objects: ObjectsRef<'world>,
    /// The searching unit while its update holds it out of its slot.
    pub(crate) me: Option<Candidate>,
}

impl Candidates<'_> {
    /// The objects of `spatial` on `tile`, in their order.
    ///
    /// # Errors
    ///
    /// An object on the tile the world does not hold, or of a master its owner's table does not
    /// hold.
    pub fn on(&self, spatial: &Spatial, tile: Tile) -> GenieResult<Vec<Candidate>> {
        spatial
            .objects_on(tile)
            .map(|(id, placed)| {
                if let Some(me) = self.me.filter(|me| me.id == id) {
                    return Ok(me);
                }
                let (owner, unit, master) =
                    self.objects.with_master(id)?.ok_or(TILE_OBJECT_NOT_HELD)?;
                Ok(Candidate::of(id, owner, unit, master, placed.position))
            })
            .collect()
    }
}

/// What a builder's search reads of the builder and the building it completed.
#[derive(Clone, Copy, Debug)]
pub struct Builder {
    /// The builder's place.
    pub(crate) position: Position,
    /// Its master's x radius (`+0x5d8`).
    pub(crate) radius: Distance,
    /// The building's place.
    pub(crate) building: Position,
    /// The search's reach in tiles: the building's line of sight held to 4 to 20 (`DAT_142eb0f48`,
    /// `DAT_142e7f38c`), truncated.
    pub(crate) range: i16,
}

/// `FUN_1415cc4e0` with `param_5` and `param_6` 0: the object the builder works next, from the
/// drop site's entry for the building, the builder's class and its owner's civ (`entry`,
/// [`crate::unit_ai::task_finder::drop_site`]; none, or one without `auto_work`, finds nothing). The search
/// covers the tiles within the reach plus the builder's radius (rounded) of the building's tile,
/// cut to the map; of those only the tiles the builder reaches count ([`reached`] on its
/// restriction's `planner`, `FUN_140f7d470`). `candidates` reads a tile's objects in the tile's
/// order from `spatial`.
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
/// A farm on a tile for an entry with a farm target, a tile off the planner's map, and an object
/// `candidates` cannot read.
pub fn more_work(
    entry: Option<&DropSite>,
    builder: &Builder,
    (spatial, planner): (&Spatial, &Planner),
    candidates: &Candidates<'_>,
    roles: &Roles,
) -> GenieResult<Option<Candidate>> {
    let map = spatial.map;
    let Some(entry) = entry.filter(|drop_site| drop_site.auto_work) else {
        return Ok(None);
    };
    // the box: (short)(int) of the building's place, ± the reach plus the builder's radius
    let reach = builder
        .range
        .wrapping_add((builder.radius + Distance::new(0.5)).get() as i32 as i16);
    let (bx, by) = (
        builder.building.x() as i32 as i16,
        builder.building.y() as i32 as i16,
    );
    let x0 = bx.wrapping_sub(reach).max(0);
    let y0 = by.wrapping_sub(reach).max(0);
    let mut x1 = bx.wrapping_add(reach);
    let mut y1 = by.wrapping_add(reach);
    if map.width() <= i32::from(x1) {
        x1 = map.width() as i16 - 1;
    }
    if map.height() <= i32::from(y1) {
        y1 = map.height() as i16 - 1;
    }
    let (width, height) = (i32::from(x1 - x0) + 1, i32::from(y1 - y0) + 1);
    let corner = TileOffset::new(i32::from(x0), i32::from(y0));
    let start = Tile::new(builder.position.x() as i32, builder.position.y() as i32) - corner;
    let grid = reached(start, GridSize::new(width, height), corner, planner)?;
    // gold, stone, food, wood: (the object, its workers, the group's least distance)
    let mut groups: [Option<(Candidate, WorkerCount)>; 4] = [None; 4];
    let mut least = [90.0f32; 4];
    for x in x0..=x1 {
        for y in y0..=y1 {
            let (gx, gy) = (i32::from(x - x0), i32::from(y - y0));
            if let Some(reached) = &grid
                && !*reached.checked((gx * height + gy) as usize)?
            {
                continue;
            }
            for candidate in candidates.on(spatial, Tile::new(i32::from(x), i32::from(y)))? {
                // The farm class (49), which both searches treat apart.
                if roles.of_class(candidate.class).contains(ClassRoles::FARM) {
                    // its own tests (owner, pasture, the targeting flag, masters 1893 and 1897)
                    // come before the entry's, which takes no farm without a farm target
                    if entry.targets.iter().any(|drop_target| {
                        roles.of_class(drop_target.class).contains(ClassRoles::FARM)
                    }) {
                        return Err(GenieError::unsupported(
                            "a farm in a builder's search for more work",
                        ));
                    }
                    continue;
                }
                if ObjectState::Undead <= candidate.state {
                    continue;
                }
                let Some(resource) =
                    entry.resource_of(candidate.class, candidate.state, candidate.workers)
                else {
                    continue;
                };
                let Some(group) = group_of(resource, roles) else {
                    continue;
                };
                let dx = candidate.position.x() - builder.position.x();
                let dy = candidate.position.y() - builder.position.y();
                let whole_distance = cvttss2si(sqrt(dy * dy + dx * dx)) as f32;
                let workers = candidate.workers.plus(-1).max(WorkerCount::ZERO);
                keep(
                    groups.checked_mut(group)?,
                    least.checked_mut(group)?,
                    (candidate, workers),
                    whole_distance,
                );
            }
        }
    }
    Ok(pick(&groups, &least))
}

/// [`more_work`]'s group of a resource: gold 0, stone 1, food 2, wood 3; None for another.
fn group_of(resource: AttributeId, roles: &Roles) -> Option<usize> {
    let held = roles.of_attribute(resource);
    if held.contains(AttributeRoles::GOLD) {
        Some(0)
    } else if held.contains(AttributeRoles::STONE) {
        Some(1)
    } else if resource.is_food_load() {
        Some(2)
    } else if held.contains(AttributeRoles::WOOD) {
        Some(3)
    } else {
        None
    }
}

/// [`more_work`]'s group keeps the object `candidate` at the distance `distance` (with its
/// `workers`) when it keeps none, or `candidate` is nearer than its least distance `low`, or as
/// near or nearer with fewer workers; the least distance falls to `distance`.
fn keep(
    group: &mut Option<(Candidate, WorkerCount)>,
    low: &mut f32,
    (candidate, workers): (Candidate, WorkerCount),
    distance: f32,
) {
    let take = match *group {
        Some((_, kept)) if *low <= distance => workers < kept,
        _ => true,
    };
    if take {
        *group = Some((candidate, workers));
        if distance < *low {
            *low = distance;
        }
    }
}

/// [`more_work`]'s pick: gold, then stone, food and wood, each at a lesser least distance.
fn pick(groups: &[Option<(Candidate, WorkerCount)>; 4], least: &[f32; 4]) -> Option<Candidate> {
    let mut best: Option<(Candidate, f32)> = None;
    for (group, &low) in groups.iter().zip(least) {
        if let Some((candidate, _)) = *group
            && best.is_none_or(|(_, distance)| low < distance)
        {
            best = Some((candidate, low));
        }
    }
    best.map(|(candidate, _)| candidate)
}

/// `FUN_1415cc1a0`: the cells of a box of `size` whose corner is the map's tile `corner` a flood
/// from `start` reaches, by columns (`[x * height + y]`): every cell next to (left, right,
/// above, below) a cell it entered whose tile's cell in `planner`'s tile level has edges
/// ([`Planner::tile_closed`]), `start` entered whatever it holds. None when `start` lies outside
/// the box.
///
/// # Errors
///
/// A tile off the planner's map.
pub fn reached(
    start: Tile,
    size: GridSize,
    corner: TileOffset,
    planner: &Planner,
) -> GenieResult<Option<Vec<bool>>> {
    if !size.holds(start) {
        return Ok(None);
    }
    // by columns, unlike the grid's own rows
    let cell = |tile: Tile| (tile.x() * size.height() + tile.y()) as usize;
    let mut seen = vec![false; (size.width() * size.height()) as usize];
    let Some(start_seen) = seen.get_mut(cell(start)) else {
        return Ok(None);
    };
    *start_seen = true;
    let mut todo = vec![start];
    let mut first = true;
    while let Some(open) = todo.pop() {
        if !take(&mut first) && planner.tile_closed(open + corner)? {
            continue;
        }
        for tile in open.beside() {
            if size.holds(tile)
                && let Some(tile_seen) = seen.get_mut(cell(tile))
                && !*tile_seen
            {
                *tile_seen = true;
                todo.push(tile);
            }
        }
    }
    Ok(Some(seen))
}

/// What `ClosestResourceObject` reads of the unit that searches.
#[derive(Clone, Copy, Debug)]
pub struct Seeker {
    /// The owner.
    pub(crate) player: PlayerId,
    /// The unit's place.
    pub(crate) position: Position,
    /// Its master's line of sight (`+0x5d0`).
    pub(crate) line_of_sight: Distance,
    /// `+0x640`.
    pub(crate) state: ObjectState,
    /// Its unit AI's current target (`+0x48`), which the search passes over.
    pub(crate) current_target: Option<ObjectId>,
    /// `+0x794`, `my_pasture_id`.
    pub(crate) my_pasture: Option<PastureId>,
    /// `param_6`: below this state the unit finds nothing (2 for the unit AIs' searches, 0 for
    /// a mule cart's).
    pub(crate) min_state: ObjectState,
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
    pub(crate) position: Position,
    /// Its master's radii (`+0x5d8`, `+0x5dc`).
    pub(crate) radius: Footprint,
    /// Its master's line of sight (`+0x5d0`).
    pub(crate) line_of_sight: Distance,
    /// The search starts from the site's footprint: the site's tile lies in the unit's zone (its
    /// restriction's zone of the tile, `FUN_1411958c0`, is the unit's own, `FUN_14118eea0`), or
    /// the site's master is 45 or 2172 (the docks). Else it starts from the site's tile alone.
    pub(crate) footprint: bool,
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

/// What [`closest_resource`] and [`closest_resource_at`] read of the world (`FUN_140c49760`'s
/// arguments past the unit, the site and the kind).
#[derive(Debug)]
pub struct ResourceWorld<'world, 'plan> {
    /// The spatial index: the map's size, the searching player's explored tiles, the tiles a
    /// unit can stand on and the objects on a tile; the reach test writes the obstruction
    /// manager's scratch in it.
    pub(crate) spatial: &'world mut Spatial,
    /// The unit's terrain mask, whose tiles it can stand on ([`Spatial::standable`],
    /// `FUN_141185690`).
    pub(crate) mask: TerrainMask,
    /// The objects on a tile.
    pub(crate) candidates: Candidates<'world>,
    /// Whether the unit reaches an object (slot 0x3d0 with the range 0).
    pub(crate) reach: UnitReach<'world, 'plan>,
}

/// `ClosestResourceObject` (`FUN_140c49760`) with no point (`param_7`, `param_8` −1: the unit's
/// place), no drop site (`param_3` −1) and `param_6` 2: the closest object of gather group `kind`
/// that the unit reaches ([`UnitReach`], slot 0x3d0 with the range 0: `FUN_14152b6e0` holds in the
/// games the slice runs), −1 for none. A unit below state 2 is not modelled.
///
/// A breadth-first walk over the tiles from the unit's tile, its queue after the tile in hand kept
/// sorted by the distance of the tiles' centres from the unit
/// ([`msvc::sort`](crate::math::msvc::sort)): a tile the player has explored
/// ([`Spatial::explored`]), within the line of sight plus 5 of the unit (`1.4` per diagonal step,
/// `+ 0.61`, truncated) and that the unit can stand on ([`Spatial::standable`], `FUN_141185690`)
/// adds its four neighbours not seen yet that lie on the map. On each such tile the first object in
/// a pasture or targetable whose master gathers by itself (`enable_auto_gather` 1) as `kind`, with
/// some amount held, and that is not the unit AI's current target, is tested: with fewer workers
/// than `max_workers` (−1 for any) a reach makes it the answer; else a reach makes it the fallback
/// when it is fewer steps away than the last. After 30 tests, or the queue's end, the fallback is
/// the answer. A tile whose last object fails the first test counts it as found (the game's loop
/// leaves it in hand).
///
/// # Errors
///
/// A unit with a pasture of its own, or a tile whose order of objects decides (two candidates,
/// or another object beside one that fails the first test).
pub fn closest_resource(
    seeker: &Seeker,
    kind: GatherGroup,
    max_workers: WorkerCount,
    reads: ResourceWorld<'_, '_>,
    gather_search_tuning: &GatherSearchTuning,
) -> GenieResult<Option<ObjectId>> {
    closest_resource_at(seeker, None, kind, max_workers, reads, gather_search_tuning)
        .map(|(id, _)| id)
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
pub fn closest_resource_at(
    seeker: &Seeker,
    site: Option<&SearchSite>,
    kind: GatherGroup,
    max_workers: WorkerCount,
    reads: ResourceWorld<'_, '_>,
    gather_search_tuning: &GatherSearchTuning,
) -> GenieResult<(Option<ObjectId>, Option<i32>)> {
    /// The fallback's steps before it has one (`1000000`).
    const NO_STEPS: i32 = 1_000_000;
    const TILE_CENTRE: f32 = GATHER_SEARCH_TILE_CENTRE;
    let ResourceWorld {
        spatial,
        mask,
        candidates,
        mut reach,
    } = reads;
    let map = spatial.map;
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
    let (sort_at, side) = sort_point_and_side(Point::new(ux, uy), los, site, gather_search_tuning);
    let half = side / 2;
    let nearer = |one: Cell, other: Cell| {
        let distance_of = |cell: Cell| {
            let dx = sort_at.0 - (cell.tile.x() as f32 + TILE_CENTRE);
            let dy = sort_at.1 - (cell.tile.y() as f32 + TILE_CENTRE);
            dy * dy + dx * dx
        };
        distance_of(one) < distance_of(other)
    };
    let passes = search_passes(
        site,
        map,
        &|tile| spatial.standable(tile, mask),
        unit_tile,
        los,
        &nearer,
    )?;
    let cap = if max_workers == WorkerCount::NO_CAP {
        WorkerCount::of_data(i32::MAX)
    } else {
        max_workers
    };
    let (mut fallback, mut fallback_steps, mut fallback_d, mut tested) = (None, NO_STEPS, 0, 0);
    for pass in passes {
        let (ox, oy) = (half - pass.origin.x(), half - pass.origin.y());
        let mut seen = vec![false; (side * side) as usize];
        let at = |tile: Tile| ((tile.y() + oy) * side + tile.x() + ox) as usize;
        for cell in &pass.seeds {
            *seen.checked_mut(at(cell.tile))? = true;
        }
        let mut queue = pass.seeds;
        let mut index = 0;
        while let Some(&Cell { tile, steps }) = queue.get(index) {
            index += 1;
            if !spatial.explored(seeker.player, tile)? {
                continue;
            }
            let pass_distance = pass_distance(pass.origin, tile, gather_search_tuning);
            if pass.max_steps < pass_distance || !map.holds(tile) {
                continue;
            }
            if spatial.standable(tile, mask) {
                push_neighbours(&mut queue, &mut seen, &at, Cell { tile, steps }, map)?;
                sort(queue.tail_mut(index)?, &nearer)?;
            }
            let on_tile = candidates.on(spatial, tile)?;
            let Some(candidate) = first_resource(&on_tile, kind)? else {
                continue;
            };
            tested += 1;
            if Some(candidate.id) == seeker.current_target {
                continue;
            }
            if candidate.workers < cap {
                if reach.reaches(spatial, candidate.id, Distance::ZERO)? {
                    return Ok((Some(candidate.id), Some(pass_distance)));
                }
            } else if steps < fallback_steps
                && reach.reaches(spatial, candidate.id, Distance::ZERO)?
            {
                fallback = Some(candidate.id);
                fallback_steps = steps;
                fallback_d = pass_distance;
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

/// `FUN_140fdd470` then `FUN_140fdd5f0`: the gather group of a resource held, and the most workers
/// the Build's search for more work lets an object of it have (`0x140ab0c57`): food (0, 15 to 17)
/// forage (1) and 6, wood (0) and 2, stone (3) and 3, gold (4) and 3, the rest fish (2) and 2.
#[must_use]
pub fn gather_group(attribute: Option<AttributeId>, roles: &Roles) -> (GatherGroup, WorkerCount) {
    let kind = {
        let held = roles.of_some_attribute(attribute.and_then(AttributeId::load_resource));
        if held.contains(AttributeRoles::FOOD) {
            GatherGroup::FORAGE
        } else if held.contains(AttributeRoles::WOOD) {
            GatherGroup::WOOD
        } else if held.contains(AttributeRoles::STONE) {
            GatherGroup::STONE
        } else if held.contains(AttributeRoles::GOLD) {
            GatherGroup::GOLD
        } else {
            GatherGroup::FISH
        }
    };
    let workers = WorkerCount::of_data(match kind {
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
    use crate::map::RestrictionId;
    use crate::map::spatial::TileSight;
    use crate::math::rng::Stream;
    use crate::movement::Planning;
    use crate::movement::Speed;
    use crate::objects::MasterHitPoints;
    use crate::objects::masters::AutoGatherMode;
    use crate::objects::masters::MasterKind;
    use crate::objects::masters::ObstructionClass;
    use crate::objects::masters::ObstructionKind;
    use crate::players::PlayerKind;
    use crate::test_ids::class_id;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::testing::attribute;
    use crate::testing::class;
    use crate::testing::ids;
    use crate::testing::roles;
    use crate::testing::scene::walking;
    use crate::testing::scene::{Scene, SceneObject};
    use crate::unit_ai::task_finder::DropTarget;
    use crate::world::World;

    /// The searching villager's id in the resource search's worlds.
    const SEEKER_ID: i32 = 500;

    /// The villager [`SEEKER_ID`] of player 1 at (10.2, 10.7), with its movement state.
    fn seeker() -> SceneObject {
        let villager = Master {
            id: ids::VILLAGER_MALE,
            kind: MasterKind::COMBAT,
            class: class::VILLAGER,
            radius: Footprint::new(0.2, 0.2),
            height: Distance::new(1.0),
            hit_points: MasterHitPoints::of_data(25),
            terrain_restriction: Some(RestrictionId::named(7)),
            move_stats: Some(walking(Speed::new(0.8))),
            ..Master::default()
        };
        SceneObject::new(
            SEEKER_ID,
            PlayerId::new(1),
            villager,
            Position::new(10.2, 10.7, 0.0),
        )
    }

    /// Gaia's resource `id` of master `master` and class `class` at (x, y), of radius `radius`,
    /// gathered by itself as `group`, holding 100 of `attribute` with `workers` at work on it.
    fn resource(
        (id, master, class, kind): (i32, i16, ClassId, MasterKind),
        obstruction_kind: ObstructionKind,
        (group, attribute): (GatherGroup, AttributeId),
        (x, y, radius): (f32, f32, f32),
        workers: WorkerCount,
    ) -> SceneObject {
        let resource_master = Master {
            id: master_id(master),
            kind,
            class,
            radius: Footprint::new(radius, radius),
            height: Distance::new(1.0),
            hit_points: MasterHitPoints::of_data(10),
            terrain_restriction: Some(RestrictionId::named(7)),
            auto_gather: AutoGatherMode::ON,
            gather_group: group,
            obstruction_kind,
            obstruction_class: ObstructionClass::RESOURCE,
            ..Master::default()
        };
        SceneObject {
            holds: Some((attribute, Amount::new(100.0))),
            workers,
            ..SceneObject::new(
                id,
                PlayerId::GAIA,
                resource_master,
                Position::new(x, y, 0.0),
            )
        }
    }

    /// Gaia's tree `id` (master 349, class 15, radius 0.5) at (x, y): 100 wood, nobody at work.
    fn tree(id: i32, x: f32, y: f32) -> SceneObject {
        resource(
            (id, 349, class::TREE, MasterKind::TREE),
            ObstructionKind::KIND_2,
            (GatherGroup::WOOD, attribute::WOOD),
            (x, y, 0.5),
            WorkerCount::ZERO,
        )
    }

    /// Gaia's fish `id` (master 53, class 5, radius 0.25) at (x, y): 100 food, nobody fishing.
    fn fish(id: i32, x: f32, y: f32) -> SceneObject {
        resource(
            (id, 53, class_id(5), MasterKind::STATIC),
            ObstructionKind::NONE,
            (GatherGroup::FISH, attribute::FOOD),
            (x, y, 0.25),
            WorkerCount::ZERO,
        )
    }

    /// The lumber camp's drop-site entry: villagers bring wood, the camp works trees by itself.
    fn lumber_camp() -> DropSite {
        DropSite {
            building: master_id(562),
            worker_class: Some(class::VILLAGER),
            workers: Vec::new(),
            civs: Vec::new(),
            auto_work: true,
            targets: vec![DropTarget {
                class: class::TREE,
                workers: WorkerCount::of_data(-1),
                state: None,
                attribute: Some(attribute::WOOD),
            }],
            livestock: false,
            resource_kinds: vec![attribute::WOOD],
        }
    }

    /// A map of 40 by 40 with the seeking villager and `objects`, for the test to change before
    /// it builds the world.
    fn resource_scene(objects: Vec<SceneObject>) -> Scene {
        objects.into_iter().fold(
            Scene::open(GridSize::new(40, 40)).object(seeker()),
            Scene::object,
        )
    }

    /// [`closest_resource_at`] for the villager [`SEEKER_ID`] of `world` as `seeker` (its place as
    /// the world holds it) from `site`, of group `kind` with fewer than `cap` workers.
    fn search_world(
        world: &mut World,
        seeker: &Seeker,
        site: Option<&SearchSite>,
        (kind, cap): (GatherGroup, WorkerCount),
    ) -> GenieResult<(Option<ObjectId>, Option<i32>)> {
        let id = object_id(SEEKER_ID);
        let objects = ObjectsRef::new(
            &world.players,
            &world.index,
            &world.model.masters,
            &world.parts,
            &world.model.roles,
            &world.model.tuning,
        );
        let (owner, unit, _) = objects
            .with_master(id)?
            .ok_or(GenieError::missing_data("the seeking villager"))?;
        let mover = world
            .parts
            .mover_of(id)
            .ok_or(GenieError::missing_data("the villager's movement state"))?;
        let walker = unit.me(
            world.parts.actions_of(id),
            world.top_speed_terms(owner, unit)?,
        );
        let mut plan = Planning {
            pathing: &mut world.pathing,
            now: world.world_time,
            player_kind: PlayerKind::default(),
            object: world.streams.get_mut(Stream::Object),
            groups: &world.groups,
        };
        closest_resource_at(
            seeker,
            site,
            kind,
            cap,
            ResourceWorld {
                spatial: &mut world.spatial,
                mask: mover.stats.restriction_mask,
                candidates: Candidates { objects, me: None },
                reach: UnitReach {
                    mover,
                    at: unit.position(),
                    walker,
                    master: unit.life.master_id,
                    plan: &mut plan,
                    objects,
                },
            },
            &world.model.tuning.gather_search,
        )
    }

    /// The trees `ids` at the places `at`.
    fn trees(ids_at: &[(i32, (f32, f32))]) -> Vec<SceneObject> {
        ids_at.iter().map(|&(id, (x, y))| tree(id, x, y)).collect()
    }

    /// `FUN_1415cc1a0`'s flood over a box of 5 by 3 at the map's corner, with rocks on its column
    /// 2: from (0, 1) it reaches the columns 0 and 1 and enters the rocks' cells, which the
    /// planner closes, never the columns 3 and 4. A start outside the box floods nothing.
    #[test]
    fn flood_enters_the_blocked_cells_next_to_what_it_reaches_but_not_past_them() {
        let mut world = Scene::open(GridSize::new(40, 40))
            .rocks((0..3).map(|y| Tile::new(2, y)))
            .build();
        let planner = &*world
            .pathing
            .planner(RestrictionId::named(7), world.spatial.tiles())
            .unwrap()
            .unwrap();
        let corner = TileOffset::new(0, 0);
        let seen = reached(Tile::new(0, 1), GridSize::new(5, 3), corner, planner)
            .unwrap()
            .unwrap();
        let col = |x: i32| {
            (0..3)
                .map(|y| seen[(x * 3 + y) as usize])
                .collect::<Vec<_>>()
        };
        assert_eq!(col(1), [true; 3]);
        assert_eq!(col(2), [true; 3]);
        assert_eq!(col(3), [false; 3]);
        assert!(
            reached(Tile::new(5, 0), GridSize::new(5, 3), corner, planner)
                .unwrap()
                .is_none()
        );
    }

    /// [`more_work`] of the builder `builder` among `objects` on its restriction's planner, as the
    /// world holds them.
    fn more_work_among(
        entry: &DropSite,
        builder: &Builder,
        objects: Vec<SceneObject>,
    ) -> GenieResult<Option<ObjectId>> {
        let mut world = resource_scene(objects).build();
        let objects = ObjectsRef::new(
            &world.players,
            &world.index,
            &world.model.masters,
            &world.parts,
            &world.model.roles,
            &world.model.tuning,
        );
        let planner = world
            .pathing
            .planner(RestrictionId::named(7), world.spatial.tiles())?
            .ok_or(GenieError::missing_data("the villagers' planner"))?;
        let candidates = Candidates { objects, me: None };
        let found = more_work(
            Some(entry),
            builder,
            (&world.spatial, planner),
            &candidates,
            &roles(),
        )?;
        Ok(found.map(|candidate| candidate.id))
    }

    /// The builder of the more-work tests at (10.5, 10.5), its building at `building`, reaching
    /// 6 tiles.
    const fn builder_at(building: Position) -> Builder {
        Builder {
            position: Position::new(10.5, 10.5, 0.0),
            radius: Distance::new(0.2),
            building,
            range: 6,
        }
    }

    #[test]
    fn more_work_takes_the_first_nearest_tree_then_fewer_workers_at_no_lesser_distance() {
        let builder = builder_at(Position::new(12.0, 10.0, 0.0));
        // 3 and 4 both 3 tiles away (truncated): x outer, so 3 (x 7) comes first; 4 is as far
        // with fewer workers than 3's kept 1 and takes over
        let worked = SceneObject {
            workers: WorkerCount::of_data(2),
            ..tree(3, 7.5, 10.5)
        };
        let objects = || vec![worked.clone(), tree(4, 13.5, 10.5)];
        assert_eq!(
            more_work_among(&lumber_camp(), &builder, objects()),
            Ok(Some(object_id(4)))
        );
        let mut off = lumber_camp();
        off.auto_work = false;
        assert_eq!(more_work_among(&off, &builder, objects()), Ok(None));
    }

    #[test]
    fn gold_wins_over_nearer_wood_unless_the_wood_is_nearer_than_the_gold() {
        let mut entry = lumber_camp();
        entry.targets.push(DropTarget {
            class: class::CLASS_32,
            workers: WorkerCount::of_data(-1),
            state: None,
            attribute: Some(attribute::GOLD),
        });
        let builder = builder_at(Position::new(10.0, 10.0, 0.0));
        let gold = resource(
            (7, 66, class::CLASS_32, MasterKind::STATIC),
            ObstructionKind::KIND_2,
            (GatherGroup::GOLD, attribute::GOLD),
            (14.5, 10.5, 0.5),
            WorkerCount::ZERO,
        );
        // the wood (2 tiles) is nearer than the gold (4 tiles): it takes over
        assert_eq!(
            more_work_among(&entry, &builder, vec![gold, tree(8, 12.5, 10.5)]),
            Ok(Some(object_id(8)))
        );
    }

    /// `FUN_1415cc4e0` walks a tile's objects in its list's order (`+0x28` up to the count
    /// `+0x1b`): of two trees on one tile at one distance with as many workers, the first listed
    /// stays the group's.
    #[test]
    fn more_work_takes_the_first_listed_of_two_trees_on_one_tile() {
        let builder = builder_at(Position::new(12.0, 10.0, 0.0));
        let listed = |first: i32, second: i32| {
            more_work_among(
                &lumber_camp(),
                &builder,
                trees(&[(first, (13.2, 10.5)), (second, (13.6, 10.5))]),
            )
        };
        assert_eq!(listed(9, 3), Ok(Some(object_id(9))));
        assert_eq!(listed(3, 9), Ok(Some(object_id(3))));
    }

    #[test]
    fn the_resource_search_walks_out_from_the_unit_and_takes_the_first_it_reaches() {
        // trees at (12, 10) and (8, 11): the second is nearer the unit's place
        let mut world = resource_scene(trees(&[(1, (12.5, 10.5)), (2, (8.5, 11.5))])).build();
        let wood = (GatherGroup::WOOD, WorkerCount::of_data(2));
        assert_eq!(
            search_world(&mut world, &SEEKER, None, wood),
            Ok((Some(object_id(2)), Some(3)))
        );
        // with the tree as the unit AI's target, the other one
        let busy = Seeker {
            current_target: Some(object_id(2)),
            ..SEEKER
        };
        assert_eq!(
            search_world(&mut world, &busy, None, wood),
            Ok((Some(object_id(1)), Some(2)))
        );
    }

    #[test]
    fn search_from_drop_site_starts_from_its_footprint_then_from_the_unit() {
        // player 1's lumber camp of 2 by 2 at (14, 10), whose record blocks its tiles
        let camp_master = Master {
            id: master_id(562),
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            radius: Footprint::new(1.0, 1.0),
            height: Distance::new(2.0),
            hit_points: MasterHitPoints::of_data(600),
            obstruction_kind: ObstructionKind::KIND_2,
            obstruction_class: ObstructionClass::BUILDING,
            ..Master::default()
        };
        let camp = SceneObject::new(
            600,
            PlayerId::new(1),
            camp_master,
            Position::new(14.0, 10.0, 0.0),
        );
        let site = SearchSite {
            position: Position::new(14.0, 10.0, 0.0),
            radius: Footprint::new(1.0, 1.0),
            line_of_sight: Distance::new(6.0),
            footprint: true,
        };
        let search = |trees_at: &[(i32, (f32, f32))], site: Option<&SearchSite>| {
            let mut objects = trees(trees_at);
            objects.push(camp.clone());
            let mut world = resource_scene(objects).build();
            search_world(
                &mut world,
                &SEEKER,
                site,
                (GatherGroup::WOOD, WorkerCount::of_data(2)),
            )
        };
        // a tree by the unit and one by the site: without the site the unit's, with it the site's
        // (3 from the site's tile)
        let both = [(1, (8.5, 11.5)), (2, (17.5, 10.5))];
        assert_eq!(search(&both, None).unwrap().0, Some(object_id(1)));
        assert_eq!(
            search(&both, Some(&site)),
            Ok((Some(object_id(2)), Some(3)))
        );
        // the site's tile outside the unit's zone: one pass from the site's tile alone, which is
        // blocked, so nothing
        let apart = SearchSite {
            footprint: false,
            ..site
        };
        assert_eq!(search(&both, Some(&apart)), Ok((None, None)));
        // nothing within the site's line of sight plus 10 of it: the second pass from the unit
        // finds the tree 8 columns and a row off (its distance from the unit's tile)
        let blind = SearchSite {
            line_of_sight: Distance::ZERO,
            ..site
        };
        assert_eq!(
            search(&[(3, (2.5, 11.5))], Some(&blind)),
            Ok((Some(object_id(3)), Some(9)))
        );
    }

    /// The searching unit of `closest_resource_at`'s tests: the villager [`SEEKER_ID`] at (10.2,
    /// 10.7), line of sight 4 (its distance at most 9), in state 2.
    const SEEKER: Seeker = Seeker {
        player: PlayerId::new(1),
        position: Position::new(10.2, 10.7, 0.0),
        line_of_sight: Distance::new(4.0),
        state: ObjectState::Alive,
        current_target: None,
        my_pasture: None,
        min_state: ObjectState::Alive,
    };

    #[test]
    fn unit_below_its_least_state_finds_nothing_and_one_with_pasture_stops() {
        let mut world = resource_scene(trees(&[(1, (12.5, 10.5))])).build();
        let wood = (GatherGroup::WOOD, WorkerCount::of_data(2));
        let low = Seeker {
            state: ObjectState::Unfinished,
            ..SEEKER
        };
        assert_eq!(search_world(&mut world, &low, None, wood), Ok((None, None)));
        let tending = Seeker {
            my_pasture: Some(PastureId::new(object_id(50))),
            ..SEEKER
        };
        assert_eq!(
            search_world(&mut world, &tending, None, wood),
            Err(GenieError::unsupported(
                "a resource search by a unit with a pasture of its own"
            ))
        );
    }

    #[test]
    fn full_resource_is_the_fallback_and_minus_1_workers_has_no_cap() {
        // a full tree (3 workers) 2 east, a free one 5 east
        let full = SceneObject {
            workers: WorkerCount::of_data(3),
            ..tree(1, 12.5, 10.5)
        };
        let both = || resource_scene(vec![full.clone(), tree(2, 15.5, 10.5)]).build();
        let wood = |cap| (GatherGroup::WOOD, WorkerCount::of_data(cap));
        // at most 2 workers: past the full one to the free one
        assert_eq!(
            search_world(&mut both(), &SEEKER, None, wood(2)),
            Ok((Some(object_id(2)), Some(5)))
        );
        // no cap: the full one first
        assert_eq!(
            search_world(&mut both(), &SEEKER, None, wood(-1)),
            Ok((Some(object_id(1)), Some(2)))
        );
        // only the full one: the pass ends with it as the fallback
        let mut alone = resource_scene(vec![full.clone()]).build();
        assert_eq!(
            search_world(&mut alone, &SEEKER, None, wood(2)),
            Ok((Some(object_id(1)), Some(2)))
        );
        // a fallback must be reachable: a full fish off the shore beside the villager, which it
        // cannot touch from the land
        let full_fish = SceneObject {
            workers: WorkerCount::of_data(3),
            ..fish(3, 10.5, 9.5)
        };
        let mut shore = resource_scene(vec![full_fish])
            .water([Tile::new(10, 9)])
            .build();
        assert_eq!(
            search_world(
                &mut shore,
                &SEEKER,
                None,
                (GatherGroup::FISH, WorkerCount::of_data(2))
            ),
            Ok((None, None))
        );
    }

    #[test]
    fn an_unexplored_tile_resource_is_not_tested() {
        let mut world = resource_scene(trees(&[(1, (12.5, 10.5)), (2, (15.5, 10.5))]))
            .sight_on([Tile::new(12, 10)], TileSight::Unknown)
            .build();
        assert_eq!(
            search_world(
                &mut world,
                &SEEKER,
                None,
                (GatherGroup::WOOD, WorkerCount::of_data(2))
            ),
            Ok((Some(object_id(2)), Some(5)))
        );
    }

    /// The fish of the 30 tests' shore: the land is the villager's row 10 (x 2 to 18) and a
    /// causeway north from it (x 10, up to y 17); the water beside the land holds `near` fish the
    /// villager cannot touch, the nearest to it first, and a fish of radius 0.5 at (10.5, 18.5)
    /// at the causeway's end, which it reaches.
    fn shore_of(near: usize) -> World {
        let land = |tile: Tile| {
            (tile.y() == 10 && (2..=18).contains(&tile.x()))
                || (tile.x() == 10 && (10..=17).contains(&tile.y()))
        };
        let tiles = (0..40).flat_map(|y| (0..40).map(move |x| Tile::new(x, y)));
        let water: Vec<Tile> = tiles.clone().filter(|&tile| !land(tile)).collect();
        let shore = |land_tile: Tile| !land(land_tile) && land_tile.beside().into_iter().any(land);
        let dist = |tile: Tile| {
            let (dx, dy) = (tile.x() as f32 + 0.5 - 10.2, tile.y() as f32 + 0.5 - 10.7);
            dx * dx + dy * dy
        };
        let mut beside: Vec<Tile> = tiles
            .filter(|&tile| shore(tile) && tile != Tile::new(10, 18))
            .collect();
        beside.sort_by(|left, right| dist(*left).total_cmp(&dist(*right)));
        let mut objects: Vec<SceneObject> = beside[..near]
            .iter()
            .enumerate()
            .map(|(index, tile)| {
                fish(
                    index as i32 + 1,
                    tile.x() as f32 + 0.5,
                    tile.y() as f32 + 0.5,
                )
            })
            .collect();
        objects.push(resource(
            (99, 53, class_id(5), MasterKind::STATIC),
            ObstructionKind::NONE,
            (GatherGroup::FISH, attribute::FOOD),
            (10.5, 18.5, 0.5),
            WorkerCount::ZERO,
        ));
        resource_scene(objects).water(water).build()
    }

    #[test]
    fn the_search_settles_after_30_resources_tested() {
        let fish = (GatherGroup::FISH, WorkerCount::of_data(2));
        // 30 tested: the 31st is the far one, taken
        assert_eq!(
            search_world(&mut shore_of(30), &SEEKER, None, fish),
            Ok((Some(object_id(99)), Some(8)))
        );
        // 31 tested: the pass ends without one
        assert_eq!(
            search_world(&mut shore_of(31), &SEEKER, None, fish),
            Ok((None, None))
        );
    }

    #[test]
    fn held_resource_names_its_gather_group_and_the_workers_the_build_lets_it_have() {
        assert_eq!(
            gather_group(Some(attribute::WOOD), &roles()),
            (GatherGroup::WOOD, WorkerCount::of_data(2))
        );
        assert_eq!(
            gather_group(Some(attribute::FARM_FOOD), &roles()),
            (GatherGroup::FORAGE, WorkerCount::of_data(6))
        );
        assert_eq!(
            gather_group(Some(attribute::STONE), &roles()),
            (GatherGroup::STONE, WorkerCount::of_data(3))
        );
        assert_eq!(
            gather_group(Some(attribute::GOLD), &roles()),
            (GatherGroup::GOLD, WorkerCount::of_data(3))
        );
        assert_eq!(
            gather_group(None, &roles()),
            (GatherGroup::FISH, WorkerCount::of_data(2))
        );
    }
}
