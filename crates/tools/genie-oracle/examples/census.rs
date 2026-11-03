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

mod support;

use genie_oracle::census::census_run;
use genie_oracle::install;
use std::env::{args, var};
use std::error::Error;
use std::fs::read;

fn main() -> Result<(), Box<dyn Error>> {
    support::install(var("RUST_LOG").ok().as_deref())?;
    let args: Vec<String> = args().collect();
    let (rec, dat, limit) = match &args[1..] {
        [rec, dat] => (rec, dat, None),
        [rec, dat, updates] => (rec, dat, Some(updates.parse()?)),
        _ => return Err("usage: census REC DAT [UPDATES]".into()),
    };
    let recording = read(rec)?;
    let masters = install::read(dat.as_ref())?;
    let run = census_run(&recording, Some(&masters), limit)?;
    println!(
        "{} updates ({:?}), {} checkpoints",
        run.updates, run.end, run.checkpoints
    );
    for (p, player) in run.players.iter().enumerate() {
        let stop = player.stop.as_ref().map_or_else(
            || "no stop".to_owned(),
            |s| format!("stops at update {}: {}", s.update, s.reason),
        );
        let mismatch = player.mismatch.as_ref().map_or_else(
            || "no checkpoint differs".to_owned(),
            |m| {
                format!(
                    "the checkpoint after {} syncs differs in words {:?}",
                    m.syncs, m.words
                )
            },
        );
        println!(
            "player {p}: {stop}; {} checkpoints hold; {mismatch}",
            player.checkpoints_held
        );
    }
    Ok(())
}
