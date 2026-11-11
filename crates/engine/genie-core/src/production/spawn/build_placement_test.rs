//! BUILD's placement test: the unit master's slot 0x50 as the building master's
//! `FUN_14123aee0` calls it, then that routine's scan of the objects near the foundation.

use crate::map::Point;
use crate::map::{Tile, TileRect};
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionKind;
use crate::players::AllyTest;
use crate::players::PlayerId;
use crate::production::spawn::PlacementCode;
use crate::production::spawn::SiteChecks;
use crate::production::spawn::WorldMap;
use crate::production::spawn::placement_test::{UNLISTED_TERRAIN, overlapping};
use crate::production::spawn::{BuildSite, hill_base, hill_fails, placement_terrains};
use crate::production::spawn::{Near, NearFlags};
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::SpawnTuning;
use crate::{GenieError, GenieResult};

/// BUILD's placement test (`FUN_140e7a400`'s call of the master's slot 0x50, the building master's
/// `FUN_14123aee0` with BUILD's flags: `p6` human, `p7`, `p8`, `p9` 1, `p10` 0, `p11` 1, no object
/// of its own, `p13` 0, `p14` 1): 0 when the foundation may go to `point`, else the game's code.
/// First the unit master's slot 0x50 ([`build_slot_test`]; its scans run only for the Build's
/// test, [`SiteChecks::SCANS`]) over the box of the site's radii: off the map 7; per tile,
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
    let code = build_slot_test(site, map, point, roles, spawn_tuning)?;
    if !code.is_allowed() {
        return Ok(code);
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

/// The unit master's slot 0x50 (`0x141264db0`) as BUILD's placement test calls it (`p10` 0,
/// `p11` the scans, [`SiteChecks::SCANS`]): `FUN_141123f80`'s test, its static test
/// ([`build_static_test`]), then for a site with a height its scan behind the zone test's gate
/// ([`base_scan_finds`]). With the scans, the slot answers 0 for a site whose radii are both at
/// most 0, whatever `FUN_141123f80` found (`XOR AL, AL` at 0x141264f09); for one with a height,
/// its own scan ([`own_scan_finds`]) answers 6 over that code. A NaN radius passes both gates
/// ([`Footprint::has_radii`]) and the scans find nothing for it. The editor's code 9 is not
/// modelled.
///
/// [`Footprint::has_radii`]: crate::map::Footprint::has_radii
fn build_slot_test(
    site: &BuildSite<'_>,
    map: &WorldMap<'_>,
    point: Point,
    roles: &Roles,
    spawn_tuning: &SpawnTuning,
) -> GenieResult<PlacementCode> {
    let scans = site.checks.contains(SiteChecks::SCANS);
    let code = if let Some(code) = build_static_test(site, map, point, spawn_tuning)? {
        code
    } else if scans
        && site.tall
        && site.radius.has_radii()
        && base_scan_finds(site, map, point, roles, spawn_tuning)?
    {
        PlacementCode::Blocked
    } else {
        PlacementCode::Allowed
    };
    if !scans {
        return Ok(code);
    }
    if !site.radius.has_radii() {
        return Ok(PlacementCode::Allowed);
    }
    if site.tall && own_scan_finds(site, map, point, roles, spawn_tuning)? {
        return Ok(PlacementCode::Blocked);
    }
    Ok(code)
}

/// `FUN_141123f80`'s scan in the Build's test: over the tiles within `SpawnTuning::reach` (rows,
/// then columns; each tile's objects in its list's order), an object other than the site's own
/// (`p12`), below state 7, not buildable on, no desolid building, with a height (a tree's farm
/// and an obstructing building have one), whose box overlaps the site's: true when one does (the
/// game's code 6).
fn base_scan_finds(
    site: &BuildSite<'_>,
    map: &WorldMap<'_>,
    point: Point,
    roles: &Roles,
    spawn_tuning: &SpawnTuning,
) -> GenieResult<bool> {
    let counts = |near: &Near| {
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
    Ok(!overlapping(map, point, site.radius, &counts, spawn_tuning)?.is_empty())
}

/// `FUN_141264db0`'s own scan in the Build's test (`p13` 1): over the tiles within
/// `SpawnTuning::reach` (rows, then columns; each tile's objects in its list's order), an object
/// other than the site's own (`p12`), not a unit (type 70), below state 7, not buildable on, no
/// desolid building but of classes 51 and 54, with a height (an obstructing building has one),
/// whose box overlaps the site's: true when one does (the game's code 6).
fn own_scan_finds(
    site: &BuildSite<'_>,
    map: &WorldMap<'_>,
    point: Point,
    roles: &Roles,
    spawn_tuning: &SpawnTuning,
) -> GenieResult<bool> {
    let counts = |near: &Near| {
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
    Ok(!overlapping(map, point, site.radius, &counts, spawn_tuning)?.is_empty())
}

/// BUILD's static test (`FUN_141123f80`'s, through [`build_slot_test`]) over the clearance box at
/// `point`: off the map 7; the placement terrains 1; per tile, columns then rows, a terrain the
/// restriction forbids 2; a hill mode's tile 3; a human who explored none of the tiles 5. None
/// when the box passes; a zone mode, whose test runs behind the gate of
/// [`Footprint::has_radii`] (a NaN radius passes it), is not modelled.
///
/// [`Footprint::has_radii`]: crate::map::Footprint::has_radii
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
    let base = hill_base(site.hill_mode, map, Tile::new(x0, y1))?;
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
    if site.radius.has_radii() && site.checks.contains(SiteChecks::ZONES) {
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
