use super::*;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::{Tile, TileOffset, TileRect};
use crate::objects::ObjectState;
use crate::objects::masters::class;
use crate::objects::masters::master_type;
use crate::ownership::CaptureKind;
use crate::test_ids::object_id;
use crate::vision::SightTiles;
use crate::vision::registry::UnitUpdate;

fn world(players: PlayerId) -> Visibility {
    let n = 120 * 120;
    Visibility::new(
        GridSize::new(120, 120),
        vec![
            VisibleMap {
                counts: vec![-1; n],
                explored: 0,
            };
            players.index()
        ],
        vec![VisionBits::NONE; n],
        Vec::new(),
        0,
        vec![false; players.index()],
    )
}

fn unit(player: PlayerId, los: Distance) -> Seer {
    Seer {
        player,
        master: ids::VILLAGER_MALE,
        los,
        state: ObjectState::Alive,
        explored: true,
        building: None,
        umv: 0,
        capture: CaptureKind::NONE,
        kind: master_type::COMBAT,
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
        x,
        y,
        ..VisibleResource::default()
    }
}

/// `RGE_Combat_Object::update`'s takeover by sight: the first player from 1 on but the owner
/// whose bit the mask holds; outside the standard modes a computer player takes over only
/// when it allows taking Gaia's units.
#[test]
fn the_first_other_player_that_sees_the_object_takes_it_over() {
    use crate::world::player::Player;
    let players = |computers: [bool; 4]| {
        computers.map(|computer| Player {
            computer,
            ..Player::default()
        })
    };
    let humans = players([false; 4]);
    let none = [false; 4];
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(0b0111),
            PlayerId::new(0),
            true,
            &humans,
            &none
        ),
        Some(PlayerId::new(1))
    );
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(0b0110),
            PlayerId::new(1),
            true,
            &humans,
            &none
        ),
        Some(PlayerId::new(2))
    );
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(0b0001),
            PlayerId::new(1),
            true,
            &humans,
            &none
        ),
        None
    );
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(1 << 33 | 0b10),
            PlayerId::new(1),
            true,
            &humans,
            &none
        ),
        None
    );
    let ai = players([false, true, false, true]);
    let allows = [false, false, false, true];
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(0b1110),
            PlayerId::new(0),
            false,
            &ai,
            &allows
        ),
        Some(PlayerId::new(2))
    );
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(0b1010),
            PlayerId::new(0),
            false,
            &ai,
            &allows
        ),
        Some(PlayerId::new(3))
    );
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(0b0010),
            PlayerId::new(0),
            false,
            &ai,
            &allows
        ),
        None
    );
    assert_eq!(
        sight_taker(
            VisionBits::from_bits_retain(0b0010),
            PlayerId::new(0),
            true,
            &ai,
            &allows
        ),
        Some(PlayerId::new(1))
    );
}

/// The combat object's sight answers a changed mask only for an object others take over by
/// seeing it (`+0x924` 1), once the registry took it.
#[test]
fn a_changed_mask_is_the_answer_for_an_object_taken_over_by_sight() {
    let mut v = world(PlayerId::new(3));
    v.registry = Some(Registry::new(3, 65));
    let mut gaia = unit(PlayerId::new(0), Distance::new(2.0));
    gaia.capture = CaptureKind::BY_SIGHT;
    v.add_seer(object_id(7), gaia);
    v.add_seer(object_id(8), unit(PlayerId::new(0), Distance::new(2.0)));
    let i = 10 * 120 + 10;
    v.unified[i] = VisionBits::from_bits_retain(0b011);
    let at = Position::new(10.5, 10.5, 0.0);
    assert_eq!(
        v.combat_sight(
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
        v.combat_sight(
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
        v.combat_sight(
            object_id(8),
            PlayerId::new(0),
            at,
            Footprint::new(0.2, 0.2),
            class::CLASS_6,
            None
        ),
        Ok(None)
    );
    let r = v.registry.as_ref().unwrap();
    assert_eq!(
        r.entry(object_id(8)).map(|e| e.mask),
        Some(VisionBits::from_bits_retain(0b011))
    );
    assert_eq!(r.lists[1][6].len(), 2);
}

/// `FUN_14120f800`: an object with an id writes the new owner into its records in every
/// list (`+0x8b0`..); one without an id has none to write.
#[test]
fn an_ownership_change_writes_the_new_owner_into_the_object_s_records() {
    let mut v = world(PlayerId::new(3));
    let mut r = Registry::new(3, 65);
    r.entries.entry(object_id(7)).or_default().mask = VisionBits::from_bits_retain(0b11);
    r.update_unit(UnitUpdate {
        id: object_id(7),
        owner: PlayerId::new(0),
        position: Point::new(10.5, 10.5),
        radii: Footprint::new(0.2, 0.2),
        class: class::CLASS_6,
        old: VisionBits::NONE,
        new: VisionBits::from_bits_retain(0b11),
    })
    .unwrap();
    v.registry = Some(r);
    v.records_owner(object_id(7), PlayerId::new(2)).unwrap();
    v.records_owner(object_id(-5), PlayerId::new(2)).unwrap();
    let r = v.registry.as_ref().unwrap();
    assert_eq!(
        [r.lists[0][6][0].owner, r.lists[1][6][0].owner],
        [PlayerId::new(2); 2]
    );
    assert_eq!(r.lists[2][6].len(), 0);
}

/// `FUN_14120f800`'s sight: an object that sees loses its sight on the old owner's map
/// (slot 0x1d8) and gains it, with its new master's line of sight, on the new one's (slot
/// 0x1d0); one that does not see (inside another, or not in state 2) changes owner and line
/// of sight alone.
#[test]
fn an_ownership_change_moves_the_sight_of_an_object_that_sees() {
    let at = Position::new(10.5, 10.5, 0.0);
    let mut v = world(PlayerId::new(3));
    v.add_seer(object_id(7), unit(PlayerId::new(0), Distance::new(2.0)));
    let _ = v
        .sight_on(object_id(7), at, Footprint::new(0.2, 0.2), false, false)
        .unwrap();
    let tile = |x: i32, y: i32| (y * 120 + x) as usize;
    assert_eq!(v.maps[0].counts[tile(12, 10)], 1);
    v.owner_changed(
        object_id(7),
        at,
        Footprint::new(0.2, 0.2),
        (PlayerId::new(1), 3.0),
        true,
    )
    .unwrap();
    assert_eq!(v.maps[0].counts[tile(12, 10)], 0);
    assert_eq!(v.maps[1].counts[tile(13, 10)], 1);
    assert_eq!(v.maps[1].counts[tile(14, 10)], -1);
    let s = v.seer(object_id(7)).copied().unwrap();
    assert_eq!((s.player, s.los), (PlayerId::new(1), Distance::new(3.0)));
    v.owner_changed(
        object_id(7),
        at,
        Footprint::new(0.2, 0.2),
        (PlayerId::new(2), 4.0),
        false,
    )
    .unwrap();
    assert_eq!(v.maps[1].counts[tile(13, 10)], 1);
    assert_eq!(v.maps[2].counts[tile(10, 10)], -1);
    let s = v.seer(object_id(7)).copied().unwrap();
    assert_eq!((s.player, s.los), (PlayerId::new(2), Distance::new(4.0)));
}

/// `AddResource` (`FUN_141288830`): the record goes last in its group's list at the object's
/// place, its distance and byte 0; a full list doubles its capacity. A manager without lists
/// adds nothing.
#[test]
fn a_resource_goes_last_in_its_group_s_list_and_a_full_list_doubles() {
    let mut r = resources(5);
    r.add(object_id(10), GatherGroup::GOLD, 3.5, 7.5);
    r.add(object_id(11), GatherGroup::GOLD, 1.5, 2.5);
    assert_eq!(r.capacities[4], 2);
    r.add(object_id(12), GatherGroup::GOLD, 0.5, 0.5);
    assert_eq!(r.capacities, [2, 2, 2, 2, 4]);
    assert_eq!(
        r.lists[4],
        [rec(10, 3.5, 7.5), rec(11, 1.5, 2.5), rec(12, 0.5, 0.5)]
    );
    assert!(r.lists[..4].iter().all(Vec::is_empty));
    let mut none = VisibleResources::default();
    none.add(object_id(10), GatherGroup::WOOD, 1.0, 1.0);
    assert_eq!(none, VisibleResources::default());
}

/// `Remove_Resource` (`FUN_141289b80`): the first record of the object takes the last one's
/// record and the list shortens; its index, 0 for an empty list, -1 when not found.
#[test]
fn a_removed_resource_s_place_takes_the_last_record() {
    let mut r = resources(5);
    for id in [1, 2, 3] {
        r.add(object_id(id), GatherGroup::WOOD, id as f32, 0.0);
    }
    assert_eq!(r.remove(object_id(1), GatherGroup::WOOD), 0);
    assert_eq!(r.lists[0], [rec(3, 3.0, 0.0), rec(2, 2.0, 0.0)]);
    assert_eq!(r.remove(object_id(2), GatherGroup::WOOD), 1);
    assert_eq!(r.lists[0], [rec(3, 3.0, 0.0)]);
    assert_eq!(r.remove(object_id(9), GatherGroup::WOOD), -1);
    assert_eq!(r.remove(object_id(9), GatherGroup::FORAGE), 0);
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
    let mut r = resources(5);
    r.add(object_id(1), GatherGroup::FISH, 13.0, 10.0);
    r.add(object_id(2), GatherGroup::FISH, 10.0, 12.0);
    r.add(object_id(3), GatherGroup::FISH, 7.0, 10.0);
    assert_eq!(r.closest(10.0, 10.0, 2, &[]), Some(1));
    assert_eq!(r.lists[2][1].distance, Distance::new(2.0));
    assert_eq!(r.closest(10.0, 10.0, 2, &[object_id(2)]), Some(0));
    assert_eq!(r.lists[2][0].distance, Distance::new(3.0));
    assert_eq!(
        r.closest(10.0, 10.0, 2, &[object_id(2), object_id(1)]),
        Some(2)
    );
    assert_eq!(
        r.closest(10.0, 10.0, 2, &[object_id(1), object_id(2), object_id(3)]),
        None
    );
    assert_eq!(r.closest(10.0, 10.0, 1, &[]), None);
    assert_eq!(r.closest(10.0, 10.0, 5, &[]), None);
    assert_eq!(r.closest(10.0, 10.0, -1, &[]), None);
}

/// The tiles a player's map explores go to its tile list (player `+0x180`, `FUN_14165d500`)
/// in order: the circle's (`explore_terrain`) and a rectangle's (`explore_terrain_sq`) newly
/// explored tiles, a revealed rectangle's never explored ones (`reveal_terrain_sq`); the
/// player's update ends with the list cleared (`FUN_141661030`).
#[test]
fn newly_explored_tiles_join_the_player_s_tile_list_until_its_update_ends() {
    let mut v = world(PlayerId::new(2));
    let _ = v
        .explore(
            PlayerId::new(1),
            Tile::new(10, 10),
            SightTiles::new(0),
            false,
        )
        .unwrap();
    let _ = v
        .explore(
            PlayerId::new(1),
            Tile::new(10, 10),
            SightTiles::new(1),
            false,
        )
        .unwrap();
    assert_eq!(
        v.new_tiles[1],
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
    let _ = v
        .explore_rect(PlayerId::new(1), TileRect::new(11, 9, 12, 9))
        .unwrap();
    let _ = v
        .reveal_rect(PlayerId::new(1), TileRect::new(11, 8, 12, 8))
        .unwrap();
    assert_eq!(
        v.new_tiles[1][9..],
        [Tile::new(12, 9), Tile::new(11, 8), Tile::new(12, 8)]
    );
    assert_eq!(v.new_tiles[0], Vec::<Tile>::new());
    v.clear_new_tiles(PlayerId::new(1));
    assert_eq!(v.new_tiles[1], Vec::<Tile>::new());
}

/// `FUN_1411980b0`'s lists: each player whose map explored the object's tile (its bit 32 + p
/// of the unified map, from 0 while the player exists) loses the object from its group's list.
#[test]
fn a_resource_leaves_the_lists_of_the_players_that_explored_its_tile() {
    let mut v = world(PlayerId::new(3));
    v.resources = vec![resources(5), resources(5), resources(5)];
    for r in &mut v.resources {
        r.add(object_id(7), GatherGroup::FORAGE, 20.5, 30.5);
    }
    let _ = v
        .explore(
            PlayerId::new(2),
            Tile::new(20, 30),
            SightTiles::new(0),
            false,
        )
        .unwrap();
    v.forget_resource(
        object_id(7),
        Position::new(20.5, 30.5, 0.0),
        GatherGroup::FORAGE,
    )
    .unwrap();
    assert_eq!(v.resources[2].lists[1], []);
    assert_eq!(v.resources[0].lists[1], [rec(7, 20.5, 30.5)]);
    assert_eq!(v.resources[1].lists[1], [rec(7, 20.5, 30.5)]);
}

#[test]
fn a_sight_counts_its_circle_explored_then_visible_and_leaves_it_explored() {
    let mut v = world(PlayerId::new(3));
    assert_eq!(
        v.explore(
            PlayerId::new(2),
            Tile::new(10, 10),
            SightTiles::new(1),
            false
        )
        .unwrap(),
        9
    );
    assert_eq!(v.maps[2].explored, 9);
    let i = v.index(Tile::new(10, 10));
    assert_eq!(v.maps[2].counts[i], 1);
    assert_eq!(v.unified[i], VisionBits::from_bits_retain(1 << 2 | 1 << 34));
    assert_eq!(
        v.explore(
            PlayerId::new(2),
            Tile::new(11, 10),
            SightTiles::new(1),
            false
        )
        .unwrap(),
        3
    );
    assert_eq!(v.maps[2].counts[i], 2);
    v.unexplore(
        PlayerId::new(2),
        Tile::new(10, 10),
        SightTiles::new(1),
        false,
    )
    .unwrap();
    assert_eq!(v.maps[2].counts[i], 1);
    v.unexplore(
        PlayerId::new(2),
        Tile::new(11, 10),
        SightTiles::new(1),
        false,
    )
    .unwrap();
    assert_eq!(v.maps[2].counts[i], 0);
    assert_eq!(
        v.unified[i],
        VisionBits::from_bits_retain(1 << 34),
        "explored, no longer visible"
    );
    assert_eq!(v.maps[2].explored, 12);
    // a tile at 0 stays 0; one never explored goes to -2
    v.unexplore(
        PlayerId::new(2),
        Tile::new(10, 10),
        SightTiles::new(0),
        false,
    )
    .unwrap();
    assert_eq!(v.maps[2].counts[i], 0);
    v.unexplore(
        PlayerId::new(2),
        Tile::new(20, 20),
        SightTiles::new(0),
        false,
    )
    .unwrap();
    assert_eq!(v.maps[2].counts[v.index(Tile::new(20, 20))], -2);
}

#[test]
fn a_unit_s_sight_moves_with_its_tile_and_shrinks_to_its_footprint_when_it_dies() {
    let mut v = world(PlayerId::new(3));
    v.add_seer(object_id(7), unit(PlayerId::new(1), Distance::new(2.0)));
    let r = Footprint::new(0.5, 0.5);
    let _ = v
        .sight_on(object_id(7), Position::new(5.5, 5.5, 0.0), r, false, false)
        .unwrap();
    let before = v.maps[1].counts.clone();
    v.moved(
        object_id(7),
        Position::new(5.5, 5.5, 0.0),
        Position::new(5.9, 5.2, 0.0),
        r,
    )
    .unwrap();
    assert_eq!(v.maps[1].counts, before, "the same tile");
    v.moved(
        object_id(7),
        Position::new(5.9, 5.2, 0.0),
        Position::new(6.1, 5.2, 0.0),
        r,
    )
    .unwrap();
    assert_eq!(v.maps[1].counts[v.index(Tile::new(8, 5))], 1);
    assert_eq!(v.maps[1].counts[v.index(Tile::new(3, 5))], 0);
    v.state_changed(
        object_id(7),
        Position::new(6.1, 5.2, 0.0),
        r,
        ObjectState::Dying,
    )
    .unwrap();
    let seen: Vec<_> = (0..120 * 120)
        .filter(|&i| 0 < v.maps[1].counts[i])
        .collect();
    assert_eq!(
        seen,
        vec![
            v.index(Tile::new(5, 4)),
            v.index(Tile::new(6, 4)),
            v.index(Tile::new(5, 5)),
            v.index(Tile::new(6, 5))
        ]
    );
    v.state_changed(
        object_id(7),
        Position::new(6.1, 5.2, 0.0),
        r,
        ObjectState::Removed,
    )
    .unwrap();
    assert!(v.maps[1].counts.iter().all(|&c| c <= 0));
}

#[test]
fn the_extent_list_matches_the_saved_entries_of_a_town_centre_and_a_house() {
    let mut v = world(PlayerId::new(1));
    assert_eq!(
        v.extent_entry(Tile::new(29, 94), TileRect::new(27, 92, 30, 95)),
        1
    );
    assert_eq!(v.extents[0].guid, 0x0101_0202);
    assert_eq!(
        v.extents[0].offsets,
        vec![
            TileOffset::new(-2, -2),
            TileOffset::new(1, -2),
            TileOffset::new(-2, 1),
            TileOffset::new(1, 1)
        ]
    );
    assert_eq!(
        v.extent_entry(Tile::new(49, 87), TileRect::new(48, 86, 49, 87)),
        2
    );
    assert_eq!(v.extents[1].guid, 257);
    assert_eq!(
        v.extents[1].offsets,
        vec![
            TileOffset::new(-1, -1),
            TileOffset::new(0, -1),
            TileOffset::new(-1, 0)
        ]
    );
    assert_eq!(
        v.extent_entry(Tile::new(10, 10), TileRect::new(9, 10, 10, 10)),
        3
    );
    assert_eq!(v.extents[2].offsets, vec![TileOffset::new(-1, 0)]);
    assert_eq!(
        v.extent_entry(Tile::new(30, 95), TileRect::new(28, 93, 31, 96)),
        1,
        "found again"
    );
    assert_eq!(v.extents_capacity, 8);
    // 5 × 5 about its centre: the corners, then a step of 2 along the rows and the columns
    assert_eq!(
        v.extent_entry(Tile::new(10, 10), TileRect::new(8, 8, 12, 12)),
        4
    );
    assert_eq!(v.extents[3].guid, 0x0202_0202);
    assert_eq!(
        v.extents[3].offsets,
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
    assert_eq!(v.extents[3].capacity, 9);
    // one tile wide and three high
    assert_eq!(
        v.extent_entry(Tile::new(10, 10), TileRect::new(10, 9, 10, 11)),
        5
    );
    assert_eq!(
        v.extents[4].offsets,
        vec![TileOffset::new(0, -1), TileOffset::new(0, 1)]
    );
    // off the map, or one tile: none
    assert_eq!(
        v.extent_entry(Tile::new(200, 10), TileRect::new(200, 10, 201, 11)),
        0
    );
    assert_eq!(
        v.extent_entry(Tile::new(10, 200), TileRect::new(10, 200, 11, 201)),
        0
    );
    assert_eq!(
        v.extent_entry(Tile::new(-5, 10), TileRect::new(-5, 10, -4, 11)),
        0
    );
    assert_eq!(
        v.extent_entry(Tile::new(10, -5), TileRect::new(10, -5, 11, -4)),
        0
    );
    assert_eq!(
        v.extent_entry(Tile::new(10, 10), TileRect::new(10, 10, 10, 10)),
        0
    );
    // the list grows by 8 once full
    for w in 1..5 {
        let _ = v.extent_entry(Tile::new(60, 60), TileRect::new(60, 60, 60 + w, 61));
    }
    assert_eq!((v.extents.len(), v.extents_capacity), (9, 16));
}

#[test]
fn a_first_sight_of_a_building_s_tile_explores_the_building() {
    let mut v = world(PlayerId::new(3));
    assert_eq!(
        v.link_building(Position::new(29.0, 94.0, 0.0), Footprint::new(2.0, 2.0))
            .unwrap(),
        1
    );
    assert_eq!(v.link(Tile::new(29, 94)).unwrap(), (ANCHOR, 1));
    assert_eq!(v.link(Tile::new(28, 93)).unwrap(), (LINKED, 0xff));
    assert_eq!(v.link(Tile::new(30, 95)).unwrap(), (LINKED, 0x11));
    assert_eq!(v.anchor_of(Tile::new(27, 92)), Some(Tile::new(29, 94)));
    assert_eq!(
        v.extent_of(Tile::new(30, 92)),
        Some(TileRect::new(27, 92, 30, 95))
    );
    // a sight of LOS 0 on the building's corner tile
    assert_eq!(
        v.explore(
            PlayerId::new(1),
            Tile::new(27, 92),
            SightTiles::new(0),
            false
        )
        .unwrap(),
        1
    );
    assert_eq!(v.maps[1].counts[v.index(Tile::new(27, 92))], 1);
    assert_eq!(v.maps[1].counts[v.index(Tile::new(30, 95))], 0);
    assert_eq!(v.maps[1].explored, 16);
    v.unlink_building(Position::new(29.0, 94.0, 0.0), Footprint::new(2.0, 2.0))
        .unwrap();
    assert_eq!(v.link(Tile::new(29, 94)).unwrap(), (0, 0));
}

#[test]
fn a_building_sees_its_footprint_while_built_then_its_circle_from_a_tile_corner() {
    let mut v = world(PlayerId::new(3));
    let site = Seer {
        master: ids::HOUSE,
        explored: false,
        state: ObjectState::Foundation,
        building: Some(BuildingSight::default()),
        ..unit(PlayerId::new(2), Distance::new(2.0))
    };
    v.add_seer(object_id(9), site);
    let (at, r) = (Position::new(10.0, 10.0, 0.0), Footprint::new(1.0, 1.0));
    assert_eq!(
        v.sight_on(object_id(9), at, r, false, false).unwrap(),
        0,
        "not started"
    );
    v.sight_off(object_id(9), at, r, true, false).unwrap();
    assert!(v.maps[2].counts.iter().all(|&c| c == -1));
    v.construction_started(object_id(9), at, r).unwrap();
    let seen = |v: &Visibility| (0..120 * 120).filter(|&i| 0 < v.maps[2].counts[i]).count();
    assert_eq!(seen(&v), 4);
    v.seer_mut(object_id(9))
        .unwrap()
        .building
        .as_mut()
        .unwrap()
        .built = true;
    v.sight_off(object_id(9), at, r, true, false).unwrap();
    let _ = v.sight_on(object_id(9), at, r, false, false).unwrap();
    // the table of a tile corner: at LOS 2 a 4 by 4 square around the corner
    assert_eq!(seen(&v), 16);
    assert_eq!(v.maps[2].counts[v.index(Tile::new(8, 9))], 1);
    assert_eq!(v.maps[2].counts[v.index(Tile::new(11, 9))], 1);
}

/// A computer player's first sight of a tile queues it for its AI module (slot 0x88), once per
/// tile; a human player's sight queues nothing.
#[test]
fn a_computer_players_newly_explored_tiles_are_queued_for_its_ai_module() {
    let mut v = world(PlayerId::new(3));
    v.computer = vec![false, true, false];
    assert_eq!(
        v.explore(
            PlayerId::new(1),
            Tile::new(10, 10),
            SightTiles::new(0),
            false
        ),
        Ok(1)
    );
    assert_eq!(
        v.explore(
            PlayerId::new(1),
            Tile::new(10, 10),
            SightTiles::new(0),
            false
        ),
        Ok(0)
    );
    assert_eq!(
        v.explore(
            PlayerId::new(2),
            Tile::new(20, 20),
            SightTiles::new(0),
            false
        ),
        Ok(1)
    );
    assert_eq!(
        v.take_ai_tiles(),
        vec![(PlayerId::new(1), Tile::new(10, 10))]
    );
    assert_eq!(v.take_ai_tiles(), vec![]);
}
