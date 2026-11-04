use super::*;
use crate::combat::damage::Victim;
use crate::garrison::transport::StartCall;
use crate::garrison::transport::TransportTarget;
use crate::gathering::hunt::tests::walker;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Restriction;
use crate::map::TerrainFlags;
use crate::map::Tile;
use crate::map::spatial::Spatial;
use crate::map::terrain::TileTerrain;
use crate::map::terrain::{Terrain, TerrainKind};
use crate::movement::Me;
use crate::movement::Speed;
use crate::movement::move_to::SubCode;
use crate::movement::move_to::{MoveDefaults, MoveTo, MoveToAction, MoveToReads, SpriteReads};
use crate::objects::ActionState;
use crate::objects::Graphics;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::UpdateCode;
use crate::objects::layers::{Motion, MotionMut, Static};
use crate::objects::masters::ActionPart;
use crate::objects::masters::MovingPart;
use crate::objects::masters::TaskSprites;
use crate::objects::sprite::ActiveSprite;
use crate::objects::unit::Unit;
use crate::objects::{FacetCount, Frame};
use crate::pathing::Pathing;
use crate::pathing::zones::ZoneId;
use crate::pathing::zones::Zones;
use crate::players::PlayerKind;
use crate::test_ids::object_id;
use crate::test_ids::task_id;
use crate::testing::tuning;
use crate::unit_ai::AiAction;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::UnitAi;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::parts::PartTable;
use std::iter::once;
use std::sync::Arc;

/// The map the transport sails on: its terrain, its zone maps and its planners, made on first
/// use from the terrain ([`MapShore`]).
struct Coast {
    spatial: Spatial,
    zones: Zones,
    pathing: Pathing,
}

/// The ship's terrain restriction: it sails [`WATER`] and [`SHOAL`].
const SAILS: RestrictionId = RestrictionId::named(1);
/// Land: the terrain table's shallows and beach bits; the ship cannot sail it.
const LAND: u8 = 0;
/// Water: the terrain table's water bit; the ship sails it.
const WATER: u8 = 1;
/// Water the ship cannot sail.
const DEEP: u8 = 2;
/// Land the ship sails.
const SHOAL: u8 = 3;

/// A [`ship`]'s map (40 by 40: the labelling starts patches only below the height on both axes,
/// `FUN_141413190`) of terrain `of` per tile ([`LAND`], [`WATER`], [`DEEP`], [`SHOAL`]);
/// restriction 0 walks [`LAND`] and [`DEEP`], [`SAILS`] sails the other two.
fn coast_of(of: impl Fn(Tile) -> u8) -> Coast {
    let size = GridSize::new(40, 40);
    let tiles: Vec<u8> = (0..40)
        .flat_map(|y| (0..40).map(move |x| Tile::new(x, y)))
        .map(of)
        .collect();
    let kind = |flags| TerrainKind {
        shown: true,
        flags,
        forest: false,
    };
    let restrictions = vec![
        Restriction::new(vec![1.0, 0.0, 1.0, 0.0]),
        Restriction::new(vec![0.0, 1.0, 0.0, 1.0]),
    ];
    let mut spatial = Spatial::new(size);
    spatial.terrain = Some(Terrain::new(
        size,
        tiles
            .iter()
            .map(|&k| TileTerrain::of_save(k, 255, -1))
            .collect(),
        vec![
            kind(TerrainFlags::SHORE),
            kind(TerrainFlags::WATER),
            kind(TerrainFlags::WATER),
            kind(TerrainFlags::SHORE),
        ],
        restrictions.clone(),
    ));
    Coast {
        spatial,
        zones: Zones::default(),
        pathing: Pathing::new(
            size,
            tiles.into_iter().map(TerrainId::named).collect(),
            restrictions,
            0,
        ),
    }
}

/// The coast: [`WATER`] west of x 20, [`LAND`] from x 20. The ship's zone map holds the water as
/// zone 0, the land as zone 1.
fn coast() -> Coast {
    coast_of(|t| if t.x() < 20 { WATER } else { LAND })
}

/// [`coast`] with a checkerboard of single-tile patches north-east of it (x 22 and up, y below
/// 16; `sailed` the terrain the ship sails there, `walked` the other) behind two columns of
/// land: the ship's zone map numbers them past 256, and zone 256 has the water's low byte (0),
/// which the near test reads ([`MapShore::near`]).
fn aliased_coast(sailed: u8, walked: u8) -> Coast {
    coast_of(move |t| match (t.x(), t.y()) {
        (x, _) if x < 20 => WATER,
        (x, y) if 22 <= x && y < 16 => {
            if (x + y) % 2 == 0 {
                sailed
            } else {
                walked
            }
        }
        _ => LAND,
    })
}

impl Coast {
    /// The map about the ship at `at` ([`SAILS`]).
    fn shore(&mut self, at: Position) -> MapShore<'_> {
        MapShore::new(
            &self.spatial,
            &mut self.zones,
            &mut self.pathing,
            Some(SAILS),
            at,
            &tuning().tile_path,
        )
        .unwrap()
    }

    /// The centre of the first tile, row by row, of the ship's zone `zone`.
    fn tile_of(&mut self, zone: u16) -> Position {
        let mut shore = self.shore(Position::ORIGIN);
        let tile = (0..40)
            .flat_map(|y| (0..40).map(move |x| Tile::new(x, y)))
            .find(|&t| shore.zone(t) == Ok(ZoneId::new(zone)))
            .unwrap();
        Position::new(tile.x() as f32 + 0.5, tile.y() as f32 + 0.5, 0.0)
    }
}

/// The sprite a hull shows first, and its master's standing sprite.
const SHOWN: Graphic = Graphic {
    id: SpriteId::named(1),
    frames: Frame::of_data(10),
    facets: FacetCount::of_data(16),
    speed_multiplier: SpeedMultiplier::ONE,
    frame_duration: Seconds::new(0.1),
    replay_delay: Seconds::ZERO,
    flags: SpriteFlags::ANIMATED,
};
const STANDING: Graphic = Graphic {
    id: SpriteId::named(300),
    ..SHOWN
};

/// The transport's unit as the update's effects read it: a mover in motion (its animation at
/// speed 1), a sprite ([`SHOWN`]); and the table of its unit AI, which takes its notifies (state
/// 2).
fn hull() -> (Unit, Motion, PartTable<UnitAi>) {
    let mut mover = walker();
    mover.animated.speed = Speed::new(1.0);
    let mut unit = Unit {
        id: object_id(9),
        place: Static::at(Position::new(12.5, 8.5, 0.0)),
        sprite: Some(ActiveSprite::start(
            SHOWN,
            &mut Well512::default(),
            tuning().sprite,
        )),
        ..Unit::default()
    };
    unit.life.state = ObjectState::Alive;
    let ais = once((unit.id, UnitAi::default())).collect();
    (unit, Motion::Moving(mover), ais)
}

/// What a state change left the hull ([`left`]).
#[derive(Debug, PartialEq)]
struct Left {
    /// Stopped: its animation at speed 0.
    stopped: bool,
    /// Standing: the standing sprite shown.
    stands: bool,
    /// The notifies it posted: kind and parameters.
    posted: Vec<(NotifyKind, [i32; 3])>,
}

/// What a state change left `unit` with its `motion` and its unit AI among `ais`.
fn left((unit, motion, ais): (&Unit, &Motion, &PartTable<UnitAi>)) -> Left {
    Left {
        stopped: motion.animated().speed == Speed::new(0.0),
        stands: unit.sprite.map(|s| s.graphic.id) == Some(STANDING.id),
        posted: ais.get(unit.id).map_or_else(Vec::new, |ai| {
            ai.notifies.iter().map(|n| (n.kind, n.p)).collect()
        }),
    }
}

/// One update's state as the unit's runs it after its `MoveTo`, which walks nowhere here
/// (none or ended: 1; failed: 3), for a human player, or a `computer` one whose AI would hear
/// the owner's alert (not modelled: an error).
fn run_on(
    t: &mut TransportAction,
    s: &Ship<'_>,
    c: &mut Coast,
    hull: (&mut Unit, MotionMut<'_>, &mut PartTable<UnitAi>),
    player_kind: PlayerKind,
) -> Result<Updated, GenieError> {
    let walked = match t.state.move_to().copied().map(|m| m.state) {
        Some(MoveTo::Failed) => SubCode::Failed,
        Some(MoveTo::Start | MoveTo::Moving | MoveTo::Waiting) => SubCode::UnderWay,
        None | Some(MoveTo::Arrived) => SubCode::Done,
    };
    let look = (Some(STANDING), &mut Well512::default());
    let mut shore = c.shore(s.position);
    t.state_update(walked, (s, &mut shore), hull, look, player_kind, tuning())
}

/// [`run_on`] on a fresh [`hull`] of a human player: the update's answer and what it left.
fn run(
    t: &mut TransportAction,
    s: &Ship<'_>,
    c: &mut Coast,
) -> (Result<Updated, GenieError>, Left) {
    let (mut unit, mut motion, mut ais) = hull();
    let r = run_on(
        t,
        s,
        c,
        (&mut unit, motion.lent(), &mut ais),
        PlayerKind::Human,
    );
    (r, left((&unit, &motion, &ais)))
}

/// The owner's alert, which a computer player's AI would hear ([`run_on`]).
fn alerts(t: &TransportAction, s: &Ship<'_>, c: &mut Coast) -> bool {
    let mut t = *t;
    let (mut unit, mut motion, mut ais) = hull();
    run_on(
        &mut t,
        s,
        c,
        (&mut unit, motion.lent(), &mut ais),
        PlayerKind::Computer,
    )
    .is_err_and(|e| {
        e.reason()
            == "a computer player's AI hearing its unit's alert (slot 0x2e8, then slot 0x3c8)"
    })
}

/// The sprites' table of a world without sprites.
static NO_GRAPHICS: Graphics = Graphics::new();

fn ship(loaded: bool) -> Ship<'static> {
    Ship {
        position: Position::new(12.5, 8.5, 0.0),
        radius: Footprint::new(0.5, 0.5),
        loaded,
        map: GridSize::new(40, 40),
        walk: MoveToReads {
            sprites: SpriteReads {
                defaults: MoveDefaults::default(),
                graphics: &NO_GRAPHICS,
            },
            state: ObjectState::Alive,
            speed: Speed::new(1.0),
            at: Position::new(12.5, 8.5, 0.0),
            ground: Footprint::new(0.5, 0.5),
        },
        given: None,
    }
}

#[test]
fn new_transport_sails_to_its_own_place_and_start_off_the_map_idles() {
    let s = ship(true);
    let mut c = coast();
    let mut t = TransportAction::new(s.position);
    assert_eq!(
        (t.state.bare(), t.mode, t.retarget),
        (TransportState::New, TransportMode::New, false)
    );
    // the list's add (first_in_stack with 1): set_state(11), its MoveTo to the shore point
    // on the way, none here, so the place itself
    let changed = t
        .start(
            StartCall::Added,
            &s,
            &mut c.shore(s.position),
            &tuning().transport,
            &tuning().move_to,
        )
        .unwrap();
    assert_eq!(changed, Changed::default());
    assert_eq!(
        (t.state.bare(), t.state.move_to().copied().map(|m| m.state)),
        (TransportState::Sail { move_to: None }, Some(MoveTo::Start))
    );
    // the planner's path from the place to itself crosses no shore: the place stays
    assert_eq!(t.point, s.position);
    // a NaN point sails too (COMISS/JA, JBE)
    let mut nan = TransportAction::new(Position::new(f32::NAN, 1.0, 0.0));
    let _ = nan
        .start(
            StartCall::Added,
            &s,
            &mut c.shore(s.position),
            &tuning().transport,
            &tuning().move_to,
        )
        .unwrap();
    assert_eq!(nan.state.bare(), TransportState::Sail { move_to: None });
    // below 0, or not first: idle with the standing sprite
    let mut off = TransportAction::new(Position::new(3.0, -1.0, 0.0));
    assert!(
        off.start(
            StartCall::Added,
            &s,
            &mut c.shore(s.position),
            &tuning().transport,
            &tuning().move_to
        )
        .unwrap()
        .stands
    );
    assert_eq!(
        (off.state.bare(), off.state.move_to().copied()),
        (TransportState::Idle, None)
    );
    let mut later = TransportAction::new(s.position);
    let _ = later
        .start(
            StartCall::Other,
            &s,
            &mut c.shore(s.position),
            &tuning().transport,
            &tuning().move_to,
        )
        .unwrap();
    assert_eq!(later.state.bare(), TransportState::Idle);
}

#[test]
fn sail_takes_the_shore_point_and_drops_its_old_move_to() {
    let s = ship(true);
    let mut c = coast();
    let mut t = TransportAction::new(s.position);
    let _ = t
        .start(
            StartCall::Added,
            &s,
            &mut c.shore(s.position),
            &tuning().transport,
            &tuning().move_to,
        )
        .unwrap();
    t.point = Position::new(25.0, 8.0, 0.0);
    // set_state(4) again: the old MoveTo's teardown stops the unit, the new one starts toward
    // the shore point, the last water tile on the planner's path to the land
    let changed = t
        .set_state(
            TransportState::SailToUnload { move_to: None },
            &s,
            &mut c.shore(s.position),
            &tuning().transport,
            &tuning().move_to,
        )
        .unwrap();
    assert!(changed.stopped && !changed.stands);
    assert_eq!(t.point, Position::new(19.5, 8.5, 0.0));
    assert_eq!(
        t.state.move_to().copied().map(|m| m.state),
        Some(MoveTo::Start)
    );
    // a unit without speed: its quiet MoveTo fails at its start
    let still = Ship {
        walk: MoveToReads {
            speed: Speed::ZERO,
            ..s.walk
        },
        ..s
    };
    let _ = t
        .set_state(
            TransportState::Sail { move_to: None },
            &still,
            &mut c.shore(still.position),
            &tuning().transport,
            &tuning().move_to,
        )
        .unwrap();
    assert_eq!(
        t.state.move_to().copied().map(|m| m.state),
        Some(MoveTo::Failed)
    );
    // a unit in state 3 or later adds no MoveTo (`FUN_140a977e0`)
    let dying = Ship {
        walk: MoveToReads {
            state: ObjectState::Dying,
            ..s.walk
        },
        ..s
    };
    let _ = t
        .set_state(
            TransportState::Sail { move_to: None },
            &dying,
            &mut c.shore(dying.position),
            &tuning().transport,
            &tuning().move_to,
        )
        .unwrap();
    assert_eq!(t.state.move_to().copied(), None);
}

#[test]
fn move_sails_to_its_point_and_stop_idles() {
    let s = ship(true);
    let mut c = coast();
    let mut t = TransportAction {
        state: TransportState::Unload,
        mode: TransportMode::Work,
        ..TransportAction::new(s.position)
    };
    let changed = t
        .move_to(
            Position::new(25.0, 8.0, 2.0),
            &s,
            &mut c.shore(s.position),
            &tuning().transport,
            &tuning().move_to,
        )
        .unwrap();
    assert_eq!(changed, Changed::default());
    assert_eq!(
        (
            t.state.bare(),
            t.mode,
            t.point,
            t.state.move_to().copied().map(|m| m.state)
        ),
        (
            TransportState::Sail { move_to: None },
            TransportMode::Move,
            Position::new(19.5, 8.5, 2.0),
            Some(MoveTo::Start)
        )
    );
    let stopped = t
        .stop(
            &s,
            &mut c.shore(s.position),
            &tuning().transport,
            &tuning().move_to,
        )
        .unwrap();
    assert!(stopped.stopped && stopped.stands);
    assert_eq!(
        (t.state.bare(), t.state.move_to().copied()),
        (TransportState::Idle, None)
    );
}

#[test]
fn work_order_near_its_zone_unloads_at_once_else_sails_there() {
    let s = ship(true);
    // within reach of the point's zone (the water) and 2 from the footprint (squared 4, at most
    // 4)
    let mut c = coast();
    let mut t = TransportAction::new(s.position);
    let _ = t
        .work(
            Position::new(15.0, 8.5, 1.0),
            TransportTarget::Ship,
            &s,
            &mut c.shore(s.position),
            &tuning().transport,
            &tuning().move_to,
        )
        .unwrap();
    assert_eq!(
        (t.state.bare(), t.mode, t.point),
        (
            TransportState::Unload,
            TransportMode::Work,
            Position::new(15.0, 8.5, 1.0)
        )
    );
    // a bit further: sail there to unload, from the unit itself, else just sail
    let mut t = TransportAction::new(s.position);
    let _ = t
        .work(
            Position::new(15.1, 8.5, 1.0),
            TransportTarget::Ship,
            &s,
            &mut c.shore(s.position),
            &tuning().transport,
            &tuning().move_to,
        )
        .unwrap();
    assert_eq!(
        (t.state.bare(), t.point),
        (
            TransportState::SailToUnload { move_to: None },
            Position::new(15.1, 8.5, 1.0)
        )
    );
    let mut t = TransportAction::new(s.position);
    let _ = t
        .work(
            Position::new(15.1, 8.5, 1.0),
            TransportTarget::Other,
            &s,
            &mut c.shore(s.position),
            &tuning().transport,
            &tuning().move_to,
        )
        .unwrap();
    assert_eq!(t.state.bare(), TransportState::Sail { move_to: None });
    // not near the zone: a big ship's footprint reaches within 2 of a point ashore, but the land
    // lies 4 from its tile (16, 8): it sails there to unload
    let big = Ship {
        position: Position::new(16.5, 8.5, 0.0),
        radius: Footprint::new(1.5, 1.5),
        ..s
    };
    let mut t = TransportAction::new(big.position);
    let _ = t
        .work(
            Position::new(20.0, 8.5, 0.0),
            TransportTarget::Ship,
            &big,
            &mut c.shore(big.position),
            &tuning().transport,
            &tuning().move_to,
        )
        .unwrap();
    assert_eq!(
        t.state.bare(),
        TransportState::SailToUnload { move_to: None }
    );
    // a point below 0 or NaN far away changes nothing but +0x34
    for bad in [
        Position::new(-1.0, 3.0, 0.0),
        Position::new(3.0, f32::NAN, 0.0),
    ] {
        let mut t = TransportAction::new(s.position);
        assert_eq!(
            t.work(
                bad,
                TransportTarget::Ship,
                &s,
                &mut c.shore(s.position),
                &tuning().transport,
                &tuning().move_to
            )
            .unwrap(),
            Changed::default()
        );
        assert_eq!(
            (t.state.bare(), t.mode, t.point),
            (TransportState::New, TransportMode::Work, s.position)
        );
    }
    // a NaN gap stays NaN (MAXSS) and is not within (COMISS/JC): it sails
    let mut t = TransportAction::new(s.position);
    let nan_ship = Ship {
        radius: Footprint::new(f32::NAN, 0.5),
        ..s
    };
    let _ = t
        .work(
            Position::new(12.5, 8.5, 0.0),
            TransportTarget::Ship,
            &nan_ship,
            &mut c.shore(nan_ship.position),
            &tuning().transport,
            &tuning().move_to,
        )
        .unwrap();
    assert_eq!(
        t.state.bare(),
        TransportState::SailToUnload { move_to: None }
    );
}

#[test]
fn the_shore_search_looks_about_the_footprint_for_the_point_zone_or_land() {
    let mut c = coast();
    let s = ship(true);
    // the ship at x 12.5 with radius 0.5 and the margin 2: tiles 10 to 15, all water (zone 0)
    let mut t = TransportAction::new(s.position);
    t.point = Position::new(17.0, 3.0, 0.0);
    assert!(
        t.next_to_shore(&s, &mut c.shore(s.position), &tuning().transport)
            .unwrap()
    );
    // the land (zone 1, x 20 and up) lies past x 15
    t.point = Position::new(25.0, 3.0, 0.0);
    assert!(
        !t.next_to_shore(&s, &mut c.shore(s.position), &tuning().transport)
            .unwrap()
    );
    // without a point: the land's shore bits lie past x 15 too, but not from x 17.5
    t.point = Position::new(NONE, 0.0, 0.0);
    assert!(
        !t.next_to_shore(&s, &mut c.shore(s.position), &tuning().transport)
            .unwrap()
    );
    let east = Ship {
        position: Position::new(17.5, 8.5, 0.0),
        ..ship(true)
    };
    assert!(
        t.next_to_shore(&east, &mut c.shore(east.position), &tuning().transport)
            .unwrap()
    );
    // the box stays on the map: a ship at the west edge reads only water, where a tile below 0
    // would read as terrain 0, the land
    let west = Ship {
        position: Position::new(0.5, 0.5, 0.0),
        ..ship(true)
    };
    assert!(
        !t.next_to_shore(&west, &mut c.shore(west.position), &tuning().transport)
            .unwrap()
    );
}

#[test]
fn an_unload_out_of_reach_or_beside_no_shore_tells_the_owner_unless_over_water() {
    let s = ship(true);
    let unloading = |point| TransportAction {
        state: TransportState::Unload,
        point,
        ..TransportAction::new(s.position)
    };
    // not near the point's zone, the land 8 from the ship's tile: the owner hears it, +0x68
    // clears, the action idles
    let land = Position::new(25.0, 3.0, 0.0);
    let mut t = TransportAction {
        retarget: true,
        ..unloading(land)
    };
    assert!(alerts(&t, &s, &mut coast()));
    let (r, Left { stands, .. }) = run(&mut t, &s, &mut coast());
    assert_eq!((r, stands), (Ok(Updated::Code(UpdateCode::UnderWay)), true));
    assert_eq!((t.state, t.retarget), (TransportState::Idle, false));
    // near, but not beside the footprint: the point's patch is zone 256, whose low byte is the
    // water's zone the ship is on; over land at the point it tells the owner, over water it
    // waits silently
    let mut dry = aliased_coast(SHOAL, LAND);
    let far = dry.tile_of(256);
    assert!(alerts(&unloading(far), &s, &mut dry));
    let mut wet = aliased_coast(WATER, DEEP);
    assert_eq!(wet.tile_of(256), far);
    assert!(!alerts(&unloading(far), &s, &mut wet));
    let (r, Left { stands, .. }) = run(&mut unloading(far), &s, &mut wet);
    assert_eq!((r, stands), (Ok(Updated::Code(UpdateCode::UnderWay)), true));
    // empty: nothing to unload, the action idles
    let mut t = unloading(Position::new(17.0, 3.0, 0.0));
    assert!(!alerts(&t, &ship(false), &mut coast()));
    assert_eq!(
        run(&mut t, &ship(false), &mut coast()).0,
        Ok(Updated::Code(UpdateCode::UnderWay))
    );
    assert_eq!(t.state, TransportState::Idle);
}

#[test]
fn an_unload_beside_the_shore_is_the_caller_then_idles_unless_it_sails_on() {
    let s = ship(true);
    let mut c = coast();
    let mut t = TransportAction {
        state: TransportState::Unload,
        point: Position::new(17.0, 3.0, 0.0),
        ..TransportAction::new(s.position)
    };
    let (
        r,
        Left {
            stopped, stands, ..
        },
    ) = run(&mut t, &s, &mut c);
    assert_eq!((r, stopped, stands), (Ok(Updated::Unload), false, false));
    assert_eq!(t.state, TransportState::Unload);
    assert!(
        t.unloaded(
            &s,
            &mut c.shore(s.position),
            &tuning().transport,
            &tuning().move_to
        )
        .unwrap()
        .stands
    );
    assert_eq!(t.state, TransportState::Idle);
    // a retarget's set_state(4) stays
    t.state = TransportState::SailToUnload { move_to: None };
    t.retarget = true;
    assert_eq!(
        t.unloaded(
            &s,
            &mut c.shore(s.position),
            &tuning().transport,
            &tuning().move_to
        ),
        Ok(Changed::default())
    );
    assert_eq!(t.state, TransportState::SailToUnload { move_to: None });
}

#[test]
fn sail_ends_with_its_move_to_and_failed_one_returns_3() {
    let s = ship(true);
    let mut c = coast();
    // state 4 without a MoveTo: the list answers 1, so it unloads
    let mut t = TransportAction {
        state: TransportState::SailToUnload { move_to: None },
        ..TransportAction::new(s.position)
    };
    assert_eq!(
        run(&mut t, &s, &mut c).0,
        Ok(Updated::Code(UpdateCode::UnderWay))
    );
    assert_eq!(t.state, TransportState::Unload);
    // state 11 with an ended MoveTo: idle, the MoveTo gone (its teardown stops the unit)
    let mut t = TransportAction {
        state: TransportState::Sail {
            move_to: Some(MoveToAction::sub_in(MoveTo::Arrived)),
        },
        ..TransportAction::new(s.position)
    };
    let (
        _,
        Left {
            stopped, stands, ..
        },
    ) = run(&mut t, &s, &mut c);
    assert_eq!(t.state, TransportState::Idle);
    assert!(stopped && stands);
    // a failed MoveTo (code 3) ends a sail to unload too
    let mut t = TransportAction {
        state: TransportState::SailToUnload {
            move_to: Some(MoveToAction::sub_in(MoveTo::Failed)),
        },
        ..TransportAction::new(s.position)
    };
    let _ = run(&mut t, &s, &mut c);
    assert_eq!(t.state, TransportState::Unload);
    // state 13: the unit's AI hears it failed, the action idles and returns 3
    let mut t = TransportAction {
        state: TransportState::Failed,
        ..TransportAction::new(s.position)
    };
    let (r, Left { stands, posted, .. }) = run(&mut t, &s, &mut c);
    let failed = (
        NotifyKind::ACTION_FAILED,
        [AiAction::Transport.value(), 0, 0],
    );
    assert_eq!(
        (r, stands, posted),
        (Ok(Updated::Code(UpdateCode::Failed)), true, vec![failed])
    );
    assert_eq!(t.state, TransportState::Idle);
    // state 1 is done; an idle one does nothing
    t.state = TransportState::Done;
    assert_eq!(
        run(&mut t, &s, &mut c).0,
        Ok(Updated::Code(UpdateCode::Done))
    );
    t.state = TransportState::Idle;
    let (
        r,
        Left {
            stopped,
            stands,
            posted,
        },
    ) = run(&mut t, &s, &mut c);
    assert_eq!(r, Ok(Updated::Code(UpdateCode::UnderWay)));
    assert!(!stopped && !stands && posted.is_empty());
    // a hull showing a sprite without its standing one: not modelled
    let mut t = TransportAction {
        state: TransportState::Failed,
        ..TransportAction::new(s.position)
    };
    let (mut unit, mut motion, mut ais) = hull();
    let look = (None, &mut Well512::default());
    assert!(
        t.state_update(
            SubCode::Done,
            (&s, &mut c.shore(s.position)),
            (&mut unit, motion.lent(), &mut ais),
            look,
            PlayerKind::Human,
            tuning()
        )
        .is_err()
    );
}

/// `FUN_140a992b0` as a Transport runs it on the step: a start asks for its path to the point
/// (the planner gives the walker its two paths) and shows its sprite before the motion step,
/// a move walks on; the units in the way go to the unit AI at once; a waiting `MoveTo` (only a
/// villager's waits) is not a ship's.
#[test]
fn sail_move_to_asks_for_its_path_then_walks_on() {
    use crate::combat::attack::tests::Rig;
    let graphic = Graphic {
        id: SpriteId::named(3),
        ..SHOWN
    };
    // its MoveTo to the point with the sprite its maker resolved
    let sailing = |state: Option<MoveTo>| TransportAction {
        state: TransportState::Sail {
            move_to: state.map(|state| MoveToAction {
                state,
                ..MoveToAction::sub_to_point(
                    Position::new(35.5, 88.5, 0.0),
                    Distance::new(tuning().transport.move_range),
                    Some(graphic),
                )
            }),
        },
        point: Position::new(35.5, 88.5, 0.0),
        ..TransportAction::new(Position::new(28.0, 89.0, 0.0))
    };
    let me = Me {
        human_order: WorldTime::ZERO,
        id: object_id(9177),
        group: None,
        action_state: ActionState::new(TransportState::Sail { move_to: None }.get()),
    };
    let rig = || {
        let mut rig = Rig::on_the_path(Victim::default());
        assert_eq!(
            Arc::make_mut(&mut rig.t.model.graphics).insert(SpriteId::named(3), graphic),
            None
        );
        rig.mover_mut().in_the_way = vec![object_id(77)];
        rig
    };
    let in_the_way = vec![(NotifyKind::IN_THE_WAY, [77, -1, -1])];
    let mut walker = rig();
    walker.mover_mut().paths.clear();
    let mut t = sailing(Some(MoveTo::Start));
    assert_eq!(
        walker.run(100, |c, u, _, _, _| t.walk(c, u, me)),
        Ok(SubCode::UnderWay)
    );
    assert_eq!(
        t.state.move_to().copied().map(|m| m.state),
        Some(MoveTo::Moving)
    );
    assert_eq!(walker.mover().paths.len(), 2);
    assert_eq!(
        (walker.sprite(), walker.posted()),
        (Some(SpriteId::named(3)), in_the_way.clone())
    );
    let mut walker = rig();
    let mut t = sailing(Some(MoveTo::Moving));
    assert_eq!(
        walker.run(100, |c, u, _, _, _| t.walk(c, u, me)),
        Ok(SubCode::UnderWay)
    );
    assert_eq!(
        (t.state.move_to().copied().map(|m| m.state), walker.posted()),
        (Some(MoveTo::Moving), in_the_way)
    );
    // a waiting MoveTo (only a human's villager waits; a saved one) runs its timer: no walk
    let mut walker = rig();
    let mut t = sailing(Some(MoveTo::Waiting));
    assert_eq!(
        walker.run(100, |c, u, _, _, _| t.walk(c, u, me)),
        Ok(SubCode::UnderWay)
    );
    assert_eq!(
        (t.state.move_to().copied().map(|m| m.state), walker.posted()),
        (Some(MoveTo::Waiting), vec![])
    );
    // the ended, the failed and a missing MoveTo walk nothing
    for (move_to, code) in [
        (Some(MoveTo::Arrived), SubCode::Done),
        (Some(MoveTo::Failed), SubCode::Failed),
        (None, SubCode::Done),
    ] {
        let mut walker = rig();
        let mut t = sailing(move_to);
        assert_eq!(walker.run(100, |c, u, _, _, _| t.walk(c, u, me)), Ok(code));
        assert_eq!(walker.posted(), vec![]);
    }
}

/// The sprite its `MoveTo` shows (`0x140aa42a0` gives `FUN_140aa0d30` one): its task's moving
/// sprite, without a task the master's walking sprite, for none the constructor's default; a
/// task the master lacks is an error.
#[test]
fn transport_move_to_shows_its_task_or_its_master_sprite() {
    use crate::objects::masters::{Master, Task};
    let master = Master {
        action: Some(ActionPart {
            tasks: vec![
                Task {
                    id: task_id(4),
                    sprites: TaskSprites::of_data([3, -1, -1, -1]),
                    ..Task::default()
                },
                Task {
                    id: task_id(5),
                    ..Task::default()
                },
            ],
            ..ActionPart::default()
        }),
        moving: Some(MovingPart {
            move_sprite: Some(SpriteId::named(7)),
            ..MovingPart::default()
        }),
        ..Master::default()
    };
    let on = |task, master: &Master| TransportAction::given_sprite(task, master);
    assert_eq!(on(Some(task_id(4)), &master), Ok(Some(SpriteId::named(3))));
    assert_eq!(on(Some(task_id(5)), &master), Ok(None));
    assert_eq!(on(None, &master), Ok(Some(SpriteId::named(7))));
    let walkless = Master {
        moving: Some(MovingPart {
            move_sprite: None,
            ..master.moving.unwrap_or_default()
        }),
        ..master.clone()
    };
    assert_eq!(on(None, &walkless), Ok(None));
    assert!(on(Some(task_id(6)), &master).is_err());
}

fn rng() -> Well512 {
    let mut r = Well512::default();
    r.set_seed(7);
    r
}

/// A `size` by `size` map of terrain `fill` but for `tiles` (x, y and their terrain), every
/// terrain up to 23 shown.
fn terrain(size: i32, fill: u8, tiles: &[([i16; 2], u8)]) -> Terrain {
    let mut kinds = vec![(fill, 255, -1); (size * size) as usize];
    for &([x, y], t) in tiles {
        kinds[(i32::from(y) * size + i32::from(x)) as usize] = (t, 255, -1);
    }
    let shown = TerrainKind {
        shown: true,
        ..TerrainKind::default()
    };
    Terrain::new(
        GridSize::new(size, size),
        kinds
            .into_iter()
            .map(|(k, r, l)| TileTerrain::of_save(k, r, l))
            .collect(),
        vec![shown; 24],
        Vec::new(),
    )
}

/// The tile of the retry's `n`th draw (from 1) about (20, 20) from [`rng`]'s seed `seed`.
fn draw_tile(seed: u32, n: usize) -> [i16; 2] {
    let mut r = Well512::default();
    r.set_seed(seed);
    let mut at = [0; 2];
    for _ in 0..n {
        let x = (r.rand15() / RETRY_STEP) as f32 + 20.0 - tuning().transport.retry_spread;
        let y = (r.rand15() / RETRY_STEP) as f32 + 20.0 - tuning().transport.retry_spread;
        at = [x as i32 as i16, y as i32 as i16];
    }
    at
}

#[test]
fn retry_draws_about_the_old_point_until_it_leaves_the_water() {
    // the draws as the game makes them: x then y, each r / 0xccc tiles
    let mut copy = rng();
    let draws: Vec<u32> = (0..6).map(|_| copy.rand15() / RETRY_STEP).collect();
    let point = Point::new(20.0, 20.0);
    let at = |i: usize| {
        Point::new(
            draws[2 * i] as f32 + 20.0 - 5.0,
            draws[2 * i + 1] as f32 + 20.0 - 5.0,
        )
    };
    // land (terrain 0) everywhere: the first draw leaves the water
    let mut r = rng();
    assert_eq!(
        retry_point(
            point,
            GridSize::new(40, 40),
            &terrain(40, 0, &[]),
            &mut r,
            &tuning().transport
        ),
        (at(0), true)
    );
    // water 22, 23 and 1 keep drawing: the third draw's tile is land
    let first = [at(0), at(1)].map(|p| [p.x() as i16, p.y() as i16]);
    let wet = terrain(40, 0, &[(first[0], 0x17), (first[1], 1)]);
    let mut r = rng();
    let (found, sails) = retry_point(
        point,
        GridSize::new(40, 40),
        &wet,
        &mut r,
        &tuning().transport,
    );
    assert_eq!((found, sails), (at(2), true));
    assert_eq!(r, copy);
    // off the map the last terrain stays: all water, 100 draws, the last one taken
    let mut r = rng();
    let (_, sails) = retry_point(
        point,
        GridSize::new(40, 40),
        &terrain(40, 0x16, &[]),
        &mut r,
        &tuning().transport,
    );
    assert!(!sails);
    assert_eq!(r.draws, rng().draws + 200);
}

#[test]
fn the_99th_draw_leaving_the_water_comes_too_late() {
    // water but the tile of the nth draw, which no draw before lands on (a seed that gives
    // one): the 99th breaks the loop with no draws to spare, the 98th with one
    for (n, sails) in [(99, false), (98, true)] {
        let seed = (0..1000)
            .find(|&seed| (1..n).all(|k| draw_tile(seed, k) != draw_tile(seed, n)))
            .expect("a seed whose nth draw lands on a new tile");
        let shore = terrain(400, 0x16, &[(draw_tile(seed, n), 0)]);
        let mut r = Well512::default();
        r.set_seed(seed);
        assert_eq!(
            retry_point(
                Point::new(20.0, 20.0),
                GridSize::new(400, 400),
                &shore,
                &mut r,
                &tuning().transport
            )
            .1,
            sails,
            "{n}"
        );
    }
}

#[test]
fn drop_location_is_drawn_on_side_past_both_footprints() {
    let around = Around {
        center: Point::new(10.0, 10.0),
        radius: Footprint::new(1.0, 1.0),
    };
    // the first draw's side and place, as the game computes them
    let mut copy = rng();
    let (r1, r2) = (copy.rand15(), copy.rand15());
    let side = (r1 as i32 * 4) / 0x7fff;
    let along = ((r2 as f32 * 2.0) / 32767.0 - 1.0) + 10.0;
    let gap = 1.0 + (0.5 + tuning().transport.drop_gap);
    let expected = match side {
        0 => Point::new(10.0 + gap, along),
        1 => Point::new(10.0 - gap, along),
        2 => Point::new(along, 10.0 + gap),
        _ => Point::new(along, 10.0 - gap),
    };
    let mut last = None;
    let mut r = rng();
    let first = DropDraws::new(
        around,
        Footprint::new(0.5, 0.5),
        GridSize::new(40, 40),
        &mut last,
        &mut r,
    )
    .next_on_map(&tuning().transport);
    assert_eq!(first, Ok(Some(expected)));
    assert_eq!(last, Some(expected));
    assert_eq!(r, copy);
    // 50 draws in all, the last place kept for the next call
    let mut r = rng();
    let mut draws = DropDraws::new(
        around,
        Footprint::new(0.5, 0.5),
        GridSize::new(40, 40),
        &mut last,
        &mut r,
    );
    let mut tries = 0;
    while draws.next_on_map(&tuning().transport).unwrap().is_some() {
        tries += 1;
    }
    assert_eq!(tries, 50);
    // off the map: those draws use their tries up, and none comes out
    let mut r = rng();
    let edge = Around {
        center: Point::new(0.0, 0.0),
        radius: Footprint::new(5.0, 5.0),
    };
    let mut none = None;
    let mut draws = DropDraws::new(
        edge,
        Footprint::new(0.5, 0.5),
        GridSize::new(4, 4),
        &mut none,
        &mut r,
    );
    let mut on_map = 0;
    while let Some(p) = draws.next_on_map(&tuning().transport).unwrap() {
        on_map += 1;
        assert!(0.0 <= p.x() && 0.0 <= p.y() && p.x() < 4.0 && p.y() < 4.0);
    }
    assert!(on_map < 50);
    // a first draw of side 4 keeps a place no draw left: not modelled
    let mut stack = Well512::default();
    let seed = (0..100_000)
        .find(|&seed| {
            stack.set_seed(seed);
            stack.rand15() == 0x7fff
        })
        .expect("a seed whose first draw is 0x7fff");
    stack.set_seed(seed);
    let mut none = None;
    let mut draws = DropDraws::new(
        around,
        Footprint::new(0.5, 0.5),
        GridSize::new(40, 40),
        &mut none,
        &mut stack,
    );
    assert_eq!(draws.next_on_map(&tuning().transport), Err(STACK_PLACE));
}

/// A drop location's zone ([`in_zone`]): any for −1; else the tile's zone (its low byte) or
/// one beside it on the zone map; without a map only zone 0's tiles, none beside.
#[test]
fn drop_location_lies_in_the_transport_zone_or_beside_it() {
    assert_eq!(in_zone(Point::new(3.5, 4.5), None, None), Ok(true));
    assert_eq!(
        in_zone(Point::new(3.5, 4.5), Some(ZoneId::new(0)), None),
        Ok(true)
    );
    assert_eq!(
        in_zone(Point::new(3.5, 4.5), Some(ZoneId::new(2)), None),
        Ok(false)
    );
    assert_eq!(
        in_zone(Point::new(-0.5, 4.5), Some(ZoneId::new(0)), None),
        Ok(true)
    );
}
