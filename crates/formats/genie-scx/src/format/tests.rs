use super::ScxFormat;
use crate::VersionBundle;
use genie_error::GenieResult;
use std::fs::File;
use std::io::{Cursor, ErrorKind, Read};

fn save_and_load(format: &ScxFormat, as_version: VersionBundle) -> GenieResult<ScxFormat> {
    let mut out = vec![];
    format.write_to(&mut out, &as_version)?;

    let mut f = Cursor::new(out);
    let scx = ScxFormat::load_scenario(&mut f)?;
    assert_consumed(f);
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
fn oldest_aoe1_scn_on_aoeheaven() {
    let mut f = File::open("test/scenarios/ The Destruction of Rome.scn").unwrap();
    let format = ScxFormat::load_scenario(&mut f).expect("failed to read");
    assert_consumed(f);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

#[test]
fn aoe1_beta_scn_reserialize() {
    let mut f = File::open("test/scenarios/Dawn of a New Age.scn").unwrap();
    let format = ScxFormat::load_scenario(&mut f).expect("failed to read");
    assert_consumed(f);
    let format2 = save_and_load(&format, format.version()).expect("save-and-load failed");

    assert_eq!(
        format.hash(),
        format2.hash(),
        "should produce exactly the same scenario"
    );
}

#[test]
fn aoe1_beta_scn_to_aoc() {
    let mut f = File::open("test/scenarios/Dawn of a New Age.scn").unwrap();
    let format = ScxFormat::load_scenario(&mut f).expect("failed to read");
    assert_consumed(f);
    let format2 = save_and_load(&format, VersionBundle::aoc()).expect("save-and-load failed");

    assert_eq!(
        format2.version(),
        VersionBundle::aoc(),
        "should have converted to AoC versions"
    );
}

/// Source: <http://aoe.heavengames.com/dl-php/showfile.php?fileid=1678>
#[test]
fn aoe1_trial_scn() {
    let mut f = File::open("test/scenarios/Bronze Age Art of War.scn").unwrap();
    let format = ScxFormat::load_scenario(&mut f).expect("failed to read");
    assert_consumed(f);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

/// Source: <http://aoe.heavengames.com/dl-php/showfile.php?fileid=2409>
#[test]
fn aoe1_ppc_trial_scn() {
    let mut f = File::open("test/scenarios/CEASAR.scn").unwrap();
    let format = ScxFormat::load_scenario(&mut f).expect("failed to read");
    assert_consumed(f);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

/// Source: <http://aoe.heavengames.com/dl-php/showfile.php?fileid=1651>
#[test]
fn aoe1_scn() {
    let mut f = File::open("test/scenarios/A New Emporer.scn").unwrap();
    let format = ScxFormat::load_scenario(&mut f).expect("failed to read");
    assert_consumed(f);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

/// Source: <http://aoe.heavengames.com/dl-php/showfile.php?fileid=880>
#[test]
fn aoe1_ror_scx() {
    let mut f = File::open("test/scenarios/Jeremiah Johnson (Update).scx").unwrap();
    let format = ScxFormat::load_scenario(&mut f).expect("failed to read");
    assert_consumed(f);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

#[test]
fn aoe1_ror_to_aoc() -> GenieResult<()> {
    let mut f = File::open("test/scenarios/El advenimiento de los hunos_.scx")?;
    let format = ScxFormat::load_scenario(&mut f)?;
    assert_consumed(f);
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
fn oldest_aok_scn_on_aokheaven() {
    let mut f = File::open("test/scenarios/CAMELOT.SCN").unwrap();
    let format = ScxFormat::load_scenario(&mut f).expect("failed to read");
    assert_consumed(f);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

#[test]
fn aoc_scx() {
    let mut f = File::open("test/scenarios/Age of Heroes b1-3-5.scx").unwrap();
    let format = ScxFormat::load_scenario(&mut f).expect("failed to read");
    assert_consumed(f);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

#[test]
fn hd_aoe2scenario() {
    let mut f = File::open("test/scenarios/Year_of_the_Pig.aoe2scenario").unwrap();
    let format = ScxFormat::load_scenario(&mut f).expect("failed to read");
    assert_consumed(f);
    let format2 = save_and_load(&format, format.version()).expect("save-and-load failed");

    assert_eq!(
        format.hash(),
        format2.hash(),
        "should produce exactly the same scenario"
    );
}

#[test]
fn hd_scx2() {
    let mut f = File::open("test/scenarios/real_world_amazon.scx").unwrap();
    let format = ScxFormat::load_scenario(&mut f).expect("failed to read");
    assert_consumed(f);
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
fn aoe_de_scn() {
    let mut f = File::open("test/scenarios/Corlis.aoescn").unwrap();
    let format = ScxFormat::load_scenario(&mut f).expect("failed to read");
    assert_consumed(f);
    let mut out = vec![];
    format
        .write_to(&mut out, &format.version())
        .expect("failed to write");
}

/// A Definitive Edition 2 scenario, SCX format version 1.36.
///
/// Source: <https://www.ageofempires.com/mods/details/2015>/
#[test]
fn aoe_de2_1_36() {
    let mut f = File::open("test/scenarios/Hotkey Trainer Buildings.aoe2scenario").unwrap();
    let format = ScxFormat::load_scenario(&mut f).expect("failed to read");
    assert_consumed(f);
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
fn aoe_de2_1_37() {
    let mut f = File::open("test/scenarios/layertest.aoe2scenario").unwrap();
    let format = ScxFormat::load_scenario(&mut f).expect("failed to read");
    assert_consumed(f);
    let format2 = save_and_load(&format, format.version()).expect("save-and-load failed");
    assert_eq!(
        format.hash(),
        format2.hash(),
        "should produce exactly the same scenario"
    );
}
