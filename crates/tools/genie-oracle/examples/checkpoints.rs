//! A recording's run from its header, nothing forced, against the recording's own checkpoints
//! (every 500 updates): per checkpoint, each player's words of the 11-word block that differ, as
//! `(word, engine, game)`, or `ok`.
//!
//! `cargo run --release -p genie-oracle --example checkpoints -- REC DAT UPDATES`
//!
//! With `RUST_LOG` set, the engine's log goes to stderr as text lines
//! (`RUST_LOG=genie_core::unsupported=warn` …, docs/DEBUGGING.md).

use genie_oracle::install::read as install_read;
use genie_oracle::views::checkpoints;
use std::env::{args, var};
use std::error::Error;
use std::fs::read;

mod support;

fn main() -> Result<(), Box<dyn Error>> {
    support::install(var("RUST_LOG").ok().as_deref())?;
    let args: Vec<String> = args().collect();
    let [_, rec, dat, updates] = &args[..] else {
        return Err("usage: checkpoints REC DAT UPDATES".into());
    };
    let recording = read(rec)?;
    let masters = install_read(dat.as_ref())?;
    print!("{}", checkpoints(&recording, &masters, updates.parse()?)?);
    Ok(())
}
