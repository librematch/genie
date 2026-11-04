//! Displays civilizations from a specified dat file.

use genie_error::GenieResult;
use genie_formats::DatFile;
use std::fs::File;
use std::path::PathBuf;
use structopt::StructOpt;

/// Display civilizations from a specified dat file.
#[derive(Debug, StructOpt)]
#[structopt(name = "displaycivs")]
struct DisplayCivs {
    /// The name of the dat file.
    file_name: PathBuf,
}

/// Executes the CLI.
fn main() -> GenieResult<()> {
    let cli_input = DisplayCivs::from_args();
    let mut file = File::open(&cli_input.file_name)?;
    let dat = DatFile::read_from(&mut file)?;
    for civ in &dat.civilizations {
        println!("{}", civ.name());
    }
    Ok(())
}
