//! The key of a cached import (FD-046-D18, D60): the SHA-256 over the files the import reads
//! (each its name and its content's hash) and the tables' layout hash, so a changed `.dat`, a
//! changed table beside it or a changed type of the tables misses the cache.

use sha2::Digest;
use sha2::Sha256;
use std::fmt::Write;

/// A cached import's key, [`key`]'s SHA-256.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key([u8; 32]);

impl Key {
    /// The key as 64 lowercase hexadecimal digits: the cache file's name.
    #[must_use]
    pub fn hex(&self) -> String {
        let mut out = String::with_capacity(64);
        for b in self.0 {
            // writing to a `String` cannot fail
            let _ = write!(out, "{b:02x}");
        }
        out
    }
}

/// The key of an import of `files` (each its name and its content) into tables of the layout
/// hash `layout`: each part changes it. A count and a name enter with their lengths, so no two
/// inputs run together.
#[must_use]
pub fn key(files: &[(&str, &[u8])], layout: &[u8; 32]) -> Key {
    let mut h = Sha256::new();
    h.update((files.len() as u64).to_le_bytes());
    for (name, content) in files {
        h.update((name.len() as u64).to_le_bytes());
        h.update(name.as_bytes());
        h.update(Sha256::digest(content));
    }
    h.update(layout);
    Key(h.finalize().into())
}

#[cfg(test)]
mod tests;
