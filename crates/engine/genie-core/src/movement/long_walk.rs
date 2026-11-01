//! A step's reach of its waypoint, the long pather's walker and the units whose tiles it lowers.

use crate::map::Distance;
use crate::map::Tile;
use crate::movement::StepPlans;
use crate::objects::ObjectId;
use crate::objects::masters::ObstructionType;
use crate::pathing::long_path::gate_lets_pass;
use crate::players::PlayerId;

use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::map::Rect;
use crate::map::spatial::{Placed, RecordId, Spatial};
use crate::math::fmath::BVector;
use crate::movement::{
    EPSILON, INCLUDED, NEW_PATHER_RADIUS, PLANS, Planning, footprint_gap, footprint_gap_squared,
};
use crate::pathing::hull_path::HullPather;
use crate::pathing::short_path::{Call, Entry, Listed};
use crate::pathing::tile_path::{Found, Path, Request};
use crate::pathing::{Pathing, PlanBudget, ShortFound};
use crate::{GenieError, math::fmath, pathing::short_path};

/// How a step relates to its waypoint (slot 196, `0x14114f400`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reach {
    /// 0: the step ends short of it.
    Short,
    /// 1: the step reaches or passes it.
    Reached,
    /// 2: the unit moves away from it.
    Away,
}

/// Compare the distances on the ground: waypoint to position, waypoint to the step's end, and the
/// step's own length.
pub(super) fn reach(waypoint: BVector, position: BVector, projected: BVector) -> Reach {
    let mut now = fmath::sub(waypoint, position);
    let mut after = fmath::sub(waypoint, projected);
    let step = fmath::sub(projected, position);
    now = now.with_height(0.0);
    after = after.with_height(0.0);
    let (now, after, step) = (
        fmath::length(now),
        fmath::length(after),
        fmath::length(step),
    );
    // COMISS eps, after; JA: only an ordered 1e-7 above `after` reaches at once
    if fmath::at_most_or_unordered(EPSILON, after) {
        // COMISS step, now; JBE: short also when either is NaN
        if fmath::at_most_or_unordered(step, now) {
            return Reach::Short;
        }
        if now < after {
            return Reach::Away;
        }
    }
    Reach::Reached
}

/// What the long pather reads of its unit (`FUN_140f9eb70`'s argument 6): its id, its own place
/// (the target's gaps are from it), its radii on the ground, its restriction's mask, whether it
/// moves (its speed, slot 0x208, is not 0, or its master is 42) and its ignore units (`+0x670`).
#[derive(Clone, Debug)]
pub(crate) struct LongWalker<'a> {
    pub(crate) id: ObjectId,
    pub(crate) position: Position,
    pub(crate) radius: Footprint,
    pub(crate) mask: crate::map::TerrainMask,
    pub(crate) moves: bool,
    pub(crate) ignore_units: &'a [ObjectId],
    /// For a walker in a group the world holds (`+0x6d0`, `FUN_14152ff40`), its group's
    /// differentiator units (`FUN_14100bbe0` with 255, [`crate::groups::UnitGroups::group_units`]).
    pub(crate) group_units: Option<Vec<ObjectId>>,
}

/// The units whose tiles the long pather lowers (`FUN_140f9eb70`'s list for `FUN_140f60440`):
/// the walker and its ignore units that wait to move or move (slots 0x2a8 and 0x5c8), not the
/// target, at most 254; for a walker in a group the units of the group's differentiator groups
/// instead ([`LongWalker::group_units`]); then the target.
pub(crate) fn lowered_units(
    walker: &LongWalker<'_>,
    world: &Spatial,
    target: Option<ObjectId>,
    by_id: crate::world::index::ObjectsRef<'_>,
) -> Vec<ObjectId> {
    /// The list's room for the walker's ignore units (`0xfe < lVar26` breaks).
    const IGNORED: usize = 254;
    let own = || {
        std::iter::once(walker.id)
            .chain(
                walker
                    .ignore_units
                    .iter()
                    .copied()
                    .filter(|&id| {
                        id != walker.id
                            && Some(id) != target
                            && world.get(id).is_some()
                            && by_id.gait(id).is_some_and(|g| g.waits || g.moving)
                    })
                    .take(IGNORED),
            )
            .collect()
    };
    let mut listed: Vec<ObjectId> = walker.group_units.clone().unwrap_or_else(own);
    if let Some(id) = target {
        listed.push(id);
    }
    listed
}

/// The long pather (`FUN_140f9eb70`) for `walker` from `from` (x, y on the map, as tiles) to the
/// tile of `to` within `range` of `target`: whether it reaches the goal, and the path at the
/// walker's height. The search lowers the tall counts of the tiles of its list
/// (`FUN_140f60440`): the walker and its ignore units that wait to move or move (slots 0x2a8 and
/// 0x5c8; not the target, at most 254), or, for a walker in a group, the units of the group's
/// differentiator groups instead ([`lowered_units`]); then the
/// target. Each listed object the world holds whose master has a height lowers its tile.
///
/// # Errors
///
/// What the search does not model.
pub(crate) fn long_path_for(
    walker: &LongWalker<'_>,
    from: Point,
    world: &Spatial,
    computer: bool,
    (to, range): (BVector, f32),
    target: Option<(ObjectId, &Placed)>,
    by_id: crate::world::index::ObjectsRef<'_>,
) -> Result<(bool, Path), GenieError> {
    let long_target = target.map(|(_, t)| {
        let gap2 = footprint_gap_squared(walker.position, walker.radius, t.position, t.radius);
        let gap = footprint_gap(walker.position, walker.radius, t.position, t.radius);
        crate::pathing::long_path::LongTarget::new(t.position, t.radius, gap, gap2)
    });
    let listed = lowered_units(walker, world, target.map(|(id, _)| id), by_id);
    let lowered = listed
        .into_iter()
        .filter_map(|id| world.get(id))
        .filter(|p| p.tall)
        .map(|p| Tile::new(p.position.x() as i32, p.position.y() as i32))
        .collect();
    let req = crate::pathing::long_path::LongRequest {
        from: Tile::new(from.x() as i32, from.y() as i32),
        to: Tile::new(to.x() as i32, to.y() as i32),
        range: Distance::new(range),
        target: long_target,
        moves: walker.moves,
        mask: walker.mask,
        computer,
        lowered,
        within_range: false,
        near_player: false,
        allies: world
            .get(walker.id)
            .map_or(crate::players::PlayerSet::NONE, |p| {
                world.allies_of(p.player)
            }),
    };
    let player = world.get(walker.id).map(|p| p.player);
    let gates = |t| player.map_or(Ok(false), |p| gate_lets_pass(world, by_id, p, t));
    crate::pathing::long_path::path(
        world.tiles(),
        world.map,
        (&req, &gates),
        walker.position.z(),
    )
}

/// What the tile pathfinder is asked besides its walker ([`tile_plan_for`]'s arguments).
#[derive(Clone, Copy, Debug)]
pub struct TileAsk<'a> {
    /// The goal (x, height, y).
    pub goal: BVector,
    /// The walker's and the target's radii added.
    pub radii: Distance,
    /// How near the goal counts.
    pub range: Distance,
    /// The target object's record.
    pub target: Option<&'a Placed>,
}

/// What the short pather is asked besides its walker ([`short_path_including`]'s goal and
/// lists, `FUN_140f9e540`'s arguments 3 to 5 and 6 to 9).
#[derive(Clone, Copy, Debug)]
pub struct ShortAsk<'a> {
    /// The goal (x, height, y).
    pub goal: BVector,
    /// How near the goal counts.
    pub range: Distance,
    /// The target object (if any).
    pub target: Option<ObjectId>,
    /// The objects the pather ignores.
    pub ignore: &'a [ObjectId],
    /// The objects the old pather tests even moving.
    pub include: &'a [ObjectId],
}

/// The tile pathfinder (`FUN_140f7c530`) for `walker` from `from` (x, y on the map) to `to`,
/// its walker's and the target's radii `radii` added, within `range`; appended to `out`,
/// smoothed with the obstruction manager's segment test for the walker (the records of the
/// smoothing's ignore list ignored, [`Pathing::smoothing_ignore`]: the walker and
/// `level_target`, the object the world holds, with a main building's annexes over it); None
/// without a planner for the walker's restriction. The planner reads the first byte of the
/// target's record (`+0x658`): 3, a box, for the obstruction types 2, 10, 11 and 13
/// (`FUN_141056460`). A computer's plan (`plan.computer`) is not full.
///
/// # Errors
///
/// What the planner does not model.
pub(crate) fn tile_plan_for(
    walker: Walker,
    from: Point,
    level_target: Option<ObjectId>,
    (world, by_id): (&mut Spatial, crate::world::index::ObjectsRef<'_>),
    plan: &mut Planning<'_>,
    ask: TileAsk<'_>,
    out: &mut Path,
) -> Result<Option<Found>, GenieError> {
    let TileAsk {
        goal: to,
        radii,
        range,
        target,
    } = ask;
    let target_box = target.and_then(|t| t.obstruction).is_some_and(|o| {
        matches!(
            o.kind,
            ObstructionType::TYPE_2
                | ObstructionType::TYPE_10
                | ObstructionType::TYPE_11
                | ObstructionType::TYPE_13
        )
    });
    let req = Request {
        from,
        to: to.ground(),
        radii,
        range,
        closed: Vec::new(),
        full: !plan.computer,
    };
    let annexes = level_target
        .filter(|&t| by_id.annex_of(t).is_none())
        .map(|t| by_id.annexes(t).ids().collect::<Vec<_>>())
        .unwrap_or_default();
    let (slots, ignore) = plan
        .pathing
        .smoothing_ignore(walker.id, level_target.map(|t| (t, annexes.as_slice())));
    let (radius, mask) = (walker.radius, walker.mask);
    let Some(planner) = plan.pathing.planner(walker.restriction, world.tiles()) else {
        return Ok(None);
    };
    let ignore: Vec<RecordId> = ignore.into_iter().map(RecordId::from).collect();
    let mut walls = crate::map::spatial::SmoothingWalls::new(world, radius, mask, &ignore);
    let found = planner.plan::<GenieError>(&req, target_box, out, Some(&mut walls))?;
    if walls.ran {
        plan.pathing.smoothing_ran(slots);
        // FUN_140f7dbb0 sets the manager's ignore list to it (FUN_1410562e0)
        world.set_manager_ignore(&ignore);
    }
    Ok(Some(found))
}

/// `0x141150f00`'s test before a solid unit's plan: a trade cart needs its owner's budget
/// (`FUN_14165d6f0` on its wait delays `+0x7fa`), and no unit makes more than [`PLANS`] plans in
/// one step (`local_19a8 < 3`). A walker without an owner has no budget to test.
pub(super) fn may_plan(
    trade_cart: bool,
    budget: Option<PlanBudget>,
    wait_delays: crate::movement::WaitDelays,
    plans: StepPlans,
) -> bool {
    let allowed = !trade_cart || budget.is_none_or(|b| b.allows(wait_delays.count()));
    allowed && plans < PLANS
}

/// What the planners read of the walking unit (the unit, or a group's lead): its id, radius on
/// x (master `+0x5d8`), terrain restriction (`+0x606`) and that restriction's mask.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Walker {
    pub(crate) id: ObjectId,
    pub(crate) radius: Distance,
    pub(crate) restriction: crate::map::RestrictionId,
    pub(crate) mask: crate::map::TerrainMask,
}

/// The walk's kept hits (`0x141150f00`'s first hit loop, 0x1411547c0..0x141154855): each joins
/// the motion step's include list (`DAT_143f4a000`, at most [`INCLUDED`]) and, without a mesh
/// shape of its own, gets a temporary one there (`FUN_141054fe0` with 1, not the object's: its
/// `+0x64c` stays -1), which the step's every exit removes; one past the list's end gets
/// neither. The new pather's test before the shape (`FUN_14152b6e0`) always holds
/// ([`old_pather_for`]).
pub(super) fn include_kept(
    world: &Spatial,
    pathing: Option<&mut Pathing>,
    kept: &[ObjectId],
    include: &mut Vec<ObjectId>,
) -> Result<(), GenieError> {
    let pathing = pathing.ok_or(GenieError::unsupported(
        "a kept hit without the world's mesh",
    ))?;
    for &id in kept {
        if INCLUDED <= include.len() {
            continue;
        }
        include.push(id);
        if pathing.shape(id).is_none() {
            let p = world
                .get_known(id)?
                .ok_or(GenieError::unsupported("a kept hit without a place"))?;
            if p.obstruction_type == ObstructionType::TYPE_11 {
                return Err(GenieError::unsupported(
                    "a kept hit of obstruction type 11 (its outline)",
                ));
            }
            pathing.add_temporary_shape(p);
        }
    }
    Ok(())
}

/// `FUN_140f9e540`'s dispatch: `COMISS` [`NEW_PATHER_RADIUS`], radius; `JC` (0x140f9e71f): a
/// unit wider than half a tile, or of an unordered radius, takes the old pather. Its other
/// condition, no new pather (`FUN_14152b6e0`: the manager's `+0` or `+8` null), never holds: the
/// manager's setup (`FUN_141054660`) makes both when `FUN_14152bc90` answers 1, and its two
/// switches have no writer (`DAT_1439e60e0` 1 in `.data`, `DAT_143fb54a8` 0 in `.bss`, each read
/// by it alone).
pub(super) fn old_pather_for(radius: Distance) -> bool {
    fmath::below_or_unordered(NEW_PATHER_RADIUS, radius.get())
}

/// The short pather (`FUN_140f9e540`) for `walker` from `from` (x, y on the map) to `to` within
/// `range` of `target`, `ignore` its ignore list, with no include list and its
/// owner's number for the player ([`short_path_including`]).
///
/// # Errors
///
/// As [`short_path_including`].
pub(crate) fn short_path_for(
    walker: Walker,
    from: Point,
    (world, by_id): (&mut Spatial, crate::world::index::ObjectsRef<'_>),
    plan: &mut Planning<'_>,
    goal: (BVector, Distance, Option<ObjectId>),
    ignore: &[ObjectId],
    out: &mut Path,
) -> Result<ShortFound, GenieError> {
    let player = owner_number(world, walker.id);
    let (to, range, target) = goal;
    let ask = ShortAsk {
        goal: to,
        range,
        target,
        ignore,
        include: &[],
    };
    short_path_including(walker, from, (world, by_id), plan, ask, player, out)
}

/// The owner's number (`+0x718` of the object's player, `+0x138`) the short pather takes for
/// the gates' passage; none (-1) for an object the world does not place.
pub(super) fn owner_number(world: &Spatial, id: ObjectId) -> Option<PlayerId> {
    world.get(id).map(|p| p.player)
}

/// The short pather (`FUN_140f9e540`) for `walker` from `from` (x, y on the map) to `to` within
/// `range` of `target` (the game's -1 for none), `ignore` its ignore list (arguments 6 and 7), `include`
/// the objects the old pather tests even moving (8 and 9), `player` the owner's number (13).
/// A start within range of the target (`FUN_14118faf0` less the radius) empties `out` and
/// returns in range; else `out` empties (`+0x440 = +0x438`) and an end off the map returns invalid. A unit
/// wider than half a tile ([`old_pather_for`]) takes the old pather (`FUN_140f89e40` with the
/// ignore list and the restriction's mask, then `FUN_140f85580` from `from` to `to` with the
/// radius, the target, the range, the player and the include list, its argument 11 0:
/// [`HullPather::find`]), whose waypoints `out` takes at height 0; else the new one. Returns
/// the result. The path's creation
/// time (`+8`, the world's time, set past the map tests) is the caller's.
///
/// # Errors
///
/// What the pathers do not model, or the new pather without the game's mesh.
pub(crate) fn short_path_including(
    walker: Walker,
    from: Point,
    (world, by_id): (&mut Spatial, crate::world::index::ObjectsRef<'_>),
    plan: &mut Planning<'_>,
    ask: ShortAsk<'_>,
    player: Option<PlayerId>,
    out: &mut Path,
) -> Result<ShortFound, GenieError> {
    let ShortAsk {
        goal: to,
        range,
        target,
        ignore,
        include,
    } = ask;
    let (x, y) = (from.x(), from.y());
    let target_object = match target {
        None => None,
        Some(id) => world.get_known(id)?.map(|t| short_path::Target {
            id,
            position: t.position.ground(),
            half_size: t.radius,
        }),
    };
    let listed = ignore
        .iter()
        .map(|&id| Listed {
            id,
            shape: plan.pathing.shape(id),
        })
        .collect();
    let call = Call {
        from: Point::new(x, y),
        to: to.ground(),
        radius: walker.radius,
        range,
        target: target_object,
        listed,
        map: world.map,
        mask: walker.mask,
    };
    // the search's box: four tiles around its ends
    let reach = 4.0 + walker.radius.get();
    world.check_known_records(Rect::new(
        x.min(to.x()) - reach,
        y.min(to.y()) - reach,
        x.max(to.x()) + reach,
        y.max(to.y()) + reach,
    ))?;
    let (query, shapes) = match call.entry() {
        Entry::InRange => {
            out.points.clear();
            return Ok(ShortFound::InRange);
        }
        Entry::Invalid => {
            out.points.clear();
            return Ok(ShortFound::Invalid);
        }
        Entry::Search { query, ignore } => (query, ignore),
    };
    out.points.clear();
    if old_pather_for(walker.radius) {
        let mut pather =
            HullPather::for_list((world, by_id), ignore, walker.mask, walker.radius, player)
                .including(include);
        let (result, path) = pather.find(from, to.ground(), target, range)?;
        for p in &path.points {
            let _ = out.push(BVector::new(p.x(), 0.0, p.y()));
        }
        return Ok(result);
    }
    let tiles = world.tiles();
    let words = |x: u32, y: u32| tiles.word(Tile::new(x as i32, y as i32)).unwrap_or(0);
    let (mesh, pather) = plan
        .pathing
        .mesh_and_pather()
        .ok_or(GenieError::unsupported(
            "a short path without the game's mesh",
        ))?;
    let found = pather.search(mesh, &words, &query, &shapes)?;
    for p in short_path::to_path(&found.points).points {
        let _ = out.push(p);
    }
    Ok(found.result)
}
