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
use crate::players::PlayerId;
use std::collections::BTreeMap;

use crate::{GenieError, GenieResult};

use crate::map::spatial::Placed;
use crate::map::spatial::Spatial;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::BuildTuning;

/// What the footprint clearing (`FUN_1415c3620`) reads of the new building: its owner
/// (`param_2`), its master's id (`param_3`), class and radii, its place (`param_4`,
/// `param_5`), and whether the caller is the builder's or a new object's (`param_6`: the trees
/// at most of 1 hit point go too, and each deleted object's record with them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClearingSite {
    /// The building's owner.
    pub(crate) player: PlayerId,
    /// Its master's id.
    pub(crate) master: MasterId,
    /// Its master's class (`+0x410`).
    pub(crate) class: ClassId,
    /// Its master's radii (`+0x5d8`, `+0x5dc`).
    pub(crate) radius: Footprint,
    /// Its place.
    pub(crate) at: Point,
    /// `param_6`: who clears.
    pub(crate) by: Clearer,
}

/// Who calls the footprint clearing (`FUN_1415c3620`'s `param_6`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Clearer {
    /// The builder (1: the Build's `set_state(7)`) or a new object with its flag
    /// (`FUN_14162ce00`): the trees go too.
    Builder,
    /// The building constructor (0, `FUN_1411de5f0`).
    Constructor,
}

/// What the footprint clearing reads of an object on a tile besides its place: its hit points
/// (`+0x278`), its master's height (`+0x5e0`) and type, its state, whether it is a desolid
/// foundation (`+0xc60`) and whether its record went already (`mROOAlreadyCalled`, `+0x686`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClearLook {
    /// `+0x278`.
    pub(crate) hp: HitPoints,
    /// Its master's `radius_z` (`+0x5e0`).
    pub(crate) height: Distance,
    /// `+0x686`.
    pub(crate) roo_called: bool,
    /// Its master's type.
    pub(crate) kind: MasterKind,
    /// `+0x640`.
    pub(crate) state: ObjectState,
    /// `+0xc60`.
    pub(crate) desolid: bool,
}

/// An object the footprint clearing deletes, in the tiles' order: a wall (class 27) first goes
/// through the building's delete core (`FUN_1411ccbb0`), then every one is destroyed (slot
/// 0xd0); with `param_6` its record goes after (`FUN_141189670`, then `+0x686` set,
/// `FUN_141189b60`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cleared {
    /// The object.
    pub(crate) id: ObjectId,
    /// It is a wall (class 27): `FUN_1411ccbb0` before its destroy.
    pub(crate) wall: bool,
    /// Its record goes in the clearing: its destroy's death part drops it (a unit or a tall
    /// building not desolid of type 70 and up below state 3 whose record is still there,
    /// [`crate::objects::lifecycle::death_parts::combat_dies`], which sets `+0x686`), or with `param_6` `FUN_141189670`
    /// after the destroy does (`+0x686` still clear, and its master tall, COMISS: a NaN height
    /// too, or the object holding its record).
    pub(crate) record_goes: bool,
}

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
    roles: &Roles,
    build_tuning: &BuildTuning,
) -> GenieResult<Vec<Cleared>> {
    use crate::math::fmath::below_or_unordered;
    let (rx, ry) = (site.radius.x(), site.radius.y());
    if !below_or_unordered(0.0, rx) && !below_or_unordered(0.0, ry) {
        return Ok(Vec::new());
    }
    let Some(([x0, x1], [y0, y1])) = clearing_tiles(site.at, world.map, build_tuning) else {
        return Ok(Vec::new());
    };
    // a stopped player's units may have felled a tree there since they froze: the builder's
    // choice, which reads the trees' hit points, wants the tiles known; the constructor's reads
    // only masters and owners, which they do not change
    if site.by == Clearer::Builder {
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
            for (id, placed) in world.objects_on(Tile::new(i32::from(x), i32::from(y))) {
                if let Some(taken) = clearing_takes(site, id, placed, looks, roles, build_tuning)? {
                    cleared.push(taken);
                }
            }
        }
    }
    Ok(cleared)
}

/// The objects on [`footprint_clearing`]'s tiles about `at` ([`clearing_tiles`]), in its order:
/// those it may read the [`ClearLook`] of.
pub(crate) fn clearing_objects(
    world: &Spatial,
    at: Point,
    build_tuning: &BuildTuning,
) -> Vec<ObjectId> {
    let Some(([x0, x1], [y0, y1])) = clearing_tiles(at, world.map, build_tuning) else {
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

/// [`footprint_clearing`]'s tiles: from `BuildTuning::clearing_reach` before the place to as far
/// past it on each axis (CVTTSS2SI to 16 bits), held on the map; none when a range is empty.
pub(super) fn clearing_tiles(
    at: Point,
    map: GridSize,
    build_tuning: &BuildTuning,
) -> Option<([i16; 2], [i16; 2])> {
    use crate::math::conversions::cvttss2si;
    let (width, height) = (map.width(), map.height());
    let range = |coordinate: f32, size: i32| {
        let low = cvttss2si(coordinate - build_tuning.clearing_reach) as i16;
        let high = cvttss2si(coordinate + build_tuning.clearing_reach) as i16;
        let high = if size <= i32::from(high) {
            (size - 1) as i16
        } else {
            high
        };
        (low.max(0), high)
    };
    let (x0, x1) = range(at.x(), width);
    let (y0, y1) = range(at.y(), height);
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
    placed: &Placed,
    looks: &BTreeMap<ObjectId, ClearLook>,
    roles: &Roles,
    build_tuning: &BuildTuning,
) -> GenieResult<Option<Cleared>> {
    use crate::math::fmath::below_or_unordered;
    if placed.master == site.master {
        return Ok(None);
    }
    // The masters whose classes (39, the gate, and 52) take their owner's walls (27) off their
    // footprint, and the tree class (15).
    let under_gate = roles
        .of_class(site.class)
        .intersects(ClassRoles::GATE | ClassRoles::TOWER);
    // The wall class (27, `0x1b`): two walls beside a builder on a footprint's diagonal stop it.
    let own_wall = placed.player == site.player
        && under_gate
        && roles.of_class(placed.class).contains(ClassRoles::WALL);
    let tree =
        site.by == Clearer::Builder && roles.of_class(placed.class).contains(ClassRoles::TREE);
    if !placed.buildable_on.cleared() && !own_wall && !tree {
        return Ok(None);
    }
    let seen = *looks.get(&id).ok_or(GenieError::unsupported(
        "a footprint clearing of an object the slice does not hold",
    ))?;
    // the tree's test: COMISS 1.0 against its hit points, JC passes it over (a NaN too)
    if !placed.buildable_on.cleared()
        && !own_wall
        && below_or_unordered(build_tuning.stump_hp, seen.hp.get())
    {
        return Ok(None);
    }
    let (rx, ry) = (site.radius.x(), site.radius.y());
    let dx = (placed.position.x() - site.at.x()).abs();
    let dy = (placed.position.y() - site.at.y()).abs();
    if !(dx < placed.radius.x() + rx && dy < placed.radius.y() + ry) {
        return Ok(None);
    }
    // COMISS 0 against the height, JNC: a flag master of height 0 or less stays
    if roles
        .of_master(placed.master)
        .contains(MasterRoles::CLEARING_FLAG)
        && !below_or_unordered(0.0, seen.height.get())
    {
        return Ok(None);
    }
    // the destroy's death part (FUN_14121cc40 below state 3) drops the record and sets +0x686
    // first; then FUN_141189670: COMISS 0 against the height, JC drops the record (a NaN too),
    // else the record held (`+0x658`) does
    let dies =
        MasterKind::COMBAT <= seen.kind && seen.state < ObjectState::Dying && !seen.roo_called;
    let destroy_drops = dies
        && (seen.kind < MasterKind::BUILDING || (!seen.desolid && Distance::ZERO < seen.height));
    let roo_drops = site.by == Clearer::Builder
        && !seen.roo_called
        && !dies
        && (below_or_unordered(0.0, seen.height.get()) || placed.obstruction.is_some());
    Ok(Some(Cleared {
        id,
        wall: roles.of_class(placed.class).contains(ClassRoles::WALL),
        record_goes: destroy_drops || roo_drops,
    }))
}
