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

mod spatial {
    use crate::fixtures::{
        building_mut, building_object, combat_master, combat_mut, combat_object, gaia, header,
        human, human_mut, master_static_mut, object_list, static_mut, with_masters,
    };
    use crate::map_and_sight::spatial;
    use crate::masters::TerrainTable;
    use crate::test_ids::object_id;
    use genie_core::map::Radii;
    use genie_core::map::Tile;
    use genie_core::map::spatial::Spatial;
    use genie_core::objects::masters::MasterKind;
    use genie_core::objects::masters::class;
    use genie_core::objects::masters::ids::VILLAGER_MALE;
    use genie_core::objects::masters::{ObstructionClass, ObstructionKind};
    use genie_core::ownership::CaptureKind;
    use genie_core::players::PlayerId;
    use genie_core::players::{ByPlayer, UnitRelation};
    use genie_save::generated::world::Master;
    use genie_save::generated::world::{Object, RgeMapRow, RgeTile};

    /// A master of id `id`, its radius `radius` and an obstruction of kind `kind`.
    fn master(id: i16, radius: Radii, kind: u8) -> Master {
        let mut m = combat_master(id);
        let s = master_static_mut(&mut m);
        (s.radius_x, s.radius_y, s.radius_z) = (radius.x(), radius.y(), radius.z());
        s.los = 4.0;
        s.obstruction_type = kind;
        s.obstruction_class = 3;
        s.read_temp_5 = 6;
        m
    }

    /// `object` placed at (`x`, `y`).
    fn at(mut object: Object, x: f32, y: f32) -> Object {
        let s = if object.tribe_building_object.is_some() {
            &mut building_mut(&mut object)
                .tribe_combat_object
                .rge_combat_object
                .rge_action_object
                .rge_moving_object
                .rge_animated_object
                .rge_static_object
        } else {
            static_mut(&mut object)
        };
        [s.tx, s.ty] = [x, y];
        s.inside_obj = -1;
        object
    }

    /// A 4 × 4 map, Gaia and a player with a unit, two buildings, a unit inside one, an object
    /// without its master and a ghost.
    fn placed() -> Spatial {
        let mut unit = at(combat_object(10, 83), 1.5, 1.5);
        combat_mut(&mut unit).rge_combat_object.capture_flag = 5;
        let mut standing = at(building_object(11, 70), 2.5, 2.5);
        let b = building_mut(&mut standing);
        b.built = 1;
        b.mupt_linked_object_references_linked_owner_id = -1;
        let mut desolid = at(building_object(12, 70), 0.5, 3.5);
        building_mut(&mut desolid).desolid_flag = 1;
        let mut inside = at(combat_object(13, 83), 2.5, 2.5);
        static_mut(&mut inside).inside_obj = 11;
        let lost = at(combat_object(14, 999), 3.5, 0.5);
        let mut player = human(vec![unit, standing, desolid, inside, lost]);
        let objects = &mut player
            .human_or_coop_player
            .as_deref_mut()
            .unwrap()
            .player_objects;
        let masters = with_masters(vec![
            master(83, Radii::new(0.2, 0.2, 1.0), 2),
            master(70, Radii::new(1.0, 1.0, 2.0), 5),
        ]);
        objects.masters = masters.masters;
        objects.doppleganger_objects.object_type_list =
            object_list(vec![at(combat_object(20, 83), 3.5, 3.5)]);
        human_mut(&mut player).base.unit_diplomacy = vec![0, 1];
        let mut save = header(vec![gaia(Vec::new()), player]);
        let map = &mut save.world.base_world.map.val;
        [map.map_width, map.map_height] = [4, 4];
        map.rows = (0..4)
            .map(|_| RgeMapRow {
                tiles: vec![RgeTile::default(); 4],
            })
            .collect();
        spatial(&save, &TerrainTable::default()).unwrap()
    }

    #[test]
    fn unit_takes_its_masters_footprint_and_obstruction() {
        let out = placed();
        let unit = out.get(object_id(10)).unwrap();
        assert_eq!(
            (unit.player, unit.master, unit.class),
            (PlayerId::new(1), VILLAGER_MALE, class::CLASS_6)
        );
        assert!(unit.tall);
        let obstruction = unit.obstruction.unwrap();
        assert_eq!(
            (obstruction.kind, obstruction.class),
            (ObstructionKind::TYPE_2, ObstructionClass::BUILDING)
        );
        assert_eq!(out.tile_list(Tile::new(1, 1)), [object_id(10)]);
        let seer = out.sight.as_ref().unwrap().seer(object_id(10)).unwrap();
        assert_eq!(
            (seer.kind, seer.capture, seer.umv),
            (MasterKind::COMBAT, CaptureKind::of_data(5), 0)
        );
        assert!(seer.explored && seer.building.is_none());
        assert_eq!(
            out.unit_diplomacy
                .get(PlayerId::new(1))
                .map(ByPlayer::values),
            Some(&[Some(UnitRelation::GAIA), Some(UnitRelation::OWN)][..])
        );
    }

    /// A standing building links its tiles (their link bits). A desolid foundation links none,
    /// has no obstruction and has not explored.
    #[test]
    fn building_links_its_tiles_unless_it_was_made_desolid() {
        let out = placed();
        let sight = out.sight.as_ref().unwrap();
        let standing = sight.seer(object_id(11)).unwrap();
        assert_eq!(standing.kind, MasterKind::BUILDING);
        assert!(standing.explored);
        assert!(standing.building.is_some_and(|b| b.built && !b.started));
        assert!(out.get(object_id(11)).unwrap().obstruction.is_some());
        assert_ne!(sight.link(Tile::new(2, 2)).unwrap().0, 0);
        let desolid = sight.seer(object_id(12)).unwrap();
        assert!(!desolid.explored);
        assert!(out.get(object_id(12)).unwrap().obstruction.is_none());
        assert_eq!(sight.link(Tile::new(0, 3)).unwrap().0, 0);
    }

    /// The unit inside the building is off the tiles; the object without a master is not
    /// placed; the ghost is on its tile.
    #[test]
    fn inside_masterless_and_ghost_objects() {
        let out = placed();
        assert!(out.get(object_id(13)).is_some());
        assert!(!out.tile_list(Tile::new(2, 2)).contains(&object_id(13)));
        assert!(out.get(object_id(14)).is_none());
        assert!(out.tile_list(Tile::new(3, 3)).contains(&object_id(20)));
    }
}
