//! The `.dat`'s tables from a game's files (FD-046-D18, D19, D60): [`load_bytes`] imports the
//! `.dat`'s bytes and the tables beside it ([`Beside`]) without the file system, as a browser
//! host calls it; behind the `fs` feature [`read`] reads them from the `.dat`'s path and
//! [`load`] keeps the import in a cache directory ([`crate::cache`]) under its files' [`key`].

#[cfg(feature = "fs")]
use crate::cache::{read_cached, write_cached};
use crate::dat::MasterTable;
#[cfg(feature = "fs")]
use crate::key::{Key, key};
use crate::tables::{drop_sites, linked_techs, unit_lines};
use genie_dat::de::DatFile;
#[cfg(feature = "fs")]
use genie_model::GenieError;
use genie_model::GenieResult;
use schemars::schema_for;
use serde_json::Value;
use sha2::{Digest, Sha256};
#[cfg(feature = "fs")]
use std::fs::read as read_file;
#[cfg(feature = "fs")]
use std::io::ErrorKind;
#[cfg(feature = "fs")]
use std::path::Path;

/// The tables beside the `.dat` the import reads, each where there is one.
pub const BESIDE: [&str; 3] = ["dropsites.json", "linkedTechs.json", "unitlines.json"];

/// The texts of the tables beside the `.dat` ([`BESIDE`]), each where there is one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Beside<'a> {
    /// `dropsites.json`'s ([`drop_sites`]).
    pub drop_sites: Option<&'a str>,
    /// `linkedTechs.json`'s ([`linked_techs`]).
    pub linked_techs: Option<&'a str>,
    /// `unitlines.json`'s ([`unit_lines`]).
    pub unit_lines: Option<&'a str>,
}

/// The tables of the `.dat` bytes `dat`, compressed as the game ships them, with the tables
/// `beside` it.
///
/// # Errors
///
/// Bytes genie-dat does not read as a `.dat`, a value the import refuses, or a table beside it
/// out of shape (the error names its file).
pub fn load_bytes(dat: &[u8], beside: Beside<'_>) -> GenieResult<MasterTable> {
    with_beside(MasterTable::from_dat(&DatFile::read_from(dat)?)?, beside)
}

/// `table` with the tables `beside` the `.dat` in place of its own, each that is there.
fn with_beside(mut table: MasterTable, beside: Beside<'_>) -> GenieResult<MasterTable> {
    if let Some(json) = beside.drop_sites {
        table.drop_sites = drop_sites(json).map_err(|e| e.with("file", BESIDE[0]))?;
    }
    if let Some(json) = beside.linked_techs {
        table.linked_techs =
            linked_techs(json).map_err(|json_error| json_error.with("file", BESIDE[1]))?;
    }
    if let Some(json) = beside.unit_lines {
        table.unit_lines = unit_lines(json).map_err(|error| error.with("file", BESIDE[2]))?;
    }
    Ok(table)
}

/// The SHA-256 of the tables' layout: [`MasterTable`]'s JSON Schema without its descriptions,
/// so that a cache file of tables of another layout is never read and a changed doc comment
/// keeps the cache.
#[must_use]
pub fn layout() -> [u8; 32] {
    let mut schema = schema_for!(MasterTable).to_value();
    drop_descriptions(&mut schema);
    Sha256::digest(schema.to_string()).into()
}

/// `schema` without its descriptions: every `description` key whose value is a text (a
/// property of that name keeps its schema, an object).
fn drop_descriptions(schema: &mut Value) {
    match schema {
        Value::Object(map) => {
            if map.get("description").is_some_and(Value::is_string) {
                let _ = map.remove("description");
            }
            map.values_mut().for_each(drop_descriptions);
        }
        Value::Array(items) => items.iter_mut().for_each(drop_descriptions),
        _ => {}
    }
}

/// The files the import reads: the `.dat`'s bytes and the texts beside it.
#[cfg(feature = "fs")]
#[derive(Debug)]
struct Files {
    dat: Vec<u8>,
    /// Each of [`BESIDE`]'s texts, where there is one.
    beside: [Option<String>; 3],
}

#[cfg(feature = "fs")]
impl Files {
    /// The `.dat` at `dat` and the tables beside it.
    fn read(dat: &Path) -> GenieResult<Self> {
        let bytes = read_file(dat).map_err(|dat_error| {
            GenieError::io("a .dat that does not read")
                .with("path", dat.display())
                .with_source(dat_error)
        })?;
        let mut beside = [None, None, None];
        for (text, name) in beside.iter_mut().zip(BESIDE) {
            let path = dat.with_file_name(name);
            match read_file(&path) {
                Ok(b) => {
                    *text = Some(String::from_utf8(b).map_err(|from_utf8_error| {
                        GenieError::invalid_input("a table beside the .dat that is not UTF-8")
                            .with("file", name)
                            .with_source(from_utf8_error)
                    })?);
                }
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(GenieError::io("a table beside the .dat that does not read")
                        .with("path", path.display())
                        .with_source(error));
                }
            }
        }
        Ok(Self { dat: bytes, beside })
    }

    /// The import of the files ([`load_bytes`]).
    fn import(&self) -> GenieResult<MasterTable> {
        let [drop_sites, linked_techs, unit_lines] = self.beside.each_ref().map(Option::as_deref);
        load_bytes(
            &self.dat,
            Beside {
                drop_sites,
                linked_techs,
                unit_lines,
            },
        )
    }

    /// The cache's key of the files: the `.dat` and each table beside it that is there, under
    /// its name, and the tables' [`layout`].
    fn key(&self) -> Key {
        let mut parts = vec![("empires2_x2_p1.dat", self.dat.as_slice())];
        for (text, name) in self.beside.iter().zip(BESIDE) {
            if let Some(t) = text {
                parts.push((name, t.as_bytes()));
            }
        }
        key(&parts, &layout())
    }
}

/// The tables of the `.dat` at `dat` (an `empires2_x2_p1.dat`, below an install's
/// `resources/_common/dat/` or a mod's) with the tables beside it, imported without a cache.
///
/// # Errors
///
/// A file that does not read, or the errors of [`load_bytes`].
#[cfg(feature = "fs")]
pub fn read(dat: &Path) -> GenieResult<MasterTable> {
    Files::read(dat)?.import()
}

/// The tables of the `.dat` at `dat` with the tables beside it, from `cache_dir` when it holds
/// them for these files' contents, else imported ([`read`]) and cached there.
///
/// # Errors
///
/// A file that does not read (the error names its path), the errors of [`load_bytes`], or a
/// cache file that does not read, decode or write.
#[cfg(feature = "fs")]
pub fn load(dat: &Path, cache_dir: &Path) -> GenieResult<MasterTable> {
    cached(&Files::read(dat)?, cache_dir, Files::import)
}

/// The tables of `files` from `dir`, else `import`ed and cached there.
#[cfg(feature = "fs")]
fn cached(
    files: &Files,
    dir: &Path,
    import: impl FnOnce(&Files) -> GenieResult<MasterTable>,
) -> GenieResult<MasterTable> {
    let key = files.key();
    if let Some(table) = read_cached(dir, &key)? {
        return Ok(table);
    }
    let table = import(files)?;
    write_cached(dir, &key, &table)?;
    Ok(table)
}

#[cfg(test)]
mod tests;
