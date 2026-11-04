use super::*;
use crate::GenieResult;
use crate::commands::GatherPoint;
use crate::commands::GatherPointState;
use crate::gathering::hunt::tests::walker;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::GroundOffset;
use crate::map::Radii;
use crate::map::Restriction;
use crate::map::TerrainFlags;
use crate::map::TerrainId;
use crate::map::Tile;
use crate::map::TileOffset;
use crate::map::TileRect;
use crate::map::spatial::TileSight;
use crate::movement::Speed;
use crate::movement::move_to::TargetMoved;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::Voice;
use crate::objects::facing::{SpriteFacets, angle_of_facet};
use crate::objects::layers::Static;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::HillMode;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::PlacementTerrains;
use crate::objects::masters::RecycleMode;
use crate::objects::masters::{Master, Masters};
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::objects::{FacetCount, Frame};
use crate::orders::OrderSource;
use crate::orders::town_bell::BellShelter;
use crate::players::Amount;
use crate::players::PlayerKind;
use crate::production::spawn::NearFlags;
use crate::production::spawn::SiteChecks;
use crate::production::spawn::ZoneTest;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::player_id;
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::roles;
use crate::testing::scene::{Scene, SceneObject, walking};
use crate::testing::tuning;
use crate::unit_ai::AiAction;
use crate::unit_ai::Issuer;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::{OrderPriority, QueuedOrder};
use crate::vision::VisionBits;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::parts::Part;
use crate::world::tests::forage_world;
use crate::world::{FrameSeed, Player};
use std::sync::Arc;

/// records/425906646's update 1883: a TC's gather point on a tree that is gone; its new
/// villager takes the closest tree of Gaia's (by centre) within its sight of the point,
/// measured from the tree's footprint, else none (it walks to the point).
#[test]
fn gather_point_whose_tree_is_gone_takes_the_closest_tree_within_sight() {
    let unit = |id, master, x: f32, y: f32| Unit {
        id,
        place: Static::at(Position::new(x, y, 0.0)),
        life: Lifecycle {
            master_id: master,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                sleeping: vec![
                    unit(object_id(10), master_id(349), 20.5, 20.5),
                    unit(object_id(11), master_id(349), 22.5, 20.5),
                ]
                .into(),
                ..Player::default()
            },
            Player {
                objects: vec![unit(object_id(99), ids::VILLAGER_MALE, 18.0, 18.0)].into(),
                ..Player::default()
            },
        ],
    );
    let tree = Master {
        id: master_id(349),
        class: class::TREE,
        radius: Footprint::new(0.5, 0.5),
        ..Master::default()
    };
    let villager = Master {
        id: ids::VILLAGER_MALE,
        class: class::VILLAGER,
        line_of_sight: Distance::new(4.0),
        ..Master::default()
    };
    t.model.masters = vec![
        Arc::new(Masters::new([tree.clone()])),
        Arc::new(Masters::new([villager, tree])),
    ];
    let g = GatherPoint {
        state: GatherPointState::Point,
        point: Position::new(21.0, 21.0, 0.0),
        target: Some(object_id(5)),
        target_master: Some(master_id(349)),
    };
    assert_eq!(
        t.lost_gather_target(PlayerId::new(1), ListIndex::new(0), g)
            .unwrap(),
        Some(object_id(10))
    );
    // 7 tiles from the closest tree's footprint, beyond the villager's sight of 4
    let far = GatherPoint {
        point: Position::new(30.0, 21.0, 0.0),
        ..g
    };
    assert_eq!(
        t.lost_gather_target(PlayerId::new(1), ListIndex::new(0), far)
            .unwrap(),
        None
    );
}

/// The hill test (mode `mode`) of tile `at` in the box whose corner is (2, 5), the corner's
/// elevation 3, on a 10 by 10 map at elevation 3 with the tiles `raised` at their own: the
/// tile's shape, as the map's load makes it of the elevations about it, and whether it fails.
fn hill_test(mode: u8, at: [i32; 2], raised: &[([i32; 2], u8)]) -> (u8, bool) {
    let scene = raised.iter().fold(
        Scene::open(GridSize::new(10, 10)).elevation(3),
        |scene, &(t, e)| scene.elevation_on([Tile::from(t)], e),
    );
    let w = scene.build();
    let map = WorldMap { turns: &w };
    let tile = Tile::from(at);
    let shape = map.shape(tile).unwrap();
    let fails = hill_fails(
        HillMode::of_data(mode),
        &map,
        tile,
        Tile::new(2, 5),
        Some(3),
    )
    .unwrap();
    (shape, fails)
}

/// Mode 1 takes flat ground and the four side slopes (shapes 5 to 8) but no saddle or corner,
/// mode 2 flat ground alone, mode 3 a tile of the box's first column or last row within one
/// step of the corner's elevation; the other modes take any tile.
#[test]
fn hill_mode_takes_flat_ground_side_slopes_or_an_edge_within_one_step() {
    let at = [3, 3];
    // mode 1: flat, the tile west of it one step up (5), the one east (8); a saddle (2: north
    // east up, west and south down) and a corner (9: south west up) fail
    assert_eq!(hill_test(1, at, &[]), (0, false));
    assert_eq!(hill_test(1, at, &[([2, 3], 4)]), (5, false));
    assert_eq!(hill_test(1, at, &[([4, 3], 4)]), (8, false));
    assert_eq!(
        hill_test(1, at, &[([4, 2], 4), ([2, 3], 2), ([3, 4], 2)]),
        (2, true)
    );
    assert_eq!(hill_test(1, at, &[([2, 4], 4)]), (9, true));
    // mode 2: flat alone; north up (6) fails
    assert_eq!(hill_test(2, at, &[]), (0, false));
    assert_eq!(hill_test(2, at, &[([3, 2], 4)]), (6, true));
    // mode 3: (2, 3) on the first column within one step passes, two steps up fails; (4, 5) on
    // the last row two steps down fails; (4, 4) on neither passes at any elevation
    assert!(!hill_test(3, [2, 3], &[([2, 3], 4)]).1);
    assert!(hill_test(3, [2, 3], &[([2, 3], 5)]).1);
    assert!(hill_test(3, [4, 5], &[([4, 5], 1)]).1);
    assert!(!hill_test(3, [4, 4], &[([4, 4], 7)]).1);
    // the other modes take any tile: a corner (12: south east up) at elevation 9
    assert_eq!(hill_test(0, at, &[([3, 3], 9), ([4, 4], 10)]), (12, false));
}

/// MarvinBlue/426048731's TC 9149 at (104, 47), radii 2: its gather point (107.2451,
/// 46.075863) lies right and below, so the right side's lower half (6) comes first among the
/// sides the first pass tries; the villager (radius 0.2) steps by 0.5 from (106.5, 47.5).
/// The sheep 9520 at (106.68, 47.15) blocks the first two candidates and stays (livestock in
/// state 2), so each ends its spot; (106.5, 46.5) is free.
#[test]
fn the_tc_spawns_its_villager_below_the_sheep_on_its_right() {
    let site = Site {
        position: Position::new(104.0, 47.0, 0.0),
        radius: Footprint::new(2.0, 2.0),
        gather: Some(Point::new(107.2451, 46.075_863)),
        walls: [false; 4],
    };
    let mut tried = Vec::new();
    let p = spawn_point(
        &site,
        Footprint::new(0.2, 0.2),
        &mut |c| {
            tried.push(c);
            Ok(if c.y() > 46.9 {
                Tested::Blocked { stays: true }
            } else {
                Tested::Free
            })
        },
        &tuning().spawn,
    )
    .unwrap();
    assert_eq!(p, Point::new(106.5, 46.5));
    assert_eq!(
        tried,
        [
            Point::new(106.5, 47.5),
            Point::new(106.5, 47.0),
            Point::new(106.5, 46.5)
        ]
    );
}

#[test]
fn blocker_that_moves_lets_the_spot_try_its_next_candidate() {
    let spot = Spot {
        at: Point::new(1.0, 1.0),
        step: GroundOffset::new(0.5, 0.0),
        dir: TileOffset::new(1, 0),
        count: 1,
    };
    let mut n = 0;
    let p = try_spot(&spot, &mut |_| {
        n += 1;
        Ok(if n == 1 {
            Tested::Blocked { stays: false }
        } else {
            Tested::Free
        })
    })
    .unwrap();
    assert_eq!(p, Some(Point::new(1.5, 1.0)));
}

fn near_sheep(state: ObjectState) -> Near {
    Near {
        id: object_id(9520),
        position: Position::new(106.68481, 47.14652, 0.0),
        radius: Footprint::new(0.35, 0.35),
        flags: NearFlags::TALL | NearFlags::MOVING,
        kind: MasterKind::COMBAT,
        class: class::LIVESTOCK,
        master: master_id(705),
        buildable_on: BuildableOn::NO,
        obstruction_kind: ObstructionKind::CIRCLE,
        state,
        player: PlayerId::new(0),
        fog_visibility: FogVisibility::INVISIBLE,
        fog_mask: None,
        hp: HitPoints::new(7.0),
        outline: Radii::ZERO,
    }
}

#[test]
fn livestock_stays_alive_tree_only_once_dead() {
    assert!(near_sheep(ObjectState::Alive).stays(&roles()));
    assert!(!near_sheep(ObjectState::Unfinished).stays(&roles()));
    let tree = Near {
        class: class::TREE,
        kind: MasterKind::STATIC,
        ..near_sheep(ObjectState::Alive)
    };
    assert!(!tree.stays(&roles()));
    assert!(
        Near {
            state: ObjectState::Dying,
            ..tree
        }
        .stays(&roles())
    );
}

/// The sheep 9520's master (705): livestock of radii 0.35 that walks, 7 hit points.
fn sheep_master() -> Master {
    Master {
        id: master_id(705),
        kind: MasterKind::COMBAT,
        class: class::LIVESTOCK,
        radius: Footprint::new(0.35, 0.35),
        height: Distance::new(0.5),
        obstruction_kind: ObstructionKind::CIRCLE,
        fog_visibility: FogVisibility::INVISIBLE,
        hit_points: MasterHitPoints::of_data(7),
        move_stats: Some(walking(Speed::new(0.7))),
        ..Master::default()
    }
}

/// Gaia's sheep 9520 at (106.68, 47.15), alive: the world's object that reads as
/// [`near_sheep`].
fn sheep() -> SceneObject {
    SceneObject::new(
        9520,
        PlayerId::GAIA,
        sheep_master(),
        Position::new(106.68481, 47.14652, 0.0),
    )
}

/// A house's master: a building of radii 1 and obstruction type 2, without a fog visibility.
fn house_master() -> Master {
    Master {
        id: ids::HOUSE,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(1.0, 1.0),
        height: Distance::new(2.0),
        obstruction_kind: ObstructionKind::KIND_2,
        obstruction_class: ObstructionClass::UNIT,
        fog_visibility: FogVisibility::INVISIBLE,
        hit_points: MasterHitPoints::of_data(550),
        ..Master::default()
    }
}

/// In the fog, of the objects without a fog visibility only the player's own block a BUILD,
/// and another player's building whose registry mask meets both the player's masks: player
/// 1 explored the map and sees none of it; player 2's house 500 at (20.5, 20.5) was last seen
/// by the players of its mask.
#[test]
fn fogged_building_blocks_build_by_its_visibility_mask() {
    let site = BuildSite {
        player: PlayerId::new(1),
        checks: SiteChecks::HUMAN,
        radius: Footprint::new(1.0, 1.0),
        tall: false,
        class: class::BUILDING,
        obstructs: true,
        restriction: &Restriction::new(vec![1.0, 0.0]),
        hill_mode: HillMode::ANY,
        placement: PlacementTerrains::NONE,
        own: &[],
    };
    let test = |owner, seen_by| {
        let house = SceneObject {
            seen_by,
            ..SceneObject::new(500, owner, house_master(), Position::new(20.5, 20.5, 0.0))
        };
        let w = Scene::open(GridSize::new(120, 120))
            .sight(TileSight::Explored)
            .players(2)
            .object(house)
            .build();
        build_placement(
            &site,
            &WorldMap { turns: &w },
            Point::new(21.0, 21.0),
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
        .code()
    };
    let (one, two) = (PlayerId::new(1), PlayerId::new(2));
    // player 1 saw the house: its mask meets what player 1 sees and explored, it blocks
    assert_eq!(
        test(
            two,
            Some(VisionBits::visible(one) | VisionBits::explored(one))
        ),
        6
    );
    // seen by player 2 alone (player 1's explored bit kept): another player's house in the
    // fog passes
    assert_eq!(
        test(
            two,
            Some(VisionBits::visible(two) | VisionBits::explored(one))
        ),
        0
    );
    // outside the registry neither, but the player's own does
    assert_eq!(test(two, None), 0);
    assert_eq!(test(one, None), 6);
}

/// The world's sheep reads as [`near_sheep`] on its tile; it blocks a villager (radius 0.2) at the
/// points whose box overlaps its own.
#[test]
fn the_sheep_blocks_the_villager_where_their_boxes_overlap() {
    let villager = Placing {
        player: PlayerId::new(1),
        radius: Footprint::new(0.2, 0.2),
        tall: true,
        class: class::VILLAGER,
        restriction: &Restriction::new(vec![1.0, 0.0]),
        hill_mode: HillMode::ANY,
        placement: PlacementTerrains::NONE,
    };
    let w = Scene::open(GridSize::new(120, 120)).object(sheep()).build();
    let map = WorldMap { turns: &w };
    assert_eq!(
        map.objects(Tile::new(106, 47)),
        Ok(vec![near_sheep(ObjectState::Alive)])
    );
    let at = |x, y| {
        placement(
            &villager,
            &map,
            Point::new(x, y),
            ZoneTest::Skipped,
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
    };
    assert_eq!(at(106.5, 47.5), Tested::Blocked { stays: true });
    assert_eq!(at(106.5, 47.0), Tested::Blocked { stays: true });
    assert_eq!(at(106.5, 46.5), Tested::Free);
}

/// Two objects on one tile that block the point differently, a sheep that stays (state 2) and
/// a soldier that does not: the first of the tile's list (the one placed first) reports
/// (the game's scan, `FUN_141264db0`), whichever stays.
#[test]
fn the_first_object_of_tile_list_reports_the_block() {
    let villager = Placing {
        player: PlayerId::new(1),
        radius: Footprint::new(0.2, 0.2),
        tall: true,
        class: class::VILLAGER,
        restriction: &Restriction::new(vec![1.0, 0.0]),
        hill_mode: HillMode::ANY,
        placement: PlacementTerrains::NONE,
    };
    let soldier = || {
        let master = Master {
            id: master_id(4),
            class: class::CLASS_6,
            ..sheep_master()
        };
        SceneObject::new(9521, PlayerId::GAIA, master, sheep().at)
    };
    let at = |first: SceneObject, second: SceneObject| {
        let w = Scene::open(GridSize::new(120, 120))
            .object(first)
            .object(second)
            .build();
        placement(
            &villager,
            &WorldMap { turns: &w },
            Point::new(106.5, 47.5),
            ZoneTest::Skipped,
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
    };
    assert_eq!(at(sheep(), soldier()), Tested::Blocked { stays: true });
    assert_eq!(at(soldier(), sheep()), Tested::Blocked { stays: false });
}

/// `FUN_140ad0240` at the forage world's town centre 700: a villager (master 83) spawns beside
/// it and stands idle; attribute 81 lowers by its population and the centre's unit AI hears
/// that its make action completed (0x1fa with 0x26b). With the centre's command queue the
/// new villager's unit AI takes each order behind its own (priority 100, ready, at the
/// centre's height). A computer player's spawn, one without the players' sight and a master
/// that does not move are not modelled.
#[test]
fn made_villager_spawns_beside_its_town_centre_and_takes_its_queue() {
    let world = || {
        let mut t = forage_world();
        // the villager's standing sprite (a graphic of ten frames, eight facets)
        for masters in &mut t.model.masters {
            Arc::make_mut(masters)
                .get_mut(ids::VILLAGER_MALE)
                .unwrap()
                .standing_sprite = Some(SpriteId::named(300));
        }
        let _ = Arc::make_mut(&mut t.model.graphics).insert(
            SpriteId::named(300),
            Graphic {
                id: SpriteId::named(300),
                frames: Frame::of_data(10),
                facets: FacetCount::of_data(8),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            },
        );
        let p = &mut t.players[1];
        p.attributes[attribute::IN_PRODUCTION.index()] = Amount::new(3.0);
        t.parts.set_unit_ai(
            p.objects[ListIndex::new(2)].id,
            Some(UnitAi {
                ai_kind: UnitAiKind::Predator,
                current_order: None,
                current_action: None,
                current_target: None,
                ..UnitAi::default()
            }),
        );
        t
    };
    let mut t = world();
    let before = t.players[1].objects.len();
    t.spawn(
        PlayerId::new(1),
        ListIndex::new(2),
        ids::VILLAGER_MALE,
        Amount::new(1.0),
    )
    .unwrap();
    let p = &t.players[1];
    assert_eq!(p.objects.len(), before + 1);
    let made = p.objects.last().unwrap();
    assert_eq!(made.life.master_id, ids::VILLAGER_MALE);
    assert_eq!(
        p.attributes[attribute::IN_PRODUCTION.index()],
        Amount::new(2.0)
    );
    let completed = t
        .parts
        .unit_ai
        .get(p.objects[ListIndex::new(2)].id)
        .unwrap()
        .notifies
        .iter()
        .any(|n| n.kind == NotifyKind::ACTION_COMPLETED && n.p == [AiAction::Make.value(), 0, 0]);
    assert!(completed);
    // the centre's command queue goes to the new unit
    let mut t = world();
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: OrderPriority::of_data(5),
        target: None,
        target_owner: None,
        point: Position::new(40.0, 80.0, 7.0),
        range: Distance::new(2.0),
        shown: Point::new(40.0, 80.0),
        ready: false,
        target_kind: None,
    };
    t.parts
        .unit_ai
        .get_mut(t.players[1].objects[ListIndex::new(2)].id)
        .unwrap()
        .building_orders
        .push(order);
    t.spawn(
        PlayerId::new(1),
        ListIndex::new(2),
        ids::VILLAGER_MALE,
        Amount::new(1.0),
    )
    .unwrap();
    let made = t.players[1].objects.last().unwrap();
    let queued = t
        .parts
        .unit_ai
        .get(made.id)
        .map(|ai| ai.orders.clone())
        .unwrap_or_default();
    assert_eq!(
        queued
            .iter()
            .map(|o| (o.order, o.priority.get(), o.point, o.range, o.ready))
            .collect::<Vec<_>>(),
        [(
            OrderKind::Move,
            100,
            Position::new(40.0, 80.0, 0.0),
            Distance::ZERO,
            true
        )]
    );
    let mut t = world();
    t.players[1].kind = PlayerKind::Computer;
    assert!(
        t.spawn(
            PlayerId::new(1),
            ListIndex::new(2),
            ids::VILLAGER_MALE,
            Amount::new(1.0)
        )
        .is_err()
    );
    let mut t = world();
    t.spatial.sight = None;
    assert!(
        t.spawn(
            PlayerId::new(1),
            ListIndex::new(2),
            ids::VILLAGER_MALE,
            Amount::new(1.0)
        )
        .is_err()
    );
    let mut t = world();
    assert!(
        t.spawn(
            PlayerId::new(1),
            ListIndex::new(2),
            master_id(59),
            Amount::new(1.0)
        )
        .is_err()
    );
}

/// A new unit's top speed (slot 65, `FUN_14114d1d0`) is the one of the sprite it holds, its
/// standing one: its master's speed times that sprite's factor, not the master's speed with its
/// running sprite (the deer 9655 of MarvinBlue/426048731 ran at the running speed).
#[test]
fn made_unit_moves_at_the_speed_of_its_standing_sprite() {
    let mut t = forage_world();
    for masters in &mut t.model.masters {
        let villager = Arc::make_mut(masters).get_mut(ids::VILLAGER_MALE).unwrap();
        villager.standing_sprite = Some(SpriteId::named(300));
        let moving = villager.move_stats.as_mut().unwrap();
        moving.base = Speed::new(0.8);
        moving.speed = Speed::new(1.6);
    }
    let _ = Arc::make_mut(&mut t.model.graphics).insert(
        SpriteId::named(300),
        Graphic {
            id: SpriteId::named(300),
            frames: Frame::of_data(10),
            facets: FacetCount::of_data(8),
            speed_multiplier: SpeedMultiplier::of_data(0.5),
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags: SpriteFlags::ANIMATED,
        },
    );
    t.players[1].attributes[attribute::IN_PRODUCTION.index()] = Amount::new(1.0);
    t.spawn(
        PlayerId::new(1),
        ListIndex::new(2),
        ids::VILLAGER_MALE,
        Amount::new(1.0),
    )
    .unwrap();
    let made = t.players[1].objects.last().unwrap();
    assert_eq!(made.life.master_id, ids::VILLAGER_MALE);
    assert_eq!(
        t.parts.mover_of(made.id).unwrap().stats.speed,
        Speed::new(0.4)
    );
}

/// `FUN_140ace9c0` sends the new unit to its building's gather point only for a state of 1
/// (`+0xc30`); a state of 0xff (SET GATHER STATE's) leaves it standing, which the unit's wait
/// to move (`+0x7f9`) shows.
#[test]
fn only_gather_point_of_state_1_sends_the_new_unit() {
    let waits = |state| {
        let mut t = forage_world();
        for masters in &mut t.model.masters {
            Arc::make_mut(masters)
                .get_mut(ids::VILLAGER_MALE)
                .unwrap()
                .standing_sprite = Some(SpriteId::named(300));
        }
        let _ = Arc::make_mut(&mut t.model.graphics).insert(
            SpriteId::named(300),
            Graphic {
                id: SpriteId::named(300),
                frames: Frame::of_data(10),
                facets: FacetCount::of_data(8),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            },
        );
        let p = &mut t.players[1];
        p.attributes[attribute::IN_PRODUCTION.index()] = Amount::new(3.0);
        t.parts.set_unit_ai(
            p.objects[ListIndex::new(2)].id,
            Some(UnitAi {
                ai_kind: UnitAiKind::Predator,
                current_order: None,
                current_action: None,
                current_target: None,
                ..UnitAi::default()
            }),
        );
        let at = p.objects[ListIndex::new(2)].id;
        let gather = Some(GatherPoint {
            state,
            point: Position::new(40.0, 80.0, 0.0),
            target: None,
            target_master: None,
        });
        t.set_part(at, Part::GatherPoint(gather)).unwrap();
        t.spawn(
            PlayerId::new(1),
            ListIndex::new(2),
            ids::VILLAGER_MALE,
            Amount::new(1.0),
        )
        .unwrap();
        t.players[1]
            .objects
            .last()
            .and_then(|u| t.parts.mover_of(u.id).map(|m| m.waiting))
    };
    assert_eq!(waits(GatherPointState::Point), Some(1));
    assert_eq!(waits(GatherPointState::Kept), Some(0));
}

/// Every candidate of `site`'s search for a unit of radius 0.5 (a step of 0.5, no gap), all
/// blocked by objects that stay: each spot's first candidate in turn, and the error of the
/// later rings.
fn all_blocked(site: &Site) -> (Vec<Point>, GenieResult<Point>) {
    let mut tried = Vec::new();
    let r = spawn_point(
        site,
        Footprint::new(0.5, 0.5),
        &mut |c| {
            tried.push(c);
            Ok(Tested::Blocked { stays: true })
        },
        &tuning().spawn,
    );
    (tried, r)
}

/// `FUN_1411d12d0` without a gather point: one pass over the eight segments of an open
/// building's frame (left 8.5, right 11.5, bottom 8.5, top 11.5): the top side's left quarter
/// and a half more (1), the left side's top part (2), the top's right part (3), the left's
/// lower part (4), the right side's top half (5) and lower half (6), the bottom side's left
/// half (7) and right half (8). Then the later rings, not modelled.
#[test]
fn spawn_search_without_gather_point_walks_the_eight_segments_once() {
    let site = Site {
        position: Position::new(10.0, 10.0, 0.0),
        radius: Footprint::new(1.0, 1.0),
        gather: None,
        walls: [false; 4],
    };
    let (tried, r) = all_blocked(&site);
    let top = [
        Point::new(8.5, 11.5),
        Point::new(9.0, 11.5),
        Point::new(9.5, 11.5),
    ];
    let left = [
        Point::new(8.5, 11.5),
        Point::new(8.5, 11.0),
        Point::new(8.5, 10.5),
    ];
    let top_right = [
        Point::new(9.75, 11.5),
        Point::new(10.25, 11.5),
        Point::new(10.75, 11.5),
        Point::new(11.25, 11.5),
    ];
    let left_low = [
        Point::new(8.5, 10.25),
        Point::new(8.5, 9.75),
        Point::new(8.5, 9.25),
        Point::new(8.5, 8.75),
    ];
    let right = [Point::new(11.5, 11.5), Point::new(11.5, 11.0)];
    let right_low = [
        Point::new(11.5, 10.5),
        Point::new(11.5, 10.0),
        Point::new(11.5, 9.5),
        Point::new(11.5, 9.0),
    ];
    let bottom = [Point::new(8.5, 8.5), Point::new(9.0, 8.5)];
    let bottom_right = [
        Point::new(9.5, 8.5),
        Point::new(10.0, 8.5),
        Point::new(10.5, 8.5),
        Point::new(11.0, 8.5),
    ];
    let segments: [&[Point]; 8] = [
        &top,
        &left,
        &top_right,
        &left_low,
        &right,
        &right_low,
        &bottom,
        &bottom_right,
    ];
    assert_eq!(tried, segments.concat());
    assert_eq!(
        r,
        Err(GenieError::unsupported(
            "a spawn point beyond the building's sides (the search's later rings)"
        ))
    );
}

/// With a gather point away from the walled corners (top right, bottom left) the first pass
/// tries nothing; the second takes the open top left corner's two segments (1, 2), then the
/// bottom side from the right (9) and the right side from the bottom (10) as one segment.
#[test]
fn second_pass_between_walled_corners_tries_the_bottom_right_corner() {
    let site = Site {
        position: Position::new(10.0, 10.0, 0.0),
        radius: Footprint::new(1.0, 1.0),
        gather: Some(Point::new(20.0, 5.0)),
        walls: [false, true, false, true],
    };
    let (tried, r) = all_blocked(&site);
    assert_eq!(
        tried,
        [
            Point::new(8.5, 11.5),
            Point::new(9.0, 11.5),
            Point::new(9.5, 11.5),
            Point::new(8.5, 11.5),
            Point::new(8.5, 11.0),
            Point::new(8.5, 10.5),
            Point::new(11.5, 8.5),
            Point::new(11.0, 8.5),
            Point::new(11.5, 8.5),
            Point::new(11.5, 9.0)
        ]
    );
    assert!(r.is_err());
}

/// A queued move of the centre's, which a new unit's notified walk forgets.
fn queued_move() -> QueuedOrder {
    QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: OrderPriority::of_data(5),
        target: None,
        target_owner: None,
        point: Position::new(40.0, 80.0, 7.0),
        range: Distance::new(2.0),
        shown: Point::new(40.0, 80.0),
        ready: false,
        target_kind: None,
    }
}

/// `FUN_140ace9c0` with a point: the new villager waits to move, takes the move order and
/// action with no target at the point, leaves the town bell and walks there (a `MoveTo` of
/// range 0); a new unit's queue empties, an unloaded unit's stays.
#[test]
fn gather_point_without_target_walks_the_new_unit_there() {
    use crate::movement::move_to::{MoveTo, MoveToAction, MoveWait};
    let walked = |source| {
        let mut t = forage_world();
        let u = &mut t.players[1].objects[ListIndex::new(0)];
        t.parts.set_town_bell_flag(u.id, BellShelter::Sheltered);
        t.parts
            .unit_ai
            .get_mut(u.id)
            .unwrap()
            .orders
            .push(queued_move());
        t.gather_move(
            PlayerId::new(1),
            ListIndex::new(0),
            Position::new(40.0, 80.0, 0.0),
            source,
        )
        .unwrap();
        t.swap_remove_live(PlayerId::new(1), ListIndex::new(0))
            .unwrap()
    };
    let u = walked(OrderSource::Direct);
    assert_eq!(u.parts.mover().unwrap().waiting, 1);
    assert!(!u.parts.town_bell.flag.is_sheltered());
    let ai = u.parts.unit_ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (Some(OrderKind::Move), Some(AiAction::Move), None)
    );
    assert_eq!(ai.target_point, Position::new(40.0, 80.0, 0.0));
    assert_eq!(ai.orders, []);
    assert_eq!(
        u.parts.actions.list.move_to().copied(),
        Some(MoveToAction {
            sprite: None,
            state: MoveTo::Start,
            point: Position::new(40.0, 80.0, 0.0),
            target: None,
            range: Distance::ZERO,
            voice: Voice::Loud,
            on_target_moved: TargetMoved::Follows,
            wait: MoveWait::default(),
        })
    );
    let unloaded = walked(OrderSource::Queued);
    assert_eq!(
        unloaded.parts.unit_ai.as_ref().unwrap().orders,
        [queued_move()]
    );
    assert_eq!(
        unloaded.parts.actions.list.move_to().copied(),
        u.parts.actions.list.move_to().copied()
    );
}

/// A unit without speed only waits to move; one that already acts, or one not in state 2,
/// is not modelled.
#[test]
fn gather_point_walk_waits_without_speed_and_stops_at_the_unmodelled() {
    let mut t = forage_world();
    t.parts
        .mover_mut(t.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .stats
        .speed = Speed::ZERO;
    t.gather_move(
        PlayerId::new(1),
        ListIndex::new(0),
        Position::new(40.0, 80.0, 0.0),
        OrderSource::Direct,
    )
    .unwrap();
    let u = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(t.parts.mover_of(u.id).unwrap().waiting, 1);
    assert_eq!(t.parts.actions_of(u.id).list.move_to().copied(), None);
    assert_eq!(t.parts.unit_ai.get(u.id).unwrap().current_order, None);
    // a second walk finds the first's `MoveTo`
    let mut t = forage_world();
    t.gather_move(
        PlayerId::new(1),
        ListIndex::new(0),
        Position::new(40.0, 80.0, 0.0),
        OrderSource::Direct,
    )
    .unwrap();
    let reason = |t: &mut World| {
        t.gather_move(
            PlayerId::new(1),
            ListIndex::new(0),
            Position::new(41.0, 80.0, 0.0),
            OrderSource::Direct,
        )
        .unwrap_err()
        .reason()
    };
    assert_eq!(
        reason(&mut t),
        "a gather point's move of a unit with an action"
    );
    let mut t = forage_world();
    t.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Dying;
    assert_eq!(
        reason(&mut t),
        "a gather point's move of a unit not in state 2"
    );
}

/// [`crate::world::tests::forage_world`] with the remains masters 800 (static, its storage of
/// flag 2 given to its owner and of flag 0 held, its death sprite 20 of eight facets starting
/// on a random one) and 801 (moving, its standing sprite 21 of eight facets).
fn remains_world() -> World {
    use crate::objects::masters::{Master, Masters};
    let mut t = forage_world();
    let graphic = |id, flags| Graphic {
        id,
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags,
    };
    for (id, flags) in [
        (20, SpriteFlags::RANDOM_FIRST_FRAME),
        (21, SpriteFlags::ANIMATED),
    ] {
        let id = SpriteId::named(id);
        let _ = Arc::make_mut(&mut t.model.graphics).insert(id, graphic(id, flags));
    }
    let static_remains = Master {
        id: master_id(800),
        kind: MasterKind::STATIC,
        death_sprite: Some(SpriteId::named(20)),
        standing_sprite: None,
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(1.0), 2),
            (Some(attribute::POPULATION), Amount::new(5.0), 0),
        ],
        ..Master::default()
    };
    let moving_remains = Master {
        id: master_id(801),
        kind: MasterKind::MOVING,
        standing_sprite: Some(SpriteId::named(21)),
        move_stats: Some(walker().stats),
        ..Master::default()
    };
    for masters in &mut t.model.masters {
        *masters = Arc::new(Masters::new(
            masters
                .iter()
                .cloned()
                .chain([static_remains.clone(), moving_remains.clone()]),
        ));
    }
    t
}

/// The master's slot 0x40 for remains (`FUN_1411a45a0`, `FUN_141141640`): a static object gives
/// its owner its storage of flag 2, holds its entry of flag 0 and dies on its death sprite's
/// eight facets; moving remains face their dead object's facet (with their own sprite's
/// facets). Another type, a static object with hit points, a recyclable master, moving remains
/// without a facet or without a moving master are not modelled.
#[test]
fn remains_take_their_storage_and_face_their_dead_object_facet() {
    use crate::objects::lifecycle::Class;
    let made = |t: &World, id| t.model.masters[1].get(id).unwrap().clone();
    let mut t = remains_world();
    let food = t.players[1].attributes[4];
    let i = t
        .plain_object(
            PlayerId::new(1),
            master_id(800),
            &made(&t, master_id(800)),
            Position::new(30.5, 85.5, 0.0),
            Facing::Dead(None),
        )
        .unwrap();
    let u = &t.players[1].objects[i];
    assert_eq!(
        (u.life.class, u.life.attribute_kind, u.life.amount),
        (Class::Static, Some(attribute::POPULATION), Amount::new(5.0))
    );
    assert_eq!(
        (u.life.death_facets, u.sprite),
        (Some(FacetCount::of_data(8)), None)
    );
    assert_eq!(t.players[1].attributes[4], food + Amount::new(1.0));
    let i = t
        .plain_object(
            PlayerId::new(1),
            master_id(801),
            &made(&t, master_id(801)),
            Position::new(31.5, 85.5, 0.0),
            Facing::Dead(Some(Facet::of_data(2))),
        )
        .unwrap();
    let u = &t.players[1].objects[i];
    let facets = SpriteFacets {
        count: FacetCount::of_data(8),
        turns: false,
    };
    assert_eq!(
        t.parts.mover_of(u.id).map(|m| m.angle),
        Some(angle_of_facet(Facet::of_data(2), facets))
    );
    let reason = |change: &dyn Fn(&mut Master), facing| {
        let mut t = remains_world();
        let mut m = made(&t, master_id(801));
        change(&mut m);
        t.plain_object(
            PlayerId::new(1),
            m.id,
            &m,
            Position::new(31.5, 85.5, 0.0),
            facing,
        )
        .unwrap_err()
        .reason()
    };
    let unchanged = |_: &mut Master| {};
    assert_eq!(
        reason(&|m| m.kind = MasterKind::COMBAT, Facing::Drawn),
        "remains of a type other than 10 and 20 to 39"
    );
    assert_eq!(
        reason(
            &|m| {
                m.kind = MasterKind::STATIC;
                m.hit_points = MasterHitPoints::of_data(5);
            },
            Facing::Drawn
        ),
        "remains with hit points (the sleeping list)"
    );
    assert_eq!(
        reason(&|m| m.recyclable = RecycleMode::RECYCLED, Facing::Drawn),
        "a new object of a recyclable master (a negative id)"
    );
    assert_eq!(
        reason(&unchanged, Facing::Dead(None)),
        "moving remains of an object without a facet the slice holds"
    );
    assert_eq!(
        reason(&|m| m.move_stats = None, Facing::Drawn),
        "moving remains without a moving master"
    );
}

/// A map of 120 by 120 tiles of land that player 1 sees as `sight`, without objects.
fn plain(sight: TileSight) -> World {
    Scene::open(GridSize::new(120, 120)).sight(sight).build()
}

/// Player 1's house site (class 3, radii 1, obstructing, no height) on terrain 0, which `restriction`
/// rates; `human` or not.
const fn house_site(restriction: &Restriction, human: bool) -> BuildSite<'_> {
    BuildSite {
        player: PlayerId::new(1),
        checks: if human {
            SiteChecks::HUMAN
        } else {
            SiteChecks::empty()
        },
        radius: Footprint::new(1.0, 1.0),
        tall: false,
        class: class::BUILDING,
        obstructs: true,
        restriction,
        hill_mode: HillMode::ANY,
        placement: PlacementTerrains::NONE,
        own: &[],
    }
}

/// BUILD's static test: off the map 7, a forbidden terrain 2, a human who explored none of
/// the tiles 5 (a computer passes); a box that passes answers none; a zone mode is not
/// modelled.
#[test]
fn build_static_test_answers_the_map_edge_terrain_and_exploration() {
    use crate::production::spawn::placement_test::build_static_test;
    let open_values = Restriction::new(vec![1.0]);
    let open = house_site(&open_values, true);
    let seen = plain(TileSight::Seen);
    let flat = WorldMap { turns: &seen };
    assert_eq!(
        build_static_test(&open, &flat, Point::new(50.0, 50.0), &tuning().spawn)
            .map(|c| c.map(PlacementCode::code)),
        Ok(None)
    );
    assert_eq!(
        build_static_test(&open, &flat, Point::new(0.5, 50.0), &tuning().spawn)
            .map(|c| c.map(PlacementCode::code)),
        Ok(Some(7))
    );
    assert_eq!(
        build_static_test(&open, &flat, Point::new(119.5, 50.0), &tuning().spawn)
            .map(|c| c.map(PlacementCode::code)),
        Ok(Some(7))
    );
    let forbidden_values = Restriction::new(vec![0.0]);
    let forbidden = house_site(&forbidden_values, true);
    assert_eq!(
        build_static_test(&forbidden, &flat, Point::new(50.0, 50.0), &tuning().spawn)
            .map(|c| c.map(PlacementCode::code)),
        Ok(Some(2))
    );
    let unknown = plain(TileSight::Unknown);
    let dark = WorldMap { turns: &unknown };
    assert_eq!(
        build_static_test(&open, &dark, Point::new(50.0, 50.0), &tuning().spawn)
            .map(|c| c.map(PlacementCode::code)),
        Ok(Some(5))
    );
    let computer_values = Restriction::new(vec![1.0]);
    let computer = house_site(&computer_values, false);
    assert_eq!(
        build_static_test(&computer, &dark, Point::new(50.0, 50.0), &tuning().spawn)
            .map(|c| c.map(PlacementCode::code)),
        Ok(None)
    );
    let zoned = BuildSite {
        checks: SiteChecks::ZONES,
        ..open
    };
    assert_eq!(
        build_static_test(&zoned, &flat, Point::new(50.0, 50.0), &tuning().spawn)
            .map(|c| c.map(PlacementCode::code))
            .unwrap_err()
            .reason(),
        "BUILD's placement test in a zone mode"
    );
}

/// BUILD's scan reads 8 tiles either way of the point, from 0 to the map's last tile.
#[test]
fn build_scan_reads_eight_tiles_either_way_within_the_map() {
    use crate::production::spawn::placement_test::build_scan_range;
    assert_eq!(build_scan_range(50.5, 120, &tuning().spawn), (42, 58));
    assert_eq!(build_scan_range(2.0, 120, &tuning().spawn), (0, 10));
    assert_eq!(build_scan_range(115.0, 120, &tuning().spawn), (107, 119));
}

/// BUILD's sight test: in sight any object but one of fog visibility 3; in the fog one with a
/// fog visibility, and of those without only the player's own; on a tile never explored any;
/// a fogged object of fog visibility 3 is not modelled.
#[test]
fn build_scan_counts_what_the_player_sees_of_tile() {
    use crate::production::spawn::placement_test::build_sees;
    let site_values = Restriction::new(vec![1.0]);
    let site = house_site(&site_values, true);
    let seen = plain(TileSight::Seen);
    let flat = WorldMap { turns: &seen };
    let explored = plain(TileSight::Explored);
    let fogged = WorldMap { turns: &explored };
    let unknown = plain(TileSight::Unknown);
    let dark = WorldMap { turns: &unknown };
    let o = |player, fog_visibility| Near {
        player,
        fog_visibility,
        ..near_sheep(ObjectState::Alive)
    };
    let sees = |map: &WorldMap<'_>, o: Near, sight| build_sees(&site, map, &o, sight, &roles());
    assert_eq!(
        sees(
            &flat,
            o(PlayerId::new(2), FogVisibility::INVISIBLE),
            (true, true)
        ),
        Ok(true)
    );
    assert_eq!(
        sees(
            &flat,
            o(PlayerId::new(2), FogVisibility::ONLY_IN_FOG),
            (true, true)
        ),
        Ok(false)
    );
    assert_eq!(
        sees(
            &fogged,
            o(PlayerId::new(2), FogVisibility::VISIBLE),
            (true, false)
        ),
        Ok(true)
    );
    assert_eq!(
        sees(
            &fogged,
            o(PlayerId::new(2), FogVisibility::INVISIBLE),
            (true, false)
        ),
        Ok(false)
    );
    assert_eq!(
        sees(
            &fogged,
            o(PlayerId::new(1), FogVisibility::INVISIBLE),
            (true, false)
        ),
        Ok(true)
    );
    assert_eq!(
        sees(
            &dark,
            o(PlayerId::new(2), FogVisibility::INVISIBLE),
            (false, false)
        ),
        Ok(true)
    );
    assert_eq!(
        sees(
            &fogged,
            o(PlayerId::new(2), FogVisibility::ONLY_IN_FOG),
            (true, false)
        )
        .unwrap_err()
        .reason(),
        "a fogged object of fog visibility 3 near a BUILD"
    );
}

/// BUILD's scan ignores a dead house (a unit only past state 6), a moving unit where the site
/// does not obstruct, a wall under the player's own gate and another player's desolid house
/// (not one of class 51); it keeps a standing house.
#[test]
fn build_scan_ignores_the_dead_the_moving_own_walls_and_foreign_foundations() {
    use crate::production::spawn::placement_test::build_ignores;
    let site_values = Restriction::new(vec![1.0]);
    let site = house_site(&site_values, true);
    let seen = plain(TileSight::Seen);
    let flat = WorldMap { turns: &seen };
    let house = Near {
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        flags: NearFlags::empty(),
        player: PlayerId::new(2),
        state: ObjectState::Alive,
        ..near_sheep(ObjectState::Alive)
    };
    let ignores = |site: &BuildSite<'_>, o: Near| build_ignores(site, &flat, &o, &roles());
    assert!(!ignores(&site, house));
    assert!(ignores(
        &site,
        Near {
            state: ObjectState::Dying,
            ..house
        }
    ));
    let unit = Near {
        kind: MasterKind::COMBAT,
        state: ObjectState::Undead,
        ..house
    };
    assert!(!ignores(&site, unit));
    assert!(ignores(
        &site,
        Near {
            state: ObjectState::Removed,
            ..unit
        }
    ));
    let walking = Near {
        flags: NearFlags::MOVING,
        ..unit
    };
    assert!(!ignores(&site, walking));
    let open = BuildSite {
        obstructs: false,
        ..site
    };
    assert!(ignores(&open, walking));
    let gate = BuildSite {
        class: class::GATE,
        ..site
    };
    let own_wall = Near {
        class: class::WALL,
        player: PlayerId::new(1),
        ..house
    };
    assert!(ignores(&gate, own_wall));
    assert!(!ignores(
        &gate,
        Near {
            player: PlayerId::new(2),
            ..own_wall
        }
    ));
    let foundation = Near {
        flags: NearFlags::DESOLID,
        ..house
    };
    assert!(ignores(&site, foundation));
    assert!(!ignores(
        &site,
        Near {
            class: class::CLASS_51,
            ..foundation
        }
    ));
}

/// BUILD's scan passes over a moving unit of the player or Gaia but not another's, one
/// buildable on, and a felled tree off a forest but not on one or with more hit points.
#[test]
fn build_scan_passes_over_moving_friends_buildable_ground_and_felled_trees() {
    use crate::production::spawn::placement_test::build_passes;
    let site_values = Restriction::new(vec![1.0]);
    let site = house_site(&site_values, true);
    let seen = plain(TileSight::Seen);
    let flat = WorldMap { turns: &seen };
    let walker = Near {
        kind: MasterKind::COMBAT,
        class: class::CLASS_6,
        flags: NearFlags::MOVING,
        player: PlayerId::new(1),
        state: ObjectState::Alive,
        ..near_sheep(ObjectState::Unfinished)
    };
    let passes = |o: Near| build_passes(&site, &flat, &o, &roles());
    assert!(passes(walker));
    assert!(passes(Near {
        player: PlayerId::new(0),
        ..walker
    }));
    assert!(!passes(Near {
        player: PlayerId::new(2),
        ..walker
    }));
    assert!(!passes(Near {
        flags: NearFlags::empty(),
        ..walker
    }));
    assert!(!passes(Near {
        state: ObjectState::Dying,
        ..walker
    }));
    let still = Near {
        flags: NearFlags::empty(),
        ..walker
    };
    assert!(passes(Near {
        buildable_on: BuildableOn::YES,
        ..still
    }));
    let stump = Near {
        class: class::TREE,
        kind: MasterKind::STATIC,
        hp: HitPoints::new(1.0),
        ..still
    };
    assert!(passes(stump));
    assert!(!passes(Near {
        flags: NearFlags::FOREST_TILE,
        ..stump
    }));
    assert!(!passes(Near {
        hp: HitPoints::new(2.0),
        ..stump
    }));
}

/// What blocks a foundation at (21, 21): another player's house whose box overlaps it; not a
/// razed one, a moving unit of the player, an object built on, a felled tree off a forest, a
/// wall under the player's own gate, another's desolid foundation or an object beyond the
/// box; an object of obstruction type 11 is not modelled.
#[test]
fn build_blockers_are_the_standing_objects_whose_box_overlaps() {
    use crate::production::spawn::placement_test::blocks_build;
    let site_values = Restriction::new(vec![1.0]);
    let site = house_site(&site_values, true);
    let seen = plain(TileSight::Seen);
    let flat = WorldMap { turns: &seen };
    let house = Near {
        id: object_id(500),
        position: Position::new(20.5, 20.5, 0.0),
        radius: Footprint::new(1.0, 1.0),
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        master: ids::HOUSE,
        obstruction_kind: ObstructionKind::KIND_2,
        flags: NearFlags::empty(),
        player: PlayerId::new(2),
        state: ObjectState::Alive,
        ..near_sheep(ObjectState::Alive)
    };
    let blocks = |site: &BuildSite<'_>, o: Near| {
        blocks_build(site, &flat, &o, Point::new(21.0, 21.0), &roles())
    };
    assert!(blocks(&site, house));
    for passed in [
        Near {
            state: ObjectState::Dying,
            ..house
        },
        Near {
            kind: MasterKind::COMBAT,
            player: PlayerId::new(1),
            flags: NearFlags::MOVING,
            ..house
        },
        Near {
            buildable_on: BuildableOn::YES,
            ..house
        },
        Near {
            class: class::TREE,
            kind: MasterKind::STATIC,
            hp: HitPoints::new(1.0),
            ..house
        },
        Near {
            flags: NearFlags::DESOLID,
            ..house
        },
        Near {
            position: Position::new(30.5, 20.5, 0.0),
            ..house
        },
    ] {
        assert!(!blocks(&site, passed), "{passed:?}");
    }
    let gate = BuildSite {
        class: class::GATE,
        ..site
    };
    let wall = Near {
        class: class::WALL,
        player: PlayerId::new(1),
        ..house
    };
    assert!(!blocks(&gate, wall));
    assert!(blocks(&site, wall));
    // obstruction types 11 to 13 take the outline's box: one that does not reach the site, or
    // a negative height, passes; a height of 0 counts as 1 for a building
    let outlined = |outline| Near {
        position: Position::new(22.6, 21.0, 0.0),
        obstruction_kind: ObstructionKind::KIND_11,
        outline,
        ..house
    };
    assert!(blocks(&site, outlined(Radii::new(1.0, 1.0, 2.0))));
    assert!(!blocks(&site, outlined(Radii::new(0.4, 0.4, 2.0))));
    assert!(blocks(&site, outlined(Radii::new(1.0, 1.0, 0.0))));
    assert!(!blocks(&site, outlined(Radii::new(1.0, 1.0, -1.0))));
}

/// The Build's test ([`BuildSite::scans`]) runs the static scans for a site with a height: they
/// find player 2's dying house 600 at (21.5, 21.5) (state 4) that BUILD's own scan passes over,
/// but not one of the site's own, a desolid foundation, or anything for a site without a height.
#[test]
fn the_build_static_scans_find_dying_house_build_passes_over() {
    let house = |desolid| SceneObject {
        state: ObjectState::DyingSprite,
        desolid,
        ..SceneObject::new(
            600,
            PlayerId::new(2),
            house_master(),
            Position::new(21.5, 21.5, 0.0),
        )
    };
    let build_values = Restriction::new(vec![1.0, 0.0]);
    let build = house_site(&build_values, false);
    let test = |site: &BuildSite<'_>, house: SceneObject| {
        let w = Scene::open(GridSize::new(120, 120))
            .players(2)
            .object(house)
            .build();
        build_placement(
            site,
            &WorldMap { turns: &w },
            Point::new(21.0, 21.0),
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
        .code()
    };
    assert_eq!(test(&build, house(false)), 0);
    let scans = BuildSite {
        tall: true,
        checks: SiteChecks::SCANS,
        ..build
    };
    assert_eq!(test(&scans, house(false)), 6);
    assert_eq!(
        test(
            &BuildSite {
                own: &[object_id(600)],
                ..scans
            },
            house(false)
        ),
        0
    );
    assert_eq!(test(&scans, house(true)), 0);
    assert_eq!(
        test(
            &BuildSite {
                tall: false,
                ..scans
            },
            house(false)
        ),
        0
    );
}

/// A 3×3 map whose tile `i` (row by row) is of terrain `kinds[i]` and whose terrain `t` has the
/// flags `flags[t]`, for the placement terrains' test ([`placement_terrains`]).
fn terrains(kinds: [u8; 9], flags: Vec<TerrainFlags>) -> World {
    let scene = (0..9).fold(
        Scene::open(GridSize::new(3, 3)).terrains(flags),
        |scene, i| {
            let tile = Tile::new(i % 3, i / 3);
            scene.terrain_on([tile], TerrainId::named(kinds[i as usize]))
        },
    );
    scene.build()
}

/// The point's tile must be of one of the master's terrains or share shallow water or shallows
/// with them; a terrain of none beside a set one reads flags the map has not
/// got, which is not modelled.
#[test]
fn placement_terrain_takes_its_own_tile_or_one_of_shared_flag() {
    use crate::production::spawn::terrains::point_terrain_allows;
    // terrains 0 (land), 1 and 2 (shallow water), 3 (shallows)
    let w = terrains(
        [0, 1, 2, 3, 0, 0, 0, 0, 0],
        vec![
            TerrainFlags::NONE,
            TerrainFlags::SHALLOW_WATER,
            TerrainFlags::SHALLOW_WATER,
            TerrainFlags::SHALLOWS,
        ],
    );
    let map = WorldMap { turns: &w };
    let t = |id: u8| Some(TerrainId::named(id));
    let at = |i: i32| Tile::new(i % 3, i / 3);
    assert_eq!(point_terrain_allows(&map, [t(1), t(3)], at(1)), Ok(true));
    assert_eq!(point_terrain_allows(&map, [t(1), t(3)], at(2)), Ok(true));
    assert_eq!(point_terrain_allows(&map, [t(2), t(2)], at(3)), Ok(false));
    assert_eq!(point_terrain_allows(&map, [t(3), t(3)], at(3)), Ok(true));
    assert_eq!(point_terrain_allows(&map, [t(1), t(2)], at(0)), Ok(false));
    assert!(point_terrain_allows(&map, [t(1), None], at(0)).is_err());
}

/// The side terrains: the master's own, then every other terrain with ice (then beach) that one
/// of its own has, in the terrains' order.
#[test]
fn side_terrains_take_the_others_of_shared_flag() {
    use crate::production::spawn::terrains::side_terrains;
    let w = terrains(
        [0; 9],
        vec![
            TerrainFlags::NONE,
            TerrainFlags::ICE,
            TerrainFlags::ICE,
            TerrainFlags::BEACH,
            TerrainFlags::ICE | TerrainFlags::BEACH,
        ],
    );
    let map = WorldMap { turns: &w };
    let t = |id: u8| Some(TerrainId::named(id));
    let ids = |v: &[u8]| v.iter().copied().map(TerrainId::named).collect::<Vec<_>>();
    assert_eq!(side_terrains(&map, [t(0), t(0)]), Ok(ids(&[0, 0])));
    assert_eq!(side_terrains(&map, [t(1), t(0)]), Ok(ids(&[1, 0, 2, 4])));
    assert_eq!(side_terrains(&map, [t(3), t(3)]), Ok(ids(&[3, 3, 4])));
    assert!(side_terrains(&map, [t(1), None]).is_err());
}

/// The placement terrains of a master: with terrains the point's tile must be of one and the box
/// shrinks to it; with side terrains a tile beside the box must be of one.
#[test]
fn master_placement_terrains_test_the_point_and_the_tiles_beside() {
    // the centre (1, 1) of terrain 5, the tile right of it (2, 1) of terrain 6
    let w = terrains([0, 0, 0, 0, 5, 6, 0, 0, 0], vec![TerrainFlags::NONE; 8]);
    let map = WorldMap { turns: &w };
    let t = |id: u8| Some(TerrainId::named(id));
    let centre = Point::new(1.5, 1.5);
    let whole = TileRect::new(0, 0, 2, 2);
    let test = |placement| placement_terrains(placement, &map, centre, whole);
    assert_eq!(test(PlacementTerrains::NONE), Ok(true));
    let on = |a, b| PlacementTerrains {
        on: [a, b],
        ..PlacementTerrains::NONE
    };
    assert_eq!(test(on(t(5), t(5))), Ok(true));
    assert_eq!(test(on(t(6), t(6))), Ok(false));
    let beside = |a, b| PlacementTerrains {
        beside: [a, b],
        ..PlacementTerrains::NONE
    };
    // the whole map's box has no tile beside it; the centre's tile has 6 to its right
    assert_eq!(test(beside(t(6), t(6))), Ok(false));
    assert_eq!(
        test(PlacementTerrains {
            beside: [t(6), t(6)],
            on: [t(5), t(5)],
        }),
        Ok(true)
    );
    assert_eq!(
        test(PlacementTerrains {
            beside: [t(7), t(7)],
            on: [t(5), t(5)],
        }),
        Ok(false)
    );
}
