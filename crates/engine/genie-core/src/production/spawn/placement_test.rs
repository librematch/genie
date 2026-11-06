//! The master's placement test (slot 0x50): the objects near a candidate, BUILD's placement
//! test and the spawn's.

use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Restriction;
use crate::map::{Tile, TileRect};
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::HillMode;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::PlacementTerrains;
use crate::players::AllyTest;
use crate::players::PlayerId;
use crate::production::spawn::PlacementCode;
use crate::production::spawn::SiteChecks;
use crate::production::spawn::WorldMap;
use crate::production::spawn::terrains::NO_ELEVATIONS;
use crate::production::spawn::{BuildSite, Tested, hill_fails, placement_terrains};
use crate::production::spawn::{Near, NearFlags};
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::SpawnTuning;
use crate::{GenieError, GenieResult};

/// A unit to put down, beside a building or ashore, of a master that does not move (no
/// terrain restriction to test its place by).
pub(crate) const NOT_MOVING: GenieError =
    GenieError::unsupported("a unit to put down of a master that does not move");
/// A terrain the master's terrain restriction does not list.
const UNLISTED_TERRAIN: GenieError =
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

/// BUILD's placement test (`FUN_140e7a400`'s call of the master's slot 0x50, the building master's
/// `FUN_14123aee0` with BUILD's flags: `p6` human, `p7`, `p8`, `p9` 1, `p10` 0, `p11` 1, no object
/// of its own, `p13` 0, `p14` 1): 0 when the foundation may go to `point`, else the game's code.
/// First the static test (`FUN_141123f80` through `FUN_141264db0`; their scans run only for the
/// Build's test, [`SiteChecks::SCANS`]) over the box of the site's radii: off the map 7; per tile,
/// columns then rows, a terrain the restriction forbids 2 (`DAT_142eb7e5c`'s threshold, the
/// spawn's); a human who explored none of the tiles 5; a hill mode's tile ([`hill_fails`]) 3. Then
/// the building master's scan of the tiles within 8 (rows, then columns; each tile's objects in its
/// list's order): an object the player sees, or on a tile it never explored (the test's explored
/// flag `+0x730` is set by then), or in the fog with a fog visibility, and only its own in the fog
/// without one, that is below state 3 (another type below 7), standing (or the master obstructs, or
/// it is below type 30), not a wall under its own gate, a desolid building only of an ally both
/// ways, not a moving unit of the player, Gaia or a mutual ally, not a felled tree off a forest (at
/// most 1 hit point; `+0x2e8` taken as its own tile), not buildable on, whose box with a height
/// overlaps the clearance box: 6. Not modelled: the placement terrains, hill modes, the zone test
/// (`FUN_1415c6400`), the editor (game mode 4), a fogged object of fog visibility 3, game type 7's
/// test of allies.
pub(crate) fn build_placement(
    site: &BuildSite<'_>,
    map: &WorldMap<'_>,
    point: Point,
    roles: &Roles,
    spawn_tuning: &SpawnTuning,
) -> GenieResult<PlacementCode> {
    if let Some(code) = build_static_test(site, map, point, spawn_tuning)? {
        return Ok(code);
    }
    if site.checks.contains(SiteChecks::SCANS)
        && static_scans_find(site, map, point, roles, spawn_tuning)?
    {
        return Ok(PlacementCode::Blocked);
    }
    // FUN_14123aee0's scan
    let size = map.size();
    let (width, height) = (size.width(), size.height());
    let ((x0, x1), (y0, y1)) = (
        build_scan_range(point.x(), width, spawn_tuning),
        build_scan_range(point.y(), height, spawn_tuning),
    );
    let player = site.player;
    for ty in y0..=y1 {
        for tx in x0..=x1 {
            let tile = Tile::new(tx, ty);
            let sight = (map.explored(player, tile), map.visible(player, tile));
            for near in map.objects(tile)? {
                if site.own.contains(&near.id) {
                    continue;
                }
                if build_sees(site, map, &near, sight, roles)?
                    && blocks_build(site, map, &near, point, roles)
                {
                    return Ok(PlacementCode::Blocked);
                }
            }
        }
    }
    Ok(PlacementCode::Allowed)
}

/// The static scans of the Build's test (`p11` and `p13` 1, [`SiteChecks::SCANS`]), for a master
/// with a height ([`BuildSite::tall`]) and radii: `FUN_141123f80`'s, then `FUN_141264db0`'s, each
/// over the tiles within `SpawnTuning::reach` (rows, then columns; each tile's objects in its
/// list's order) for an object other than the site's own (`p12`), below state 7, not buildable on,
/// no desolid building (`FUN_141264db0`'s passes classes 51 and 54), with a height
/// (`FUN_141123f80`'s gives a tree's farm and an obstructing building one, `FUN_141264db0`'s an
/// obstructing building; the latter passes over the units, type 70), whose box overlaps the site's:
/// true when one does (the game's code 6).
fn static_scans_find(
    site: &BuildSite<'_>,
    map: &WorldMap<'_>,
    point: Point,
    roles: &Roles,
    spawn_tuning: &SpawnTuning,
) -> GenieResult<bool> {
    let (rx, ry) = (site.radius.x(), site.radius.y());
    if !site.tall || !(0.0 < rx || 0.0 < ry) {
        return Ok(false);
    }
    let first = |near: &Near| {
        let listed = !site.own.contains(&near.id)
            && near.state < ObjectState::Removed
            && !near.buildable_on.passed()
            && (near.kind != MasterKind::BUILDING || !near.flags.contains(NearFlags::DESOLID));
        let tall = near.flags.contains(NearFlags::TALL)
            || (roles.of_class(site.class).contains(ClassRoles::TREE)
                && roles.of_class(near.class).contains(ClassRoles::FARM))
            || (near.kind == MasterKind::BUILDING
                && near.obstruction_kind != ObstructionKind::NONE);
        listed.then_some(tall)
    };
    if !overlapping(map, point, site.radius, &first, spawn_tuning)?.is_empty() {
        return Ok(true);
    }
    let second = |near: &Near| {
        let listed = !site.own.contains(&near.id)
            && near.state < ObjectState::Removed
            && !near.buildable_on.passed()
            && near.kind != MasterKind::COMBAT
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
        listed.then_some(tall)
    };
    Ok(!overlapping(map, point, site.radius, &second, spawn_tuning)?.is_empty())
}

/// BUILD's static test (`FUN_141123f80` through `FUN_141264db0`) over the clearance box at `point`:
/// off the map 7; the placement terrains 1; per tile, columns then rows, a terrain the
/// restriction forbids 2; a hill mode's tile 3; a human who explored none of the tiles 5. None
/// when the box passes; a zone mode is not modelled.
pub(super) fn build_static_test(
    site: &BuildSite<'_>,
    map: &WorldMap<'_>,
    point: Point,
    spawn_tuning: &SpawnTuning,
) -> GenieResult<Option<PlacementCode>> {
    let (x, y) = (point.x(), point.y());
    let (rx, ry) = (site.radius.x(), site.radius.y());
    let size = map.size();
    let (width, height) = (size.width(), size.height());
    let (far_x, far_y) = (
        (x + rx) - spawn_tuning.inside,
        (y + ry) - spawn_tuning.inside,
    );
    if x - rx < 0.0 || width as f32 <= far_x || y - ry < 0.0 || height as f32 <= far_y {
        return Ok(Some(PlacementCode::OffMap));
    }
    let mut explored = false;
    let (x0, x1, y0, y1) = ((x - rx) as i32, far_x as i32, (y - ry) as i32, far_y as i32);
    if !placement_terrains(site.placement, map, point, TileRect::new(x0, y0, x1, y1))? {
        return Ok(Some(PlacementCode::PlacementTerrain));
    }
    // hill mode 3 measures the box's edge against the elevation of its first column's last row
    let base = if site.hill_mode == HillMode::ONE_STEP {
        Some(map.elevation(Tile::new(x0, y1)).ok_or(NO_ELEVATIONS)?)
    } else {
        None
    };
    for tx in x0..=x1 {
        for ty in y0..=y1 {
            let tile = Tile::new(tx, ty);
            let terrain_id = map.terrain(tile)?;
            let value = site.restriction.value(terrain_id).ok_or(UNLISTED_TERRAIN)?;
            if value <= spawn_tuning.passable {
                return Ok(Some(PlacementCode::Terrain));
            }
            explored = explored
                || (site.checks.contains(SiteChecks::HUMAN) && map.explored(site.player, tile));
            if hill_fails(site.hill_mode, map, tile, Tile::new(x0, y1), base)? {
                return Ok(Some(PlacementCode::Hill));
            }
        }
    }
    if site.checks.contains(SiteChecks::HUMAN) && !explored {
        return Ok(Some(PlacementCode::Unexplored));
    }
    if (0.0 < rx || 0.0 < ry) && site.checks.contains(SiteChecks::ZONES) {
        return Err(GenieError::unsupported(
            "BUILD's placement test in a zone mode",
        ));
    }
    Ok(None)
}

/// The tiles BUILD's scan reads along one axis about `coordinate` (`FUN_14123aee0`): within
/// `SpawnTuning::build_scan`, each end truncated into a short, from 0 to the map's last tile of `size`.
pub(super) fn build_scan_range(
    coordinate: f32,
    size: i32,
    spawn_tuning: &SpawnTuning,
) -> (i32, i32) {
    let low = (coordinate - spawn_tuning.build_scan) as i32 as i16;
    let high = (coordinate + spawn_tuning.build_scan) as i32 as i16;
    (
        i32::from(low.max(0)),
        if i32::from(high) < size {
            i32::from(high)
        } else {
            size - 1
        },
    )
}

/// Whether BUILD's scan counts the object `near` by what the player sees of its tile (`seen`
/// explored, `visible`): in sight, any but one of fog visibility 3; in the fog, by its fog
/// visibility (one of 0 only when it is the player's own or allied, or a building whose
/// visibility mask meets both the player's masks); on a tile never explored, any. A fogged
/// object of fog visibility 3 is not modelled.
pub(super) fn build_sees(
    site: &BuildSite<'_>,
    map: &WorldMap<'_>,
    near: &Near,
    (seen, visible): (bool, bool),
    roles: &Roles,
) -> GenieResult<bool> {
    let player = site.player;
    if !(seen && visible) {
        let fog = if seen {
            near.fog_visibility
        } else {
            FogVisibility::VISIBLE
        };
        if fog == FogVisibility::ONLY_IN_FOG {
            return Err(GenieError::unsupported(
                "a fogged object of fog visibility 3 near a BUILD",
            ));
        }
        // without a fog visibility only the player's own objects count, and a
        // building (not a farm) whose visibility mask meets both the player's masks
        if fog == FogVisibility::INVISIBLE {
            let masks = map.vision(player);
            let shown = !roles.of_class(near.class).contains(ClassRoles::FARM)
                && MasterKind::RGE_COMBAT <= near.kind
                && near.fog_mask.is_some_and(|vision_bits| {
                    vision_bits.intersects(masks.explored) && vision_bits.intersects(masks.visible)
                });
            if !shown
                && (near.player != player
                    || !map.allied(near.player, player, AllyTest::WithRelation4))
            {
                return Ok(false);
            }
        }
    } else if near.fog_visibility == FogVisibility::ONLY_IN_FOG {
        return Ok(false);
    }
    Ok(true)
}

/// Whether an object BUILD's scan counts blocks the foundation at `at`: one BUILD neither
/// ignores ([`build_ignores`]) nor passes ([`build_passes`]), whose box with a height overlaps
/// the site's box. Its box is its master's radii and height, for obstruction types 11 to 13 its
/// outline's ([`Near::outline`]); a height of 0 counts as 1 for a building and for an object
/// holding food, wood, stone or gold.
pub(super) fn blocks_build(
    site: &BuildSite<'_>,
    map: &WorldMap<'_>,
    near: &Near,
    at: Point,
    roles: &Roles,
) -> bool {
    let (x, y) = (at.x(), at.y());
    if build_ignores(site, map, near, roles) || build_passes(site, map, near, roles) {
        return false;
    }
    let (or, tall) =
        if (ObstructionKind::KIND_11..=ObstructionKind::KIND_13).contains(&near.obstruction_kind) {
            let oz = near.outline.z();
            (
                near.outline.ground(),
                0.0 < oz
                    || (oz == 0.0
                        && (near.kind == MasterKind::BUILDING
                            || near.flags.contains(NearFlags::HOLDS))),
            )
        } else {
            (
                near.radius,
                near.flags.contains(NearFlags::TALL)
                    || near.kind == MasterKind::BUILDING
                    || near.flags.contains(NearFlags::HOLDS),
            )
        };
    let footprint = site.radius;
    0.0 < or.x()
        && 0.0 < or.y()
        && tall
        && (near.position.x() - x).abs() < or.x() + footprint.x()
        && (near.position.y() - y).abs() < or.y() + footprint.y()
}

/// The objects BUILD's scan ignores: one dead, a moving one where the site does not obstruct,
/// a wall under the player's own gate, a desolid building (but classes 51 and 54) of a player
/// that is not an ally both ways.
pub(super) fn build_ignores(
    site: &BuildSite<'_>,
    map: &WorldMap<'_>,
    near: &Near,
    roles: &Roles,
) -> bool {
    let player = site.player;
    let alive = near.state < ObjectState::Dying
        || (near.kind != MasterKind::BUILDING && near.state < ObjectState::Removed);
    let standing =
        site.obstructs || near.kind < MasterKind::MOVING || !near.flags.contains(NearFlags::MOVING);
    let gate_wall = roles.of_class(site.class).contains(ClassRoles::GATE)
        && roles.of_class(near.class).contains(ClassRoles::WALL)
        && near.player == player;
    let foundation = near.kind == MasterKind::BUILDING
        && !roles
            .of_class(near.class)
            .intersects(ClassRoles::PACKED_TREBUCHET | ClassRoles::UNPACKED_TREBUCHET)
        && near.flags.contains(NearFlags::DESOLID)
        && !(map.allied(player, near.player, AllyTest::WithRelation4)
            && map.allied(near.player, player, AllyTest::WithRelation4));
    !alive || !standing || gate_wall || foundation
}

/// The objects BUILD's scan passes over: a moving unit of the player, Gaia or a mutual ally,
/// one buildable on, a felled tree off a forest.
pub(super) fn build_passes(
    site: &BuildSite<'_>,
    map: &WorldMap<'_>,
    near: &Near,
    roles: &Roles,
) -> bool {
    let player = site.player;
    // a moving unit of the player, of Gaia or of a mutual ally passes (the game type
    // 7's own test of allies is not modelled)
    let passes = MasterKind::MOVING <= near.kind
        && near.state == ObjectState::Alive
        && near.flags.contains(NearFlags::MOVING)
        && (near.player == player
            || near.player == PlayerId::GAIA
            || (map.allied(player, near.player, AllyTest::AlliesOnly)
                && map.allied(near.player, player, AllyTest::AlliesOnly)));
    // a felled tree (at most 1 hit point) passes off a forest (`FUN_140f5efb0` on its
    // tile's terrain)
    passes
        || near.buildable_on.passed()
        || (roles.of_class(near.class).contains(ClassRoles::TREE)
            && near.hp <= HitPoints::new(1.0)
            && !near.flags.contains(NearFlags::FOREST_TILE))
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

/// The master's slot 0x50 (`0x141264db0`) with the spawn's arguments: `FUN_141123f80`'s test
/// (the map's edge, then per tile of the box the terrain restriction and the explored tiles,
/// then the objects overlapping), then its own scan of the objects overlapping, which reports
/// the first one. The editor's case (the game's mode 4), the placement terrains, the hill modes,
/// the zone test (`FUN_1415c6400`, the world's `+0x820` mode 1) and the moving objects' own
/// check (`DAT_143f01833`, probed 0) are not modelled.
pub(crate) fn placement(
    me: &Placing<'_>,
    map: &WorldMap<'_>,
    point: Point,
    zones: ZoneTest,
    roles: &Roles,
    spawn_tuning: &SpawnTuning,
) -> GenieResult<Tested> {
    let (x, y) = (point.x(), point.y());
    let (rx, ry) = (me.radius.x(), me.radius.y());
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
        } else if 0.0 < rx || 0.0 < ry {
            if zones == ZoneTest::Tested {
                return Err(GenieError::unsupported("a placement test in a zone mode"));
            }
            let counts = |near: &Near| {
                let listed = near.state < ObjectState::Removed
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
        } else {
            true
        }
    };
    // slot 0x50's own scan
    let counts = |near: &Near| {
        let listed = near.state < ObjectState::Removed
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
    let found = if me.tall && (0.0 < rx || 0.0 < ry) {
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
