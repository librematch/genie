use crate::VersionBundle;
use crate::format::ScxFormat;
use genie_error::GenieResult;
use std::fs::File;
use std::io::{Cursor, ErrorKind, Read};

fn save_and_load(format: &ScxFormat, as_version: VersionBundle) -> GenieResult<ScxFormat> {
    let mut out = vec![];
    format.write_to(&mut out, &as_version)?;

    let mut cursor = Cursor::new(out);
    let scx = ScxFormat::load_scenario(&mut cursor)?;
    assert_consumed(cursor);
    Ok(scx)
}

fn assert_consumed(mut input: impl Read) {
    let byte = &mut [0];
    match input.read_exact(byte) {
        Err(err) if err.kind() == ErrorKind::UnexpectedEof => (),
        Err(err) => panic!("{}", err),
        Ok(()) => {
            let mut trailing_data = vec![byte[0]];
            let _ = input.read_to_end(&mut trailing_data).unwrap();
            panic!("data left in buffer ({}): {:?}", trailing_data.len(), {
                trailing_data.truncate(32);
                trailing_data
            });
        }
    }
}

/// Source: <http://aoe.heavengames.com/dl-php/showfile.php?fileid=42>
#[test]
fn oldest_aoe1_scenario_on_aoe_heaven_reads_to_its_end_and_writes() {
    let mut file = File::open("test/scenarios/ The Destruction of Rome.scn").unwrap();
    let format = ScxFormat::load_scenario(&mut file).expect("failed to read");
    assert_consumed(file);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

#[test]
fn aoe1_beta_scenario_writes_back_the_same_scenario() {
    let mut file = File::open("test/scenarios/Dawn of a New Age.scn").unwrap();
    let format = ScxFormat::load_scenario(&mut file).expect("failed to read");
    assert_consumed(file);
    let format2 = save_and_load(&format, format.version()).expect("save-and-load failed");

    assert_eq!(
        format.hash(),
        format2.hash(),
        "should produce exactly the same scenario"
    );
}

#[test]
fn aoe1_beta_scenario_converts_to_the_aoc_versions() {
    let mut file = File::open("test/scenarios/Dawn of a New Age.scn").unwrap();
    let format = ScxFormat::load_scenario(&mut file).expect("failed to read");
    assert_consumed(file);
    let format2 = save_and_load(&format, VersionBundle::aoc()).expect("save-and-load failed");

    assert_eq!(
        format2.version(),
        VersionBundle::aoc(),
        "should have converted to AoC versions"
    );
}

/// Source: <http://aoe.heavengames.com/dl-php/showfile.php?fileid=1678>
#[test]
fn aoe1_trial_scenario_reads_to_its_end_and_writes() {
    let mut file = File::open("test/scenarios/Bronze Age Art of War.scn").unwrap();
    let format = ScxFormat::load_scenario(&mut file).expect("failed to read");
    assert_consumed(file);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

/// Source: <http://aoe.heavengames.com/dl-php/showfile.php?fileid=2409>
#[test]
fn aoe1_ppc_trial_scenario_reads_to_its_end_and_writes() {
    let mut file = File::open("test/scenarios/CEASAR.scn").unwrap();
    let format = ScxFormat::load_scenario(&mut file).expect("failed to read");
    assert_consumed(file);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

/// Source: <http://aoe.heavengames.com/dl-php/showfile.php?fileid=1651>
#[test]
fn aoe1_scenario_reads_to_its_end_and_writes() {
    let mut file = File::open("test/scenarios/A New Emporer.scn").unwrap();
    let format = ScxFormat::load_scenario(&mut file).expect("failed to read");
    assert_consumed(file);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

/// Source: <http://aoe.heavengames.com/dl-php/showfile.php?fileid=880>
#[test]
fn aoe1_ror_scenario_reads_to_its_end_and_writes() {
    let mut file = File::open("test/scenarios/Jeremiah Johnson (Update).scx").unwrap();
    let format = ScxFormat::load_scenario(&mut file).expect("failed to read");
    assert_consumed(file);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

#[test]
fn aoe1_ror_scenario_converts_to_the_aoc_versions() -> GenieResult<()> {
    let mut file = File::open("test/scenarios/El advenimiento de los hunos_.scx")?;
    let format = ScxFormat::load_scenario(&mut file)?;
    assert_consumed(file);
    let format2 = save_and_load(&format, VersionBundle::aoc())?;

    assert_eq!(
        format2.version(),
        VersionBundle::aoc(),
        "should have converted to AoC versions"
    );

    Ok(())
}

/// Source: <http://aok.heavengames.com/blacksmith/showfile.php?fileid=1271>
#[test]
fn oldest_aok_scenario_on_aok_heaven_reads_to_its_end_and_writes() {
    let mut file = File::open("test/scenarios/CAMELOT.SCN").unwrap();
    let format = ScxFormat::load_scenario(&mut file).expect("failed to read");
    assert_consumed(file);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

#[test]
fn aoc_scenario_reads_to_its_end_and_writes() {
    let mut file = File::open("test/scenarios/Age of Heroes b1-3-5.scx").unwrap();
    let format = ScxFormat::load_scenario(&mut file).expect("failed to read");
    assert_consumed(file);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

#[test]
fn hd_aoe2scenario_writes_back_the_same_scenario() {
    let mut file = File::open("test/scenarios/Year_of_the_Pig.aoe2scenario").unwrap();
    let format = ScxFormat::load_scenario(&mut file).expect("failed to read");
    assert_consumed(file);
    let format2 = save_and_load(&format, format.version()).expect("save-and-load failed");

    assert_eq!(
        format.hash(),
        format2.hash(),
        "should produce exactly the same scenario"
    );
}

#[test]
fn hd_scx2_scenario_reads_to_its_end_and_writes() {
    let mut file = File::open("test/scenarios/real_world_amazon.scx").unwrap();
    let format = ScxFormat::load_scenario(&mut file).expect("failed to read");
    assert_consumed(file);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

/// A Definitive Edition scenario.
///
/// (Ignored because it doesn't work yet.)
/// Source: <http://aoe.heavengames.com/dl-php/showfile.php?fileid=2708>
#[test]
#[ignore = "Definitive Edition (AoE1) scenarios are not supported yet"]
fn aoe1_de_scenario_reads_to_its_end_and_writes() {
    let mut file = File::open("test/scenarios/Corlis.aoescn").unwrap();
    let format = ScxFormat::load_scenario(&mut file).expect("failed to read");
    assert_consumed(file);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

/// A Definitive Edition 2 scenario, SCX format version 1.36.
///
/// Source: <https://www.ageofempires.com/mods/details/2015>/
#[test]
fn aoe2_de_scenario_of_1_36_writes_back_the_same_scenario() {
    let mut file = File::open("test/scenarios/Hotkey Trainer Buildings.aoe2scenario").unwrap();
    let format = ScxFormat::load_scenario(&mut file).expect("failed to read");
    assert_consumed(file);
    let format2 = save_and_load(&format, format.version()).expect("save-and-load failed");
    assert_eq!(
        format.hash(),
        format2.hash(),
        "should produce exactly the same scenario"
    );
}

/// A Definitive Edition 2 scenario, based on the included `AIImprovementsBucket10Test` file,
/// saved as a 1.37 format version with some layered terrain in the corners.
#[test]
fn aoe2_de_scenario_of_1_37_with_layered_terrain_writes_back_the_same_scenario() {
    let mut file = File::open("test/scenarios/layertest.aoe2scenario").unwrap();
    let format = ScxFormat::load_scenario(&mut file).expect("failed to read");
    assert_consumed(file);
    let format2 = save_and_load(&format, format.version()).expect("save-and-load failed");
    assert_eq!(
        format.hash(),
        format2.hash(),
        "should produce exactly the same scenario"
    );
}
