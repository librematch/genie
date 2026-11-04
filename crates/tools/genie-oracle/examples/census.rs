//! A recording played from its header by the genie engine, nothing forced, against the
//! checkpoints the recording holds: each player's first update the engine does not model yet and
//! first checkpoint that differs.
//!
//! `cargo run --release -p genie-oracle --example census -- REC DAT [UPDATES]`
//!
//! REC is an `.aoe2record` of build 158041, DAT the `empires2_x2_p1.dat` of that build's game
//! (with `dropsites.json` beside it, if there is one).
//!
//! With `RUST_LOG` set, the engine's log goes to stderr as text lines
//! (`RUST_LOG=genie_core::unsupported=warn` …, docs/DEBUGGING.md).

mod cli;
mod support;

use crate::cli::{number, usage};
use genie_core::GenieResult;
use genie_oracle::census::census_run;
use genie_oracle::install::read as install_read;
use std::env::{args, var};
use std::fs::read;

fn main() -> GenieResult<()> {
    support::install(var("RUST_LOG").ok().as_deref())?;
    let args: Vec<String> = args().collect();
    let (rec, dat, limit) = match &args[1..] {
        [rec, dat] => (rec, dat, None),
        [rec, dat, updates] => (rec, dat, Some(number(updates)?)),
        _ => return Err(usage("usage: census REC DAT [UPDATES]")),
    };
    let recording = read(rec)?;
    let masters = install_read(dat.as_ref())?;
    let run = census_run(&recording, Some(&masters), limit)?;
    println!(
        "{} updates ({:?}), {} checkpoints",
        run.updates, run.end, run.checkpoints
    );
    for (index, player) in run.players.iter().enumerate() {
        let stop = player.stop.as_ref().map_or_else(
            || "no stop".to_owned(),
            |census_stop| {
                format!(
                    "stops at update {}: {}",
                    census_stop.update, census_stop.reason
                )
            },
        );
        let mismatch = player.mismatch.as_ref().map_or_else(
            || "no checkpoint differs".to_owned(),
            |census_mismatch| {
                format!(
                    "the checkpoint after {} syncs differs in words {:?}",
                    census_mismatch.syncs, census_mismatch.words
                )
            },
        );
        println!(
            "player {index}: {stop}; {} checkpoints hold; {mismatch}",
            player.checkpoints_held
        );
    }
    Ok(())
}
