//! The footprint clearing (`FUN_1415c3620`) of a building: the objects on its tiles that go.

use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Rect;
use crate::map::Tile;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::master_type::COMBAT;
use crate::players::PlayerId;
use std::collections::BTreeMap;

use crate::GenieError;

use crate::map::spatial::Spatial;

/// What the footprint clearing (`FUN_1415c3620`) reads of the new building: its owner
/// (`param_2`), its master's id (`param_3`), class and radii, its place (`param_4`,
/// `param_5`), and whether the caller is the builder's or a new object's (`param_6`: the trees
/// at most of 1 hit point go too, and each deleted object's record with them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClearingSite {
    /// The building's owner.
    pub player: PlayerId,
    /// Its master's id.
    pub master: MasterId,
    /// Its master's class (`+0x410`).
    pub class: ClassId,
    /// Its master's radii (`+0x5d8`, `+0x5dc`).
    pub radius: Footprint,
    /// Its place.
    pub at: Point,
    /// `param_6`: the builder's call (the Build's `set_state(7)`) or a new object's
    /// (`FUN_14162ce00` with its flag).
    pub trees: bool,
}

/// What the footprint clearing reads of an object on a tile besides its place: its hit points
/// (`+0x278`), its master's height (`+0x5e0`) and type, its state, whether it is a desolid
/// foundation (`+0xc60`) and whether its record went already (`mROOAlreadyCalled`, `+0x686`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClearLook {
    /// `+0x278`.
    pub hp: HitPoints,
    /// Its master's `radius_z` (`+0x5e0`).
    pub height: Distance,
    /// `+0x686`.
    pub roo_called: bool,
    /// Its master's type.
    pub kind: MasterKind,
    /// `+0x640`.
    pub state: ObjectState,
    /// `+0xc60`.
    pub desolid: bool,
}

/// An object the footprint clearing deletes, in the tiles' order: a wall (class 27) first goes
/// through the building's delete core (`FUN_1411ccbb0`), then every one is destroyed (slot
/// 0xd0); with `param_6` its record goes after (`FUN_141189670`, then `+0x686` set,
/// `FUN_141189b60`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cleared {
    /// The object.
    pub id: ObjectId,
    /// It is a wall (class 27): `FUN_1411ccbb0` before its destroy.
    pub wall: bool,
    /// Its record goes in the clearing: its destroy's death part drops it (a unit or a tall
    /// building not desolid of type 70 and up below state 3 whose record is still there,
    /// [`crate::objects::lifecycle::death_parts::combat_dies`], which sets `+0x686`), or with `param_6` `FUN_141189670`
    /// after the destroy does (`+0x686` still clear, and its master tall, COMISS: a NaN height
    /// too, or the object holding its record).
    pub record_goes: bool,
}

/// The flag masters (`FUN_140fde270`: 274, 1689 and 1785), which the footprint clearing leaves
/// alone unless their master has height.
pub(super) const FLAG_MASTERS: [MasterId; 3] = [
    MasterId::named(0x112),
    MasterId::named(0x699),
    MasterId::named(0x6f9),
];
/// The tiles the footprint clearing scans each way of the building's place (`DAT_142eb0f48`).
pub(crate) const CLEARING_REACH: f32 = crate::math::image::build::CLEARING_REACH;
/// The hit points a tree at most has that the builder's clearing takes (`DAT_14330eb8c`).
pub(super) const STUMP_HP: f32 = crate::math::image::build::STUMP_HP;

/// `FUN_1415c3620` with `param_7` 0, its choice: the objects whose centre is on the tiles from 4
/// before the building's place to 4 past it (CVTTSS2SI to 16 bits, held on the map), rows then
/// columns, each tile's list in its order (`objects_on`: the ghosts, type 25, are not on it), of
/// another master than the building's, that may be built on (`+0x5e9`), or a wall (27) of the
/// building's owner under a gate (39) or a master of class 52, or with `param_6` a tree (15)
/// of at most 1 hit point (COMISS: a NaN is not), whose box meets the building's (on each axis
/// the gap below the two radii summed; the gap's sign flips only below 0). Of those, a flag
/// master (`FUN_140fde270`) whose height is at most 0 stays; the others go ([`Cleared`]). A
/// building without radii clears nothing (COMISS: a NaN radius is not 0 or less). The game's
/// other branch, `param_7` or the editor's screen mode (`FUN_140c87f20` 4: the objects only leave
/// their tiles, `FUN_140f5f3c0`), never runs in a game: `param_7` is the map generator's
/// (`FUN_140fad9c0`).
///
/// # Errors
///
/// An object on a tile the caller does not hold (`looks` holds none), and for the builder's call a
/// frozen object that may be on the tiles ([`Spatial::check_known`]).
pub(crate) fn footprint_clearing(
    world: &Spatial,
    site: &ClearingSite,
    looks: &BTreeMap<ObjectId, ClearLook>,
) -> Result<Vec<Cleared>, GenieError> {
    use crate::math::fmath::below_or_unordered;
    let (rx, ry) = (site.radius.x(), site.radius.y());
    if !below_or_unordered(0.0, rx) && !below_or_unordered(0.0, ry) {
        return Ok(Vec::new());
    }
    let Some(([x0, x1], [y0, y1])) = clearing_tiles(site.at, world.map) else {
        return Ok(Vec::new());
    };
    // a stopped player's units may have felled a tree there since they froze: the builder's
    // choice, which reads the trees' hit points, wants the tiles known; the constructor's reads
    // only masters and owners, which they do not change
    if site.trees {
        world.check_known(Rect::new(
            f32::from(x0),
            f32::from(y0),
            f32::from(x1) + 1.0,
            f32::from(y1) + 1.0,
        ))?;
    }
    let mut cleared = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            for (id, o) in world.objects_on(Tile::new(i32::from(x), i32::from(y))) {
                if let Some(c) = clearing_takes(site, id, o, looks)? {
                    cleared.push(c);
                }
            }
        }
    }
    Ok(cleared)
}

/// The objects on [`footprint_clearing`]'s tiles about `at` ([`clearing_tiles`]), in its order:
/// those it may read the [`ClearLook`] of.
pub(crate) fn clearing_objects(world: &Spatial, at: Point) -> Vec<ObjectId> {
    let Some(([x0, x1], [y0, y1])) = clearing_tiles(at, world.map) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            ids.extend(
                world
                    .objects_on(Tile::new(i32::from(x), i32::from(y)))
                    .map(|(id, _)| id),
            );
        }
    }
    ids
}

/// [`footprint_clearing`]'s tiles: from 4 before the place to 4 past it on each axis
/// (CVTTSS2SI to 16 bits), held on the map; none when a range is empty.
pub(super) fn clearing_tiles(at: Point, map: GridSize) -> Option<([i16; 2], [i16; 2])> {
    use crate::math::conversions::cvttss2si;
    let (w, h) = (map.width(), map.height());
    let range = |v: f32, size: i32| {
        let low = cvttss2si(v - CLEARING_REACH) as i16;
        let high = cvttss2si(v + CLEARING_REACH) as i16;
        let high = if size <= i32::from(high) {
            (size - 1) as i16
        } else {
            high
        };
        (low.max(0), high)
    };
    let (x0, x1) = range(at.x(), w);
    let (y0, y1) = range(at.y(), h);
    (x0 <= x1 && y0 <= y1).then_some(([x0, x1], [y0, y1]))
}

/// [`footprint_clearing`]'s test of one object `id` on its tiles: of another master than the
/// building's, that may be built on, or the owner's wall under a gate or class 52, or a tree of
/// at most 1 hit point when the clearing takes trees; its box meeting the building's; not a
/// flag master of height 0 or less. Then what goes of it ([`Cleared`]).
///
/// # Errors
///
/// An object the caller does not hold (`looks` holds none).
pub(super) fn clearing_takes(
    site: &ClearingSite,
    id: ObjectId,
    o: &crate::map::spatial::Placed,
    looks: &BTreeMap<ObjectId, ClearLook>,
) -> Result<Option<Cleared>, GenieError> {
    use crate::math::fmath::below_or_unordered;
    if o.master == site.master {
        return Ok(None);
    }
    // The masters whose classes (39, the gate, and 52) take their owner's walls (27) off their
    // footprint, and the tree class (15).
    let under_gate = matches!(site.class, class::GATE | class::TOWER);
    // The wall class (27, `0x1b`): two walls beside a builder on a footprint's diagonal stop it.
    let own_wall = o.player == site.player && under_gate && o.class == class::WALL;
    let tree = site.trees && o.class == class::TREE;
    if !o.buildable_on.cleared() && !own_wall && !tree {
        return Ok(None);
    }
    let seen = *looks.get(&id).ok_or(GenieError::unsupported(
        "a footprint clearing of an object the slice does not hold",
    ))?;
    // the tree's test: COMISS 1.0 against its hit points, JC passes it over (a NaN too)
    if !o.buildable_on.cleared() && !own_wall && below_or_unordered(STUMP_HP, seen.hp.get()) {
        return Ok(None);
    }
    let (rx, ry) = (site.radius.x(), site.radius.y());
    let dx = (o.position.x() - site.at.x()).abs();
    let dy = (o.position.y() - site.at.y()).abs();
    if !(dx < o.radius.x() + rx && dy < o.radius.y() + ry) {
        return Ok(None);
    }
    // COMISS 0 against the height, JNC: a flag master of height 0 or less stays
    if FLAG_MASTERS.contains(&o.master) && !below_or_unordered(0.0, seen.height.get()) {
        return Ok(None);
    }
    // the destroy's death part (FUN_14121cc40 below state 3) drops the record and sets +0x686
    // first; then FUN_141189670: COMISS 0 against the height, JC drops the record (a NaN too),
    // else the record held (`+0x658`) does
    let dies = COMBAT <= seen.kind && seen.state < ObjectState::Dying && !seen.roo_called;
    let destroy_drops =
        dies && (seen.kind < BUILDING || (!seen.desolid && Distance::ZERO < seen.height));
    let roo_drops = site.trees
        && !seen.roo_called
        && !dies
        && (below_or_unordered(0.0, seen.height.get()) || o.obstruction.is_some());
    Ok(Some(Cleared {
        id,
        wall: o.class == class::WALL,
        record_goes: destroy_drops || roo_drops,
    }))
}
