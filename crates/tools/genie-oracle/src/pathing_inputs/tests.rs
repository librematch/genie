use crate::pathing_inputs::{mesh_from_dump, mesh_owners_from_dump};
use crate::test_ids::object_id;
use genie_core::pathing::{FixedDistance, FixedPoint, FixedRect};
use genie_core::pathing::{ShapeId, ShapeSide};

/// A mesh 2 tiles wide with one shape, its side 1's contact and the tile (1, 0) holding it.
const MESH: &str = "\
mesh width 2 shapes 1 free 0x10
shape 0 1 2 3 4
  side 1 5 2 a b c 1 d
tile 1 0 0
owner 900 0
";

#[test]
fn mesh_dump_reads_its_shapes_contacts_and_tiles() {
    let mesh = mesh_from_dump(MESH).unwrap();
    assert_eq!((mesh.width, mesh.free), (2, Some(ShapeId::new(0x10))));
    assert_eq!(mesh.shapes, [FixedRect::new(1, 2, 3, 4)]);
    let contact = &mesh.sides[0][1][0];
    assert_eq!(
        (contact.other, contact.side),
        (ShapeId::new(5), ShapeSide::HighY)
    );
    assert_eq!((contact.at, contact.along), (FixedPoint::new(10, 11), 12));
    assert_eq!(
        (contact.at_side_start, contact.half),
        (true, FixedDistance::new(13))
    );
    assert_eq!(mesh.tiles[1], [ShapeId::new(0)]);
    assert_eq!(
        mesh_owners_from_dump(MESH),
        [(object_id(900), ShapeId::new(0))]
    );
    let none = mesh_from_dump("mesh width 2 shapes 0 free 0xffffffff\n").unwrap();
    assert_eq!(none.free, None);
    // a contact's side past 3 is a broken dump
    assert!(mesh_from_dump(&MESH.replace("side 1 5 2", "side 1 5 4")).is_none());
    // the flag is its word's byte (the upper bytes the stack's), 0 or 1
    let stacked = mesh_from_dump(&MESH.replace(" c 1 d", " c 7f01 d")).unwrap();
    assert!(stacked.sides[0][1][0].at_side_start);
    assert!(mesh_from_dump(&MESH.replace(" c 1 d", " c 2 d")).is_none());
}

/// Without its mesh line, or with a header out of shape, a dump holds no mesh.
#[test]
fn mesh_dump_without_its_mesh_line_holds_none() {
    assert!(mesh_from_dump("shape 0 1 2 3 4\n").is_none());
    assert!(mesh_from_dump("side 1 0 0 0 0 0 0 0\n").is_none());
    assert!(mesh_from_dump("tile 0 0 1\n").is_none());
    assert!(mesh_from_dump("mesh width x shapes 0 free 0\n").is_none());
    assert!(mesh_from_dump("mesh width 2 shapes 0 free zz\n").is_none());
    assert!(mesh_from_dump("mesh width 2\n").is_none());
    let mesh = "mesh width 2 shapes 0 free 0\n";
    assert!(mesh_from_dump(&format!("{mesh}side x 0 0 0 0 0 0 0\n")).is_none());
    assert!(mesh_from_dump(&format!("{mesh}side 0 0 0 0 0 0 0 0\n")).is_none());
    assert!(mesh_from_dump(&format!("{mesh}tile x 0\n")).is_none());
    assert!(mesh_from_dump(&format!("{mesh}tile 0 y\n")).is_none());
    assert!(mesh_from_dump(&format!("{mesh}other\n")).is_some());
}

mod unit_terrain {
    use crate::fixtures::{
        combat_master, combat_object, gaia, header, holding_masters, human, master_static_mut,
        recording,
    };
    use crate::pathing_inputs::unit_terrain;
    use genie_core::map::RestrictionId;

    /// A save of a player with unit 7 (master 83, x radius 0.4, terrain restriction 7), unit 8
    /// of a master the save does not hold and unit 9 of a master without a restriction.
    fn save() -> Vec<u8> {
        let mut walker = combat_master(83);
        let s = master_static_mut(&mut walker);
        s.radius_x = 0.4;
        s.terrain_restriction_mask_id = 7;
        let mut free = combat_master(84);
        master_static_mut(&mut free).terrain_restriction_mask_id = -1;
        let player = holding_masters(
            human(vec![
                combat_object(7, 83),
                combat_object(8, 999),
                combat_object(9, 84),
            ]),
            vec![walker, free],
        );
        recording(&header(vec![gaia(Vec::new()), player]), &[])
    }

    #[test]
    fn units_master_gives_its_radius_and_terrain_restriction() {
        let save = save();
        let (radius, restriction) = unit_terrain(&save, 7).unwrap().unwrap();
        assert_eq!(
            (radius.to_bits(), restriction),
            (0.4_f32.to_bits(), RestrictionId::named(7))
        );
    }

    /// No answer for an object the save does not hold, one without its master or a master
    /// without a restriction; a save that does not read is an error.
    #[test]
    fn otherwise_there_is_no_answer() {
        let save = save();
        assert_eq!(unit_terrain(&save, 8).unwrap(), None);
        assert_eq!(unit_terrain(&save, 9).unwrap(), None);
        assert_eq!(unit_terrain(&save, 10).unwrap(), None);
        assert!(unit_terrain(&[], 7).is_err());
    }
}
