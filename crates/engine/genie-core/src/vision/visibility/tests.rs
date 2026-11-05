use super::*;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::{Tile, TileOffset, TileRect};
use crate::objects::ObjectState;
use crate::objects::masters::MasterKind;
use crate::ownership::CaptureKind;
use crate::players::PlayerKind;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::roles;
use crate::testing::tuning;
use crate::vision::SightCentre;
use crate::vision::SightLayer;
use crate::vision::SightShape;
use crate::vision::SightTiles;
use crate::vision::registry::UnitUpdate;
use crate::vision::visibility::Seeing;
use crate::world::GameMode;

fn world(players: PlayerId) -> Visibility {
    let tiles = 120 * 120;
    Visibility::new(
        GridSize::new(120, 120),
        vec![
            VisibleMap {
                counts: vec![-1; tiles],
                explored: TileCount::ZERO,
            };
            players.index()
        ],
        vec![VisionBits::NONE; tiles],
        Vec::new(),
        0,
        vec![PlayerKind::Human; players.index()],
    )
}

fn unit(player: PlayerId, los: Distance) -> Seer {
    Seer {
        player,
        master: ids::VILLAGER_MALE,
        los,
        los_powerup: Distance::ZERO,
        state: ObjectState::Alive,
        explored: true,
        building: None,
        umv: 0,
        capture: CaptureKind::None,
        kind: MasterKind::COMBAT,
    }
}

fn resources(lists: usize) -> VisibleResources {
    VisibleResources {
        capacities: vec![2; lists],
        lists: vec![Vec::new(); lists],
    }
}

fn rec(id: i32, x: f32, y: f32) -> VisibleResource {
    VisibleResource {
        id: object_id(id),
        at: Point::new(x, y),
        ..VisibleResource::default()
    }
}

/// `RGE_Combat_Object::update`'s takeover by sight: the first player from 1 on but the owner
/// whose bit the mask holds; outside the standard modes a computer player takes over only
/// when it allows taking Gaia's units.
#[test]
fn the_first_other_player_that_sees_the_object_takes_it_over() {
    use crate::world::player::Player;
    let (human, computer) = (PlayerKind::Human, PlayerKind::Computer);
    let players = |kinds: [PlayerKind; 4]| {
        kinds.map(|kind| Player {
            kind,
            ..Player::default()
        })
    };
    let humans = players([human; 4]);
    let none = [false; 4];
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(0b0111),
            PlayerId::new(0),
            GameMode::RandomMap,
            &humans,
            &none
        ),
        Some(PlayerId::new(1))
    );
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(0b0110),
            PlayerId::new(1),
            GameMode::RandomMap,
            &humans,
            &none
        ),
        Some(PlayerId::new(2))
    );
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(0b0001),
            PlayerId::new(1),
            GameMode::RandomMap,
            &humans,
            &none
        ),
        None
    );
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(1 << 33 | 0b10),
            PlayerId::new(1),
            GameMode::RandomMap,
            &humans,
            &none
        ),
        None
    );
    let ai = players([human, computer, human, computer]);
    let allows = [false, false, false, true];
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(0b1110),
            PlayerId::new(0),
            GameMode::Scenario,
            &ai,
            &allows
        ),
        Some(PlayerId::new(2))
    );
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(0b1010),
            PlayerId::new(0),
            GameMode::Scenario,
            &ai,
            &allows
        ),
        Some(PlayerId::new(3))
    );
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(0b0010),
            PlayerId::new(0),
            GameMode::Scenario,
            &ai,
            &allows
        ),
        None
    );
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(0b0010),
            PlayerId::new(0),
            GameMode::RandomMap,
            &ai,
            &allows
        ),
        Some(PlayerId::new(1))
    );
}

/// The combat object's sight answers a changed mask only for an object others take over by
/// seeing it (`+0x924` 1), once the registry took it.
#[test]
fn changed_mask_is_the_answer_for_an_object_taken_over_by_sight() {
    let mut visibility = world(PlayerId::new(3));
    visibility.registry = Some(Registry::new(3, 65));
    let mut gaia = unit(PlayerId::new(0), Distance::new(2.0));
    gaia.capture = CaptureKind::BySight;
    visibility.add_seer(object_id(7), gaia);
    visibility.add_seer(object_id(8), unit(PlayerId::new(0), Distance::new(2.0)));
    let tile = 10 * 120 + 10;
    visibility.unified[tile] = VisionBits::from_bits_retain(0b011);
    let at = Position::new(10.5, 10.5, 0.0);
    assert_eq!(
        visibility.combat_sight(
            object_id(7),
            PlayerId::new(0),
            at,
            Footprint::new(0.2, 0.2),
            class::CLASS_6,
            None
        ),
        Ok(Some(VisionBits::from_bits_retain(0b011)))
    );
    assert_eq!(
        visibility.combat_sight(
            object_id(7),
            PlayerId::new(0),
            at,
            Footprint::new(0.2, 0.2),
            class::CLASS_6,
            None
        ),
        Ok(None)
    );
    assert_eq!(
        visibility.combat_sight(
            object_id(8),
            PlayerId::new(0),
            at,
            Footprint::new(0.2, 0.2),
            class::CLASS_6,
            None
        ),
        Ok(None)
    );
    let registry = visibility.registry.as_ref().unwrap();
    assert_eq!(
        registry.entry(object_id(8)).map(|entry| entry.mask),
        Some(VisionBits::from_bits_retain(0b011))
    );
    assert_eq!(registry.lists[1][6].len(), 2);
}

/// `FUN_14120f800`: an object with an id writes the new owner into its records in every
/// list (`+0x8b0`..); one without an id has none to write.
#[test]
fn an_ownership_change_writes_the_new_owner_into_the_object_records() {
    let mut visibility = world(PlayerId::new(3));
    let mut registry = Registry::new(3, 65);
    registry
        .entries
        .get_or_insert_with(object_id(7), Default::default)
        .unwrap()
        .mask = VisionBits::from_bits_retain(0b11);
    registry
        .update_unit(UnitUpdate {
            id: object_id(7),
            owner: PlayerId::new(0),
            position: Point::new(10.5, 10.5),
            radii: Footprint::new(0.2, 0.2),
            class: class::CLASS_6,
            old: VisionBits::NONE,
            new: VisionBits::from_bits_retain(0b11),
        })
        .unwrap();
    visibility.registry = Some(registry);
    visibility
        .records_owner(object_id(7), PlayerId::new(2))
        .unwrap();
    visibility
        .records_owner(object_id(-5), PlayerId::new(2))
        .unwrap();
    let registry = visibility.registry.as_ref().unwrap();
    assert_eq!(
        [registry.lists[0][6][0].owner, registry.lists[1][6][0].owner],
        [PlayerId::new(2); 2]
    );
    assert_eq!(registry.lists[2][6].len(), 0);
}

/// `FUN_14120f800`'s sight: an object that sees loses its sight on the old owner's map
/// (slot 0x1d8) and gains it, with its new master's line of sight, on the new one's (slot
/// 0x1d0); one that does not see (inside another, or not in state 2) changes owner and line
/// of sight alone.
#[test]
fn an_ownership_change_moves_the_sight_of_an_object_that_sees() {
    let at = Position::new(10.5, 10.5, 0.0);
    let mut visibility = world(PlayerId::new(3));
    visibility.add_seer(object_id(7), unit(PlayerId::new(0), Distance::new(2.0)));
    let _ = visibility
        .sight_on(
            object_id(7),
            at,
            Footprint::new(0.2, 0.2),
            SightShape::Circle,
            SightCentre::Tile,
            SightLayer {
                roles: &roles(),
                visibility_tuning: tuning().visibility,
            },
        )
        .unwrap();
    let tile = |x: i32, y: i32| (y * 120 + x) as usize;
    assert_eq!(visibility.maps[0].counts[tile(12, 10)], 1);
    visibility
        .owner_changed(
            object_id(7),
            at,
            Footprint::new(0.2, 0.2),
            (PlayerId::new(1), 3.0),
            Seeing::Sees,
            SightLayer {
                roles: &roles(),
                visibility_tuning: tuning().visibility,
            },
        )
        .unwrap();
    assert_eq!(visibility.maps[0].counts[tile(12, 10)], 0);
    assert_eq!(visibility.maps[1].counts[tile(13, 10)], 1);
    assert_eq!(visibility.maps[1].counts[tile(14, 10)], -1);
    let seer = visibility.seer(object_id(7)).copied().unwrap();
    assert_eq!(
        (seer.player, seer.los),
        (PlayerId::new(1), Distance::new(3.0))
    );
    visibility
        .owner_changed(
            object_id(7),
            at,
            Footprint::new(0.2, 0.2),
            (PlayerId::new(2), 4.0),
            Seeing::Blind,
            SightLayer {
                roles: &roles(),
                visibility_tuning: tuning().visibility,
            },
        )
        .unwrap();
    assert_eq!(visibility.maps[1].counts[tile(13, 10)], 1);
    assert_eq!(visibility.maps[2].counts[tile(10, 10)], -1);
    let seer = visibility.seer(object_id(7)).copied().unwrap();
    assert_eq!(
        (seer.player, seer.los),
        (PlayerId::new(2), Distance::new(4.0))
    );
}

/// `AddResource` (`FUN_141288830`): the record goes last in its group's list at the object's
/// place, its distance and byte 0; a full list doubles its capacity. A manager without lists
/// adds nothing.
#[test]
fn resource_goes_last_in_its_group_list_and_full_list_doubles() {
    let mut visible_resources = resources(5);
    visible_resources.add(object_id(10), GatherGroup::GOLD, Point::new(3.5, 7.5));
    visible_resources.add(object_id(11), GatherGroup::GOLD, Point::new(1.5, 2.5));
    assert_eq!(visible_resources.capacities[4], 2);
    visible_resources.add(object_id(12), GatherGroup::GOLD, Point::new(0.5, 0.5));
    assert_eq!(visible_resources.capacities, [2, 2, 2, 2, 4]);
    assert_eq!(
        visible_resources.lists[4],
        [rec(10, 3.5, 7.5), rec(11, 1.5, 2.5), rec(12, 0.5, 0.5)]
    );
    assert!(visible_resources.lists[..4].iter().all(Vec::is_empty));
    let mut none = VisibleResources::default();
    none.add(object_id(10), GatherGroup::WOOD, Point::new(1.0, 1.0));
    assert_eq!(none, VisibleResources::default());
}

/// `Remove_Resource` (`FUN_141289b80`): the first record of the object takes the last one's
/// record and the list shortens; its index, 0 for an empty list, -1 when not found.
#[test]
fn removed_resource_place_takes_the_last_record() {
    let mut visible_resources = resources(5);
    for id in [1, 2, 3] {
        visible_resources.add(object_id(id), GatherGroup::WOOD, Point::new(id as f32, 0.0));
    }
    assert_eq!(visible_resources.remove(object_id(1), GatherGroup::WOOD), 0);
    assert_eq!(
        visible_resources.lists[0],
        [rec(3, 3.0, 0.0), rec(2, 2.0, 0.0)]
    );
    assert_eq!(visible_resources.remove(object_id(2), GatherGroup::WOOD), 1);
    assert_eq!(visible_resources.lists[0], [rec(3, 3.0, 0.0)]);
    assert_eq!(
        visible_resources.remove(object_id(9), GatherGroup::WOOD),
        -1
    );
    assert_eq!(
        visible_resources.remove(object_id(9), GatherGroup::FORAGE),
        0
    );
    assert_eq!(
        VisibleResources::default().remove(object_id(3), GatherGroup::WOOD),
        -1
    );
}

/// `Get_Closest_Resource` (`FUN_141289000`): the group's record nearest the point (squared,
/// the first of equals), past the ids to skip, takes its distance; none for a group out of
/// range or an empty list.
#[test]
fn the_closest_resource_skips_the_ids_given_and_keeps_its_distance() {
    let mut visible_resources = resources(5);
    visible_resources.add(object_id(1), GatherGroup::FISH, Point::new(13.0, 10.0));
    visible_resources.add(object_id(2), GatherGroup::FISH, Point::new(10.0, 12.0));
    visible_resources.add(object_id(3), GatherGroup::FISH, Point::new(7.0, 10.0));
    assert_eq!(
        visible_resources.closest(Point::new(10.0, 10.0), GatherGroup::FISH, &[]),
        Some(1)
    );
    assert_eq!(visible_resources.lists[2][1].distance, Distance::new(2.0));
    assert_eq!(
        visible_resources.closest(Point::new(10.0, 10.0), GatherGroup::FISH, &[object_id(2)]),
        Some(0)
    );
    assert_eq!(visible_resources.lists[2][0].distance, Distance::new(3.0));
    assert_eq!(
        visible_resources.closest(
            Point::new(10.0, 10.0),
            GatherGroup::FISH,
            &[object_id(2), object_id(1)]
        ),
        Some(2)
    );
    assert_eq!(
        visible_resources.closest(
            Point::new(10.0, 10.0),
            GatherGroup::FISH,
            &[object_id(1), object_id(2), object_id(3)]
        ),
        None
    );
    assert_eq!(
        visible_resources.closest(Point::new(10.0, 10.0), GatherGroup::FORAGE, &[]),
        None
    );
}

/// The tiles a player's map explores go to its tile list (player `+0x180`, `FUN_14165d500`)
/// in order: the circle's (`explore_terrain`) and a rectangle's (`explore_terrain_sq`) newly
/// explored tiles, a revealed rectangle's never explored ones (`reveal_terrain_sq`); the
/// player's update ends with the list cleared (`FUN_141661030`).
#[test]
fn newly_explored_tiles_join_the_player_tile_list_until_its_update_ends() {
    let mut visibility = world(PlayerId::new(2));
    let _ = visibility
        .explore(
            PlayerId::new(1),
            Tile::new(10, 10),
            SightTiles::new(0),
            SightCentre::Tile,
        )
        .unwrap();
    let _ = visibility
        .explore(
            PlayerId::new(1),
            Tile::new(10, 10),
            SightTiles::new(1),
            SightCentre::Tile,
        )
        .unwrap();
    assert_eq!(
        visibility.new_tiles[1],
        [
            Tile::new(10, 10),
            Tile::new(9, 9),
            Tile::new(10, 9),
            Tile::new(11, 9),
            Tile::new(9, 10),
            Tile::new(11, 10),
            Tile::new(9, 11),
            Tile::new(10, 11),
            Tile::new(11, 11)
        ]
    );
    let _ = visibility
        .explore_rect(PlayerId::new(1), TileRect::new(11, 9, 12, 9))
        .unwrap();
    let _ = visibility
        .reveal_rect(PlayerId::new(1), TileRect::new(11, 8, 12, 8))
        .unwrap();
    assert_eq!(
        visibility.new_tiles[1][9..],
        [Tile::new(12, 9), Tile::new(11, 8), Tile::new(12, 8)]
    );
    assert_eq!(visibility.new_tiles[0], Vec::<Tile>::new());
    visibility.clear_new_tiles(PlayerId::new(1));
    assert_eq!(visibility.new_tiles[1], Vec::<Tile>::new());
}

/// `FUN_1411980b0`'s lists: each player whose map explored the object's tile (its bit 32 + p
/// of the unified map, from 0 while the player exists) loses the object from its group's list.
#[test]
fn resource_leaves_the_lists_of_the_players_that_explored_its_tile() {
    let mut visibility = world(PlayerId::new(3));
    visibility.resources = vec![resources(5), resources(5), resources(5)];
    for visible_resources in &mut visibility.resources {
        visible_resources.add(object_id(7), GatherGroup::FORAGE, Point::new(20.5, 30.5));
    }
    let _ = visibility
        .explore(
            PlayerId::new(2),
            Tile::new(20, 30),
            SightTiles::new(0),
            SightCentre::Tile,
        )
        .unwrap();
    visibility
        .forget_resource(
            object_id(7),
            Position::new(20.5, 30.5, 0.0),
            GatherGroup::FORAGE,
        )
        .unwrap();
    assert_eq!(visibility.resources[2].lists[1], []);
    assert_eq!(visibility.resources[0].lists[1], [rec(7, 20.5, 30.5)]);
    assert_eq!(visibility.resources[1].lists[1], [rec(7, 20.5, 30.5)]);
}

#[test]
fn sight_counts_its_circle_explored_then_visible_and_leaves_it_explored() {
    let mut visibility = world(PlayerId::new(3));
    assert_eq!(
        visibility
            .explore(
                PlayerId::new(2),
                Tile::new(10, 10),
                SightTiles::new(1),
                SightCentre::Tile
            )
            .unwrap(),
        TileCount::new(9)
    );
    assert_eq!(visibility.maps[2].explored, TileCount::new(9));
    let index = visibility.index(Tile::new(10, 10));
    assert_eq!(visibility.maps[2].counts[index], 1);
    assert_eq!(
        visibility.unified[index],
        VisionBits::from_bits_retain(1 << 2 | 1 << 34)
    );
    assert_eq!(
        visibility
            .explore(
                PlayerId::new(2),
                Tile::new(11, 10),
                SightTiles::new(1),
                SightCentre::Tile
            )
            .unwrap(),
        TileCount::new(3)
    );
    assert_eq!(visibility.maps[2].counts[index], 2);
    visibility
        .unexplore(
            PlayerId::new(2),
            Tile::new(10, 10),
            SightTiles::new(1),
            SightCentre::Tile,
        )
        .unwrap();
    assert_eq!(visibility.maps[2].counts[index], 1);
    visibility
        .unexplore(
            PlayerId::new(2),
            Tile::new(11, 10),
            SightTiles::new(1),
            SightCentre::Tile,
        )
        .unwrap();
    assert_eq!(visibility.maps[2].counts[index], 0);
    assert_eq!(
        visibility.unified[index],
        VisionBits::from_bits_retain(1 << 34),
        "explored, no longer visible"
    );
    assert_eq!(visibility.maps[2].explored, TileCount::new(12));
    // a tile at 0 stays 0; one never explored goes to -2
    visibility
        .unexplore(
            PlayerId::new(2),
            Tile::new(10, 10),
            SightTiles::new(0),
            SightCentre::Tile,
        )
        .unwrap();
    assert_eq!(visibility.maps[2].counts[index], 0);
    visibility
        .unexplore(
            PlayerId::new(2),
            Tile::new(20, 20),
            SightTiles::new(0),
            SightCentre::Tile,
        )
        .unwrap();
    assert_eq!(
        visibility.maps[2].counts[visibility.index(Tile::new(20, 20))],
        -2
    );
}

#[test]
fn unit_sight_moves_with_its_tile_and_shrinks_to_its_footprint_when_it_dies() {
    let mut visibility = world(PlayerId::new(3));
    visibility.add_seer(object_id(7), unit(PlayerId::new(1), Distance::new(2.0)));
    let footprint = Footprint::new(0.5, 0.5);
    let _ = visibility
        .sight_on(
            object_id(7),
            Position::new(5.5, 5.5, 0.0),
            footprint,
            SightShape::Circle,
            SightCentre::Tile,
            SightLayer {
                roles: &roles(),
                visibility_tuning: tuning().visibility,
            },
        )
        .unwrap();
    let before = visibility.maps[1].counts.clone();
    visibility
        .moved(
            object_id(7),
            Position::new(5.5, 5.5, 0.0),
            Position::new(5.9, 5.2, 0.0),
            footprint,
            &roles(),
            tuning().visibility,
        )
        .unwrap();
    assert_eq!(visibility.maps[1].counts, before, "the same tile");
    visibility
        .moved(
            object_id(7),
            Position::new(5.9, 5.2, 0.0),
            Position::new(6.1, 5.2, 0.0),
            footprint,
            &roles(),
            tuning().visibility,
        )
        .unwrap();
    assert_eq!(
        visibility.maps[1].counts[visibility.index(Tile::new(8, 5))],
        1
    );
    assert_eq!(
        visibility.maps[1].counts[visibility.index(Tile::new(3, 5))],
        0
    );
    visibility
        .state_changed(
            object_id(7),
            Position::new(6.1, 5.2, 0.0),
            footprint,
            ObjectState::Dying,
            &roles(),
            tuning().visibility,
        )
        .unwrap();
    let seen: Vec<_> = (0..120 * 120)
        .filter(|&tile| 0 < visibility.maps[1].counts[tile])
        .collect();
    assert_eq!(
        seen,
        vec![
            visibility.index(Tile::new(5, 4)),
            visibility.index(Tile::new(6, 4)),
            visibility.index(Tile::new(5, 5)),
            visibility.index(Tile::new(6, 5))
        ]
    );
    visibility
        .state_changed(
            object_id(7),
            Position::new(6.1, 5.2, 0.0),
            footprint,
            ObjectState::Removed,
            &roles(),
            tuning().visibility,
        )
        .unwrap();
    assert!(visibility.maps[1].counts.iter().all(|&count| count <= 0));
}

#[test]
fn the_extent_list_matches_the_saved_entries_of_town_centre_and_house() {
    let mut visibility = world(PlayerId::new(1));
    assert_eq!(
        visibility.extent_entry(Tile::new(29, 94), TileRect::new(27, 92, 30, 95)),
        1
    );
    assert_eq!(visibility.extents[0].guid, 0x0101_0202);
    assert_eq!(
        visibility.extents[0].offsets,
        vec![
            TileOffset::new(-2, -2),
            TileOffset::new(1, -2),
            TileOffset::new(-2, 1),
            TileOffset::new(1, 1)
        ]
    );
    assert_eq!(
        visibility.extent_entry(Tile::new(49, 87), TileRect::new(48, 86, 49, 87)),
        2
    );
    assert_eq!(visibility.extents[1].guid, 257);
    assert_eq!(
        visibility.extents[1].offsets,
        vec![
            TileOffset::new(-1, -1),
            TileOffset::new(0, -1),
            TileOffset::new(-1, 0)
        ]
    );
    assert_eq!(
        visibility.extent_entry(Tile::new(10, 10), TileRect::new(9, 10, 10, 10)),
        3
    );
    assert_eq!(visibility.extents[2].offsets, vec![TileOffset::new(-1, 0)]);
    assert_eq!(
        visibility.extent_entry(Tile::new(30, 95), TileRect::new(28, 93, 31, 96)),
        1,
        "found again"
    );
    assert_eq!(visibility.extents_capacity, 8);
    // 5 × 5 about its centre: the corners, then a step of 2 along the rows and the columns
    assert_eq!(
        visibility.extent_entry(Tile::new(10, 10), TileRect::new(8, 8, 12, 12)),
        4
    );
    assert_eq!(visibility.extents[3].guid, 0x0202_0202);
    assert_eq!(
        visibility.extents[3].offsets,
        vec![
            TileOffset::new(-2, -2),
            TileOffset::new(2, -2),
            TileOffset::new(-2, 2),
            TileOffset::new(2, 2),
            TileOffset::new(0, -2),
            TileOffset::new(0, 2),
            TileOffset::new(-2, 0),
            TileOffset::new(2, 0)
        ]
    );
    assert_eq!(visibility.extents[3].capacity, 9);
    // one tile wide and three high
    assert_eq!(
        visibility.extent_entry(Tile::new(10, 10), TileRect::new(10, 9, 10, 11)),
        5
    );
    assert_eq!(
        visibility.extents[4].offsets,
        vec![TileOffset::new(0, -1), TileOffset::new(0, 1)]
    );
    // off the map, or one tile: none
    assert_eq!(
        visibility.extent_entry(Tile::new(200, 10), TileRect::new(200, 10, 201, 11)),
        0
    );
    assert_eq!(
        visibility.extent_entry(Tile::new(10, 200), TileRect::new(10, 200, 11, 201)),
        0
    );
    assert_eq!(
        visibility.extent_entry(Tile::new(-5, 10), TileRect::new(-5, 10, -4, 11)),
        0
    );
    assert_eq!(
        visibility.extent_entry(Tile::new(10, -5), TileRect::new(10, -5, 11, -4)),
        0
    );
    assert_eq!(
        visibility.extent_entry(Tile::new(10, 10), TileRect::new(10, 10, 10, 10)),
        0
    );
    // the list grows by 8 once full
    for index in 1..5 {
        let _ = visibility.extent_entry(Tile::new(60, 60), TileRect::new(60, 60, 60 + index, 61));
    }
    assert_eq!(
        (visibility.extents.len(), visibility.extents_capacity),
        (9, 16)
    );
}

#[test]
fn first_sight_of_building_tile_explores_the_building() {
    let mut visibility = world(PlayerId::new(3));
    assert_eq!(
        visibility
            .link_building(
                Position::new(29.0, 94.0, 0.0),
                Footprint::new(2.0, 2.0),
                tuning().visibility
            )
            .unwrap(),
        1
    );
    assert_eq!(visibility.link(Tile::new(29, 94)).unwrap(), (ANCHOR, 1));
    assert_eq!(visibility.link(Tile::new(28, 93)).unwrap(), (LINKED, 0xff));
    assert_eq!(visibility.link(Tile::new(30, 95)).unwrap(), (LINKED, 0x11));
    assert_eq!(
        visibility.anchor_of(Tile::new(27, 92)),
        Some(Tile::new(29, 94))
    );
    assert_eq!(
        visibility.extent_of(Tile::new(30, 92)),
        Some(TileRect::new(27, 92, 30, 95))
    );
    // a sight of LOS 0 on the building's corner tile
    assert_eq!(
        visibility
            .explore(
                PlayerId::new(1),
                Tile::new(27, 92),
                SightTiles::new(0),
                SightCentre::Tile
            )
            .unwrap(),
        TileCount::ONE
    );
    assert_eq!(
        visibility.maps[1].counts[visibility.index(Tile::new(27, 92))],
        1
    );
    assert_eq!(
        visibility.maps[1].counts[visibility.index(Tile::new(30, 95))],
        0
    );
    assert_eq!(visibility.maps[1].explored, TileCount::new(16));
    visibility
        .unlink_building(
            Position::new(29.0, 94.0, 0.0),
            Footprint::new(2.0, 2.0),
            tuning().visibility,
        )
        .unwrap();
    assert_eq!(visibility.link(Tile::new(29, 94)).unwrap(), (0, 0));
}

#[test]
fn building_sees_its_footprint_while_built_then_its_circle_from_tile_corner() {
    let mut world = world(PlayerId::new(3));
    let site = Seer {
        master: ids::HOUSE,
        explored: false,
        state: ObjectState::Foundation,
        building: Some(BuildingSight::default()),
        ..unit(PlayerId::new(2), Distance::new(2.0))
    };
    world.add_seer(object_id(9), site);
    let (at, footprint) = (Position::new(10.0, 10.0, 0.0), Footprint::new(1.0, 1.0));
    assert_eq!(
        world
            .sight_on(
                object_id(9),
                at,
                footprint,
                SightShape::Circle,
                SightCentre::Tile,
                SightLayer {
                    roles: &roles(),
                    visibility_tuning: tuning().visibility
                }
            )
            .unwrap(),
        TileCount::ZERO,
        "not started"
    );
    world
        .sight_off(
            object_id(9),
            at,
            footprint,
            SightShape::Footprint,
            SightCentre::Tile,
            SightLayer {
                roles: &roles(),
                visibility_tuning: tuning().visibility,
            },
        )
        .unwrap();
    assert!(world.maps[2].counts.iter().all(|&count| count == -1));
    world
        .construction_started(object_id(9), at, footprint, &roles(), tuning().visibility)
        .unwrap();
    let seen = |visibility: &Visibility| {
        (0..120 * 120)
            .filter(|&tile| 0 < visibility.maps[2].counts[tile])
            .count()
    };
    assert_eq!(seen(&world), 4);
    *world
        .seer_mut(object_id(9))
        .unwrap()
        .building
        .as_mut()
        .unwrap() = BuildingSight::Built;
    world
        .sight_off(
            object_id(9),
            at,
            footprint,
            SightShape::Footprint,
            SightCentre::Tile,
            SightLayer {
                roles: &roles(),
                visibility_tuning: tuning().visibility,
            },
        )
        .unwrap();
    let _ = world
        .sight_on(
            object_id(9),
            at,
            footprint,
            SightShape::Circle,
            SightCentre::Tile,
            SightLayer {
                roles: &roles(),
                visibility_tuning: tuning().visibility,
            },
        )
        .unwrap();
    // the table of a tile corner: at LOS 2 a 4 by 4 square around the corner
    assert_eq!(seen(&world), 16);
    assert_eq!(world.maps[2].counts[world.index(Tile::new(8, 9))], 1);
    assert_eq!(world.maps[2].counts[world.index(Tile::new(11, 9))], 1);
}

/// A computer player's first sight of a tile queues it for its AI module (slot 0x88), once per
/// tile; a human player's sight queues nothing.
#[test]
fn computer_players_newly_explored_tiles_are_queued_for_its_ai_module() {
    let mut visibility = world(PlayerId::new(3));
    visibility.player_kinds = vec![PlayerKind::Human, PlayerKind::Computer, PlayerKind::Human];
    assert_eq!(
        visibility.explore(
            PlayerId::new(1),
            Tile::new(10, 10),
            SightTiles::new(0),
            SightCentre::Tile
        ),
        Ok(TileCount::ONE)
    );
    assert_eq!(
        visibility.explore(
            PlayerId::new(1),
            Tile::new(10, 10),
            SightTiles::new(0),
            SightCentre::Tile
        ),
        Ok(TileCount::ZERO)
    );
    assert_eq!(
        visibility.explore(
            PlayerId::new(2),
            Tile::new(20, 20),
            SightTiles::new(0),
            SightCentre::Tile
        ),
        Ok(TileCount::ONE)
    );
    assert_eq!(
        visibility.take_ai_tiles(),
        vec![(PlayerId::new(1), Tile::new(10, 10))]
    );
    assert_eq!(visibility.take_ai_tiles(), vec![]);
}

#[test]
fn tile_circle_sight_is_circle_centred_on_seer_tile() {
    let (at, radius) = (Position::new(10.5, 10.5, 0.0), Footprint::new(0.5, 0.5));
    let sight = LosSight::tile_circle(at, radius, SightTiles::new(3));
    assert_eq!(
        sight,
        LosSight {
            position: at,
            radius,
            shape: SightShape::Circle,
            los: SightTiles::new(3),
            centre: SightCentre::Tile,
        }
    );
}
