//! Where an Enter meets its target (`FUN_140a85f30`): the landing tile, the walled-off test,
//! the meeting point and the reach.

use crate::GenieError;
use crate::garrison::{EnterAction, EnterTarget, NO_SPEED, WalkStart};
use crate::map::Distance;
use crate::map::GroundOffset;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::map::spatial::Spatial;
use crate::math::conversions::cvttss2si;
use crate::movement::MoveStats;
use crate::movement::Speed;
use crate::movement::footprint_gap_squared;
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::{Master, Masters, class, player_master};

/// How near the point a transport must reach for an Enter (slot 0x3d8's range), and the range up
/// to which an Enter's reach of a target with speed tests the tiles about the two
/// (`FUN_140a85c30`): `DAT_14330eb8c`, 1.0.
pub(super) const ENTER_NEAR: Distance = Distance::new(crate::math::image::garrison::ENTER_NEAR);
/// Half a tile (`DAT_142e7f384`, 0.5): what an Enter's meeting point adds to a tile or to a
/// place before `floorf` (`FUN_140a85f30`), and how far a target leaves its point before the
/// Enter walks again (`FUN_140a7bd10`).
pub(super) const HALF_TILE: f32 = crate::math::image::garrison::HALF_TILE;

/// An Enter whose transport cannot reach where it meets the unit (`set_state(13)` in
/// `set_state(4)`), which is not modelled.
pub(crate) const UNREACHED: GenieError =
    GenieError::unsupported("an Enter's transport that cannot reach the unit (state 13)");

/// What [`meeting_point`] (`FUN_140a85f30`) reads of an Enter's target.
#[derive(Clone, Debug)]
pub struct MeetingTarget {
    /// It has room for the unit (slot 0x198 with the unit).
    pub room: bool,
    /// Its speed (slot 0x208).
    pub speed: Speed,
    /// Its top action's state (slot 0x2b0, `RGE_Action_Object::actionState`; 0 without one).
    pub state: ActionState,
    /// Its class (`+0x410`).
    pub class: ClassId,
    /// Its place, x and y.
    pub position: Point,
    /// Its top action's point (`FUN_140a7be40`, `FUN_140a7be50`), or why the slice has not got it.
    pub action_point: Result<Point, GenieError>,
}

/// Where `FUN_140a85f30` meets an Enter's target, by the target alone ([`meet_at`]); the
/// shore's search and the beach test are [`meeting_point`]'s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
pub fn meet_at(t: &MeetingTarget) -> MeetAt {
    if !t.room || t.speed <= Speed::ZERO {
        return MeetAt::Place;
    }
    if matches!(t.state.get(), 0 | 2) {
        return MeetAt::Shore;
    }
    if t.class == class::SIEGE || !matches!(t.state.get(), 11 | 4 | 3) {
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
            self.at = self.at.moved(self.step);
            let tile = self.at.tile();
            if tile != self.last {
                self.last = tile;
                return Some((tile, self.at));
            }
        }
        None
    }
}

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
    masters: &[Masters],
    (id, stats): (ObjectId, &MoveStats),
) -> Result<bool, GenieError> {
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
                )?
            {
                continue;
            }
            count += buildings_on(spatial, masters, tile)?;
        }
    }
    Ok(count == 2)
}

/// The buildings on `tile` for [`walled_off`]: the objects of the tile's list whose
/// master (`masters`, each player's) is of type 80.
///
/// # Errors
///
/// An object whose master the slice lacks.
fn buildings_on(spatial: &Spatial, masters: &[Masters], tile: Tile) -> Result<usize, GenieError> {
    let mut n = 0;
    for (_, o) in spatial.objects_on(tile) {
        let m = player_master(masters, o.player, o.master).ok_or(GenieError::unsupported(
            "an object on a tile of a master the slice lacks",
        ))?;
        n += usize::from(m.kind == BUILDING);
    }
    Ok(n)
}

/// [`MeetingTarget`] of the target `t` (of master `tm`) with `room` for the unit: its top speed
/// ([`crate::movement::MoveStats::speed`]), its top action's state and its point (`+0x38`,
/// `+0x3c`): a `MoveTo`'s ([`crate::movement::move_to::MoveToAction::point`]) or a Transport's.
/// An action the slice does not hold has neither; another no point the slice keeps.
pub(crate) fn meeting_target(
    t: &crate::objects::unit::Unit,
    tm: &Master,
    room: bool,
) -> Result<MeetingTarget, GenieError> {
    if let Some(e) = t.unmodelled_action.clone() {
        return Err(e);
    }
    let action_point = match (t.top_action_kind(), t.actions.move_to().copied()) {
        (Some(crate::movement::move_to::MOVE_TO), Some(m)) => Ok(m.point.ground()),
        // the Transport's point (+0x38, +0x3c), where it sails
        (Some(crate::garrison::transport::TRANSPORT), _) => t
            .actions
            .transport()
            .map(|tr| tr.point.ground())
            .ok_or(UNPLACED),
        _ => Err(UNPLACED),
    };
    Ok(MeetingTarget {
        room,
        speed: t.body.mover().map_or(Speed::ZERO, |m| m.stats.speed),
        state: t.top_action().map_or(ActionState::NO_ACTION, |a| a.state),
        class: tm.class,
        position: t.position().ground(),
        action_point,
    })
}

/// A transport's top action whose point the slice does not keep (`FUN_140a7be40`).
pub(super) const UNPLACED: GenieError =
    GenieError::unsupported("an Enter's transport under an action the slice does not place");

/// `FUN_140a85f30`: where an Enter of `unit` (its id, place and movement state) meets its
/// target `t` and the code its `set_state(4)` reads (1: the transport moves there), by
/// [`meet_at`]: at the target's place, where its top action goes, or at the shore
/// ([`shore_or_beach`]).
///
/// # Errors
///
/// A target under an action whose point the slice does not keep, and the shore's search's.
pub(crate) fn meeting_point(
    t: &MeetingTarget,
    unit: (ObjectId, Position, &MoveStats),
    spatial: &mut Spatial,
    zones: &mut crate::pathing::zones::Zones,
) -> Result<(Point, u8), GenieError> {
    match meet_at(t) {
        MeetAt::Place => Ok((t.position, 0)),
        MeetAt::ActionPoint => Ok((t.action_point.clone()?, 0)),
        MeetAt::Shore => shore_or_beach(t, unit, spatial, zones),
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
    t: &MeetingTarget,
    unit: (ObjectId, Position, &MoveStats),
    spatial: &mut Spatial,
    zones: &mut crate::pathing::zones::Zones,
) -> Result<(Point, u8), GenieError> {
    let (ux, uy) = (unit.1.x(), unit.1.y());
    if let Some(tile) = shore(t, unit, spatial, zones)? {
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
    Ok((t.position, 0))
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
    t: &MeetingTarget,
    (id, at, stats): (ObjectId, Position, &MoveStats),
    spatial: &mut Spatial,
    zones: &mut crate::pathing::zones::Zones,
) -> Result<Option<Tile>, GenieError> {
    let (ux, uy) = (at.x(), at.y());
    let terrain = spatial
        .terrain
        .as_ref()
        .ok_or(crate::pathing::zones::NO_TERRAIN)?;
    let values = terrain
        .restriction(stats.restriction)
        .ok_or(crate::pathing::zones::UNKNOWN_RESTRICTION)?;
    let map = zones.of(spatial.map, &terrain.kinds, values)?;
    let own = map.zone(Tile::new(cvttss2si(ux), cvttss2si(uy)));
    let from = t.position.tile();
    for (tile, place) in LandingWalk::new(from, Tile::new(cvttss2si(ux), cvttss2si(uy))) {
        let (x, y) = (tile.x(), tile.y());
        let terrain = spatial
            .terrain
            .as_ref()
            .ok_or(crate::pathing::zones::NO_TERRAIN)?;
        if !terrain.beach(tile) {
            continue;
        }
        let zone = if x < 0 || y < 0 {
            crate::pathing::zones::ZoneId::OFF_MAP
        } else {
            map.zone(Tile::new(x, y))
        };
        if zone == own
            && spatial.point_clear(
                id,
                place,
                Distance::new(stats.radius.x()),
                stats.restriction_mask,
            )?
        {
            return Ok(Some(tile));
        }
    }
    Ok(None)
}

/// What `FUN_140a85c30`'s tile test ([`walled_off`]) reads.
#[derive(Debug)]
pub struct Walls<'a> {
    /// Where the unit last moved to (`+0x824`), x and y.
    pub me: Point,
    /// Where the target last moved to (slot 0x520), x and y.
    pub them: Point,
    /// The map's places and tiles.
    pub spatial: &'a mut Spatial,
    /// Each player's masters, which tell the buildings.
    pub masters: &'a [Masters],
    /// The unit's id.
    pub id: ObjectId,
    /// The unit's movement state.
    pub stats: &'a MoveStats,
}

impl Walls<'_> {
    /// [`walled_off`] about these places.
    ///
    /// # Errors
    ///
    /// [`walled_off`]'s.
    pub fn walled_off(&mut self) -> Result<bool, GenieError> {
        walled_off(
            self.me,
            self.them,
            self.spatial,
            self.masters,
            (self.id, self.stats),
        )
    }
}

/// [`EnterAction::in_reach`] of the unit `id` (its movement state `stats`, its place in
/// `spatial`) for its target `target` (its top speed and where it last moved to): the squared
/// footprints' gap of their places, and the tile test ([`walled_off`]) about where each last
/// moved to (slot 0x520), its buildings those whose master (`masters`, each player's) is of
/// type 80.
pub(crate) fn enter_reach(
    spatial: &mut Spatial,
    masters: &[Masters],
    (id, stats, moved_at): (ObjectId, &MoveStats, Position),
    (target, target_mover): (ObjectId, Option<&crate::movement::Mover>),
    range: Distance,
) -> Result<bool, GenieError> {
    let me = spatial
        .get_known(id)?
        .ok_or(GenieError::unsupported("an entering unit without a place"))?;
    let them = spatial.get_known(target)?.ok_or(GenieError::unsupported(
        "an Enter's building without a place",
    ))?;
    let gap = footprint_gap_squared(me.position, me.radius, them.position, them.radius);
    let speed = target_mover.map_or(Speed::ZERO, |m| m.stats.speed);
    let them_at = target_mover.map_or(them.position, |m| m.moved_at);
    let mut walls = Walls {
        me: moved_at.ground(),
        them: them_at.ground(),
        spatial,
        masters,
        id,
        stats,
    };
    EnterAction::in_reach(range, gap, speed, &mut walls)
}

/// What an Enter's update reads of its target `t` (of master `tm`, with `room` for `unit`): the
/// [`EnterTarget`] (allied: its own player's; the reach by [`enter_reach`] with the task's
/// `range`), and the footprints' gap (`FUN_14118f680`) and the unit's speed for a new `MoveTo`
/// ([`WalkStart`]).
///
/// # Errors
///
/// A unit without movement state ([`NO_SPEED`]), and what the reach does not model.
pub(crate) fn enter_view(
    spatial: &mut Spatial,
    masters: &[Masters],
    unit: &crate::objects::unit::Unit,
    (t, tm, room): (&crate::objects::unit::Unit, &Master, bool),
    range: Distance,
) -> Result<(EnterTarget, WalkStart), GenieError> {
    let building = tm.kind == BUILDING;
    let mover = unit.body.mover();
    let stats = mover.map(|m| m.stats).ok_or(NO_SPEED)?;
    let moved_at = mover.map_or_else(|| unit.position(), |m| m.moved_at);
    let target_mover = if building { None } else { t.body.mover() };
    let in_reach = enter_reach(
        spatial,
        masters,
        (unit.id, &stats, moved_at),
        (t.id, target_mover),
        range,
    )?;
    let placed = |id: ObjectId| {
        spatial.get(id).ok_or_else(|| {
            GenieError::unreachable("an Enter's unit or target unplaced after its reach")
                .with("object", id)
        })
    };
    let (me, them) = (placed(unit.id)?, placed(t.id)?);
    let gap = crate::movement::footprint_gap(me.position, me.radius, them.position, them.radius);
    Ok((
        EnterTarget {
            state: t.life.state,
            allied: true,
            room,
            position: t.position(),
            in_reach,
            building,
        },
        WalkStart { gap },
    ))
}

/// What `FUN_140a85f30` ([`meeting_point`]) reads where an Enter's `set_state(4)` or its constructor
/// meets its target: the map and its zones, the target with its master and whether it has room
/// for the unit, and the unit.
pub(crate) struct Meeting<'a> {
    /// The map, its places and its terrain.
    pub(crate) spatial: &'a mut Spatial,
    /// The zone maps the shore's test reads (built on first use).
    pub(crate) zones: &'a mut crate::pathing::zones::Zones,
    /// The target.
    pub(crate) target: &'a crate::objects::unit::Unit,
    /// The target's master.
    pub(crate) master: &'a Master,
    /// The target has room for the unit (slot 0x198).
    pub(crate) room: bool,
    /// The entering unit.
    pub(crate) unit: &'a crate::objects::unit::Unit,
}

impl Meeting<'_> {
    /// Where the unit meets the target and the code `set_state(4)` reads: a building at its
    /// place (code 0); a transport by [`meeting_target`] and [`meeting_point`].
    ///
    /// # Errors
    ///
    /// A unit without movement state ([`NO_SPEED`]), and what the meeting does not model.
    pub(crate) fn point(self) -> Result<(Point, u8), GenieError> {
        let at = self.target.position();
        if self.master.kind == BUILDING {
            return Ok((at.ground(), 0));
        }
        let stats = self.unit.body.mover().map(|m| m.stats).ok_or(NO_SPEED)?;
        let t = meeting_target(self.target, self.master, self.room)?;
        meeting_point(
            &t,
            (self.unit.id, self.unit.position(), &stats),
            self.spatial,
            self.zones,
        )
    }
}

/// A transport's slot 0x3d8 (`FUN_14114ffc0`) for an Enter: whether `ship` reaches within
/// [`ENTER_NEAR`] of the point's tile (the point truncated), [`crate::movement::Mover::reaches`]
/// from where it last moved to; one without movement state does not.
pub(crate) fn ship_reaches(
    ship: &crate::objects::unit::Unit,
    world: &mut Spatial,
    plan: &mut crate::movement::Planning<'_>,
    to: Position,
) -> Result<bool, GenieError> {
    let Some(m) = ship.body.mover() else {
        return Ok(false);
    };
    let me = crate::movement::Me {
        id: ship.id,
        action_state: ship
            .top_action()
            .map_or(ActionState::NO_ACTION, |a| a.state),
        group: ship.group.id,
    };
    let goal = Point::new(cvttss2si(to.x()) as f32, cvttss2si(to.y()) as f32);
    m.reaches(&me, world, plan, goal, ENTER_NEAR, ship.life.master_id)
}
