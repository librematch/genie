use super::*;
use crate::commands::GatherPoint;
use crate::commands::GatherPointState;
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
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::HillMode;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::PlacementTerrains;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::master_type;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::attribute;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::player_id;
use crate::unit_ai::ActionKind;
use crate::unit_ai::Issuer;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitAiType;
use crate::vision::VisionBits;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WorldTime;

/// records/425906646's update 1883: a TC's gather point on a tree that is gone; its new
/// villager takes the closest tree of Gaia's (by centre) within its sight of the point,
/// measured from the tree's footprint, else none (it walks to the point).
#[test]
fn a_gather_point_whose_tree_is_gone_takes_the_closest_tree_within_sight() {
    let unit = |id, master, x: f32, y: f32| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(x, y, 0.0)),
        life: Lifecycle {
            master_id: master,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            crate::world::Player {
                sleeping: vec![
                    unit(object_id(10), master_id(349), 20.5, 20.5),
                    unit(object_id(11), master_id(349), 22.5, 20.5),
                ]
                .into(),
                ..crate::world::Player::default()
            },
            crate::world::Player {
                objects: vec![unit(object_id(99), ids::VILLAGER_MALE, 18.0, 18.0)].into(),
                ..crate::world::Player::default()
            },
        ],
    );
    let tree = crate::objects::masters::Master {
        id: master_id(349),
        class: class::TREE,
        radius: Footprint::new(0.5, 0.5),
        ..crate::objects::masters::Master::default()
    };
    let villager = crate::objects::masters::Master {
        id: ids::VILLAGER_MALE,
        class: class::VILLAGER,
        line_of_sight: Distance::new(4.0),
        ..crate::objects::masters::Master::default()
    };
    t.masters = vec![
        crate::objects::masters::Masters::new([tree.clone()]),
        crate::objects::masters::Masters::new([villager, tree]),
    ];
    let g = GatherPoint {
        state: GatherPointState::POINT,
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

/// MarvinBlue/426048731's TC 9149 at (104, 47), radii 2: its gather point (107.2451,
/// 46.075863) lies right and below, so the right side's lower half (6) comes first among the
/// sides the first pass tries; the villager (radius 0.2) steps by 0.5 from (106.5, 47.5).
/// The sheep 9520 at (106.68, 47.15) blocks the first two candidates and stays (livestock in
/// state 2), so each ends its spot; (106.5, 46.5) is free.
/// A map of one tile's shape and elevation for the hill test.
struct Hill(u8, u8);

impl PlaceMap for Hill {
    fn size(&self) -> GridSize {
        GridSize::new(10, 10)
    }
    fn terrain(&self, _: Tile) -> Result<TerrainId, GenieError> {
        Ok(TerrainId::named(0))
    }
    fn explored(&self, _: PlayerId, _: Tile) -> bool {
        true
    }
    fn visible(&self, _: PlayerId, _: Tile) -> bool {
        true
    }
    fn vision(&self, _: PlayerId) -> crate::vision::VisionMasks {
        crate::vision::VisionMasks::default()
    }
    fn elevation(&self, _: Tile) -> Option<u8> {
        Some(self.1)
    }
    fn shape(&self, _: Tile) -> Option<u8> {
        Some(self.0)
    }
    fn allied(&self, a: PlayerId, b: PlayerId, _: bool) -> bool {
        a == b
    }
    fn objects(&self, _: Tile) -> Result<Vec<Near>, GenieError> {
        Ok(Vec::new())
    }
}

#[test]
fn a_hill_mode_takes_flat_ground_side_slopes_or_an_edge_within_one_step() {
    let fails = |mode, shape, elevation, at| {
        hill_fails(
            HillMode::of_data(mode),
            &Hill(shape, elevation),
            Tile::from(at),
            Tile::new(2, 5),
            Some(3),
        )
        .unwrap()
    };
    // mode 1: flat and the four side slopes (5 to 8) pass, corners and saddles fail
    assert!(!fails(1, 0, 3, [3, 3]) && !fails(1, 5, 3, [3, 3]) && !fails(1, 8, 3, [3, 3]));
    assert!(fails(1, 2, 3, [3, 3]) && fails(1, 9, 3, [3, 3]));
    // mode 2: flat alone
    assert!(!fails(2, 0, 3, [3, 3]) && fails(2, 6, 3, [3, 3]));
    // mode 3: the first column and the last row within one step of the corner's elevation
    assert!(!fails(3, 9, 4, [2, 3]) && fails(3, 0, 5, [2, 3]) && fails(3, 0, 1, [4, 5]));
    assert!(!fails(3, 0, 7, [4, 4]));
    // the other modes take any tile
    assert!(!fails(0, 12, 9, [3, 3]));
}

#[test]
fn the_tc_spawns_its_villager_below_the_sheep_on_its_right() {
    let site = Site {
        position: Position::new(104.0, 47.0, 0.0),
        radius: Footprint::new(2.0, 2.0),
        gather: Some(Point::new(107.2451, 46.075_863)),
        walls: [false; 4],
    };
    let mut tried = Vec::new();
    let p = spawn_point(&site, Footprint::new(0.2, 0.2), &mut |c| {
        tried.push(c);
        Ok(if c.y() > 46.9 {
            Tested::Blocked { stays: true }
        } else {
            Tested::Free
        })
    })
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
fn a_blocker_that_moves_lets_the_spot_try_its_next_candidate() {
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

fn sheep(state: ObjectState) -> Near {
    Near {
        id: object_id(9520),
        position: Position::new(106.68481, 47.14652, 0.0),
        radius: Footprint::new(0.35, 0.35),
        tall: true,
        kind: master_type::COMBAT,
        class: class::LIVESTOCK,
        master: master_id(705),
        buildable_on: BuildableOn::NO,
        obstruction_type: ObstructionType::CIRCLE,
        auto_gather: false,
        state,
        desolid: false,
        player: PlayerId::new(0),
        moving: true,
        fog_visibility: FogVisibility::INVISIBLE,
        fog_mask: None,
        holds: false,
        hp: HitPoints::new(7.0),
        forest_tile: false,
        outline: Radii::ZERO,
    }
}

#[test]
fn livestock_stays_alive_a_tree_only_once_dead() {
    assert!(sheep(ObjectState::Alive).stays());
    assert!(!sheep(ObjectState::Unfinished).stays());
    let tree = Near {
        class: class::TREE,
        kind: master_type::STATIC,
        ..sheep(ObjectState::Alive)
    };
    assert!(!tree.stays());
    assert!(
        Near {
            state: ObjectState::Dying,
            ..tree
        }
        .stays()
    );
}

/// A map the player explored but does not see now, with its masks.
struct Fogged(Vec<Near>, [u64; 2]);

impl PlaceMap for Fogged {
    fn size(&self) -> GridSize {
        GridSize::new(120, 120)
    }
    fn terrain(&self, _: Tile) -> Result<TerrainId, GenieError> {
        Ok(TerrainId::named(0))
    }
    fn explored(&self, _: PlayerId, _: Tile) -> bool {
        true
    }
    fn visible(&self, _: PlayerId, _: Tile) -> bool {
        false
    }
    fn vision(&self, _: PlayerId) -> crate::vision::VisionMasks {
        let [visible, explored] = self.1.map(VisionBits::from_bits_retain);
        crate::vision::VisionMasks { visible, explored }
    }
    fn elevation(&self, _: Tile) -> Option<u8> {
        Some(0)
    }
    fn shape(&self, _: Tile) -> Option<u8> {
        Some(0)
    }
    fn allied(&self, a: PlayerId, b: PlayerId, _: bool) -> bool {
        a == b
    }
    fn objects(&self, tile: Tile) -> Result<Vec<Near>, GenieError> {
        Ok(self
            .0
            .iter()
            .filter(|o| o.position.tile() == tile)
            .copied()
            .collect())
    }
}

/// In the fog, of the objects without a fog visibility only the player's own block a BUILD,
/// and another player's building whose visibility mask meets both the player's masks.
#[test]
fn a_fogged_building_blocks_a_build_by_its_visibility_mask() {
    let house = Near {
        id: object_id(500),
        position: Position::new(20.5, 20.5, 0.0),
        radius: Footprint::new(1.0, 1.0),
        kind: master_type::BUILDING,
        class: class::BUILDING,
        master: ids::HOUSE,
        obstruction_type: ObstructionType::TYPE_2,
        moving: false,
        player: PlayerId::new(2),
        state: ObjectState::Alive,
        fog_mask: Some(VisionBits::from_bits_retain((1 << 1) | (1 << 33))),
        ..sheep(ObjectState::Alive)
    };
    let site = BuildSite {
        player: PlayerId::new(1),
        human: true,
        radius: Footprint::new(1.0, 1.0),
        tall: false,
        scans: false,
        class: class::BUILDING,
        obstructs: true,
        restriction: &Restriction::new(vec![1.0]),
        hill_mode: HillMode::ANY,
        placement: PlacementTerrains::NONE,
        zones: false,
        own: &[],
    };
    let test = |o: Near, masks: [u64; 2]| {
        build_placement(&site, &Fogged(vec![o], masks), Point::new(21.0, 21.0))
            .unwrap()
            .code()
    };
    // the house's mask meets what player 1 sees and explored: it blocks
    assert_eq!(test(house, [1 << 1, 1 << 33]), 6);
    // not what it sees: another player's house in the fog passes
    assert_eq!(test(house, [1 << 2, 1 << 33]), 0);
    // without a UMV entry neither, but its own does
    let unmasked = Near {
        fog_mask: None,
        ..house
    };
    assert_eq!(test(unmasked, [1 << 1, 1 << 33]), 0);
    assert_eq!(
        test(
            Near {
                player: PlayerId::new(1),
                ..unmasked
            },
            [0, 0]
        ),
        6
    );
}

struct Flat(Vec<Near>);

impl PlaceMap for Flat {
    fn size(&self) -> GridSize {
        GridSize::new(120, 120)
    }
    fn terrain(&self, _: Tile) -> Result<TerrainId, GenieError> {
        Ok(TerrainId::named(0))
    }
    fn explored(&self, _: PlayerId, _: Tile) -> bool {
        true
    }
    fn visible(&self, _: PlayerId, _: Tile) -> bool {
        true
    }
    fn vision(&self, _: PlayerId) -> crate::vision::VisionMasks {
        crate::vision::VisionMasks::default()
    }
    fn elevation(&self, _: Tile) -> Option<u8> {
        Some(0)
    }
    fn shape(&self, _: Tile) -> Option<u8> {
        Some(0)
    }
    fn allied(&self, a: PlayerId, b: PlayerId, _: bool) -> bool {
        a == b
    }
    fn objects(&self, tile: Tile) -> Result<Vec<Near>, GenieError> {
        Ok(self
            .0
            .iter()
            .filter(|o| o.position.tile() == tile)
            .copied()
            .collect())
    }
}

#[test]
fn the_sheep_blocks_the_villager_where_their_boxes_overlap() {
    let villager = Placing {
        player: PlayerId::new(1),
        radius: Footprint::new(0.2, 0.2),
        tall: true,
        class: class::VILLAGER,
        restriction: &Restriction::new(vec![1.0]),
        hill_mode: HillMode::ANY,
        placement: PlacementTerrains::NONE,
    };
    let map = Flat(vec![sheep(ObjectState::Alive)]);
    let at = |x, y| placement(&villager, &map, Point::new(x, y), false).unwrap();
    assert_eq!(at(106.5, 47.5), Tested::Blocked { stays: true });
    assert_eq!(at(106.5, 47.0), Tested::Blocked { stays: true });
    assert_eq!(at(106.5, 46.5), Tested::Free);
}

/// Two objects on one tile that block the point differently: the first of the tile's list
/// reports (the game's scan, `FUN_141264db0`), whichever stays.
#[test]
fn the_first_object_of_a_tile_s_list_reports_the_block() {
    let villager = Placing {
        player: PlayerId::new(1),
        radius: Footprint::new(0.2, 0.2),
        tall: true,
        class: class::VILLAGER,
        restriction: &Restriction::new(vec![1.0]),
        hill_mode: HillMode::ANY,
        placement: PlacementTerrains::NONE,
    };
    // a sheep that stays (state 2) and a soldier that does not, on the same tile
    let soldier = Near {
        id: object_id(9521),
        class: class::CLASS_6,
        master: master_id(4),
        ..sheep(ObjectState::Alive)
    };
    let at = |list: Vec<Near>| {
        placement(&villager, &Flat(list), Point::new(106.5, 47.5), false).unwrap()
    };
    assert_eq!(
        at(vec![sheep(ObjectState::Alive), soldier]),
        Tested::Blocked { stays: true }
    );
    assert_eq!(
        at(vec![soldier, sheep(ObjectState::Alive)]),
        Tested::Blocked { stays: false }
    );
}

/// `FUN_140ad0240` at the forage world's town centre 700: a villager (master 83) spawns beside
/// it and stands idle; attribute 81 lowers by its population and the centre's unit AI hears
/// that its make action completed (0x1fa with 0x26b). With the centre's command queue the
/// new villager's unit AI takes each order behind its own (priority 100, ready, at the
/// centre's height). A computer player's spawn, one without the players' sight and a master
/// that does not move are not modelled.
#[test]
fn a_made_villager_spawns_beside_its_town_centre_and_takes_its_queue() {
    let world = || {
        let mut t = crate::world::tests::forage_world();
        // the villager's standing sprite (a graphic of ten frames, eight facets)
        for masters in &mut t.masters {
            masters.get_mut(ids::VILLAGER_MALE).unwrap().standing_sprite =
                Some(SpriteId::named(300));
        }
        let _ = t.graphics.insert(
            SpriteId::named(300),
            crate::objects::sprite::Graphic {
                id: SpriteId::named(300),
                frames: crate::objects::Frame::of_data(10),
                facets: crate::objects::FacetCount::of_data(8),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            },
        );
        let p = &mut t.players[1];
        p.attributes[attribute::IN_PRODUCTION.index()] = Amount::new(3.0);
        p.objects[ListIndex::new(2)].combat.ai = Some(UnitAi {
            ai_type: UnitAiType::PREDATOR,
            current_order: None,
            current_action: None,
            current_target: None,
            ..UnitAi::default()
        });
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
    let completed = p.objects[ListIndex::new(2)]
        .combat
        .ai
        .as_ref()
        .unwrap()
        .notifies
        .iter()
        .any(|n| n.kind == NotifyKind::ACTION_COMPLETED && n.p == [ActionKind::Make.value(), 0, 0]);
    assert!(completed);
    // the centre's command queue goes to the new unit
    let mut t = world();
    let order = crate::unit_ai::QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: crate::unit_ai::OrderPriority::of_data(5),
        target: None,
        target_owner: None,
        point: Position::new(40.0, 80.0, 7.0),
        range: Distance::new(2.0),
        shown: Point::new(40.0, 80.0),
        ready: false,
        target_type: None,
    };
    t.players[1].objects[ListIndex::new(2)]
        .combat
        .ai
        .as_mut()
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
    let queued = made
        .combat
        .ai
        .as_ref()
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
    t.players[1].computer = true;
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
fn a_made_unit_moves_at_the_speed_of_its_standing_sprite() {
    let mut t = crate::world::tests::forage_world();
    for masters in &mut t.masters {
        let villager = masters.get_mut(ids::VILLAGER_MALE).unwrap();
        villager.standing_sprite = Some(SpriteId::named(300));
        let moving = villager.moving.as_mut().unwrap();
        moving.base = Speed::new(0.8);
        moving.speed = Speed::new(1.6);
    }
    let _ = t.graphics.insert(
        SpriteId::named(300),
        crate::objects::sprite::Graphic {
            id: SpriteId::named(300),
            frames: crate::objects::Frame::of_data(10),
            facets: crate::objects::FacetCount::of_data(8),
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
    assert_eq!(made.body.mover().unwrap().stats.speed, Speed::new(0.4));
}

/// `FUN_140ace9c0` sends the new unit to its building's gather point only for a state of 1
/// (`+0xc30`); a state of 0xff (SET GATHER STATE's) leaves it standing, which the unit's wait
/// to move (`+0x7f9`) shows.
#[test]
fn only_a_gather_point_of_state_1_sends_the_new_unit() {
    let waits = |state| {
        let mut t = crate::world::tests::forage_world();
        for masters in &mut t.masters {
            masters.get_mut(ids::VILLAGER_MALE).unwrap().standing_sprite =
                Some(SpriteId::named(300));
        }
        let _ = t.graphics.insert(
            SpriteId::named(300),
            crate::objects::sprite::Graphic {
                id: SpriteId::named(300),
                frames: crate::objects::Frame::of_data(10),
                facets: crate::objects::FacetCount::of_data(8),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            },
        );
        let p = &mut t.players[1];
        p.attributes[attribute::IN_PRODUCTION.index()] = Amount::new(3.0);
        p.objects[ListIndex::new(2)].combat.ai = Some(UnitAi {
            ai_type: UnitAiType::PREDATOR,
            current_order: None,
            current_action: None,
            current_target: None,
            ..UnitAi::default()
        });
        p.objects[ListIndex::new(2)].building.gather_point = Some(GatherPoint {
            state,
            point: Position::new(40.0, 80.0, 0.0),
            target: None,
            target_master: None,
        });
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
            .and_then(|u| u.body.mover().map(|m| m.waiting))
    };
    assert_eq!(waits(GatherPointState::POINT), Some(1));
    assert_eq!(waits(GatherPointState::KEPT), Some(0));
}

/// Every candidate of `site`'s search for a unit of radius 0.5 (a step of 0.5, no gap), all
/// blocked by objects that stay: each spot's first candidate in turn, and the error of the
/// later rings.
fn all_blocked(site: &Site) -> (Vec<Point>, Result<Point, GenieError>) {
    let mut tried = Vec::new();
    let r = spawn_point(site, Footprint::new(0.5, 0.5), &mut |c| {
        tried.push(c);
        Ok(Tested::Blocked { stays: true })
    });
    (tried, r)
}

/// `FUN_1411d12d0` without a gather point: one pass over the eight segments of an open
/// building's frame (left 8.5, right 11.5, bottom 8.5, top 11.5): the top side's left quarter
/// and a half more (1), the left side's top part (2), the top's right part (3), the left's
/// lower part (4), the right side's top half (5) and lower half (6), the bottom side's left
/// half (7) and right half (8). Then the later rings, not modelled.
#[test]
fn a_spawn_search_without_a_gather_point_walks_the_eight_segments_once() {
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
fn a_second_pass_between_walled_corners_tries_the_bottom_right_corner() {
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
fn queued_move() -> crate::unit_ai::QueuedOrder {
    crate::unit_ai::QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: crate::unit_ai::OrderPriority::of_data(5),
        target: None,
        target_owner: None,
        point: Position::new(40.0, 80.0, 7.0),
        range: Distance::new(2.0),
        shown: Point::new(40.0, 80.0),
        ready: false,
        target_type: None,
    }
}

/// `FUN_140ace9c0` with a point: the new villager waits to move, takes the move order and
/// action with no target at the point, leaves the town bell and walks there (a `MoveTo` of
/// range 0); a new unit's queue empties, an unloaded unit's stays.
#[test]
fn a_gather_point_without_a_target_walks_the_new_unit_there() {
    use crate::movement::move_to::{MoveTo, MoveToAction, MoveWait};
    let walked = |notified| {
        let mut t = crate::world::tests::forage_world();
        let u = &mut t.players[1].objects[ListIndex::new(0)];
        u.combat.town_bell.flag = true;
        u.combat.ai.as_mut().unwrap().orders.push(queued_move());
        t.gather_move(
            PlayerId::new(1),
            ListIndex::new(0),
            Position::new(40.0, 80.0, 0.0),
            notified,
        )
        .unwrap();
        t.swap_remove_live(PlayerId::new(1), ListIndex::new(0))
            .unwrap()
    };
    let u = walked(true);
    assert_eq!(u.unit.body.mover().unwrap().waiting, 1);
    assert!(!u.unit.combat.town_bell.flag);
    let ai = u.unit.combat.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (Some(OrderKind::Move), Some(ActionKind::Move), None)
    );
    assert_eq!(ai.target_point, Position::new(40.0, 80.0, 0.0));
    assert_eq!(ai.orders, []);
    assert_eq!(
        u.unit.actions.list.move_to().copied(),
        Some(MoveToAction {
            sprite: None,
            state: MoveTo::Start,
            point: Position::new(40.0, 80.0, 0.0),
            target: None,
            range: Distance::ZERO,
            quiet: false,
            on_target_moved: crate::movement::move_to::TargetMoved::Follows,
            wait: MoveWait::default(),
        })
    );
    let unloaded = walked(false);
    assert_eq!(
        unloaded.unit.combat.ai.as_ref().unwrap().orders,
        [queued_move()]
    );
    assert_eq!(
        unloaded.unit.actions.list.move_to().copied(),
        u.unit.actions.list.move_to().copied()
    );
}

/// A unit without speed only waits to move; one that already acts, or one not in state 2,
/// is not modelled.
#[test]
fn a_gather_point_s_walk_waits_without_speed_and_stops_at_the_unmodelled() {
    let mut t = crate::world::tests::forage_world();
    t.players[1].objects[ListIndex::new(0)]
        .body
        .mover_mut()
        .unwrap()
        .stats
        .speed = Speed::ZERO;
    t.gather_move(
        PlayerId::new(1),
        ListIndex::new(0),
        Position::new(40.0, 80.0, 0.0),
        true,
    )
    .unwrap();
    let u = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(u.body.mover().unwrap().waiting, 1);
    assert_eq!(u.actions.list.move_to().copied(), None);
    assert_eq!(u.combat.ai.as_ref().unwrap().current_order, None);
    // a second walk finds the first's `MoveTo`
    let mut t = crate::world::tests::forage_world();
    t.gather_move(
        PlayerId::new(1),
        ListIndex::new(0),
        Position::new(40.0, 80.0, 0.0),
        true,
    )
    .unwrap();
    let reason = |t: &mut World| {
        t.gather_move(
            PlayerId::new(1),
            ListIndex::new(0),
            Position::new(41.0, 80.0, 0.0),
            true,
        )
        .unwrap_err()
        .reason()
    };
    assert_eq!(
        reason(&mut t),
        "a gather point's move of a unit with an action"
    );
    let mut t = crate::world::tests::forage_world();
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
    let mut t = crate::world::tests::forage_world();
    let graphic = |id, flags| crate::objects::sprite::Graphic {
        id,
        frames: crate::objects::Frame::of_data(10),
        facets: crate::objects::FacetCount::of_data(8),
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
        let _ = t.graphics.insert(id, graphic(id, flags));
    }
    let static_remains = Master {
        id: master_id(800),
        kind: master_type::STATIC,
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
        kind: master_type::MOVING,
        standing_sprite: Some(SpriteId::named(21)),
        moving: Some(crate::gathering::hunt::tests::walker().stats),
        ..Master::default()
    };
    for masters in &mut t.masters {
        *masters = Masters::new(
            masters
                .iter()
                .cloned()
                .chain([static_remains.clone(), moving_remains.clone()]),
        );
    }
    t
}

/// The master's slot 0x40 for remains (`FUN_1411a45a0`, `FUN_141141640`): a static object gives
/// its owner its storage of flag 2, holds its entry of flag 0 and dies on its death sprite's
/// eight facets; moving remains face their dead object's facet (with their own sprite's
/// facets). Another type, a static object with hit points, a recyclable master, moving remains
/// without a facet or without a moving master are not modelled.
#[test]
fn remains_take_their_storage_and_face_their_dead_object_s_facet() {
    use crate::objects::lifecycle::Class;
    let made = |t: &World, id| t.masters[1].get(id).unwrap().clone();
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
        (u.life.class, u.life.attribute_type, u.life.amount),
        (Class::Static, Some(attribute::POPULATION), Amount::new(5.0))
    );
    assert_eq!(
        (u.life.death_facets, u.sprite),
        (Some(crate::objects::FacetCount::of_data(8)), None)
    );
    assert_eq!(t.players[1].attributes[4], food + Amount::new(1.0));
    let i = t
        .plain_object(
            PlayerId::new(1),
            master_id(801),
            &made(&t, master_id(801)),
            Position::new(31.5, 85.5, 0.0),
            Facing::Dead(Some(crate::objects::Facet::of_data(2))),
        )
        .unwrap();
    let u = &t.players[1].objects[i];
    let facets = crate::objects::facing::SpriteFacets {
        count: crate::objects::FacetCount::of_data(8),
        turns: false,
    };
    assert_eq!(
        u.body.mover().map(|m| m.angle),
        Some(crate::objects::facing::angle_of_facet(
            crate::objects::Facet::of_data(2),
            facets
        ))
    );
    let reason = |change: &dyn Fn(&mut crate::objects::masters::Master), facing| {
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
    let unchanged = |_: &mut crate::objects::masters::Master| {};
    assert_eq!(
        reason(&|m| m.kind = master_type::COMBAT, Facing::Drawn),
        "remains of a type other than 10 and 20 to 39"
    );
    assert_eq!(
        reason(
            &|m| {
                m.kind = master_type::STATIC;
                m.hit_points = MasterHitPoints::of_data(5);
            },
            Facing::Drawn
        ),
        "remains with hit points (the sleeping list)"
    );
    assert_eq!(
        reason(&|m| m.recyclable = true, Facing::Drawn),
        "a new object of a recyclable master (a negative id)"
    );
    assert_eq!(
        reason(&unchanged, Facing::Dead(None)),
        "moving remains of an object without a facet the slice holds"
    );
    assert_eq!(
        reason(&|m| m.moving = None, Facing::Drawn),
        "moving remains without a moving master"
    );
}

/// A map of `Flat`'s ground the player never explored.
struct Dark;

impl PlaceMap for Dark {
    fn size(&self) -> GridSize {
        GridSize::new(120, 120)
    }
    fn terrain(&self, _: Tile) -> Result<TerrainId, GenieError> {
        Ok(TerrainId::named(0))
    }
    fn explored(&self, _: PlayerId, _: Tile) -> bool {
        false
    }
    fn visible(&self, _: PlayerId, _: Tile) -> bool {
        false
    }
    fn vision(&self, _: PlayerId) -> crate::vision::VisionMasks {
        crate::vision::VisionMasks::default()
    }
    fn elevation(&self, _: Tile) -> Option<u8> {
        Some(0)
    }
    fn shape(&self, _: Tile) -> Option<u8> {
        Some(0)
    }
    fn allied(&self, a: PlayerId, b: PlayerId, _: bool) -> bool {
        a == b
    }
    fn objects(&self, _: Tile) -> Result<Vec<Near>, GenieError> {
        Ok(Vec::new())
    }
}

/// Player 1's house site (class 3, radii 1, obstructing, no height) on terrain 0, which `restriction`
/// rates; `human` or not.
const fn house_site(restriction: &Restriction, human: bool) -> BuildSite<'_> {
    BuildSite {
        player: PlayerId::new(1),
        human,
        radius: Footprint::new(1.0, 1.0),
        tall: false,
        scans: false,
        class: class::BUILDING,
        obstructs: true,
        restriction,
        hill_mode: HillMode::ANY,
        placement: PlacementTerrains::NONE,
        zones: false,
        own: &[],
    }
}

/// BUILD's static test: off the map 7, a forbidden terrain 2, a human who explored none of
/// the tiles 5 (a computer passes); a box that passes answers none; a zone mode is not
/// modelled.
#[test]
fn build_s_static_test_answers_the_map_s_edge_terrain_and_exploration() {
    use crate::production::spawn::placement_test::build_static_test;
    let open_values = Restriction::new(vec![1.0]);
    let open = house_site(&open_values, true);
    let flat = Flat(Vec::new());
    assert_eq!(
        build_static_test(&open, &flat, Point::new(50.0, 50.0)).map(|c| c.map(PlacementCode::code)),
        Ok(None)
    );
    assert_eq!(
        build_static_test(&open, &flat, Point::new(0.5, 50.0)).map(|c| c.map(PlacementCode::code)),
        Ok(Some(7))
    );
    assert_eq!(
        build_static_test(&open, &flat, Point::new(119.5, 50.0))
            .map(|c| c.map(PlacementCode::code)),
        Ok(Some(7))
    );
    let forbidden_values = Restriction::new(vec![0.0]);
    let forbidden = house_site(&forbidden_values, true);
    assert_eq!(
        build_static_test(&forbidden, &flat, Point::new(50.0, 50.0))
            .map(|c| c.map(PlacementCode::code)),
        Ok(Some(2))
    );
    assert_eq!(
        build_static_test(&open, &Dark, Point::new(50.0, 50.0)).map(|c| c.map(PlacementCode::code)),
        Ok(Some(5))
    );
    let computer_values = Restriction::new(vec![1.0]);
    let computer = house_site(&computer_values, false);
    assert_eq!(
        build_static_test(&computer, &Dark, Point::new(50.0, 50.0))
            .map(|c| c.map(PlacementCode::code)),
        Ok(None)
    );
    let zoned = BuildSite {
        zones: true,
        ..open
    };
    assert_eq!(
        build_static_test(&zoned, &flat, Point::new(50.0, 50.0))
            .map(|c| c.map(PlacementCode::code))
            .unwrap_err()
            .reason(),
        "BUILD's placement test in a zone mode"
    );
}

/// BUILD's scan reads 8 tiles either way of the point, from 0 to the map's last tile.
#[test]
fn build_s_scan_reads_eight_tiles_either_way_within_the_map() {
    use crate::production::spawn::placement_test::build_scan_range;
    assert_eq!(build_scan_range(50.5, 120), (42, 58));
    assert_eq!(build_scan_range(2.0, 120), (0, 10));
    assert_eq!(build_scan_range(115.0, 120), (107, 119));
}

/// BUILD's sight test: in sight any object but one of fog visibility 3; in the fog one with a
/// fog visibility, and of those without only the player's own; on a tile never explored any;
/// a fogged object of fog visibility 3 is not modelled.
#[test]
fn build_s_scan_counts_what_the_player_sees_of_a_tile() {
    use crate::production::spawn::placement_test::build_sees;
    let site_values = Restriction::new(vec![1.0]);
    let site = house_site(&site_values, true);
    let flat = Flat(Vec::new());
    let fogged = Fogged(Vec::new(), [0; 2]);
    let o = |player, fog_visibility| Near {
        player,
        fog_visibility,
        ..sheep(ObjectState::Alive)
    };
    let sees = |map: &dyn PlaceMap, o: Near, sight| build_sees(&site, map, &o, sight);
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
            &Dark,
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
fn build_s_scan_ignores_the_dead_the_moving_own_walls_and_foreign_foundations() {
    use crate::production::spawn::placement_test::build_ignores;
    let site_values = Restriction::new(vec![1.0]);
    let site = house_site(&site_values, true);
    let flat = Flat(Vec::new());
    let house = Near {
        kind: master_type::BUILDING,
        class: class::BUILDING,
        moving: false,
        player: PlayerId::new(2),
        state: ObjectState::Alive,
        ..sheep(ObjectState::Alive)
    };
    let ignores = |site: &BuildSite<'_>, o: Near| build_ignores(site, &flat, &o);
    assert!(!ignores(&site, house));
    assert!(ignores(
        &site,
        Near {
            state: ObjectState::Dying,
            ..house
        }
    ));
    let unit = Near {
        kind: master_type::COMBAT,
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
        moving: true,
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
        desolid: true,
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
fn build_s_scan_passes_over_moving_friends_buildable_ground_and_felled_trees() {
    use crate::production::spawn::placement_test::build_passes;
    let site_values = Restriction::new(vec![1.0]);
    let site = house_site(&site_values, true);
    let flat = Flat(Vec::new());
    let walker = Near {
        kind: master_type::COMBAT,
        class: class::CLASS_6,
        moving: true,
        player: PlayerId::new(1),
        state: ObjectState::Alive,
        ..sheep(ObjectState::Unfinished)
    };
    let passes = |o: Near| build_passes(&site, &flat, &o);
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
        moving: false,
        ..walker
    }));
    assert!(!passes(Near {
        state: ObjectState::Dying,
        ..walker
    }));
    let still = Near {
        moving: false,
        ..walker
    };
    assert!(passes(Near {
        buildable_on: BuildableOn::YES,
        ..still
    }));
    let stump = Near {
        class: class::TREE,
        kind: master_type::STATIC,
        hp: HitPoints::new(1.0),
        ..still
    };
    assert!(passes(stump));
    assert!(!passes(Near {
        forest_tile: true,
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
fn build_s_blockers_are_the_standing_objects_whose_box_overlaps() {
    use crate::production::spawn::placement_test::blocks_build;
    let site_values = Restriction::new(vec![1.0]);
    let site = house_site(&site_values, true);
    let flat = Flat(Vec::new());
    let house = Near {
        id: object_id(500),
        position: Position::new(20.5, 20.5, 0.0),
        radius: Footprint::new(1.0, 1.0),
        kind: master_type::BUILDING,
        class: class::BUILDING,
        master: ids::HOUSE,
        obstruction_type: ObstructionType::TYPE_2,
        moving: false,
        player: PlayerId::new(2),
        state: ObjectState::Alive,
        ..sheep(ObjectState::Alive)
    };
    let blocks =
        |site: &BuildSite<'_>, o: Near| blocks_build(site, &flat, &o, Point::new(21.0, 21.0));
    assert!(blocks(&site, house));
    for passed in [
        Near {
            state: ObjectState::Dying,
            ..house
        },
        Near {
            kind: master_type::COMBAT,
            player: PlayerId::new(1),
            moving: true,
            ..house
        },
        Near {
            buildable_on: BuildableOn::YES,
            ..house
        },
        Near {
            class: class::TREE,
            kind: master_type::STATIC,
            hp: HitPoints::new(1.0),
            ..house
        },
        Near {
            desolid: true,
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
        obstruction_type: ObstructionType::TYPE_11,
        outline,
        ..house
    };
    assert!(blocks(&site, outlined(Radii::new(1.0, 1.0, 2.0))));
    assert!(!blocks(&site, outlined(Radii::new(0.4, 0.4, 2.0))));
    assert!(blocks(&site, outlined(Radii::new(1.0, 1.0, 0.0))));
    assert!(!blocks(&site, outlined(Radii::new(1.0, 1.0, -1.0))));
}

/// The Build's test ([`BuildSite::scans`]) runs the static scans for a site with a height: they
/// find a dying house (state 4) that BUILD's own scan passes over, but not one of the site's own,
/// a desolid foundation, or anything for a site without a height.
#[test]
fn the_build_s_static_scans_find_a_dying_house_build_passes_over() {
    let house = Near {
        id: object_id(600),
        position: Position::new(21.5, 21.5, 0.0),
        radius: Footprint::new(1.0, 1.0),
        kind: master_type::BUILDING,
        class: class::BUILDING,
        moving: false,
        player: PlayerId::new(2),
        state: ObjectState::DyingSprite,
        obstruction_type: ObstructionType::TYPE_2,
        ..sheep(ObjectState::Alive)
    };
    let build_values = Restriction::new(vec![1.0]);
    let build = house_site(&build_values, false);
    let test = |site: &BuildSite<'_>, o: Near| {
        build_placement(site, &Flat(vec![o]), Point::new(21.0, 21.0))
            .unwrap()
            .code()
    };
    assert_eq!(test(&build, house), 0);
    let scans = BuildSite {
        tall: true,
        scans: true,
        ..build
    };
    assert_eq!(test(&scans, house), 6);
    assert_eq!(
        test(
            &BuildSite {
                own: &[object_id(600)],
                ..scans
            },
            house
        ),
        0
    );
    assert_eq!(
        test(
            &scans,
            Near {
                desolid: true,
                ..house
            }
        ),
        0
    );
    assert_eq!(
        test(
            &BuildSite {
                tall: false,
                ..scans
            },
            house
        ),
        0
    );
}

/// A 3×3 map of terrains `kinds` (row by row) whose terrain `t` has the flags `flags[t]`, for
/// the placement terrains' test ([`placement_terrains`]).
struct Terrains {
    kinds: [u8; 9],
    flags: Vec<TerrainFlags>,
}

impl PlaceMap for Terrains {
    fn size(&self) -> GridSize {
        GridSize::new(3, 3)
    }
    fn terrain(&self, tile: Tile) -> Result<TerrainId, GenieError> {
        let i = self
            .size()
            .index(tile)
            .ok_or(GenieError::unsupported("off the map"))?;
        Ok(TerrainId::named(self.kinds[i]))
    }
    fn terrain_flags(&self, t: usize) -> Option<TerrainFlags> {
        self.flags.get(t).copied()
    }
    fn terrain_count(&self) -> usize {
        self.flags.len()
    }
    fn explored(&self, _: PlayerId, _: Tile) -> bool {
        true
    }
    fn visible(&self, _: PlayerId, _: Tile) -> bool {
        true
    }
    fn vision(&self, _: PlayerId) -> crate::vision::VisionMasks {
        crate::vision::VisionMasks::default()
    }
    fn elevation(&self, _: Tile) -> Option<u8> {
        Some(0)
    }
    fn shape(&self, _: Tile) -> Option<u8> {
        Some(0)
    }
    fn allied(&self, a: PlayerId, b: PlayerId, _: bool) -> bool {
        a == b
    }
    fn objects(&self, _: Tile) -> Result<Vec<Near>, GenieError> {
        Ok(Vec::new())
    }
}

/// The point's tile must be of one of the master's terrains or share shallow water or shallows
/// with them; a terrain of none beside a set one reads flags the map has not
/// got, which is not modelled.
#[test]
fn a_placement_terrain_takes_its_own_tile_or_one_of_a_shared_flag() {
    use crate::production::spawn::terrains::point_terrain_allows;
    // terrains 0 (land), 1 and 2 (shallow water), 3 (shallows)
    let map = Terrains {
        kinds: [0, 1, 2, 3, 0, 0, 0, 0, 0],
        flags: vec![
            TerrainFlags::NONE,
            TerrainFlags::SHALLOW_WATER,
            TerrainFlags::SHALLOW_WATER,
            TerrainFlags::SHALLOWS,
        ],
    };
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
fn side_terrains_take_the_others_of_a_shared_flag() {
    use crate::production::spawn::terrains::side_terrains;
    let map = Terrains {
        kinds: [0; 9],
        flags: vec![
            TerrainFlags::NONE,
            TerrainFlags::ICE,
            TerrainFlags::ICE,
            TerrainFlags::BEACH,
            TerrainFlags::ICE | TerrainFlags::BEACH,
        ],
    };
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
fn a_master_s_placement_terrains_test_the_point_and_the_tiles_beside() {
    // the centre (1, 1) of terrain 5, the tile right of it (2, 1) of terrain 6
    let map = Terrains {
        kinds: [0, 0, 0, 0, 5, 6, 0, 0, 0],
        flags: vec![TerrainFlags::NONE; 8],
    };
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
