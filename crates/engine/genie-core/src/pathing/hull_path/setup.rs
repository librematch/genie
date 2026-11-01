//! The pather's construction ([`HullPather`]) and `canQuickPath` (object slot 0x3e0).

use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Rect;
use crate::map::Tile;
use crate::objects::ObjectId;
use crate::pathing::hull_path::NO_AREA;
use crate::players::PlayerId;
use std::collections::BTreeSet;

use crate::GenieError;
use crate::map::spatial::{RecordId, Spatial, tile_record_id};
use crate::math::conversions::cvttss2si;
use crate::pathing::hull_path::{
    ClosedLists, HALF, HullPather, KeptHulls, Nodes, ONE, PATHS, Point, Waypoints,
};

/// A quick path without the map's terrain.
pub(crate) const NO_TERRAIN: GenieError =
    GenieError::unsupported("a quick path without the map's terrain");
/// A quick path of a master without a terrain restriction.
pub(crate) const NO_RESTRICTION: GenieError =
    GenieError::unsupported("a quick path of a master without a terrain restriction");

/// `canQuickPath` (object slot 0x3e0, `FUN_1411506e0`) of object `me` (its master's terrain
/// `mask` and `radius` on x) to object `target` within `range`, from `me`'s place in the world
/// ([`quick_reach_from`]); false for a target the world does not hold.
///
/// # Errors
///
/// Where the pather meets what the slice does not model, or an object the world does not place.
pub fn quick_reach(
    world: &mut Spatial,
    me: ObjectId,
    target: Option<ObjectId>,
    mask: crate::map::TerrainMask,
    radius: Distance,
    range: Distance,
) -> Result<bool, GenieError> {
    let from = world
        .get(me)
        .ok_or(GenieError::unsupported(
            "a quick path of an object without a place",
        ))?
        .position;
    let reach = quick_reach_from(world, me, from.ground(), target, mask, radius, range)?;
    Ok(reach.is_some())
}

/// `canQuickPath` (object slot 0x3e0, `FUN_1411506e0`) of object `me` at `from` (its own x, y)
/// to object `target` within `range`: the old short pather's quick path alone
/// ([`HullPather::find_quick`], `me`'s player, no include list) from `from` to the target's
/// place, ignoring both, finds it there or reaches it (a result below 2). Returns the path's
/// length then (`FUN_140f95a60`, which the slot writes to its `param_4` when given), else None;
/// None for a target the world does not hold. The manager's temporary tile records go after
/// ([`Spatial::clear_tile_records`]).
///
/// # Errors
///
/// Where the pather meets what the slice does not model.
pub fn quick_reach_from(
    world: &mut Spatial,
    me: ObjectId,
    from: Point,
    target: Option<ObjectId>,
    mask: crate::map::TerrainMask,
    radius: Distance,
    range: Distance,
) -> Result<Option<Distance>, GenieError> {
    let Some(t) = world.get_known(target)? else {
        return Ok(None);
    };
    let to = t.position.ground();
    let mut pather = HullPather::new(world, me, target, mask, radius);
    let (result, path) = pather.find_quick(from, to, target, range)?;
    // the slot removes the manager's temporary tile records after (FUN_1410561f0), which the
    // quick path's exit leaves
    world.clear_tile_records();
    Ok(result.reaches().then(|| path.length()))
}

impl<'w> HullPather<'w> {
    /// The pather for unit `unit` (terrain `mask`, `radius`, its player) with the ignore list slot
    /// 0x3e8 sets ([`HullPather::for_list`] with the unit and `target`): both objects (the game's
    /// −1 for no target matches no record, so it is left out), and the tile under one of one
    /// tile on a tile's middle whose terrain the mask lacks.
    ///
    /// The rest is `MShortPather`'s constructor (`FUN_140f84e40`): no hulls (`+0x8`, `+0x10`), the
    /// open lists' heads without links (`+0x28`..`+0x38`, `+0x60`..`+0x70`), two empty closed
    /// lists (`+0x90`, `LAB_140f84e30`), the area -1 (`+0xe8`..`+0xf4`), bounds 0 (`+0x118`,
    /// `+0x120`), four exit and four entrance paths, each empty with length 0 (`+0x108`, `+0x110`,
    /// `LAB_140f850c0`: `FUN_140f8f5d0` with id -1). Its node pool (`+0xb0`, `+0xb8`) has no
    /// counterpart: genie never reuses a node, and `addNode` writes every field of one it reuses.
    /// None of the pather's routines reads `+0xd8` or `+0xe0`. The game keeps one pather per
    /// obstruction manager; of what one search leaves, only the hulls and the area reach the next,
    /// when it keeps them ([`HullPather::find_keeping`]), so a caller that keeps this pather
    /// across searches holds the same state.
    #[must_use]
    pub fn new(
        world: &'w mut Spatial,
        unit: ObjectId,
        target: Option<ObjectId>,
        mask: crate::map::TerrainMask,
        radius: Distance,
    ) -> Self {
        let player = world.get(unit).map(|p| p.player);
        let ids: Vec<ObjectId> = [Some(unit), target].into_iter().flatten().collect();
        Self::for_list(world, &ids, mask, radius, player)
    }

    /// The pather for a unit of terrain `mask`, `radius` and player `player` (`FindShortPath`'s
    /// argument 12) with the ignore list `FUN_140f89e40` sets from the caller's `ids`: each id in
    /// turn (`FUN_140f8a960` appends, no test), then for each listed object the world holds whose
    /// master's radii are 0.5 and 0.5 and whose place lies on a tile's centre (`fmodf` by 1 is
    /// 0.5 on both axes), the tile's record ([`tile_record_id`], of `floorf` of each) when the
    /// tile's word holds a terrain class the mask lacks. The list is on (`+0xc0`) only for ids
    /// given; an empty one ignores nothing either way. The rest is the constructor's state
    /// ([`HullPather::new`]).
    #[must_use]
    pub fn for_list(
        world: &'w mut Spatial,
        ids: &[ObjectId],
        mask: crate::map::TerrainMask,
        radius: Distance,
        player: Option<PlayerId>,
    ) -> Self {
        let mut ignore: Vec<RecordId> = ids.iter().copied().map(RecordId::from).collect();
        for &id in ids {
            let Some(p) = world.get(id) else {
                continue;
            };
            let (x, y) = (p.position.x(), p.position.y());
            if p.radius == Footprint::new(HALF, HALF) && x % ONE == HALF && y % ONE == HALF {
                let (tx, ty) = (cvttss2si(x.floor()), cvttss2si(y.floor()));
                let word = world.tiles().word(Tile::new(tx, ty)).unwrap_or(0);
                if word & mask.blocked() != 0 {
                    ignore.push(tile_record_id(Tile::new(tx, ty)));
                }
            }
        }
        Self {
            world,
            mask,
            radius,
            player,
            gates: None,
            include: Vec::new(),
            buffer: None,
            ignore,
            hulls: Vec::new(),
            nodes: Nodes::heads(),
            closed: ClosedLists::default(),
            area: NO_AREA,
            start: Point::new(0.0, 0.0),
            goal: Point::new(0.0, 0.0),
            exits: vec![Waypoints::default(); PATHS],
            entrances: vec![Waypoints::default(); PATHS],
            bounds: Rect::default(),
        }
    }

    /// The pather with the gates the unit's player may pass (slot 0x678, by id): such a gate in
    /// its area stays out of the hulls.
    #[must_use]
    pub const fn with_gates(mut self, passable: &'w BTreeSet<ObjectId>) -> Self {
        self.gates = Some(passable);
        self
    }

    /// The pather with `FindShortPath`'s include list (its arguments 13 and 14): these objects go
    /// into the hulls even when ignored or moving.
    #[must_use]
    pub fn including(mut self, ids: &[ObjectId]) -> Self {
        self.include = ids.to_vec();
        self
    }

    /// The pather with the area and the hulls a search of the manager's one pather left
    /// ([`HullPather::kept`]), which a kept search (`FindShortPath`'s argument 11,
    /// [`HullPather::find_keeping`]) reuses.
    #[must_use]
    pub fn with_kept(mut self, kept: KeptHulls) -> Self {
        self.area = kept.area;
        self.hulls = kept.hulls;
        self
    }

    /// What this pather's searches leave for the next search of the manager's one pather
    /// (`+0x380`): its area (`+0xe8`) and its hulls (`+0x8`, `+0x10`), the only state a search
    /// reads that an earlier one wrote.
    #[must_use]
    pub fn kept(&self) -> KeptHulls {
        KeptHulls {
            area: self.area,
            hulls: self.hulls.clone(),
        }
    }
}
