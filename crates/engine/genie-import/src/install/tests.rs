use crate::dat::MasterTable;
use crate::install::{BESIDE, with_beside};
use crate::install::{Beside, Files, cached, drop_descriptions, layout, load, load_bytes, read};
use crate::scratch::scratch;
use genie_model::ErrorKind;
use genie_model::technology::TechDef;
use serde_json::json;
use std::cell::Cell;
use std::fs::{create_dir_all, read_dir, remove_dir_all, write};
use std::path::Path;

/// A table the stand-in import returns: the default one with a technology.
fn imported() -> MasterTable {
    let mut t = MasterTable::default();
    t.techs = vec![TechDef::default()];
    t
}

/// The files of a `.dat` of `bytes` without the tables beside it.
fn files(bytes: &[u8]) -> Files {
    Files {
        dat: bytes.to_vec(),
        beside: [None, None, None],
    }
}

/// The cache files in `dir`.
fn cache_files(dir: &Path) -> usize {
    read_dir(dir).map_or(0, Iterator::count)
}

/// The first load imports and caches; the second of the same files reads the cache and does
/// not import.
#[test]
fn loads_import_once_then_read_the_cache() {
    let dir = scratch("install-once");
    let imports = Cell::new(0);
    let import = |_: &Files| {
        imports.set(imports.get() + 1);
        Ok(imported())
    };
    assert_eq!(cached(&files(b"dat"), &dir, import).unwrap(), imported());
    assert_eq!(cached(&files(b"dat"), &dir, import).unwrap(), imported());
    assert_eq!((imports.get(), cache_files(&dir)), (1, 1));
    remove_dir_all(&dir).unwrap();
}

/// A `.dat` with one byte changed, or a table beside it more, has a key of its own: both
/// tables are imported and cached side by side.
#[test]
fn modded_files_get_keys_of_their_own() {
    let dir = scratch("install-modded");
    let imports = Cell::new(0);
    let import = |_: &Files| {
        imports.set(imports.get() + 1);
        Ok(imported())
    };
    let mut beside = files(b"dat");
    beside.beside[2] = Some(String::new());
    for f in [files(b"dat"), files(b"dau"), beside] {
        let _ = cached(&f, &dir, import).unwrap();
    }
    assert_eq!((imports.get(), cache_files(&dir)), (3, 3));
    remove_dir_all(&dir).unwrap();
}

/// A failed import caches nothing.
#[test]
fn failed_imports_cache_nothing() {
    let dir = scratch("install-failed");
    let error = cached(&files(b"dat"), &dir, |_| {
        Err(genie_model::GenieError::invalid_input("a test import"))
    })
    .unwrap_err();
    assert_eq!(
        (error.kind(), cache_files(&dir)),
        (ErrorKind::InvalidInput, 0)
    );
}

/// A missing `.dat` fails with its path; one that is not a compressed `.dat` fails to inflate.
#[test]
fn missing_or_foreign_dats_fail_with_their_path_or_their_reason() {
    let dir = scratch("install-no-dat");
    create_dir_all(&dir).unwrap();
    let dat = dir.join("empires2_x2_p1.dat");
    for error in [
        read(&dat).unwrap_err(),
        load(&dat, &dir.join("cache")).unwrap_err(),
    ] {
        assert_eq!(error.kind(), ErrorKind::Io);
        assert!(
            error
                .context()
                .any(|(k, v)| k == "path" && v == dat.display().to_string())
        );
    }
    write(&dat, b"not a dat").unwrap();
    assert_eq!(read(&dat).unwrap_err().kind(), ErrorKind::Io);
    assert_eq!(cache_files(&dir.join("cache")), 0);
    remove_dir_all(&dir).unwrap();
}

/// The files read are the `.dat` and each table beside it that is there.
#[test]
fn the_files_are_the_dat_and_the_tables_beside_it_that_are_there() {
    let dir = scratch("install-files");
    create_dir_all(&dir).unwrap();
    let dat = dir.join("empires2_x2_p1.dat");
    write(&dat, b"dat").unwrap();
    write(dir.join(BESIDE[1]), "{}").unwrap();
    let f = Files::read(&dat).unwrap();
    assert_eq!(
        (f.dat.as_slice(), f.beside),
        (&b"dat"[..], [None, Some("{}".to_owned()), None])
    );
    remove_dir_all(&dir).unwrap();
}

/// The tables beside the `.dat` replace the table's own where they are; one out of shape
/// fails with its file's name, and none leaves the table as it was.
#[test]
fn the_tables_beside_replace_their_own_where_they_are() {
    let table = with_beside(MasterTable::default(), Beside::default()).unwrap();
    assert_eq!(table, MasterTable::default());
    let good = Beside {
        drop_sites: Some(r#"{"drop_site_list": []}"#),
        linked_techs: Some(r#"{"LinkedTechs": []}"#),
        unit_lines: Some(r#"{"UnitLines": []}"#),
    };
    assert_eq!(
        with_beside(MasterTable::default(), good).unwrap(),
        MasterTable::default()
    );
    let broken = [
        Beside {
            drop_sites: Some("["),
            ..good
        },
        Beside {
            linked_techs: Some("["),
            ..good
        },
        Beside {
            unit_lines: Some("["),
            ..good
        },
    ];
    for (beside, file) in broken.into_iter().zip(BESIDE) {
        let error = with_beside(MasterTable::default(), beside).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput);
        assert!(
            error.context().any(|(k, v)| k == "file" && v == file),
            "{file}"
        );
    }
}

/// Bytes that are not a compressed `.dat` do not import.
#[test]
fn foreign_bytes_do_not_import() {
    assert!(load_bytes(b"not a dat", Beside::default()).is_err());
}

/// The layout hash is the same on each call; the descriptions go, a property named so stays.
#[test]
fn the_layout_hash_drops_the_descriptions_only() {
    assert_eq!(layout(), layout());
    let mut schema = json!({
        "description": "a doc",
        "properties": {"description": {"type": "string", "description": "its doc"}},
        "items": [{"description": "x", "type": "integer"}],
    });
    drop_descriptions(&mut schema);
    assert_eq!(
        schema,
        json!({
            "properties": {"description": {"type": "string"}},
            "items": [{"type": "integer"}],
        })
    );
}
