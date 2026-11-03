//! A test's world drawn as text (FD-043-D47): the map's tiles, what the scene's player sees of
//! them and the scout on it, built into a [`World`] whose layers agree. A tile's land or water
//! is its class in the obstruction manager's words, its terrain for the zones and the path
//! planners and its restrictions' value; a rock is a static record on its tile; the sight is the
//! unified map's words. Tests build their world here instead of standing in for its reads.

use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Radii;
use crate::map::Restriction;
use crate::map::RestrictionId;
use crate::map::TerrainFlags;
use crate::map::TerrainId;
use crate::map::TerrainMask;
use crate::map::Tile;
use crate::map::spatial::TileSight;
use crate::map::spatial::{Obstruction, Placed, Spatial, Tiles};
use crate::map::terrain::TileTerrain;
use crate::map::terrain::{Terrain, TerrainKind};
use crate::math::Angle;
use crate::math::fmath::BVector;
use crate::movement::Me;
use crate::movement::MoveAlgorithm;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::movement::TurnRate;
use crate::movement::scout_area::ScoutArea;
use crate::movement::{MoveStats, SubstituteCount, WaitDelays};
use crate::objects::ActionState;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::{Animated, Static};
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::FlyMode;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::{Master, Masters};
use crate::objects::unit::Unit;
use crate::pathing::Pathing;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::testing::class;
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::vision::visibility::{Visibility, VisibleMap};
use crate::world::FrameSeed;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::parts::Object;
use crate::world::player::Player;
use std::sync::Arc;

/// The scene's player: the scout's owner, whose sight [`Scene::sight_on`] sets.
pub const PLAYER: PlayerId = PlayerId::new(1);

/// The scout's id in a scene's world.
///
/// # Panics
///
/// Never: 30 is a valid id.
#[must_use]
pub fn scout_id() -> ObjectId {
    ObjectId::try_from(30).expect("a valid id")
}

/// The scout's master in a scene's world.
pub const SCOUT_MASTER: MasterId = MasterId::named(448);

/// The first rock's id; the others follow in the order the scene placed them.
const FIRST_ROCK: i32 = 1000;

/// A land tile's class in the obstruction manager's words: one the scout's terrain mask holds.
const LAND_CLASS: u8 = 4;
/// A water tile's class: one the scout's terrain mask does not hold.
const WATER_CLASS: u8 = 1;
/// Land's terrain, which every restriction passes.
const LAND: u8 = 0;
/// Water's terrain, which no restriction passes.
const WATER: u8 = 1;
/// The restrictions of a scene's terrain: as many as the scout's restriction needs.
const RESTRICTIONS: usize = 8;

/// The scene's scout: a walker of [`PLAYER`] at its place, with its master's line of sight.
#[derive(Clone, Copy, Debug)]
struct Scout {
    at: Position,
    line_of_sight: Distance,
}

/// A test's map and what stands on it, built into a [`World`] by [`Scene::build`].
#[derive(Clone, Debug)]
pub struct Scene {
    size: GridSize,
    /// Each tile's water, row by row from y 0.
    water: Vec<bool>,
    rocks: Vec<Tile>,
    /// What [`PLAYER`] sees of each tile, row by row.
    sight: Vec<TileSight>,
    scout: Option<Scout>,
}

impl Scene {
    /// A map of `size` tiles of land, every tile seen by [`PLAYER`].
    #[must_use]
    pub fn open(size: GridSize) -> Self {
        let n = (size.width() * size.height()) as usize;
        Self {
            size,
            water: vec![false; n],
            rocks: Vec::new(),
            sight: vec![TileSight::Seen; n],
            scout: None,
        }
    }

    /// A map drawn row by row, the first row y 0, a character per tile: `.` land, `~` water,
    /// `#` a rock on land. Every tile is seen by [`PLAYER`].
    ///
    /// # Panics
    ///
    /// Rows of unequal length, or another character.
    #[must_use]
    pub fn rows(rows: &[&str]) -> Self {
        let width = rows.first().map_or(0, |r| r.len());
        assert!(rows.iter().all(|r| r.len() == width), "rows of one length");
        let mut scene = Self::open(GridSize::new(width as i32, rows.len() as i32));
        for (y, row) in rows.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                let tile = Tile::new(x as i32, y as i32);
                match c {
                    '.' => {}
                    '~' => scene = scene.water([tile]),
                    '#' => scene = scene.rocks([tile]),
                    other => panic!("no tile drawn as {other:?}"),
                }
            }
        }
        scene
    }

    /// The tiles `tiles` are water.
    #[must_use]
    pub fn water(mut self, tiles: impl IntoIterator<Item = Tile>) -> Self {
        for t in tiles {
            let i = self.index(t);
            self.water[i] = true;
        }
        self
    }

    /// A rock stands on each of `tiles`: a static record of a resource's obstruction (type 2),
    /// half a tile about the tile's centre.
    #[must_use]
    pub fn rocks(mut self, tiles: impl IntoIterator<Item = Tile>) -> Self {
        self.rocks.extend(tiles);
        self
    }

    /// [`PLAYER`] sees every tile as `sight`.
    #[must_use]
    pub fn sight(mut self, sight: TileSight) -> Self {
        self.sight.fill(sight);
        self
    }

    /// [`PLAYER`] sees each of `tiles` as `sight`.
    #[must_use]
    pub fn sight_on(mut self, tiles: impl IntoIterator<Item = Tile>, sight: TileSight) -> Self {
        for t in tiles {
            let i = self.index(t);
            self.sight[i] = sight;
        }
        self
    }

    /// [`PLAYER`]'s scout ([`scout_id`]) stands at `at`, its master's line of sight
    /// `line_of_sight`.
    #[must_use]
    pub const fn scout(mut self, at: Position, line_of_sight: Distance) -> Self {
        self.scout = Some(Scout { at, line_of_sight });
        self
    }

    /// The world: Gaia and [`PLAYER`], the map's tiles, terrain, path planners and sight, the
    /// rocks and the scout.
    ///
    /// # Panics
    ///
    /// More rocks than ids.
    #[must_use]
    pub fn build(self) -> World {
        let size = self.size;
        let n = self.water.len();
        let mut w = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(0),
            PLAYER,
            vec![Millis::ZERO; 2],
            vec![Player::default(), Player::default()],
        );
        let classes = self
            .water
            .iter()
            .map(|&wet| if wet { WATER_CLASS } else { LAND_CLASS });
        let kinds: Vec<u8> = self
            .water
            .iter()
            .map(|&wet| if wet { WATER } else { LAND })
            .collect();
        w.spatial = Spatial::with_tiles(size, Tiles::new(size, classes.collect()));
        let restrictions = vec![Restriction::new(vec![1.0, 0.0]); RESTRICTIONS];
        let kind = |flags| TerrainKind {
            shown: true,
            flags,
            forest: false,
        };
        let mut terrain = Terrain::new(
            size,
            kinds
                .iter()
                .map(|&k| TileTerrain::of_save(k, 255, -1))
                .collect(),
            vec![kind(TerrainFlags::NONE), kind(TerrainFlags::WATER)],
            restrictions.clone(),
        );
        terrain.set_elevations(vec![0; n]);
        w.spatial.terrain = Some(terrain);
        w.pathing = Pathing::new(
            size,
            kinds.into_iter().map(TerrainId::named).collect(),
            restrictions,
            0,
        );
        w.pathing.start_mesh(size.width() as u32);
        w.spatial.sight = Some(sight(size, &self.sight));
        for (i, &t) in self.rocks.iter().enumerate() {
            let id = ObjectId::try_from(FIRST_ROCK + i as i32).expect("a valid id");
            w.spatial.insert(id, rock(t));
        }
        if let Some(scout) = self.scout {
            place_scout(&mut w, scout);
        }
        w.reindex();
        w
    }

    /// The tile's index in the scene's rows.
    fn index(&self, t: Tile) -> usize {
        self.size.index(t).expect("a tile of the scene")
    }
}

/// The unified map's words for [`PLAYER`]'s `sight` of each tile; Gaia sees nothing.
fn sight(size: GridSize, sight: &[TileSight]) -> Visibility {
    let word = |s: &TileSight| match s {
        TileSight::Unknown => VisionBits::empty(),
        TileSight::Explored => VisionBits::explored(PLAYER),
        TileSight::Seen => VisionBits::explored(PLAYER) | VisionBits::visible(PLAYER),
    };
    Visibility::new(
        size,
        (0..2)
            .map(|_| VisibleMap {
                counts: vec![0; sight.len()],
                explored: TileCount::ZERO,
            })
            .collect(),
        sight.iter().map(word).collect(),
        Vec::new(),
        0,
        vec![PlayerKind::Human; 2],
    )
}

/// A rock's record on tile `t`.
fn rock(t: Tile) -> Placed {
    Placed {
        position: Position::new(t.x() as f32 + 0.5, t.y() as f32 + 0.5, 0.0),
        radius: Footprint::new(0.5, 0.5),
        class: class::CLASS_7,
        player: PlayerId::new(0),
        master: MasterId::named(102),
        obstruction_kind: ObstructionKind::KIND_2,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: Some(Obstruction {
            kind: ObstructionKind::KIND_2,
            class: ObstructionClass::RESOURCE,
        }),
    }
}

/// The scout's movement: a walker of radius 0.2 at 0.8 tiles a second, its restriction 7 and
/// terrain mask 6 (the land's class).
fn walk_stats() -> MoveStats {
    MoveStats {
        fly_mode: FlyMode::GROUND,
        speed: Speed::new(0.8),
        base: Speed::new(0.8),
        turn_rate: TurnRate::MAX,
        turn_radius: Distance::ZERO,
        turn_radius_speed: Speed::MAX,
        radius: Radii::new(0.2, 0.2, 0.5),
        restriction: RestrictionId::named(7),
        restriction_mask: TerrainMask::of_bits(6),
        turn_time: Seconds::ZERO,
        move_algorithm: MoveAlgorithm::NONE,
        min_collision: 1.0,
        fast_turn: false,
        facets: None,
        weapon_range: Distance::ZERO,
    }
}

/// The scout, its master in both players' tables and its standing mover, last moved at its
/// place.
fn place_scout(w: &mut World, scout: Scout) {
    let master = Master {
        id: SCOUT_MASTER,
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        radius: Footprint::new(0.2, 0.2),
        height: Distance::new(1.0),
        line_of_sight: scout.line_of_sight,
        move_stats: Some(walk_stats()),
        ..Master::default()
    };
    w.model.masters = vec![
        Arc::new(Masters::new([master.clone()])),
        Arc::new(Masters::new([master])),
    ];
    let unit = Unit {
        id: scout_id(),
        place: Static::at(scout.at),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(25.0),
            master_id: SCOUT_MASTER,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mover = Mover {
        forward: BVector::new(1.0, 0.0, 0.0),
        right: BVector::new(0.0, 0.0, -1.0),
        velocity: None,
        paths: Vec::new(),
        factor: 1.0,
        stats: walk_stats(),
        waiting: 0,
        wait_delays: WaitDelays::of_data(0),
        attack_moving: false,
        last_move: WorldTime::ZERO,
        moved_at: scout.at,
        substitute_count: SubstituteCount::of_data(0),
        ignore_units: Vec::new(),
        user_waypoints: Vec::new(),
        last_turn: Seconds::ZERO,
        angle: Angle::ZERO,
        animated: Animated::default(),
        pauses: Vec::new(),
        in_the_way: Vec::new(),
        sprite_facets: None,
    };
    let _ = w
        .push_live(PLAYER, Object::moving(unit, mover))
        .expect("the scout joins its player");
}

/// The scout's reads for its searches ([`ScoutArea::of`]), the map's size and its line of
/// sight.
///
/// # Panics
///
/// A world without the scene's scout.
pub(crate) fn scout_area(w: &mut World) -> (ScoutArea<'_>, GridSize, Distance) {
    let (_, unit) = w.object(scout_id()).expect("the scene's scout");
    let unit = unit.clone();
    let me = Me {
        human_order: WorldTime::ZERO,
        id: unit.id,
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    ScoutArea::of(w, &unit, me, PLAYER, PlayerKind::Human).expect("the scout's reads")
}
