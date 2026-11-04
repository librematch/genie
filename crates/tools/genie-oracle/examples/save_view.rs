//! Views of a save's state: a recording's header save, or any save of the game.
//!
//! `cargo run --release -p genie-oracle --example save_view -- VIEW SAVE ARGS…`
//!
//! - `objects SAVE PLAYER [ID]`: a player's live objects (id, master, state, hit points,
//!   position, top action, the unit AI's retarget timer); ID prints that object's actions too
//! - `movement SAVE PLAYER ID…`: position, velocity, orientation and paths
//! - `unit-ais SAVE PLAYER ID…`: orders, actions, targets and timers
//! - `tiles SAVE X0 X1 Y0 Y1`: tile heights in a window, in both axis orders

use crate::cli::{number, usage};
use genie_core::GenieResult;
use genie_oracle::views::{movement, objects, tiles, unit_ais};
use std::env::args;
use std::fs::read;

mod cli;

/// The usage line, the error of arguments that match no view.
const USAGE: &str = "usage: save_view objects|movement|unit-ais|tiles SAVE ARGS…";

fn main() -> GenieResult<()> {
    let args: Vec<String> = args().collect();
    let [_, view, save, rest @ ..] = &args[..] else {
        return Err(usage(USAGE));
    };
    let data = read(save)?;
    let numbers = |from: usize| -> GenieResult<Vec<i32>> {
        rest[from..].iter().map(|text| number(text)).collect()
    };
    let text = match (view.as_str(), rest) {
        ("objects", [player]) => objects(&data, number(player)?, None)?,
        ("objects", [player, id]) => objects(&data, number(player)?, Some(number(id)?))?,
        ("movement", [player, _, ..]) => movement(&data, number(player)?, &numbers(1)?)?,
        ("unit-ais", [player, _, ..]) => unit_ais(&data, number(player)?, &numbers(1)?)?,
        ("tiles", [x0, x1, y0, y1]) => tiles(
            &data,
            (number(x0)?, number(x1)?),
            (number(y0)?, number(y1)?),
        )?,
        _ => return Err(usage(USAGE)),
    };
    print!("{text}");
    Ok(())
}
