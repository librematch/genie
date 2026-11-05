//! Where an Enter meets its target (`FUN_140a85f30`): the landing tile, the walled-off test,
//! the meeting point and the reach.

use crate::garrison::ContainerId;
use crate::garrison::EnterKind;
use crate::garrison::transport::TRANSPORT;
use crate::garrison::{EnterAction, EnterTarget, NO_SPEED, WalkStart};
use crate::map::Distance;
use crate::map::GroundOffset;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::map::spatial::Spatial;
use crate::math::conversions::cvttss2si;
use crate::math::image::garrison::HALF_TILE as GARRISON_HALF_TILE;
use crate::movement::MoveStats;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::movement::TopSpeed;
use crate::movement::footprint_gap_squared;
use crate::movement::move_to::MOVE_TO;
use crate::movement::{Planning, footprint_gap};
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::masters::ClassId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::unit::Unit;
use crate::pathing::zones::{NO_TERRAIN, UNKNOWN_RESTRICTION, ZoneId, Zones};
use crate::world::Unread;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::objects_ref::ObjectsRef;
use crate::{GenieError, GenieResult};
use std::iter::FusedIterator;

/// Half a tile (`DAT_142e7f384`, 0.5): what an Enter's meeting point adds to a tile or to a
/// place before `floorf` (`FUN_140a85f30`), and how far a target leaves its point before the
/// Enter walks again (`FUN_140a7bd10`).
pub(super) const HALF_TILE: f32 = GARRISON_HALF_TILE;

/// An Enter whose transport cannot reach where it meets the unit (`set_state(13)` in
/// `set_state(4)`), which is not modelled.
pub(crate) const UNREACHED: GenieError =
    GenieError::unsupported("an Enter's transport that cannot reach the unit (state 13)");

/// What [`meeting_point`] (`FUN_140a85f30`) reads of an Enter's target.
#[derive(Clone, Debug)]
pub struct MeetingTarget {
    /// It has room for the unit (slot 0x198 with the unit).
    pub(crate) room: bool,
    /// Its speed (slot 0x208).
    pub(crate) speed: Speed,
    /// Its top action's state (slot 0x2b0, `RGE_Action_Object::actionState`; 0 without one).
    pub(crate) state: ActionState,
    /// Its class (`+0x410`).
    pub(crate) class: ClassId,
    /// Its place, x and y.
    pub(crate) position: Point,
    /// Its top action's point (`FUN_140a7be40`, `FUN_140a7be50`), or why the slice has not got it.
    pub(crate) action_point: GenieResult<Point>,
}

/// Where `FUN_140a85f30` meets an Enter's target, by the target alone ([`meet_at`]); the
/// shore's search and the beach test are [`meeting_point`]'s.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeetAt {
    /// At its place (code 0).
    Place,
    /// The shore on the way, else a beach under the unit (code 1), else its place.
    Shore,
    /// Where its top action goes (code 0).
    ActionPoint,
}

/// `FUN_140a85f30`'s branch by the target: one without room for the unit (slot 0x198), or
/// without speed (`COMISS 0, speed` with `JNC`: a NaN speed has some), is met at its place. One
/// idle (its action state 0) or in state 2 meets the unit at the shore. One under an action in
/// state 3, 4 or 11 meets it where that action goes, but a siege weapon (class 13) at its
/// place; any other state at its place.
#[must_use]
pub fn meet_at(meeting_target: &MeetingTarget, roles: &Roles) -> MeetAt {
    if !meeting_target.room || meeting_target.speed <= Speed::ZERO {
        return MeetAt::Place;
    }
    if matches!(meeting_target.state.get(), 0 | 2) {
        return MeetAt::Shore;
    }
    if roles
        .of_class(meeting_target.class)
        .contains(ClassRoles::SIEGE)
        || !matches!(meeting_target.state.get(), 11 | 4 | 3)
    {
        return MeetAt::Place;
    }
    MeetAt::ActionPoint
}

/// `FUN_141157840`'s walk (a unit's slot 0x428, its last argument 1): from tile `from` (a
/// transport's) toward tile `to` (the unit's) in twice as many steps as the longer axis's tiles
/// (the `y` axis only when it is strictly longer), each step adding the whole difference over
/// the step count in floats. It yields each new tile on the way with the float place reached
/// there; the first that lands (a beach in the unit's zone where the unit could stand, slot
/// 0x278: [`shore`]) is where the two meet. Nothing when the tiles are one.
#[derive(Clone, Debug)]
pub struct LandingWalk {
    /// The difference over the step count.
    step: GroundOffset,
    /// The float place reached.
    at: Point,
    /// The steps left.
    left: i32,
    /// The tile last yielded (-999, -999 before the first).
    last: Tile,
}

impl LandingWalk {
    /// The walk from tile `from` toward tile `to`.
    #[must_use]
    pub fn new(from: Tile, to: Tile) -> Self {
        let (dx, dy) = (to.x().wrapping_sub(from.x()), to.y().wrapping_sub(from.y()));
        let (fx, fy) = (dx as f32, dy as f32);
        let longer = if fy.abs() <= fx.abs() { dx } else { dy };
        let steps = cvttss2si(longer as f32).wrapping_abs().wrapping_mul(2);
        let left = if fx == 0.0 && fy == 0.0 {
            0
        } else {
            steps.max(0)
        };
        Self {
            step: GroundOffset::new(fx / steps as f32, fy / steps as f32),
            at: Point::new(from.x() as f32, from.y() as f32),
            left,
            last: Tile::new(-999, -999),
        }
    }
}

impl Iterator for LandingWalk {
    type Item = (Tile, Point);

    fn next(&mut self) -> Option<Self::Item> {
        while 0 < self.left {
            self.left -= 1;
            self.at += self.step;
            let tile = self.at.tile();
            if tile != self.last {
                self.last = tile;
                return Some((tile, self.at));
            }
        }
        None
    }
}

/// A walk with no step left takes none again.
impl FusedIterator for LandingWalk {}

/// `FUN_140a85c30`'s tile test for a range of at most 1 to a target with speed: about the tiles
/// of the unit's and the target's movement places (slot 0x520, `+0x824`; `me` and `them`),
/// their box (when it spans more than one tile on an axis, its first tile and the next, within
/// the map), each tile but those two where the unit (`unit`: its id and movement state) could
/// not stand (slot 0x278 at the tile's corner, [`Spatial::point_clear`] with its x radius and
/// its restriction's mask) counts the buildings on it ([`buildings_on`]). True when they are
/// exactly two: the unit is not in reach.
///
/// # Errors
///
/// What slot 0x278 and the count give.
pub fn walled_off(
    me: Point,
    them: Point,
    spatial: &mut Spatial,
    (id, stats): (ObjectId, &MoveStats),
    by_id: ObjectsRef<'_>,
) -> GenieResult<bool> {
    let map = spatial.map;
    let (mx, my) = (cvttss2si(me.x()), cvttss2si(me.y()));
    let (tx, ty) = (cvttss2si(them.x()), cvttss2si(them.y()));
    let (x0, y0) = (mx.min(tx), my.min(ty));
    let (mut x1, mut y1) = (mx.max(tx), my.max(ty));
    if 1 < x1.wrapping_sub(x0) || 1 < y1.wrapping_sub(y0) {
        x1 = (x0 + 1).min(map.width() - 1);
        y1 = (y0 + 1).min(map.height() - 1);
    }
    let mut count = 0usize;
    for x in x0..=x1 {
        for y in y0..=y1 {
            let tile = Tile::new(x, y);
            if (x, y) == (mx, my)
                || (x, y) == (tx, ty)
                || spatial.point_clear(
                    id,
                    tile.corner(),
                    Distance::new(stats.radius.x()),
                    stats.restriction_mask,
                    by_id,
                )?
            {
                continue;
            }
            count += buildings_on(spatial, by_id, tile)?;
        }
    }
    Ok(count == 2)
}

/// The buildings on `tile` for [`walled_off`]: the objects of the tile's list whose master
/// (`by_id`'s, its own first: [`crate::objects::masters::MasterTables::of_placed`]) is of type
/// 80.
///
/// # Errors
///
/// An object without its master.
fn buildings_on(spatial: &Spatial, by_id: ObjectsRef<'_>, tile: Tile) -> GenieResult<usize> {
    let mut count = 0;
    for (id, placed) in spatial.objects_on(tile) {
        let master = by_id.masters().of_placed(id, placed)?;
        count += usize::from(master.kind == MasterKind::BUILDING);
    }
    Ok(count)
}

/// [`MeetingTarget`] of the target `unit` with its action layer `target_actions`, why the load
/// could not read it whole `target_unread` ([`crate::world::Unread`]) and its top speed
/// `target_speed` (slot 0x208, [`crate::world::World::top_speed`]; of master `tm`) with `room`
/// for the unit: that speed, its top action's state and its point (`+0x38`,
/// `+0x3c`): a `MoveTo`'s ([`crate::movement::move_to::MoveToAction::point`]) or a Transport's. An
/// action the slice does not hold has neither; another no point the slice keeps.
pub(crate) fn meeting_target(
    (unit, target_actions, target_unread, target_speed): (&Unit, &Action, Option<&Unread>, Speed),
    tm: &Master,
    room: bool,
) -> GenieResult<MeetingTarget> {
    if let Some(unread) = target_unread {
        return Err(unread.error());
    }
    let action_point = match (
        target_actions.top_action_kind(),
        target_actions.list.move_to().copied(),
    ) {
        (Some(MOVE_TO), Some(move_to_action)) => Ok(move_to_action.point.ground()),
        // the Transport's point (+0x38, +0x3c), where it sails
        (Some(TRANSPORT), _) => target_actions
            .list
            .transport()
            .map(|tr| tr.point.ground())
            .ok_or(UNPLACED),
        _ => Err(UNPLACED),
    };
    Ok(MeetingTarget {
        room,
        speed: target_speed,
        state: target_actions
            .top_action()
            .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
        class: tm.class,
        position: unit.position().ground(),
        action_point,
    })
}

/// A transport's top action whose point the slice does not keep (`FUN_140a7be40`).
pub(super) const UNPLACED: GenieError =
    GenieError::unsupported("an Enter's transport under an action the slice does not place");

/// `FUN_140a85f30`: where an Enter of `unit` (its id, place and movement state) meets its
/// target `meeting_target` and the code its `set_state(4)` reads (1: the transport moves there), by
/// [`meet_at`]: at the target's place, where its top action goes, or at the shore
/// ([`shore_or_beach`]).
///
/// # Errors
///
/// A target under an action whose point the slice does not keep, and the shore's search's.
pub(crate) fn meeting_point(
    meeting_target: &MeetingTarget,
    unit: (ObjectId, Position, &MoveStats),
    spatial: &mut Spatial,
    zones: &mut Zones,
    by_id: ObjectsRef<'_>,
) -> GenieResult<(Point, u8)> {
    let roles = by_id.roles();
    match meet_at(meeting_target, roles) {
        MeetAt::Place => Ok((meeting_target.position, 0)),
        MeetAt::ActionPoint => Ok((meeting_target.action_point.clone()?, 0)),
        MeetAt::Shore => shore_or_beach(meeting_target, unit, spatial, zones, by_id),
    }
}

/// The shore case of `FUN_140a85f30`: where the unit's slot 0x428 ([`shore`]) finds the shore,
/// at the tile's centre (code 1); else, the unit standing on a beach (`FUN_1413fbf30`,
/// [`crate::map::terrain::Terrain::beach`]), at the unit's place rounded to the nearest corner
/// (`floorf` of each plus 0.5, code 1); else at the target's place.
///
/// # Errors
///
/// The shore's search's.
fn shore_or_beach(
    meeting_target: &MeetingTarget,
    unit: (ObjectId, Position, &MoveStats),
    spatial: &mut Spatial,
    zones: &mut Zones,
    by_id: ObjectsRef<'_>,
) -> GenieResult<(Point, u8)> {
    let (ux, uy) = (unit.1.x(), unit.1.y());
    if let Some(tile) = shore(meeting_target, unit, spatial, zones, by_id)? {
        let (x, y) = (tile.x() as f32, tile.y() as f32);
        return Ok((Point::new(x + HALF_TILE, y + HALF_TILE), 1));
    }
    let terrain = spatial.terrain.as_ref();
    if terrain.is_some_and(|tr| tr.beach(unit.1.ground().tile())) {
        return Ok((
            Point::new((ux + HALF_TILE).floor(), (uy + HALF_TILE).floor()),
            1,
        ));
    }
    Ok((meeting_target.position, 0))
}

/// The shore on the way from the target's tile to the unit's ([`LandingWalk`]): a beach
/// ([`crate::map::terrain::Terrain::beach`]) whose zone for the unit's restriction
/// (`FUN_1411958c0`: 0 for a tile below 0) is the zone of the unit's own tile (`FUN_14118eea0`),
/// where the unit could stand (slot 0x278, [`crate::map::spatial::Spatial::point_clear`] with
/// the master's x radius and the restriction's mask).
///
/// # Errors
///
/// A map without terrain or the unit's restriction, and slot 0x278's.
fn shore(
    meeting_target: &MeetingTarget,
    (id, at, stats): (ObjectId, Position, &MoveStats),
    spatial: &mut Spatial,
    zones: &mut Zones,
    by_id: ObjectsRef<'_>,
) -> GenieResult<Option<Tile>> {
    let (ux, uy) = (at.x(), at.y());
    let terrain = spatial.terrain.as_ref().ok_or(NO_TERRAIN)?;
    let values = terrain
        .restriction(stats.restriction)
        .ok_or(UNKNOWN_RESTRICTION)?;
    let map = zones.of(spatial.map, &terrain.kinds, values)?;
    let own = map.zone(Tile::new(cvttss2si(ux), cvttss2si(uy)));
    let from = meeting_target.position.tile();
    for (tile, place) in LandingWalk::new(from, Tile::new(cvttss2si(ux), cvttss2si(uy))) {
        let (x, y) = (tile.x(), tile.y());
        let terrain = spatial.terrain.as_ref().ok_or(NO_TERRAIN)?;
        if !terrain.beach(tile) {
            continue;
        }
        let zone = if x < 0 || y < 0 {
            ZoneId::OFF_MAP
        } else {
            map.zone(Tile::new(x, y))
        };
        if zone == own
            && spatial.point_clear(
                id,
                place,
                Distance::new(stats.radius.x()),
                stats.restriction_mask,
                by_id,
            )?
        {
            return Ok(Some(tile));
        }
    }
    Ok(None)
}

/// What `FUN_140a85c30`'s tile test ([`walled_off`]) reads.
#[derive(Debug)]
pub struct Walls<'world> {
    /// Where the unit last moved to (`+0x824`), x and y.
    pub(crate) me: Point,
    /// Where the target last moved to (slot 0x520), x and y.
    pub(crate) them: Point,
    /// The map's places and tiles.
    pub(crate) spatial: &'world mut Spatial,
    /// The unit's id.
    pub(crate) id: ObjectId,
    /// The unit's movement state.
    pub(crate) stats: &'world MoveStats,
}

impl Walls<'_> {
    /// [`walled_off`] about these places.
    ///
    /// # Errors
    ///
    /// [`walled_off`]'s.
    pub fn walled_off(&mut self, by_id: ObjectsRef<'_>) -> GenieResult<bool> {
        walled_off(
            self.me,
            self.them,
            self.spatial,
            (self.id, self.stats),
            by_id,
        )
    }
}

/// [`EnterAction::in_reach`] of the unit `id` (its movement state `stats`, its place in
/// `spatial`) for its target `target` (where it last moved to, by its mover, and its top speed
/// slot 0x208, `0x140a85c82`): the squared
/// footprints' gap of their places, and the tile test ([`walled_off`]) about where each last
/// moved to (slot 0x520), its buildings those whose master (`by_id`'s) is of type 80.
pub(crate) fn enter_reach(
    spatial: &mut Spatial,
    (id, stats, moved_at): (ObjectId, &MoveStats, Position),
    (target, target_mover, target_speed): (ObjectId, Option<&Mover>, Speed),
    range: Distance,
    by_id: ObjectsRef<'_>,
) -> GenieResult<bool> {
    let me = spatial
        .get_known(id)?
        .ok_or(GenieError::unsupported("an entering unit without a place"))?;
    let them = spatial.get_known(target)?.ok_or(GenieError::unsupported(
        "an Enter's building without a place",
    ))?;
    let gap = footprint_gap_squared(me.position, me.radius, them.position, them.radius);
    let speed = target_speed;
    let them_at = target_mover.map_or(them.position, |mover| mover.moved_at);
    let mut walls = Walls {
        me: moved_at.ground(),
        them: them_at.ground(),
        spatial,
        id,
        stats,
    };
    EnterAction::in_reach(range, gap, speed, &mut walls, by_id)
}

/// What an Enter's update reads of its target `target` with its mover (of master `tm`, with `room`
/// for `unit`, with its mover): the
/// [`EnterTarget`] (allied: its own player's; the reach by [`enter_reach`] with the task's
/// `range`), and the footprints' gap (`FUN_14118f680`) and the unit's speed for a new `MoveTo`
/// ([`WalkStart`]).
///
/// # Errors
///
/// A unit without movement state ([`NO_SPEED`]), and what the reach does not model.
pub(crate) fn enter_view(
    spatial: &mut Spatial,
    (unit, mover): (&Unit, Option<&Mover>),
    ((target, target_mover, target_speed), tm, room): (
        (&Unit, Option<&Mover>, Speed),
        &Master,
        bool,
    ),
    range: Distance,
    by_id: ObjectsRef<'_>,
) -> GenieResult<(EnterTarget, WalkStart)> {
    let kind = if tm.kind == MasterKind::BUILDING {
        EnterKind::Building
    } else {
        EnterKind::Transport
    };
    let stats = mover.map(|mover| mover.stats).ok_or(NO_SPEED)?;
    let moved_at = mover.map_or_else(|| unit.position(), |mover| mover.moved_at);
    let target_mover = if kind == EnterKind::Building {
        None
    } else {
        target_mover
    };
    let in_reach = enter_reach(
        spatial,
        (unit.id, &stats, moved_at),
        (target.id, target_mover, target_speed),
        range,
        by_id,
    )?;
    // the reach above read both records (`enter_reach`), and nothing between removes one
    let placed = |id: ObjectId| {
        spatial.get(id).ok_or_else(|| {
            GenieError::unreachable("an Enter's unit or target unplaced after its reach")
                .with("object", id)
        })
    };
    let (me, them) = (placed(unit.id)?, placed(target.id)?);
    let gap = footprint_gap(me.position, me.radius, them.position, them.radius);
    Ok((
        EnterTarget {
            id: ContainerId::new(target.id),
            state: target.life.state,
            allied: true,
            room,
            position: target.position(),
            in_reach,
            kind,
        },
        WalkStart { gap },
    ))
}

/// What `FUN_140a85f30` ([`meeting_point`]) reads where an Enter's `set_state(4)` or its constructor
/// meets its target: the map and its zones, the target with its master and whether it has room
/// for the unit, and the unit.
pub(crate) struct Meeting<'world> {
    /// The map, its places and its terrain.
    pub(crate) spatial: &'world mut Spatial,
    /// The zone maps the shore's test reads (built on first use).
    pub(crate) zones: &'world mut Zones,
    /// The target.
    pub(crate) target: &'world Unit,
    /// The target's action layer.
    pub(crate) target_actions: &'world Action,
    /// Why the load could not read the target whole, if it could not ([`crate::world::Unread`]).
    pub(crate) target_unread: Option<&'world Unread>,
    /// The target's master.
    pub(crate) master: &'world Master,
    /// The target has room for the unit (slot 0x198).
    pub(crate) room: bool,
    /// The entering unit.
    pub(crate) unit: &'world Unit,
    /// The entering unit's mover (its motion layer's), none for one that does not move.
    pub(crate) mover: Option<&'world Mover>,
    /// The target's top speed (slot 0x208, `0x140a8602b`).
    pub(crate) target_speed: Speed,
}

impl Meeting<'_> {
    /// Where the unit meets the target and the code `set_state(4)` reads: a building at its
    /// place (code 0); a transport by [`meeting_target`] and [`meeting_point`].
    ///
    /// # Errors
    ///
    /// A unit without movement state ([`NO_SPEED`]), and what the meeting does not model.
    pub(crate) fn point(self, by_id: ObjectsRef<'_>) -> GenieResult<(Point, u8)> {
        let at = self.target.position();
        if self.master.kind == MasterKind::BUILDING {
            return Ok((at.ground(), 0));
        }
        let stats = self.mover.map(|mover| mover.stats).ok_or(NO_SPEED)?;
        let meeting_target = meeting_target(
            (
                self.target,
                self.target_actions,
                self.target_unread,
                self.target_speed,
            ),
            self.master,
            self.room,
        )?;
        meeting_point(
            &meeting_target,
            (self.unit.id, self.unit.position(), &stats),
            self.spatial,
            self.zones,
            by_id,
        )
    }
}

/// A transport's slot 0x3d8 (`FUN_14114ffc0`) for an Enter: whether `ship` reaches within
/// `GarrisonTuning::enter_near` of the point's tile (the point truncated), [`crate::movement::Mover::reaches`]
/// from where it last moved to (`actions`, its action layer; `top_speed`, what its top speed
/// reads); one without movement state does not.
pub(crate) fn ship_reaches(
    (ship, actions, (mover, top_speed)): (&Unit, &Action, (Option<&Mover>, TopSpeed)),
    world: &mut Spatial,
    plan: &mut Planning<'_>,
    to: Position,
    by_id: ObjectsRef<'_>,
) -> GenieResult<bool> {
    let garrison_tuning = &by_id.tuning().garrison;
    let Some(mover) = mover else {
        return Ok(false);
    };
    let me = ship.me(actions, top_speed);
    let goal = Point::new(cvttss2si(to.x()) as f32, cvttss2si(to.y()) as f32);
    mover.reaches(
        (ship.position(), &me),
        (world, by_id),
        plan,
        goal,
        Distance::new(garrison_tuning.enter_near),
        ship.life.master_id,
    )
}

#[cfg(test)]
mod tests {
    use crate::garrison::LandingWalk;
    use crate::map::Tile;

    /// A walk takes each tile it enters once, then none again.
    #[test]
    fn landing_walk_stays_at_its_end() {
        let mut walk = LandingWalk::new(Tile::new(0, 0), Tile::new(3, 0));
        let tiles: Vec<Tile> = walk.by_ref().map(|(tile, _)| tile).collect();
        assert_eq!(tiles.last(), Some(&Tile::new(3, 0)));
        assert!(walk.next().is_none() && walk.next().is_none());
    }
}
