//! `contracts [--check]`: writes the model's Nickel contracts from genie-model's types to
//! `contracts/genie.ncl` (FD-046-D15); `--check` fails when the file differs from them.

use genie_model::model::Model;
use genie_nickel::contracts::Outcome;
use genie_nickel::contracts::contracts;
use genie_nickel::contracts::write_or_check;
use schemars::schema_for;
use std::env;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let check = env::args().skip(1).any(|a| a == "--check");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../contracts/genie.ncl");
    let outcome =
        contracts(&schema_for!(Model)).and_then(|text| write_or_check(&path, &text, check));
    match outcome {
        Ok(Outcome::Stale) => {
            eprintln!("contracts/genie.ncl is stale: run just contracts");
            ExitCode::FAILURE
        }
        Ok(Outcome::Written | Outcome::Current) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("contracts: {e}");
            ExitCode::FAILURE
        }
    }
}
