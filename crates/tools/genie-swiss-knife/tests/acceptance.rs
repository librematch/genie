//! Acceptance test: runs the application as a subprocess and asserts its
//! output for given argument combinations matches what is expected.
//!
//! For more information, see:
//! <https://docs.rs/abscissa_core/latest/abscissa_core/testing/index.html>

#![forbid(unsafe_code)]
#![warn(
    missing_docs,
    rust_2018_idioms,
    trivial_casts,
    unused_lifetimes,
    unused_qualifications
)]

use abscissa_core::testing::prelude::*;
use std::{io::Read, sync::LazyLock};

/// Runs the built `gsk` instead of `cargo run`, so no cargo runs inside `cargo test`.
///
/// The [`LazyLock`] static makes every test take the runner's mutex, so the runs do not overlap
/// while `cargo test` runs the tests in parallel.
pub static RUNNER: LazyLock<CmdRunner> = LazyLock::new(|| {
    let mut runner = CmdRunner::new(env!("CARGO_BIN_EXE_gsk"));
    // `exclusive` returns the runner again, for chaining.
    let _ = runner.exclusive();
    runner
});

/// gsk has no subcommands yet: without one it must fail, not pretend to work.
#[test]
// False positive: `cmd` holds the runner's exclusive lock, which the consuming `wait` passes on.
#[allow(clippy::significant_drop_tightening)]
fn without_subcommand_gsk_fails() {
    let mut runner = RUNNER.clone();
    let mut cmd = runner.capture_stderr().run();
    let mut stderr = String::new();
    // The byte count is `stderr.len()`.
    let _ = cmd.stderr().read_to_string(&mut stderr).unwrap();
    assert!(stderr.contains("Usage: gsk"), "stderr: {stderr}");
    cmd.wait().unwrap().expect_code(2);
}

/// The version line names the command `gsk`, not the crate.
#[test]
// False positive: `cmd` holds the runner's exclusive lock, which the consuming `wait` passes on.
#[allow(clippy::significant_drop_tightening)]
fn version_names_the_command_gsk() {
    let mut runner = RUNNER.clone();
    let mut cmd = runner.arg("--version").capture_stdout().run();
    cmd.stdout()
        .expect_line(concat!("gsk ", env!("CARGO_PKG_VERSION")));
    cmd.wait().unwrap().expect_success();
}
