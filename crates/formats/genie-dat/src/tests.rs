use super::*;
use flate2::Compression;
use flate2::write::DeflateEncoder;
use genie_error::GenieResult;
use std::fs::read;
use std::{
    collections::hash_map::DefaultHasher,
    fs::File,
    hash::{Hash, Hasher},
    io::{Cursor, Write},
};

#[test]
fn file_version_text_parses_padded_with_zeros() -> GenieResult<()> {
    assert_eq!(
        FileVersion::try_from("VER 5.7")?,
        FileVersion(*b"VER 5.7\0")
    );
    assert_eq!(
        FileVersion::try_from("VER 5.8\0")?,
        FileVersion(*b"VER 5.8\0")
    );
    Ok(())
}

#[test]
fn file_version_text_of_more_than_8_bytes_does_not_parse() {
    assert_eq!(
        FileVersion::try_from("VER 5.7\0\0").map_err(|error| error.reason()),
        Err("a file version of more than 8 bytes")
    );
}

#[test]
fn file_version_not_of_the_form_ver_digit_dot_digit_does_not_parse() {
    for bytes in [
        *b"VER ?.7\0",
        *b"VER 5,7\0",
        *b"VER 5.x\0",
        *b"VER 5.7 ",
        *b"ver 5.7\0",
    ] {
        assert_eq!(
            FileVersion::try_from(bytes).map_err(|error| error.reason()),
            Err("a file version not of the form VER d.d")
        );
    }
    assert_eq!(
        FileVersion::try_from("VER 5").map_err(|error| error.reason()),
        Err("a file version not of the form VER d.d")
    );
}

#[test]
fn data_file_whose_version_is_not_ver_digit_dot_digit_does_not_read() -> GenieResult<()> {
    let mut compressed = DeflateEncoder::new(vec![], Compression::default());
    compressed.write_all(b"VER \0.7\0")?;
    let compressed = compressed.finish()?;
    assert_eq!(
        DatFile::read_from(&compressed[..])
            .err()
            .map(|error| error.reason()),
        Some("a file version not of the form VER d.d")
    );
    Ok(())
}

#[test]
fn aok_data_file_reads_its_14_civilizations() -> GenieResult<()> {
    let mut file = File::open("fixtures/aok.dat")?;
    let dat = DatFile::read_from(&mut file)?;
    assert_eq!(dat.civilizations.len(), 14);
    Ok(())
}

#[test]
fn aoc_data_file_reads_its_19_civilizations() -> GenieResult<()> {
    let mut file = File::open("fixtures/aoc1.0c.dat")?;
    let dat = DatFile::read_from(&mut file)?;
    assert_eq!(dat.civilizations.len(), 19);
    Ok(())
}

#[test]
fn tech_name_beyond_ascii_reads_as_its_text() -> GenieResult<()> {
    let mut file = File::open("fixtures/age-of-chivalry.dat")?;
    let dat = DatFile::read_from(&mut file)?;
    assert_eq!(dat.techs[859].name(), "Székely (enable)");
    Ok(())
}

#[test]
fn hd_edition_data_file_reads_its_32_civilizations() -> GenieResult<()> {
    let mut file = File::open("fixtures/hd.dat")?;
    let dat = DatFile::read_from(&mut file)?;
    assert_eq!(dat.civilizations.len(), 32);
    Ok(())
}

#[test]
fn aoc_data_file_written_and_read_again_holds_the_same_values() -> GenieResult<()> {
    let original = read("fixtures/aoc1.0c.dat")?;
    let mut cursor = Cursor::new(&original);
    let dat = DatFile::read_from(&mut cursor)?;
    let mut serialized = vec![];
    dat.write_to(&mut serialized)?;

    let mut cursor = Cursor::new(&serialized);
    let dat2 = DatFile::read_from(&mut cursor)?;

    let mut orig_hasher = DefaultHasher::new();
    let mut new_hasher = DefaultHasher::new();
    format!("{dat:?}").hash(&mut orig_hasher);
    format!("{dat2:?}").hash(&mut new_hasher);
    assert_eq!(orig_hasher.finish(), new_hasher.finish());

    Ok(())
}
