use crate::map::Radii;
use crate::map::Tile;
use crate::map::spatial::Spatial;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::{ObstructionClass, ObstructionKind};
use crate::ownership::CaptureKind;
use crate::players::PlayerId;
use crate::players::{ByPlayer, UnitDiplomacy, UnitRelation};
use crate::test_ids::object_id;
use crate::testing::ids::VILLAGER_MALE;
use crate::testing::layer;
use crate::vision::visibility::BuildingSight;
use crate::world::load::fixtures::{
    building_mut, building_object, combat_master, combat_mut, combat_object, gaia, header, human,
    human_mut, master_static_mut, object_list, static_mut, with_masters,
};
use crate::world::load::map::spatial;
use genie_import::dat::TerrainTable;
use genie_save::generated::world::Master;
use genie_save::generated::world::{Object, RgeMapRow, RgeTile};

/// A master of id `id`, its radius `radius` and an obstruction of kind `kind`.
fn master(id: i16, radius: Radii, kind: u8) -> Master {
    let mut combat_master = combat_master(id);
    let rge_master_static_object = master_static_mut(&mut combat_master);
    (
        rge_master_static_object.radius_x,
        rge_master_static_object.radius_y,
        rge_master_static_object.radius_z,
    ) = (radius.x(), radius.y(), radius.z());
    rge_master_static_object.los = 4.0;
    rge_master_static_object.obstruction_kind = kind;
    rge_master_static_object.obstruction_class = 3;
    rge_master_static_object.read_temp_5 = 6;
    combat_master
}

/// `object` placed at (`x`, `y`).
fn at(mut object: Object, x: f32, y: f32) -> Object {
    let rge_static_object = if object.tribe_building_object.is_some() {
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
    [rge_static_object.tx, rge_static_object.ty] = [x, y];
    rge_static_object.inside_obj = -1;
    object
}

/// A 4 × 4 map, Gaia and a player with a unit, two buildings, a unit inside one, an object
/// without its master and a ghost.
fn placed() -> Spatial {
    let mut unit = at(combat_object(10, 83), 1.5, 1.5);
    combat_mut(&mut unit).rge_combat_object.capture_flag = 2;
    let mut standing = at(building_object(11, 70), 2.5, 2.5);
    let tribe_building_object = building_mut(&mut standing);
    tribe_building_object.built = 1;
    tribe_building_object.mupt_linked_object_references_linked_owner_id = -1;
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
    objects.doppleganger_objects.object_kind_list =
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
    spatial(&save, &TerrainTable::default(), layer()).unwrap()
}

#[test]
fn unit_takes_its_masters_footprint_and_obstruction() {
    let out = placed();
    let unit = out.get(object_id(10)).unwrap();
    assert_eq!(
        (unit.player, unit.master, unit.class),
        (PlayerId::new(1), VILLAGER_MALE, ClassId::named(6))
    );
    assert!(unit.tall);
    let obstruction = unit.obstruction.unwrap();
    assert_eq!(
        (obstruction.kind, obstruction.class),
        (ObstructionKind::KIND_2, ObstructionClass::BUILDING)
    );
    assert_eq!(out.tile_list(Tile::new(1, 1)), [object_id(10)]);
    let seer = out.sight.as_ref().unwrap().seer(object_id(10)).unwrap();
    assert_eq!(
        (seer.kind, seer.capture, seer.umv),
        (MasterKind::COMBAT, CaptureKind::Again, 0)
    );
    assert!(seer.explored && seer.building.is_none());
    assert_eq!(
        out.unit_diplomacy
            .get(PlayerId::new(1))
            .map(ByPlayer::values),
        Some(
            &[
                UnitDiplomacy::from(UnitRelation::Gaia),
                UnitDiplomacy::from(UnitRelation::Own)
            ][..]
        )
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
    assert_eq!(standing.building, Some(BuildingSight::Built));
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
