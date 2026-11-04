use crate::errors::{bad_pool_dump, bad_registry_dump};
use crate::save_objects::{ghost_pool_from_dump, recycle_pools_from_dump, registry_from_dump};
use crate::test_ids::object_id;
use genie_core::players::PlayerId;
use genie_core::vision::VisionBits;
use genie_core::vision::registry::SLOTS;

/// Two pools: the missiles (type 60), the next to come back first, and the ghosts (type 25).
const POOLS: &str = "pool 60 2\n101 1\n102 0\n\npool 25 2\n201 0\n202 0\n";

#[test]
fn the_missile_pool_comes_back_in_the_slice_order() {
    assert_eq!(
        recycle_pools_from_dump(POOLS).unwrap(),
        [(object_id(102), false), (object_id(101), true)]
    );
    assert_eq!(
        ghost_pool_from_dump(POOLS).unwrap(),
        [object_id(202), object_id(201)]
    );
}

/// An object line before a pool line, a word that is not a number or a line of another shape
/// is out of shape.
#[test]
fn pool_dump_out_of_shape_is_an_error() {
    for text in [
        "101 1\n",
        "pool sixty 1\n",
        "pool 60 1\nx 1\n",
        "pool 60 1\n101 2 3\n",
        "pool 60 1\n101 one\n",
    ] {
        assert!(
            recycle_pools_from_dump(text).is_err_and(|text_error| text_error == bad_pool_dump()),
            "{text:?}"
        );
    }
    for text in [
        "201 0\n",
        "pool ghosts 1\n",
        "pool 25 1\nx 0\n",
        "pool 25 1\n1\n",
    ] {
        assert!(
            ghost_pool_from_dump(text).is_err_and(|error| error == bad_pool_dump()),
            "{text:?}"
        );
    }
}

/// A mask line: the id, the mask in hex and the record's slot in each player's list (-1 for
/// none).
fn mask_line(id: i32, slots: &[i64]) -> String {
    let words: Vec<String> = (0..SLOTS)
        .map(|index| slots.get(index).copied().unwrap_or(-1).to_string())
        .collect();
    format!("mask {id} 3 {}", words.join(" "))
}

#[test]
fn registry_dump_reads_its_lists_and_masks() {
    let one = 1.0_f32.to_bits();
    let text = format!(
        "registry 2 3\nlist 1 2 1 4\nrec 77 {one:x} 1 {one:x} {:x} 0 0\n{}\n",
        2.0_f32.to_bits(),
        mask_line(77, &[-1, 0])
    );
    let registry = registry_from_dump(&text).unwrap();
    assert_eq!((registry.players, registry.classes), (2, 3));
    let record = registry.lists[1][2][0];
    assert_eq!((record.id, record.owner), (object_id(77), PlayerId::new(1)));
    assert_eq!(record.distance.get().to_bits(), one);
    assert_eq!(record.at.y().to_bits(), 2.0_f32.to_bits());
    let entry = registry.entries.get(object_id(77)).unwrap();
    assert_eq!(entry.mask, VisionBits::from_bits_retain(3));
    assert_eq!(entry.slots[..2], [None, Some(0)]);
}

#[test]
fn registry_dump_out_of_shape_is_an_error() {
    let rec = "rec 77 0 1 0 0 0 0";
    for text in [
        String::new(),
        "registry 2\n".to_owned(),
        "registry two 3\n".to_owned(),
        "registry 2 3\nlist 2 0 0 0\n".to_owned(),
        "registry 2 3\nlist 0 3 0 0\n".to_owned(),
        format!("registry 2 3\n{rec}\n"),
        "registry 2 3\nlist 0 0 0 0\nrec 77 zz 1 0 0 0 0\n".to_owned(),
        "registry 2 3\nlist 0 0 0 0\nrec x 0 1 0 0 0 0\n".to_owned(),
        format!(
            "registry 2 3\n{}\n",
            mask_line(77, &[]).replace(" 3 ", " zz ")
        ),
        "registry 2 3\nmask 77 3\n".to_owned(),
        "registry 2 3\nwhat\n".to_owned(),
    ] {
        assert!(
            registry_from_dump(&text).is_err_and(|error| error == bad_registry_dump()),
            "{text:?}"
        );
    }
}
