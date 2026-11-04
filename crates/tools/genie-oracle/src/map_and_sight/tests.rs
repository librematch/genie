use crate::map_and_sight::{resource_differences, visibility_differences};
use crate::test_ids::object_id;
use genie_core::map::Distance;
use genie_core::map::GridSize;
use genie_core::map::Point;
use genie_core::vision::TileCount;
use genie_core::vision::VisionBits;
use genie_core::vision::visibility::{Visibility, VisibleMap, VisibleResource, VisibleResources};

/// A 2 × 2 map, Gaia and one player, each tile counted once and the player's unified bits set.
fn sight() -> Visibility {
    let map = || VisibleMap {
        counts: vec![1; 4],
        explored: TileCount::new(4),
    };
    let resources = || VisibleResources {
        capacities: vec![4],
        lists: vec![vec![VisibleResource {
            id: object_id(7),
            distance: Distance::ZERO,
            zone: 0,
            at: Point::new(1.5, 2.5),
        }]],
    };
    let mut sight = Visibility::default();
    sight.size = GridSize::new(2, 2);
    sight.maps = vec![map(), map()];
    sight.unified = vec![VisionBits::from_bits_retain(1 << 1 | 1 << 33); 4];
    sight.resources = vec![resources(), resources()];
    sight
}

#[test]
fn the_same_sight_has_no_difference() {
    assert_eq!(
        visibility_differences(&sight(), &sight(), 1),
        Vec::<String>::new()
    );
}

/// The explored count, a tile's count and its two unified bits, each with the tile's place.
#[test]
fn tile_differs_by_its_count_or_its_unified_bits() {
    let mut slice = sight();
    slice.maps[1].explored = TileCount::new(3);
    slice.maps[1].counts[3] = 0;
    slice.unified[2] = VisionBits::from_bits_retain(1 << 1);
    assert_eq!(
        visibility_differences(&slice, &sight(), 1),
        [
            "player 1 explored 3 tiles, the game 4",
            "player 1 tile (0, 1) unified bits 0x2, the game 0x200000002",
            "player 1 tile (1, 1) count 0, the game 1",
        ]
    );
    // another player's bits do not count
    slice = sight();
    slice.unified[0] |= VisionBits::from_bits_retain(1);
    assert_eq!(
        visibility_differences(&slice, &sight(), 1),
        Vec::<String>::new()
    );
}

#[test]
fn player_without_map_or_lists_says_so() {
    assert_eq!(
        visibility_differences(&sight(), &sight(), 2),
        ["player 2 has no visible map"]
    );
    assert_eq!(
        resource_differences(&sight(), &sight(), 2),
        ["player 2 has no visible-resource lists"]
    );
}

/// The count of lists, the capacities, a list's first differing record (a float by its bits)
/// or its length.
#[test]
fn the_resource_lists_differ_by_count_capacity_record_or_length() {
    let mut slice = sight();
    slice.resources[1].capacities = vec![8];
    slice.resources[1].lists[0][0].at = Point::new(1.75, 2.5);
    let lines = resource_differences(&slice, &sight(), 1);
    assert_eq!(
        lines[0],
        "player 1 visible-resource capacities [8], the game [4]"
    );
    assert!(lines[1].starts_with("player 1 visible resources 0: record 0 VisibleResource"));
    assert_eq!(lines.len(), 2);

    let mut longer = sight();
    let record = longer.resources[1].lists[0][0];
    longer.resources[1].lists[0].push(record);
    longer.resources[1].lists.push(vec![]);
    assert_eq!(
        resource_differences(&longer, &sight(), 1),
        [
            "player 1 holds 2 visible-resource lists, the game 1",
            "player 1 visible resources 0: 2 records, the game 1",
        ]
    );
    // the differences reach the visibility's too
    assert_eq!(visibility_differences(&longer, &sight(), 1).len(), 2);
}
