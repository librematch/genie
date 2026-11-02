//! Views of a save's state: a recording's header save, or any save of the game.
//!
//! `cargo run --release -p genie-oracle --example save_view -- VIEW SAVE ARGS…`
//!
//! - `objects SAVE PLAYER [ID]`: a player's live objects (id, master, state, hit points,
//!   position, top action, the unit AI's retarget timer); ID prints that object's actions too
//! - `movement SAVE PLAYER ID…`: position, velocity, orientation and paths
//! - `unit-ais SAVE PLAYER ID…`: orders, actions, targets and timers
//! - `tiles SAVE X0 X1 Y0 Y1`: tile heights in a window, in both axis orders

use genie_oracle::views;
use std::env::args;
use std::error::Error;
use std::fs::read;
use std::num::ParseIntError;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = args().collect();
    let usage = "usage: save_view objects|movement|unit-ais|tiles SAVE ARGS…";
    let [_, view, save, rest @ ..] = &args[..] else {
        return Err(usage.into());
    };
    let data = read(save)?;
    let numbers = |from: usize| -> Result<Vec<i32>, ParseIntError> {
        rest[from..].iter().map(|a| a.parse()).collect()
    };
    let text = match (view.as_str(), rest) {
        ("objects", [player]) => views::objects(&data, player.parse()?, None)?,
        ("objects", [player, id]) => views::objects(&data, player.parse()?, Some(id.parse()?))?,
        ("movement", [player, _, ..]) => views::movement(&data, player.parse()?, &numbers(1)?)?,
        ("unit-ais", [player, _, ..]) => views::unit_ais(&data, player.parse()?, &numbers(1)?)?,
        ("tiles", [x0, x1, y0, y1]) => views::tiles(
            &data,
            (x0.parse()?, x1.parse()?),
            (y0.parse()?, y1.parse()?),
        )?,
        _ => return Err(usage.into()),
    };
    print!("{text}");
    Ok(())
}
