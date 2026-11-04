use crate::cache::{decode, encode, read_cached, write_cached};
use crate::dat::{DefaultAction, MasterTable};
use crate::key::key;
use crate::scratch::scratch;
use genie_model::objects::FacetCount;
use genie_model::objects::SpeedMultiplier;
use genie_model::objects::SpriteFlags;
use genie_model::objects::SpriteId;
use genie_model::objects::lifecycle::MasterData;
use genie_model::objects::masters::{ClassId, Master, MasterId, MasterKind};
use genie_model::players::CivId;
use genie_model::technology::{Effect, EffectCommand, EffectId, Effects, TechDef};
use std::fs::{read_dir, remove_dir_all};

/// A table of one master of each type (the layers its type holds; the static one without a
/// class, −1), two technologies, one effect and one sprite.
fn small_table() -> MasterTable {
    let kinds = [
        MasterKind::STATIC,
        MasterKind::TREE,
        MasterKind::ANIMATED,
        MasterKind::DOPPELGANGER,
        MasterKind::MOVING,
        MasterKind::ACTION,
        MasterKind::RGE_COMBAT,
        MasterKind::MISSILE,
        MasterKind::COMBAT,
        MasterKind::BUILDING,
    ];
    let civ = CivId::new(1);
    let id = |i: usize| MasterId::named(i16::try_from(i).unwrap());
    let mut master_table = MasterTable::from_rows(
        (0..kinds.len()).map(|i| ((civ, id(i)), MasterData::default(), DefaultAction::None)),
    );
    for (index, &kind) in kinds.iter().enumerate() {
        let class = if index == 0 {
            ClassId::of_master(-1)
        } else {
            ClassId::named(i16::try_from(index).unwrap())
        };
        let master = Master {
            id: id(index),
            class,
            ..Master::of_kind(kind)
        };
        master_table = master_table.with_work(civ, id(index), master);
    }
    master_table.techs = vec![
        TechDef::default(),
        TechDef {
            effect_id: EffectId::of_data(0),
            ..TechDef::default()
        },
    ];
    master_table.effects = Effects::new(vec![Effect {
        commands: vec![EffectCommand::of_data(1, 3, 1, -1, 20.0).unwrap()],
    }]);
    master_table.with_sprite(
        SpriteId::of_data(5).unwrap(),
        SpeedMultiplier::of_data(1.5),
        FacetCount::of_data(8),
        SpriteFlags::empty(),
    )
}

/// The tables' form decodes to the same tables: every layer, the class −1, the technologies,
/// the effect and the sprite.
#[test]
fn tables_encode_and_decode_to_themselves() {
    let table = small_table();
    assert_eq!(decode(&encode(&table).unwrap()).unwrap(), table);
}

/// Cut-off bytes are an error, not tables.
#[test]
fn cut_off_bytes_do_not_decode() {
    let bytes = encode(&small_table()).unwrap();
    assert!(decode(&bytes[..bytes.len() / 2]).is_err());
}

/// The cache returns what it wrote under its key and nothing under another; no temporary file
/// stays behind.
#[test]
fn the_cache_returns_what_it_wrote_and_nothing_for_another_key() {
    let dir = scratch("cache");
    let table = small_table();
    let k = key(&[("a", b"1")], &[0; 32]);
    write_cached(&dir, &k, &table).unwrap();
    assert_eq!(read_cached(&dir, &k).unwrap(), Some(table));
    let other = key(&[("a", b"2")], &[0; 32]);
    assert_eq!(read_cached(&dir, &other).unwrap(), None);
    assert!(read_dir(&dir).unwrap().all(|dir_entry| {
        !dir_entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp-")
    }));
    remove_dir_all(&dir).unwrap();
}

/// A second write under the same key replaces the first.
#[test]
fn second_writes_replace_the_first() {
    let dir = scratch("rewrite");
    let key = key(&[], &[0; 32]);
    write_cached(&dir, &key, &MasterTable::default()).unwrap();
    write_cached(&dir, &key, &small_table()).unwrap();
    assert_eq!(read_cached(&dir, &key).unwrap(), Some(small_table()));
    remove_dir_all(&dir).unwrap();
}
