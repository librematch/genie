//! `contracts [--check]`: writes the Nickel contracts of genie-model's types to
//! `contracts/genie.ncl` (FD-046-D15), the model's and the build layer's; `--check` fails when
//! the file differs from them.

use genie_model::layer::BuildLayer;
use genie_model::model::Model;
use genie_nickel::contracts::ContractsMode;
use genie_nickel::contracts::Outcome;
use genie_nickel::contracts::contracts;
use genie_nickel::contracts::write_or_check;
use schemars::SchemaGenerator;
use schemars::json_schema;
use std::env;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mode = if env::args().skip(1).any(|text| text == "--check") {
        ContractsMode::Check
    } else {
        ContractsMode::Write
    };
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../contracts/genie.ncl");
    let mut generator = SchemaGenerator::default();
    let _ = generator.subschema_for::<Model>();
    let _ = generator.subschema_for::<BuildLayer>();
    let schema = json_schema!({ "$defs": generator.take_definitions(true) });
    let outcome = contracts(&schema).and_then(|text| write_or_check(&path, &text, mode));
    match outcome {
        Ok(Outcome::Stale) => {
            eprintln!("contracts/genie.ncl is stale: run just contracts");
            ExitCode::FAILURE
        }
        Ok(Outcome::Written | Outcome::Current) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("contracts: {error}");
            ExitCode::FAILURE
        }
    }
}
