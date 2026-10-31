//! The master's placement test (slot 0x50): the objects near a candidate, BUILD's placement
//! test and the spawn's.

use crate::GenieError;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::Radii;
use crate::map::TerrainId;
use crate::map::{Tile, TileRect};
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::PlacementTerrains;
use crate::objects::masters::class;
use crate::objects::masters::ids::RELIC;
use crate::objects::masters::master_type;
use crate::objects::masters::master_type::{BUILDING, COMBAT};
use crate::players::PlayerId;
use crate::production::spawn::{
    BuildSite, INSIDE, PASSABLE, REACH, Tested, hill_fails, placement_terrains,
};

/// An object near a candidate, as the placement test and the blocker test read it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Near {
    /// The object.
    pub id: ObjectId,
    /// Its place.
    pub position: Position,
    /// Its master's radii on the ground and whether its height radius is above 0.
    pub radius: Footprint,
    /// Its master's height radius is above 0.
    pub tall: bool,
    /// Its master's type (`+0x8`), class, id, `can_be_built_on` (`+0x5e9`), `obstruction_type`
    /// (`+0x651`) and `enable_auto_gather` (`+0x681`).
    pub kind: MasterKind,
    /// Its master's class.
    pub class: ClassId,
    /// Its master.
    pub master: MasterId,
    /// Its master's `can_be_built_on` (`+0x5e9`).
    pub buildable_on: u8,
    /// Its master's `obstruction_type` (`+0x651`).
    pub obstruction_type: ObstructionType,
    /// Its master's `enable_auto_gather` (`+0x681`).
    pub auto_gather: bool,
    /// Its state (`+0x640`) and, for a building, its `desolid_flag` (`+0xc60`).
    pub state: ObjectState,
    /// A building's `desolid_flag` (`+0xc60`).
    pub desolid: bool,
    /// Its owner.
    pub player: PlayerId,
    /// Its master moves (speed above 0).
    pub moving: bool,
    /// Its master's `fog_visibility` (`+0x605`).
    pub fog_visibility: u8,
    /// A building's visibility mask (`+0x8f8`, the registry's) when its footprint has a UMV entry
    /// (`+0x904` not 0); None otherwise.
    pub fog_mask: Option<u64>,
    /// It holds an amount (`+0x310` above 0) of food, wood, stone or gold (its attribute,
    /// `+0x308`, through `FUN_140fdd470` below 4: 0 to 3 and the meats 15 to 17).
    pub holds: bool,
    /// Its hit points.
    pub hp: HitPoints,
    /// The terrain of its tile (`+0x2e8`, taken as the tile it stands on) is a forest
    /// ([`crate::map::terrain::TerrainKind::forest`]).
    pub forest_tile: bool,
    /// Its master's outline radii and height (`+0x688`, `+0x68c`, `+0x690`), BUILD's box for
    /// obstruction types 11 to 13.
    pub outline: Radii,
}

impl Near {
    /// The blocker tests of `FUN_1411c5b60`: a building (`FUN_1411942c0`), a tree past state 2
    /// (`FUN_141195660`), the relic (`FUN_141194e80`), and `FUN_141194170`'s master with
    /// `enable_auto_gather` or of a class that stays from state 2 on (5, 9, 10, 29, 31, 33, 38,
    /// 40, 41, 58, 61) or from state 3 on (7, 8, 15, 32, 48, 63).
    #[must_use]
    pub fn stays(&self) -> bool {
        if self.kind == BUILDING
            || (self.class == class::TREE && ObjectState::Alive < self.state)
            || self.master == RELIC
        {
            return true;
        }
        if self.auto_gather {
            return true;
        }
        match self.class {
            class::CLASS_5
            | class::PREY
            | class::PREDATOR
            | class::CLASS_29
            | class::CLASS_31
            | class::CLASS_33
            | class::CLASS_38
            | class::CLASS_40
            | class::CLASS_41
            | class::LIVESTOCK
            | class::CLASS_61 => ObjectState::Unfinished < self.state,
            class::CLASS_7
            | class::CLASS_8
            | class::TREE
            | class::CLASS_32
            | class::CLASS_48
            | class::CLASS_63 => ObjectState::Alive < self.state,
            _ => false,
        }
    }
}

/// A unit to put down, beside a building or ashore, of a master that does not move (no
/// terrain restriction to test its place by).
pub(crate) const NOT_MOVING: GenieError =
    GenieError::unsupported("a unit to put down of a master that does not move");

/// The master a placement test places (its slot 0x50 with the spawn's arguments: by its radii,
/// the terrain checked, the player's explored tiles needed, the elevation checked by its hill
/// mode, the objects checked).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placing<'a> {
    /// The unit's player.
    pub player: PlayerId,
    /// Its master's radii.
    pub radius: Footprint,
    /// Its master's height radius is above 0.
    pub tall: bool,
    /// Its master's class.
    pub class: ClassId,
    /// Its terrain restriction's value per terrain.
    pub restriction: &'a [f32],
    /// `hill_mode` (`+0x604`).
    pub hill_mode: u8,
    /// The placement terrains (`+0x5f2`..`+0x5f8`, [`placement_terrains`]).
    pub placement: PlacementTerrains,
}

/// What the placement test reads of the map.
pub trait PlaceMap {
    /// The map's size in tiles.
    fn size(&self) -> GridSize;
    /// A tile's terrain (the map tile's `+0x9`).
    fn terrain(&self, tile: Tile) -> Result<TerrainId, GenieError>;
    /// Terrain `t`'s flags (the `.dat`'s `is_water` byte, the map's terrain record `+2`), None
    /// beyond the terrains.
    fn terrain_flags(&self, _t: usize) -> Option<u8> {
        None
    }
    /// How many terrains the map's table holds.
    fn terrain_count(&self) -> usize {
        0
    }
    /// `FUN_141663500` is not 0: the player sees or saw the tile.
    fn explored(&self, player: PlayerId, tile: Tile) -> bool;
    /// The player sees the tile now (its mask `+0x2a8` meets the tile's visible bits).
    fn visible(&self, player: PlayerId, tile: Tile) -> bool;
    /// The player's masks: what it sees (`+0x2a8`) and what it explored (`+0x2a0`).
    fn vision(&self, player: PlayerId) -> [u64; 2];
    /// A tile's elevation (`+0x14`), when the map holds it.
    fn elevation(&self, tile: Tile) -> Option<u8>;
    /// A tile's shape (`+0x8`, [`crate::map::terrain::Terrain::set_elevations`]), when the map holds
    /// it.
    fn shape(&self, tile: Tile) -> Option<u8>;
    /// Player `a` counts player `b` as an ally (`FUN_141663dc0`): itself; never Gaia; relation 0,
    /// or with `with_4` relation 4 too (the zone mode's relation 1 is not modelled).
    fn allied(&self, a: PlayerId, b: PlayerId, with_4: bool) -> bool;
    /// The objects on `tile` (their centre there), in the tile list's order; `Err` where it
    /// cannot tell.
    fn objects(&self, tile: Tile) -> Result<Vec<Near>, GenieError>;
}

/// The tiles within `REACH` of a point, clamped to the map (`(int)` of each bound, then the map's
/// edges), row by row.
pub(super) fn reach(map: GridSize, at: Point) -> impl Iterator<Item = Tile> {
    let (x, y) = (at.x(), at.y());
    let x0 = ((x - REACH) as i32 as i16).max(0);
    let x1 = ((x + REACH) as i32 as i16).min((map.width() - 1) as i16);
    let y0 = ((y - REACH) as i32 as i16).max(0);
    let y1 = ((y + REACH) as i32 as i16).min((map.height() - 1) as i16);
    (y0..=y1).flat_map(move |ty| (x0..=x1).map(move |tx| Tile::new(i32::from(tx), i32::from(ty))))
}

/// An object a placement test's scan found, with its tile.
pub(super) type Found = (Tile, Near);

/// The objects whose box overlaps a box of radii `r` at `p` among those `counts` lets through,
/// with their tile, in the scan's order.
pub(super) fn overlapping(
    map: &dyn PlaceMap,
    p: Point,
    r: Footprint,
    counts: &dyn Fn(&Near) -> Option<bool>,
) -> Result<Vec<Found>, GenieError> {
    let mut out = Vec::new();
    for t in reach(map.size(), p) {
        for o in map.objects(t)? {
            let Some(tall) = counts(&o) else { continue };
            if !(0.0 < o.radius.x() && 0.0 < o.radius.y() && tall) {
                continue;
            }
            let dx = (o.position.x() - p.x()).abs();
            let dy = (o.position.y() - p.y()).abs();
            if dx < o.radius.x() + r.x() && dy < o.radius.y() + r.y() {
                out.push((t, o));
            }
        }
    }
    Ok(out)
}

/// BUILD's placement test (`FUN_140e7a400`'s call of the master's slot 0x50, the building
/// master's `FUN_14123aee0` with BUILD's flags: `p6` human, `p7`, `p8`, `p9` 1, `p10` 0, `p11` 1,
/// no object of its own, `p13` 0, `p14` 1): 0 when the foundation may go to `p`, else the game's
/// code. First the static test (`FUN_141123f80` through `FUN_141264db0`; their scans run only
/// for the Build's test, [`BuildSite::scans`]) over the box of the site's radii: off the map 7; per tile, columns then rows, a terrain the
/// restriction forbids 2 (`DAT_142eb7e5c`'s threshold, the spawn's); a human who explored none of
/// the tiles 5; a hill mode's tile ([`hill_fails`]) 3. Then the building master's scan of the tiles within 8 (rows, then columns; each
/// tile's objects in its list's order): an object the player sees, or on a tile it never
/// explored (the test's explored flag `+0x730` is set by then), or in the fog with a fog
/// visibility, and only its own in the fog without one, that is below state 3 (another type
/// below 7), standing (or the master obstructs, or it is below type 30), not a wall under its
/// own gate, a desolid building only of an ally both ways, not a moving unit of the player, Gaia
/// or a mutual ally, not a felled tree off a forest (at most 1 hit point; `+0x2e8` taken as its
/// own tile), not buildable on, whose box with a
/// height overlaps the clearance box: 6. Not modelled: the placement terrains, hill modes, the
/// zone test (`FUN_1415c6400`), the editor (game mode 4), a fogged object of fog visibility 3,
/// game type 7's test of allies.
pub fn build_placement(
    site: &BuildSite<'_>,
    map: &dyn PlaceMap,
    p: Point,
) -> Result<u8, GenieError> {
    if let Some(code) = build_static_test(site, map, p)? {
        return Ok(code);
    }
    if site.scans && static_scans_find(site, map, p)? {
        return Ok(6);
    }
    // FUN_14123aee0's scan
    let size = map.size();
    let (w, h) = (size.width(), size.height());
    let ((x0, x1), (y0, y1)) = (build_scan_range(p.x(), w), build_scan_range(p.y(), h));
    let player = site.player;
    for ty in y0..=y1 {
        for tx in x0..=x1 {
            let tile = Tile::new(tx, ty);
            let sight = (map.explored(player, tile), map.visible(player, tile));
            for o in map.objects(tile)? {
                if site.own.contains(&o.id) {
                    continue;
                }
                if build_sees(site, map, &o, sight)? && blocks_build(site, map, &o, p) {
                    return Ok(6);
                }
            }
        }
    }
    Ok(0)
}

/// The static scans of the Build's test (`p11` and `p13` 1, [`BuildSite::scans`]), for a master with
/// a height ([`BuildSite::tall`]) and radii: `FUN_141123f80`'s, then `FUN_141264db0`'s, each over
/// the tiles within [`REACH`] (rows, then columns; each tile's objects in its list's order) for an
/// object other than the site's own (`p12`), below state 7, not buildable on, no desolid building
/// (`FUN_141264db0`'s passes classes 51 and 54), with a height (`FUN_141123f80`'s gives a tree's
/// farm and an obstructing building one, `FUN_141264db0`'s an obstructing building; the
/// latter passes over the units, type 70), whose box overlaps the site's: true when one does
/// (the game's code 6).
fn static_scans_find(
    site: &BuildSite<'_>,
    map: &dyn PlaceMap,
    p: Point,
) -> Result<bool, GenieError> {
    let (rx, ry) = (site.radius.x(), site.radius.y());
    if !site.tall || !(0.0 < rx || 0.0 < ry) {
        return Ok(false);
    }
    let first = |o: &Near| {
        let listed = !site.own.contains(&o.id)
            && o.state < ObjectState::Removed
            && o.buildable_on != 1
            && (o.kind != BUILDING || !o.desolid);
        let tall = o.tall
            || (site.class == class::TREE && o.class == class::FARM)
            || (o.kind == BUILDING && o.obstruction_type != ObstructionType::NONE);
        listed.then_some(tall)
    };
    if !overlapping(map, p, site.radius, &first)?.is_empty() {
        return Ok(true);
    }
    let second = |o: &Near| {
        let listed = !site.own.contains(&o.id)
            && o.state < ObjectState::Removed
            && o.buildable_on != 1
            && o.kind != COMBAT
            && (o.kind != BUILDING
                || o.class == class::CLASS_51
                || o.class == class::CLASS_54
                || !o.desolid);
        let tall = o.tall || (o.kind == BUILDING && o.obstruction_type.recorded());
        listed.then_some(tall)
    };
    Ok(!overlapping(map, p, site.radius, &second)?.is_empty())
}

/// BUILD's static test (`FUN_141123f80` through `FUN_141264db0`) over the clearance box at `p`:
/// off the map 7; the placement terrains 1; per tile, columns then rows, a terrain the
/// restriction forbids 2; a hill mode's tile 3; a human who explored none of the tiles 5. None
/// when the box passes; a zone mode is not modelled.
pub(super) fn build_static_test(
    site: &BuildSite<'_>,
    map: &dyn PlaceMap,
    p: Point,
) -> Result<Option<u8>, GenieError> {
    let (x, y) = (p.x(), p.y());
    let (rx, ry) = (site.radius.x(), site.radius.y());
    let size = map.size();
    let (w, h) = (size.width(), size.height());
    let (far_x, far_y) = ((x + rx) - INSIDE, (y + ry) - INSIDE);
    if x - rx < 0.0 || w as f32 <= far_x || y - ry < 0.0 || h as f32 <= far_y {
        return Ok(Some(7));
    }
    let mut explored = false;
    let (x0, x1, y0, y1) = ((x - rx) as i32, far_x as i32, (y - ry) as i32, far_y as i32);
    if !placement_terrains(site.placement, map, p, TileRect::new(x0, y0, x1, y1))? {
        return Ok(Some(1));
    }
    // hill mode 3 measures the box's edge against the elevation of its first column's last row
    let base = if site.hill_mode == 3 {
        Some(
            map.elevation(Tile::new(x0, y1))
                .ok_or(GenieError::unsupported(
                    "a hill mode where the elevations are not known",
                ))?,
        )
    } else {
        None
    };
    for tx in x0..=x1 {
        for ty in y0..=y1 {
            let tile = Tile::new(tx, ty);
            let t = map.terrain(tile)?;
            let value = site
                .restriction
                .get(t.index())
                .ok_or(GenieError::unsupported(
                    "a terrain its restriction does not list",
                ))?;
            if *value <= PASSABLE {
                return Ok(Some(2));
            }
            explored = explored || (site.human && map.explored(site.player, tile));
            if hill_fails(site.hill_mode, map, tile, Tile::new(x0, y1), base)? {
                return Ok(Some(3));
            }
        }
    }
    if site.human && !explored {
        return Ok(Some(5));
    }
    if (0.0 < rx || 0.0 < ry) && site.zones {
        return Err(GenieError::unsupported(
            "BUILD's placement test in a zone mode",
        ));
    }
    Ok(None)
}

/// The tiles BUILD's scan reads along one axis about `v` (`FUN_14123aee0`): within
/// [`BUILD_SCAN`], each end truncated into a short, from 0 to the map's last tile of `size`.
pub(super) fn build_scan_range(v: f32, size: i32) -> (i32, i32) {
    let low = (v - BUILD_SCAN) as i32 as i16;
    let high = (v + BUILD_SCAN) as i32 as i16;
    (
        i32::from(low.max(0)),
        if i32::from(high) < size {
            i32::from(high)
        } else {
            size - 1
        },
    )
}

/// Whether BUILD's scan counts the object `o` by what the player sees of its tile (`seen`
/// explored, `visible`): in sight, any but one of fog visibility 3; in the fog, by its fog
/// visibility (one of 0 only when it is the player's own or allied, or a building whose
/// visibility mask meets both the player's masks); on a tile never explored, any. A fogged
/// object of fog visibility 3 is not modelled.
pub(super) fn build_sees(
    site: &BuildSite<'_>,
    map: &dyn PlaceMap,
    o: &Near,
    (seen, visible): (bool, bool),
) -> Result<bool, GenieError> {
    let player = site.player;
    if !(seen && visible) {
        let fog = if seen { o.fog_visibility } else { 1 };
        if fog == 3 {
            return Err(GenieError::unsupported(
                "a fogged object of fog visibility 3 near a BUILD",
            ));
        }
        // without a fog visibility only the player's own objects count, and a
        // building (not a farm) whose visibility mask meets both the player's masks
        if fog == 0 {
            let [sees, explored] = map.vision(player);
            let shown = o.class != class::FARM
                && master_type::RGE_COMBAT <= o.kind
                && o.fog_mask
                    .is_some_and(|m| m & explored != 0 && m & sees != 0);
            if !shown && (o.player != player || !map.allied(o.player, player, true)) {
                return Ok(false);
            }
        }
    } else if o.fog_visibility == 3 {
        return Ok(false);
    }
    Ok(true)
}

/// Whether an object BUILD's scan counts blocks the foundation at `p`: one BUILD neither
/// ignores ([`build_ignores`]) nor passes ([`build_passes`]), whose box with a height overlaps
/// the site's box. Its box is its master's radii and height, for obstruction types 11 to 13 its
/// outline's ([`Near::outline`]); a height of 0 counts as 1 for a building and for an object
/// holding food, wood, stone or gold.
pub(super) fn blocks_build(site: &BuildSite<'_>, map: &dyn PlaceMap, o: &Near, at: Point) -> bool {
    let (x, y) = (at.x(), at.y());
    if build_ignores(site, map, o) || build_passes(site, map, o) {
        return false;
    }
    let (or, tall) =
        if (ObstructionType::TYPE_11..=ObstructionType::TYPE_13).contains(&o.obstruction_type) {
            let oz = o.outline.z();
            (
                o.outline.ground(),
                0.0 < oz || (oz == 0.0 && (o.kind == BUILDING || o.holds)),
            )
        } else {
            (o.radius, o.tall || o.kind == BUILDING || o.holds)
        };
    let r = site.radius;
    0.0 < or.x()
        && 0.0 < or.y()
        && tall
        && (o.position.x() - x).abs() < or.x() + r.x()
        && (o.position.y() - y).abs() < or.y() + r.y()
}

/// The objects BUILD's scan ignores: one dead, a moving one where the site does not obstruct,
/// a wall under the player's own gate, a desolid building (but classes 51 and 54) of a player
/// that is not an ally both ways.
pub(super) fn build_ignores(site: &BuildSite<'_>, map: &dyn PlaceMap, o: &Near) -> bool {
    let player = site.player;
    let alive =
        o.state < ObjectState::Dying || (o.kind != BUILDING && o.state < ObjectState::Removed);
    let standing = site.obstructs || o.kind < master_type::MOVING || !o.moving;
    let gate_wall = site.class == class::GATE && o.class == class::WALL && o.player == player;
    let foundation = o.kind == BUILDING
        && !matches!(o.class, class::CLASS_51 | class::CLASS_54)
        && o.desolid
        && !(map.allied(player, o.player, true) && map.allied(o.player, player, true));
    !alive || !standing || gate_wall || foundation
}

/// The objects BUILD's scan passes over: a moving unit of the player, Gaia or a mutual ally,
/// one buildable on, a felled tree off a forest.
pub(super) fn build_passes(site: &BuildSite<'_>, map: &dyn PlaceMap, o: &Near) -> bool {
    let player = site.player;
    // a moving unit of the player, of Gaia or of a mutual ally passes (the game type
    // 7's own test of allies is not modelled)
    let passes = master_type::MOVING <= o.kind
        && o.state == ObjectState::Alive
        && o.moving
        && (o.player == player
            || o.player == PlayerId::GAIA
            || (map.allied(player, o.player, false) && map.allied(o.player, player, false)));
    // a felled tree (at most 1 hit point) passes off a forest (`FUN_140f5efb0` on its
    // tile's terrain)
    passes
        || o.buildable_on == 1
        || (o.class == class::TREE && o.hp <= HitPoints::new(1.0) && !o.forest_tile)
}

/// How far BUILD's placement scan looks around the point (`FUN_14123aee0`, `DAT_142eb9430`).
pub(super) const BUILD_SCAN: f32 = crate::math::image::spawn::BUILD_SCAN;

/// The first object of a scan, which the game reports as the blocker: the tiles row by row, each
/// tile's objects in its list's order ([`crate::map::spatial::Spatial::objects_on`], the game's: the
/// load files them in the save's order, as the game's load does).
pub(super) fn first(found: &[Found]) -> Option<Near> {
    found.first().map(|&(_, o)| o)
}

/// The master's slot 0x50 (`0x141264db0`) with the spawn's arguments: `FUN_141123f80`'s test
/// (the map's edge, then per tile of the box the terrain restriction and the explored tiles,
/// then the objects overlapping), then its own scan of the objects overlapping, which reports
/// the first one. The editor's case (the game's mode 4), the placement terrains, the hill modes,
/// the zone test (`FUN_1415c6400`, the world's `+0x820` mode 1) and the moving objects' own
/// check (`DAT_143f01833`, probed 0) are not modelled.
pub fn placement(
    me: &Placing<'_>,
    map: &dyn PlaceMap,
    p: Point,
    zones: bool,
) -> Result<Tested, GenieError> {
    let (x, y) = (p.x(), p.y());
    let (rx, ry) = (me.radius.x(), me.radius.y());
    let size = map.size();
    let (w, h) = (size.width(), size.height());
    let (far_x, far_y) = ((x + rx) - INSIDE, (y + ry) - INSIDE);
    // FUN_141123f80's own result: 0 free; the objects of its scan report the blocker (6)
    let mut base_blocker = None;
    let off_map = x - rx < 0.0 || w as f32 <= far_x || y - ry < 0.0 || h as f32 <= far_y;
    let base_free = if off_map
        || !placement_terrains(
            me.placement,
            map,
            p,
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
                let t = map.terrain(tile)?;
                let value = me
                    .restriction
                    .get(t.index())
                    .ok_or(GenieError::unsupported(
                        "a terrain its restriction does not list",
                    ))?;
                if *value <= PASSABLE {
                    passable = false;
                    break 'tiles;
                }
                seen = seen || map.explored(me.player, tile);
                if me.hill_mode != 0 {
                    return Err(GenieError::unsupported("a placement test of a hill mode"));
                }
            }
        }
        if !passable || !seen {
            false
        } else if 0.0 < rx || 0.0 < ry {
            if zones {
                return Err(GenieError::unsupported("a placement test in a zone mode"));
            }
            let counts = |o: &Near| {
                let listed = o.state < ObjectState::Removed
                    && o.buildable_on != 1
                    && (o.kind != BUILDING || !o.desolid);
                let tall = o.tall
                    || (me.class == class::TREE && o.class == class::FARM)
                    || (o.kind == BUILDING && o.obstruction_type != ObstructionType::NONE);
                listed.then_some(tall && me.tall)
            };
            let found = if me.tall {
                overlapping(map, p, me.radius, &counts)?
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
    let counts = |o: &Near| {
        let listed = o.state < ObjectState::Removed
            && o.buildable_on != 1
            && (o.kind != BUILDING
                || o.class == class::CLASS_51
                || o.class == class::CLASS_54
                || !o.desolid);
        let tall = o.tall || (o.kind == BUILDING && o.obstruction_type.recorded());
        listed.then_some(tall && me.tall)
    };
    let found = if me.tall && (0.0 < rx || 0.0 < ry) {
        overlapping(map, p, me.radius, &counts)?
    } else {
        Vec::new()
    };
    let blocker = first(&found).or(base_blocker);
    if base_free && found.is_empty() {
        return Ok(Tested::Free);
    }
    Ok(Tested::Blocked {
        stays: blocker.is_some_and(|b| b.stays()),
    })
}
