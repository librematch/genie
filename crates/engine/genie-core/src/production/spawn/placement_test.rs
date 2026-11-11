//! The master's placement test (slot 0x50) as the spawn and the transport call it, and the
//! objects near a candidate. BUILD's call of it is in
//! [`crate::production::spawn::build_placement_test`].

use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Restriction;
use crate::map::{Tile, TileRect};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::HillMode;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::PlacementTerrains;
use crate::players::PlayerId;
use crate::production::spawn::WorldMap;
use crate::production::spawn::{Near, NearFlags};
use crate::production::spawn::{Tested, placement_terrains};
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::SpawnTuning;
use crate::{GenieError, GenieResult};

/// A unit to put down, beside a building or ashore, of a master that does not move (no
/// terrain restriction to test its place by).
pub(crate) const NOT_MOVING: GenieError =
    GenieError::unsupported("a unit to put down of a master that does not move");
/// A terrain the master's terrain restriction does not list.
pub(super) const UNLISTED_TERRAIN: GenieError =
    GenieError::unsupported("a terrain its restriction does not list");

/// The master a placement test places (its slot 0x50 with the spawn's arguments: by its radii,
/// the terrain checked, the player's explored tiles needed, the elevation checked by its hill
/// mode, the objects checked).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placing<'world> {
    /// The unit's player.
    pub(crate) player: PlayerId,
    /// Its master's radii.
    pub(crate) radius: Footprint,
    /// Its master's height radius is above 0.
    pub(crate) tall: bool,
    /// Its master's class.
    pub(crate) class: ClassId,
    /// Its terrain restriction's value per terrain.
    pub(crate) restriction: &'world Restriction,
    /// `hill_mode` (`+0x604`).
    pub(crate) hill_mode: HillMode,
    /// The placement terrains (`+0x5f2`..`+0x5f8`, [`placement_terrains`]).
    pub(crate) placement: PlacementTerrains,
}

/// The tiles within `SpawnTuning::reach` of a point, clamped to the map (`(int)` of each bound, then the map's
/// edges), row by row.
pub(super) fn reach(
    map: GridSize,
    at: Point,
    spawn_tuning: &SpawnTuning,
) -> impl Iterator<Item = Tile> {
    let (x, y) = (at.x(), at.y());
    let x0 = ((x - spawn_tuning.reach) as i32 as i16).max(0);
    let x1 = ((x + spawn_tuning.reach) as i32 as i16).min((map.width() - 1) as i16);
    let y0 = ((y - spawn_tuning.reach) as i32 as i16).max(0);
    let y1 = ((y + spawn_tuning.reach) as i32 as i16).min((map.height() - 1) as i16);
    (y0..=y1).flat_map(move |ty| (x0..=x1).map(move |tx| Tile::new(i32::from(tx), i32::from(ty))))
}

/// An object a placement test's scan found, with its tile.
pub(super) type Found = (Tile, Near);

/// The objects whose box overlaps a box of radii `footprint` at `point` among those `counts` lets
/// through, with their tile, in the scan's order.
pub(super) fn overlapping(
    map: &WorldMap<'_>,
    point: Point,
    footprint: Footprint,
    counts: &dyn Fn(&Near) -> Option<bool>,
    spawn_tuning: &SpawnTuning,
) -> GenieResult<Vec<Found>> {
    let mut out = Vec::new();
    for tile in reach(map.size(), point, spawn_tuning) {
        for near in map.objects(tile)? {
            let Some(tall) = counts(&near) else { continue };
            if !(0.0 < near.radius.x() && 0.0 < near.radius.y() && tall) {
                continue;
            }
            let dx = (near.position.x() - point.x()).abs();
            let dy = (near.position.y() - point.y()).abs();
            if dx < near.radius.x() + footprint.x() && dy < near.radius.y() + footprint.y() {
                out.push((tile, near));
            }
        }
    }
    Ok(out)
}

/// The first object of a scan, which the game reports as the blocker: the tiles row by row, each
/// tile's objects in its list's order ([`crate::map::spatial::Spatial::objects_on`], the game's: the
/// load files them in the save's order, as the game's load does).
pub(super) fn first(found: &[Found]) -> Option<Near> {
    found.first().map(|&(_, near)| near)
}

/// Whether a placement runs the zone test (`FUN_1415c6400`, the world's `+0x820` mode 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneTest {
    /// The zone mode is on.
    Tested,
    /// It is off.
    Skipped,
}

/// Whether the master's slot 0x50 counts the units (type 70) its own scan meets: its argument
/// `[0x60]` (`param_13`), 1 for a mule cart's spot (`FUN_1411c5b60`), 0 for the spawn's rings
/// and the transport's drop (`FUN_141192d10`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitScan {
    /// 0: every object its scan meets counts.
    Counted,
    /// 1: its scan passes over the units; `FUN_141123f80`'s scan before it still counts them.
    Passed,
}

/// The arguments of a call of the master's slot 0x50 that change what [`placement`] answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacementCall {
    /// Whether it runs the zone test.
    pub(crate) zones: ZoneTest,
    /// `[0x58]` (`param_12`): the object both scans pass over (the inner ring's building), none
    /// for the null pointer.
    pub(crate) passed: Option<ObjectId>,
    /// `[0x60]` (`param_13`): whether its own scan counts the units.
    pub(crate) units: UnitScan,
}

/// The master's slot 0x50 (`0x141264db0`) with the arguments of `call`: `FUN_141123f80`'s test
/// (the map's edge, then per tile of the box the terrain restriction and the explored tiles,
/// then the objects overlapping), then its own scan of the objects overlapping, which reports
/// the first one. Both scans pass over the call's object (`[0x58]`); the own scan passes over
/// the units where the call says so (`[0x60]` 1). With `[0x50]` set (`param_11`, every call of
/// genie's), a master whose radii (`+0x5d8`, `+0x5dc`) are both at most 0 is free whatever
/// `FUN_141123f80` found: off the map, on a forbidden terrain, unexplored. A NaN radius is not
/// at most 0: it takes both scans, whose overlap test it fails. The editor's case (the game's
/// mode 4), the hill modes, the zone test (`FUN_1415c6400`, the world's `+0x820` mode 1), the
/// call's `[0x60]` 2 (the units whose slot 0x1f8 is above 0 passed) and the moving objects' own
/// check (`DAT_143f01833`, probed 0) are not modelled.
pub(crate) fn placement(
    me: &Placing<'_>,
    map: &WorldMap<'_>,
    point: Point,
    call: PlacementCall,
    roles: &Roles,
    spawn_tuning: &SpawnTuning,
) -> GenieResult<Tested> {
    let (x, y) = (point.x(), point.y());
    let (rx, ry) = (me.radius.x(), me.radius.y());
    // without radii the slot answers free whatever FUN_141123f80 found; a NaN radius takes
    // the scans
    if !me.radius.has_radii() {
        return Ok(Tested::Free);
    }
    let size = map.size();
    let (width, height) = (size.width(), size.height());
    let (far_x, far_y) = (
        (x + rx) - spawn_tuning.inside,
        (y + ry) - spawn_tuning.inside,
    );
    // FUN_141123f80's own result: 0 free; the objects of its scan report the blocker (6)
    let mut base_blocker = None;
    let off_map = x - rx < 0.0 || width as f32 <= far_x || y - ry < 0.0 || height as f32 <= far_y;
    let base_free = if off_map
        || !placement_terrains(
            me.placement,
            map,
            point,
            TileRect::new((x - rx) as i32, (y - ry) as i32, far_x as i32, far_y as i32),
        )? {
        false
    } else {
        let (x0, y0, x1, y1) = ((x - rx) as i32, (y - ry) as i32, far_x as i32, far_y as i32);
        let mut passable = true;
        let mut seen = false;
        'tiles: for tx in x0..=x1 {
            for ty in y0..=y1 {
                let tile = Tile::new(tx, ty);
                let terrain_id = map.terrain(tile)?;
                let value = me.restriction.value(terrain_id).ok_or(UNLISTED_TERRAIN)?;
                if value <= spawn_tuning.passable {
                    passable = false;
                    break 'tiles;
                }
                seen = seen || map.explored(me.player, tile);
                if me.hill_mode != HillMode::ANY {
                    return Err(GenieError::unsupported("a placement test of a hill mode"));
                }
            }
        }
        if !passable || !seen {
            false
        } else {
            // its scan's gate is the slot's, which a NaN radius passes too (`JC`)
            if call.zones == ZoneTest::Tested {
                return Err(GenieError::unsupported("a placement test in a zone mode"));
            }
            let counts = |near: &Near| {
                let listed = call.passed != Some(near.id)
                    && near.state < ObjectState::Removed
                    && !near.buildable_on.passed()
                    && (near.kind != MasterKind::BUILDING
                        || !near.flags.contains(NearFlags::DESOLID));
                let tall = near.flags.contains(NearFlags::TALL)
                    || (roles.of_class(me.class).contains(ClassRoles::TREE)
                        && roles.of_class(near.class).contains(ClassRoles::FARM))
                    || (near.kind == MasterKind::BUILDING
                        && near.obstruction_kind != ObstructionKind::NONE);
                listed.then_some(tall && me.tall)
            };
            let found = if me.tall {
                overlapping(map, point, me.radius, &counts, spawn_tuning)?
            } else {
                Vec::new()
            };
            base_blocker = first(&found);
            base_blocker.is_none()
        }
    };
    // slot 0x50's own scan
    let counts = |near: &Near| {
        let listed = call.passed != Some(near.id)
            && !(call.units == UnitScan::Passed && near.kind == MasterKind::COMBAT)
            && near.state < ObjectState::Removed
            && !near.buildable_on.passed()
            && (near.kind != MasterKind::BUILDING
                || roles
                    .of_class(near.class)
                    .contains(ClassRoles::PACKED_TREBUCHET)
                || roles
                    .of_class(near.class)
                    .contains(ClassRoles::UNPACKED_TREBUCHET)
                || !near.flags.contains(NearFlags::DESOLID));
        let tall = near.flags.contains(NearFlags::TALL)
            || (near.kind == MasterKind::BUILDING && near.obstruction_kind.recorded());
        listed.then_some(tall && me.tall)
    };
    let found = if me.tall {
        overlapping(map, point, me.radius, &counts, spawn_tuning)?
    } else {
        Vec::new()
    };
    let blocker = first(&found).or(base_blocker);
    if base_free && found.is_empty() {
        return Ok(Tested::Free);
    }
    Ok(Tested::Blocked {
        stays: blocker.is_some_and(|near| near.stays(roles)),
    })
}
