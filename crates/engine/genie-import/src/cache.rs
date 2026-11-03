//! The cached import (FD-046-D18, D59): the `.dat`'s tables ([`MasterTable`]) in postcard's
//! binary form, and behind the `fs` feature a cache directory of them by [`Key`]. A write goes
//! to a temporary file of its own and is renamed into place, so parallel instances share a
//! directory.

use crate::dat::MasterTable;
#[cfg(feature = "fs")]
use crate::key::Key;
use genie_model::GenieError;
#[cfg(feature = "fs")]
use std::fs::{create_dir_all, read, remove_file, rename, write};
#[cfg(feature = "fs")]
use std::io::{Error as IoError, ErrorKind};
#[cfg(feature = "fs")]
use std::path::Path;
#[cfg(feature = "fs")]
use std::process;
#[cfg(feature = "fs")]
use std::sync::atomic::{AtomicU64, Ordering};

/// The tables in the cache's form.
///
/// # Errors
///
/// A value postcard cannot write (none of the tables' types holds one).
pub fn encode(table: &MasterTable) -> Result<Vec<u8>, GenieError> {
    postcard::to_allocvec(table)
        .map_err(|e| GenieError::invalid_input("a table postcard cannot write").with_source(e))
}

/// The tables of the cache's form `bytes`.
///
/// # Errors
///
/// Bytes that are not a table's form: cut off, of another layout, or a value a type refuses.
pub fn decode(bytes: &[u8]) -> Result<MasterTable, GenieError> {
    postcard::from_bytes(bytes)
        .map_err(|e| GenieError::invalid_input("cached tables that do not read").with_source(e))
}

/// The tables cached in `dir` under `key`; None when the directory holds none for it.
///
/// # Errors
///
/// A cache file that does not open or read, or does not decode ([`decode`]).
#[cfg(feature = "fs")]
pub fn read_cached(dir: &Path, key: &Key) -> Result<Option<MasterTable>, GenieError> {
    let path = dir.join(key.hex());
    match read(&path) {
        Ok(bytes) => decode(&bytes)
            .map(Some)
            .map_err(|e| e.with("path", path.display())),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(GenieError::io("a cache file that does not read")
            .with("path", path.display())
            .with_source(e)),
    }
}

/// Each write's own temporary file in a process.
#[cfg(feature = "fs")]
static WRITES: AtomicU64 = AtomicU64::new(0);

/// Caches `table` in `dir` under `key`: written to `<key>.tmp-<pid>-<n>`, then renamed to
/// `<key>`, so a reader finds the whole file or none. The directory is made when missing.
///
/// # Errors
///
/// The tables do not encode ([`encode`]), or the directory or the file cannot be made.
#[cfg(feature = "fs")]
pub fn write_cached(dir: &Path, key: &Key, table: &MasterTable) -> Result<(), GenieError> {
    let bytes = encode(table)?;
    let io = |reason: &'static str, path: &Path, e: IoError| {
        GenieError::io(reason)
            .with("path", path.display())
            .with_source(e)
    };
    create_dir_all(dir).map_err(|e| io("a cache directory that cannot be made", dir, e))?;
    let n = WRITES.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!("{}.tmp-{}-{n}", key.hex(), process::id()));
    write(&tmp, &bytes).map_err(|e| io("a cache file that cannot be written", &tmp, e))?;
    let path = dir.join(key.hex());
    rename(&tmp, &path).map_err(|e| {
        // the temporary file goes too; a failed removal leaves only a stray file
        let _ = remove_file(&tmp);
        io("a cache file that cannot be put in place", &path, e)
    })
}

#[cfg(test)]
mod tests;
