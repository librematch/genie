//! The tests' scratch directories.

use std::env::temp_dir;
use std::fs::remove_dir_all;
use std::path::PathBuf;
use std::process;

/// A directory of its own for `test` under the system's temporary one, not there yet: what
/// writes to it makes it.
pub fn scratch(test: &str) -> PathBuf {
    let dir = temp_dir().join(format!("genie-import-{test}-{}", process::id()));
    let _ = remove_dir_all(&dir);
    dir
}
