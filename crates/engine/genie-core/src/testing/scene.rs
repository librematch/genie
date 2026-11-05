//! A test's world drawn as text (FD-043-D47): the map's tiles, their terrains and elevations, what
//! the scene's player sees of them, the scout and the other objects on it, built into a [`World`]
//! whose layers agree. A tile's terrain is its class in the obstruction manager's words, its kind
//! for the zones and the path planners and its restrictions' value; an elevation makes the tiles'
//! shapes as the map's load does; a rock is a static record on its tile; the sight is the unified
//! map's words; an object ([`SceneObject`]) is filed as the game files a new one. Tests build their
//! world here instead of standing in for its reads.

use crate::objects::layers::Speedup;

use crate::construction::BuilderCount;
use crate::construction::MainBuildingId;
use crate::construction::build::Construction;
use crate::garrison::ContainerId;
use crate::gathering::PastureId;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Radii;
use crate::map::Restriction;
use crate::map::RestrictionId;
use crate::map::Snow;
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
use crate::math::rng::Well512;
use crate::movement::Me;
use crate::movement::MoveAlgorithm;
use crate::movement::MoveState;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::movement::TopSpeed;
use crate::movement::TurnRate;
use crate::movement::scout_area::ScoutArea;
use crate::movement::{MoveStats, SubstituteCount, WaitDelays};
use crate::objects::ActionState;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::WorkerCount;
use crate::objects::layers::Action;
use crate::objects::layers::Building;
use crate::objects::layers::{Animated, Static};
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::AnimatedPart;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::FlyMode;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::{Master, Masters};
use crate::objects::sprite::{ActiveSprite, Graphic};
use crate::objects::unit::Unit;
use crate::objects::unit::UnitFlags;
use crate::ownership::CaptureKind;
use crate::pathing::Pathing;
use crate::players::Amount;
use crate::players::ByPlayer;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::Relation;
use crate::players::Relations;
use crate::players::attribute::AttributeId;
use crate::players::{UnitRelation, UnitRelations};
use crate::testing::class;
use crate::unit_ai::UnitAi;
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::vision::registry::{CLASSES, Entry, Registry, UnitUpdate};
use crate::vision::visibility::{BuildingSight, Seer, Visibility, VisibleMap};
use crate::world::FrameSeed;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WholeSeconds;
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

/// Land's terrain (its flags [`TerrainFlags::LAND`], the land class of the restrictions' masks),
/// which every restriction passes.
pub const LAND: TerrainId = TerrainId::named(0);
/// Water's terrain in the scene's own table ([`Scene::terrains`] replaces it), which no
/// restriction passes.
pub const WATER: TerrainId = TerrainId::named(1);

/// The first rock's id; the others follow in the order the scene placed them.
const FIRST_ROCK: i32 = 1000;

/// A land tile's class in the obstruction manager's words: one the scout's terrain mask holds.
const LAND_CLASS: u8 = 4;
/// A water tile's class: one the scout's terrain mask does not hold.
const WATER_CLASS: u8 = 1;
/// The restrictions of a scene's terrain: as many as the scout's restriction needs.
const RESTRICTIONS: usize = 8;

/// The scene's scout: a walker of [`PLAYER`] at its place, with its master's line of sight.
#[derive(Clone, Copy, Debug)]
struct Scout {
    at: Position,
    line_of_sight: Distance,
}

/// An object of a scene beside its scout: of any master and player, at its place
/// ([`Scene::object`]).
#[derive(Clone, Debug)]
pub struct SceneObject {
    /// Its id.
    pub(crate) id: ObjectId,
    /// Its owner.
    pub(crate) player: PlayerId,
    /// Its master, which every player's table of the scene holds.
    pub(crate) master: Master,
    /// Its place.
    pub(crate) at: Position,
    /// Its state.
    pub(crate) state: ObjectState,
    /// Its hit points.
    pub(crate) hp: HitPoints,
    /// In the sight registry, the mask its last sight update found (`RGE_Combat_Object::update`'s,
    /// [`Visibility::combat_sight`]): the players who saw its tiles then, whose lists hold it
    /// and whose bits its entry keeps after they look away; a building with its UMV entry. None
    /// for an object outside the registry (with a seer only if it holds a capture flag).
    pub(crate) seen_by: Option<VisionBits>,
    /// A foundation no builder has started: desolid, without its obstruction record.
    pub(crate) desolid: bool,
    /// Its unit AI, none for an object without one.
    pub(crate) ai: Option<UnitAi>,
    /// Its flag bytes (`+0x685`).
    pub(crate) flags: UnitFlags,
    /// The object it is inside.
    pub(crate) inside: Option<ContainerId>,
    /// Its action layer, none for an empty one.
    pub(crate) actions: Option<Action>,
    /// Its capture flag (`+0x924`), which its seer in the sight registry holds.
    pub(crate) capture: CaptureKind,
    /// The pasture it stands in (`+0x790`).
    pub(crate) pasture: Option<PastureId>,
    /// Its sprite, started on the scene's graphic of that id ([`Scene::graphic`]); none for
    /// an object without one.
    pub(crate) sprite: Option<SpriteId>,
    /// The main building it is an annex of (`+0xbb8`), none for an object that is none's; the
    /// scene links it into the main building's annexes (`+0xbc0`), as a save holds both.
    pub(crate) annex_of: Option<MainBuildingId>,
    /// What it holds (`+0x308`, `+0x310`): a resource's attribute and amount, as a save holds
    /// them; none for an object that holds nothing.
    pub(crate) holds: Option<(AttributeId, Amount)>,
    /// `+0x644`, `worker_num`: the actions that have it as their target, as a save holds them.
    pub(crate) workers: WorkerCount,
}

impl SceneObject {
    /// Object `id` of `player` and `master` at `at`: alive, with its master's hit points, outside
    /// the sight registry, not a foundation, attackable (the flag a spawn gives a new object),
    /// without a unit AI, other flags, actions, capture flag, pasture, main building, resource
    /// or workers, on the map. A moving master's object stands with its movement state
    /// ([`Mover`]) at its place.
    ///
    /// # Panics
    ///
    /// An id the game does not give.
    #[must_use]
    pub fn new(id: i32, player: PlayerId, master: Master, at: Position) -> Self {
        Self {
            id: ObjectId::try_from(id).expect("a valid id"),
            player,
            hp: HitPoints::new(f32::from(master.hit_points.get())),
            master,
            at,
            state: ObjectState::Alive,
            seen_by: None,
            desolid: false,
            ai: None,
            flags: UnitFlags::ATTACKABLE,
            inside: None,
            actions: None,
            capture: CaptureKind::None,
            pasture: None,
            sprite: None,
            annex_of: None,
            holds: None,
            workers: WorkerCount::ZERO,
        }
    }
}

/// A test's map and what stands on it, built into a [`World`] by [`Scene::build`].
#[derive(Clone, Debug)]
pub struct Scene {
    size: GridSize,
    /// Each tile's terrain, row by row from y 0.
    terrain: Vec<TerrainId>,
    /// The terrain table: each terrain's flags, by its id.
    terrains: Vec<TerrainFlags>,
    /// Each tile's elevation, row by row.
    elevation: Vec<u8>,
    rocks: Vec<Tile>,
    /// What [`PLAYER`] sees of each tile, row by row.
    sight: Vec<TileSight>,
    scout: Option<Scout>,
    /// The players past Gaia.
    players: u8,
    /// The relations [`Scene::relation`] set, from each player to another.
    relations: Vec<(PlayerId, PlayerId, Relation)>,
    /// The players' types [`Scene::kind`] set.
    kinds: Vec<(PlayerId, PlayerKind)>,
    /// The graphics' table [`Scene::graphic`] fills.
    graphics: Vec<Graphic>,
    objects: Vec<SceneObject>,
}

impl Scene {
    /// A map of `size` tiles of land at elevation 0, every tile seen by [`PLAYER`], the only
    /// player past Gaia; its terrain table holds [`LAND`] and [`WATER`].
    #[must_use]
    pub fn open(size: GridSize) -> Self {
        let tiles = (size.width() * size.height()) as usize;
        Self {
            size,
            terrain: vec![LAND; tiles],
            terrains: vec![TerrainFlags::LAND, TerrainFlags::WATER],
            elevation: vec![0; tiles],
            rocks: Vec::new(),
            sight: vec![TileSight::Seen; tiles],
            scout: None,
            players: 1,
            relations: Vec::new(),
            kinds: Vec::new(),
            graphics: Vec::new(),
            objects: Vec::new(),
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
        let width = rows.first().map_or(0, |text| text.len());
        assert!(
            rows.iter().all(|text| text.len() == width),
            "rows of one length"
        );
        let mut scene = Self::open(GridSize::new(width as i32, rows.len() as i32));
        for (y, row) in rows.iter().enumerate() {
            for (x, character) in row.chars().enumerate() {
                let tile = Tile::new(x as i32, y as i32);
                match character {
                    '.' => {}
                    '~' => scene = scene.water([tile]),
                    '#' => scene = scene.rocks([tile]),
                    other => panic!("no tile drawn as {other:?}"),
                }
            }
        }
        scene
    }

    /// The tiles `tiles` are water ([`WATER`]).
    #[must_use]
    pub fn water(self, tiles: impl IntoIterator<Item = Tile>) -> Self {
        self.terrain_on(tiles, WATER)
    }

    /// The terrain table: terrain `index` has the flags `flags[index]`. A terrain with water is of
    /// the water class and no restriction passes it; every other is of the land class and passes.
    #[must_use]
    pub fn terrains(mut self, flags: impl IntoIterator<Item = TerrainFlags>) -> Self {
        self.terrains = flags.into_iter().collect();
        self
    }

    /// The tiles `tiles` are of terrain `terrain`.
    #[must_use]
    pub fn terrain_on(mut self, tiles: impl IntoIterator<Item = Tile>, terrain: TerrainId) -> Self {
        for tile in tiles {
            let index = self.index(tile);
            self.terrain[index] = terrain;
        }
        self
    }

    /// The tiles `tiles` lie at elevation `elevation`; the map's load makes the shapes of the
    /// tiles about them ([`Terrain::set_elevations`]).
    #[must_use]
    pub fn elevation_on(mut self, tiles: impl IntoIterator<Item = Tile>, elevation: u8) -> Self {
        for tile in tiles {
            let index = self.index(tile);
            self.elevation[index] = elevation;
        }
        self
    }

    /// Every tile lies at elevation `elevation`.
    #[must_use]
    pub fn elevation(mut self, elevation: u8) -> Self {
        self.elevation.fill(elevation);
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
        for tile in tiles {
            let index = self.index(tile);
            self.sight[index] = sight;
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

    /// The world holds `players` players past Gaia: each its own ally and every other's enemy
    /// ([`Scene::relation`] sets another relation).
    #[must_use]
    pub const fn players(mut self, players: u8) -> Self {
        self.players = players;
        self
    }

    /// Player `from`'s relation to player `to` (`player+0x260`) is `relation`.
    #[must_use]
    pub fn relation(mut self, from: PlayerId, to: PlayerId, relation: Relation) -> Self {
        self.relations.push((from, to, relation));
        self
    }

    /// Player `player`'s type (`player+0x10`) is `kind`; a player's type is
    /// [`PlayerKind::default`] else.
    #[must_use]
    pub fn kind(mut self, player: PlayerId, kind: PlayerKind) -> Self {
        self.kinds.push((player, kind));
        self
    }

    /// The graphics' table holds `graphic`: a master's sprite and an object's
    /// ([`SceneObject::sprite`]).
    #[must_use]
    pub fn graphic(mut self, graphic: Graphic) -> Self {
        self.graphics.push(graphic);
        self
    }

    /// `object` stands on the map, after the objects placed before it on its tile.
    #[must_use]
    pub fn object(mut self, object: SceneObject) -> Self {
        self.objects.push(object);
        self
    }

    /// The world: Gaia and the players, the map's tiles, terrain, path planners and sight, the
    /// rocks, the scout and the objects.
    ///
    /// # Panics
    ///
    /// More rocks than ids, an object of a player the scene has not got, or a building in the
    /// sight registry whose footprint leaves the map.
    #[must_use]
    pub fn build(self) -> World {
        let size = self.size;
        let players = usize::from(self.players) + 1;
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(0),
            PLAYER,
            vec![Millis::ZERO; players],
            (0..players)
                .map(|index| player(index, players, &self.relations, &self.kinds))
                .collect(),
        );
        let wet = |terrain_id: &TerrainId| {
            self.terrains
                .get(terrain_id.index())
                .is_some_and(|terrain_flags| terrain_flags.contains(TerrainFlags::WATER))
        };
        let classes = self.terrain.iter().map(|terrain_id| {
            if wet(terrain_id) {
                WATER_CLASS
            } else {
                LAND_CLASS
            }
        });
        world.spatial = Spatial::with_tiles(size, Tiles::new(size, classes.collect()));
        let values = self.terrains.iter().map(|terrain_flags| {
            if terrain_flags.contains(TerrainFlags::WATER) {
                0.0
            } else {
                1.0
            }
        });
        let restrictions = vec![Restriction::new(values.collect()); RESTRICTIONS];
        let kinds = self.terrains.iter().map(|&flags| TerrainKind {
            shown: true,
            flags,
            forest: false,
        });
        let mut terrain = Terrain::new(
            size,
            self.terrain
                .iter()
                .map(|terrain_id| TileTerrain::of_save(terrain_id.get(), 255, -1))
                .collect(),
            kinds.collect(),
            restrictions.clone(),
        );
        terrain.set_elevations(self.elevation.clone());
        world.spatial.terrain = Some(terrain);
        world.pathing = Pathing::new(size, self.terrain.clone(), restrictions, 0);
        world.pathing.start_mesh(size.width() as u32);
        world.spatial.sight = Some(sight(size, &self.sight, players));
        world.spatial.relations = ByPlayer::new(
            world
                .players
                .iter()
                .map(|player| player.relations.clone())
                .collect(),
        );
        world.spatial.unit_diplomacy = ByPlayer::new(
            world
                .players
                .iter()
                .enumerate()
                .map(|(index, player)| unit_diplomacy(index, &player.relations))
                .collect(),
        );
        for (index, &tile) in self.rocks.iter().enumerate() {
            let id = ObjectId::try_from(FIRST_ROCK + index as i32).expect("a valid id");
            world.spatial.insert(id, rock(tile));
        }
        if !self.graphics.is_empty() {
            world.model.graphics = Arc::new(
                self.graphics
                    .iter()
                    .map(|graphic| (graphic.id, *graphic))
                    .collect(),
            );
        }
        if self.scout.is_some() || !self.objects.is_empty() {
            let masters = self.masters();
            world.model.masters = (0..players).map(|_| Arc::new(masters.clone())).collect();
        }
        if let Some(scout) = self.scout {
            place_scout(&mut world, scout);
        }
        let annexes: Vec<(ObjectId, MainBuildingId)> = self
            .objects
            .iter()
            .filter_map(|scene_object| Some((scene_object.id, scene_object.annex_of?)))
            .collect();
        for object in self.objects {
            place_object(&mut world, object);
        }
        link_annexes(&mut world, &annexes);
        world.reindex();
        world
    }

    /// The masters every player's table holds: the scout's and the objects'.
    fn masters(&self) -> Masters {
        let scout = self.scout.map(|scout| scout_master(scout.line_of_sight));
        Masters::new(
            scout.into_iter().chain(
                self.objects
                    .iter()
                    .map(|scene_object| scene_object.master.clone()),
            ),
        )
    }

    /// The tile's index in the scene's rows.
    fn index(&self, tile: Tile) -> usize {
        self.size.index(tile).expect("a tile of the scene")
    }
}

/// Player `own` of `players`: its own ally and every other's enemy, but for `relations`, of the
/// type the last of `kinds` for it gives.
fn player(
    own: usize,
    players: usize,
    relations: &[(PlayerId, PlayerId, Relation)],
    kinds: &[(PlayerId, PlayerKind)],
) -> Player {
    let mut row: Vec<Relation> = (0..players)
        .map(|other| {
            if other == own {
                Relation::Ally
            } else {
                Relation::Enemy
            }
        })
        .collect();
    for &(from, to, relation) in relations {
        if from.index() == own {
            row[to.index()] = relation;
        }
    }
    let kind = kinds
        .iter()
        .rev()
        .find(|(player_id, _)| player_id.index() == own)
        .map_or_else(PlayerKind::default, |&(_, kind)| kind);
    Player {
        kind,
        relations: ByPlayer::new(row),
        ..Player::default()
    }
}

/// What player `own`'s queries make of each player's objects (`player+0x278`) as its diplomacy
/// `relations` writes it: its own, an ally's, a neutral's or an enemy's.
fn unit_diplomacy(own: usize, relations: &Relations) -> UnitRelations {
    relations
        .values()
        .iter()
        .enumerate()
        .map(|(other, &relation)| {
            if other == own {
                return Some(UnitRelation::Own);
            }
            match relation {
                Relation::Ally | Relation::Relation4 => Some(UnitRelation::Ally),
                Relation::Neutral => Some(UnitRelation::Neutral),
                Relation::Enemy => Some(UnitRelation::Enemy),
                Relation::Other(_) => None,
            }
        })
        .collect()
}

/// The unified map's words for [`PLAYER`]'s `sight` of each tile, in a world of `players`,
/// with the objects-in-sight registry, empty; the others see nothing.
fn sight(size: GridSize, sight: &[TileSight], players: usize) -> Visibility {
    let word = |tile_sight: &TileSight| match tile_sight {
        TileSight::Unknown => VisionBits::empty(),
        TileSight::Explored => VisionBits::explored(PLAYER),
        TileSight::Seen => VisionBits::explored(PLAYER) | VisionBits::visible(PLAYER),
    };
    let mut visibility = Visibility::new(
        size,
        (0..players)
            .map(|_| VisibleMap {
                counts: vec![0; sight.len()],
                explored: TileCount::ZERO,
            })
            .collect(),
        sight.iter().map(word).collect(),
        Vec::new(),
        0,
        vec![PlayerKind::Human; players],
    );
    visibility.registry = Some(Registry::new(players, CLASSES));
    visibility
}

/// A rock's record on tile `tile`.
fn rock(tile: Tile) -> Placed {
    Placed {
        position: Position::new(tile.x() as f32 + 0.5, tile.y() as f32 + 0.5, 0.0),
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

/// The scout's movement: a walker of radius 0.2 at 0.8 tiles a second ([`walking`]).
fn walk_stats() -> MoveStats {
    walking(Speed::new(0.8))
}

/// A walker's movement at `speed` tiles a second: of radius 0.2, its restriction 7 and terrain
/// mask 6 (the land's class), turning at once.
#[must_use]
pub fn walking(speed: Speed) -> MoveStats {
    MoveStats {
        fly_mode: FlyMode::GROUND,
        speed,
        base: speed,
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

/// A master moving by `stats` and nothing else: its movement and its animated layer, whose speed
/// (`+0x740`, the master's slot 0xe8) is the movement's base, as the masters' loader fills both.
#[must_use]
pub fn moving_master(stats: MoveStats) -> Master {
    let mut master = Master::default();
    move_by(&mut master, stats);
    master
}

/// Master `master` moving by `stats` ([`moving_master`]'s two layers).
pub fn move_by(master: &mut Master, stats: MoveStats) {
    master.animated = Some(AnimatedPart {
        max_speed: stats.base,
    });
    master.move_stats = Some(stats);
}

/// The scout's master: a walking villager of `line_of_sight`.
fn scout_master(line_of_sight: Distance) -> Master {
    Master {
        id: SCOUT_MASTER,
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        radius: Footprint::new(0.2, 0.2),
        height: Distance::new(1.0),
        line_of_sight,
        ..moving_master(walk_stats())
    }
}

/// The scout and its standing mover, last moved at its place.
fn place_scout(world: &mut World, scout: Scout) {
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
    let _ = world
        .push_live(
            PLAYER,
            Object::moving(unit, standing(walk_stats(), scout.at)),
        )
        .expect("the scout joins its player");
}

/// The movement state of a unit of `stats` standing at `at`, facing east, last moved there.
fn standing(stats: MoveStats, at: Position) -> Mover {
    Mover {
        forward: BVector::new(1.0, 0.0, 0.0),
        right: BVector::new(0.0, 0.0, -1.0),
        velocity: None,
        paths: Vec::new(),
        factor: 1.0,
        stats,
        waiting: 0,
        wait_delays: WaitDelays::of_data(0),
        attack_moving: false,
        last_move: WorldTime::ZERO,
        moved_at: at,
        substitute_count: SubstituteCount::of_data(0),
        speedup: Speedup::NONE,
        ignore_units: Vec::new(),
        user_waypoints: Vec::new(),
        last_turn: Seconds::ZERO,
        angle: Angle::ZERO,
        animated: Animated::default(),
        pauses: Vec::new(),
        in_the_way: Vec::new(),
        sprite_facets: None,
    }
}

/// The object as the make action's spawn files a new one (`FUN_140ad0240`): its row in its
/// owner's list, a foundation's construction, its record on its tile (one with radii and a
/// height whose master obstructs, a desolid foundation none), the path planners' standing
/// object, and in the sight registry its seer, its UMV entry and its entry.
fn place_object(world: &mut World, scene_object: SceneObject) {
    let SceneObject {
        id,
        player,
        ref master,
        at,
        state,
        hp,
        seen_by,
        desolid,
        ref ai,
        flags,
        inside,
        ref actions,
        capture: _,
        pasture,
        sprite,
        annex_of: _,
        holds,
        workers,
    } = scene_object;
    let sprite = sprite.map(|id| {
        let graphic = *world.model.graphics.get(id).expect("the scene's graphic");
        ActiveSprite::start(graphic, &mut Well512::default(), world.model.tuning.sprite)
    });
    let unit = Unit {
        id,
        place: Static::at(at),
        life: Lifecycle {
            state,
            hp,
            master_id: master.id,
            inside,
            attribute_kind: holds.map(|(kind, _)| kind),
            amount: holds.map_or(Amount::ZERO, |(_, amount)| amount),
            ..Lifecycle::default()
        },
        flags,
        pasture,
        sprite,
        workers,
        ..Unit::default()
    };
    let mut object = if desolid {
        Object::with_building(unit, foundation(master))
    } else {
        Object::from(unit)
    };
    if let Some(stats) = master.move_stats {
        object = Object::moving(object, standing(stats, at));
    }
    if let Some(ai) = ai {
        object = Object::with_ai(object, ai.clone());
    }
    if let Some(layer) = actions {
        object = Object::with_actions(object, layer.clone());
    }
    let _ = world
        .push_live(player, object)
        .expect("the object joins its player");
    let solid = 0.0 < master.radius.x() && 0.0 < master.radius.y() && master.is_tall();
    let placed = Placed {
        position: at,
        radius: master.radius,
        class: master.class,
        player,
        master: master.id,
        obstruction_kind: master.obstruction_kind,
        buildable_on: master.buildable_on,
        tall: master.is_tall(),
        moves: master.moves(),
        unknown: None,
        obstruction: (master.obstruction_kind != ObstructionKind::NONE && solid && !desolid)
            .then_some(Obstruction {
                kind: master.obstruction_kind,
                class: master.obstruction_class,
            }),
    };
    world.spatial.insert(id, placed);
    world.pathing.moved(id, &placed, MoveState::Standing);
    world.pathing.tiles_changed(world.spatial.take_changes());
    // the capture flag is the seer's (`+0x924`): an object that holds one has its seer, in
    // the registry's lists or not
    if seen_by.is_some() || scene_object.capture != CaptureKind::None {
        register(world, &scene_object, seen_by);
    }
}

/// Each annex `(id, main)` names its main building, which holds it in its next free slot, in
/// the scene's order.
fn link_annexes(world: &mut World, annexes: &[(ObjectId, MainBuildingId)]) {
    for (index, &(id, main)) in annexes.iter().enumerate() {
        world
            .parts
            .building_mut(id)
            .expect("an annex's building layer")
            .annex_of = Some(main);
        let slot = annexes[..index]
            .iter()
            .filter(|&&(_, role_id)| role_id == main)
            .count();
        world
            .parts
            .building_mut(main.id())
            .expect("a main building's layer")
            .annexes
            .link(slot, id)
            .expect("a main building's free annex slot");
    }
}

/// A desolid foundation's construction: no work yet, no builder.
fn foundation(master: &Master) -> Building {
    Building {
        construction: Some(Construction {
            progress: Seconds::ZERO,
            desolid: true,
            builders: BuilderCount::of_data(0),
            hit_points: master.hit_points,
            build_time: WholeSeconds::of_data(0),
            record: Obstruction {
                kind: master.obstruction_kind,
                class: master.obstruction_class,
            },
            snow: Snow::None,
            sprite_override: false,
        }),
        ..Building::default()
    }
}

/// The object enters the sight registry: its seer with its capture flag, a building's sight and UMV
/// entry ([`Visibility::link_building`]), then what its sight update writes when it finds `mask`
/// ([`Visibility::combat_sight`]): the entry's mask and the records in the lists of the players of
/// `mask` ([`Registry::update_unit`]); none for an object outside the registry's lists.
fn register(world: &mut World, object: &SceneObject, mask: Option<VisionBits>) {
    let SceneObject {
        id,
        player,
        ref master,
        at,
        state,
        desolid,
        capture,
        ..
    } = *object;
    let building = master.kind == MasterKind::BUILDING;
    let players = world.players.len();
    let tuning = world.model.tuning.visibility;
    let visibility = world.spatial.sight.as_mut().expect("the scene's sight");
    let umv = if building {
        visibility
            .link_building(at, master.radius, tuning)
            .expect("a footprint on the map")
    } else {
        0
    };
    visibility.add_seer(
        id,
        Seer {
            player,
            master: master.id,
            los: master.line_of_sight,
            los_powerup: Distance::ZERO,
            state,
            explored: false,
            building: building.then_some(if desolid {
                BuildingSight::Unstarted
            } else {
                BuildingSight::Built
            }),
            umv: u8::try_from(umv).expect("a UMV entry of a byte"),
            capture,
            kind: master.kind,
        },
    );
    let Some(mask) = mask else {
        return;
    };
    let registry = visibility
        .registry
        .get_or_insert_with(|| Registry::new(players, CLASSES));
    registry
        .entries
        .get_or_insert_with(id, Entry::default)
        .expect("an object's entry")
        .mask = mask;
    registry
        .update_unit(UnitUpdate {
            id,
            owner: player,
            position: at.ground(),
            radii: master.radius,
            class: master.class,
            old: VisionBits::empty(),
            new: mask,
        })
        .expect("a class of the registry");
}

/// The scout's reads for its searches ([`ScoutArea::of`]), the map's size and its line of
/// sight.
///
/// # Panics
///
/// A world without the scene's scout.
pub(crate) fn scout_area(world: &mut World) -> (ScoutArea<'_>, GridSize, Distance) {
    let (_, unit) = world.object(scout_id()).expect("the scene's scout");
    let unit = unit.clone();
    let me = Me {
        human_order: WorldTime::ZERO,
        id: unit.id,
        group: None,
        action_state: ActionState::NO_ACTION,
        top_speed: TopSpeed::Own,
    };
    ScoutArea::of(world, &unit, me, PLAYER, PlayerKind::Human).expect("the scout's reads")
}
