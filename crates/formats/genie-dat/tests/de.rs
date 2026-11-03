//! The DE data file against a game's: `GENIE_DAT` names an `empires2_x2_p1.dat` (compressed, or
//! inflated); skips without it. The values come from genieutils-py on the same files: VER 8.8 as
//! build 158041 ships it (Steam build 20295388), VER 8.9 as build 185872 does.

use flate2::read::DeflateDecoder;
use genie_dat::de::{DatFile, Version};
use std::env::var_os;
use std::fs::read;
use std::io::Read;

/// The file's inflated bytes: compressed as the game ships it, or already inflated (starting with
/// its version string, as the load oracle's VER 8.8 rewrite).
fn inflated() -> Option<Vec<u8>> {
    let raw = read(var_os("GENIE_DAT")?).unwrap();
    if raw.starts_with(b"VER ") {
        return Some(raw);
    }
    let mut data = Vec::new();
    // The byte count is `data.len()`.
    let _ = DeflateDecoder::new(&raw[..])
        .read_to_end(&mut data)
        .unwrap();
    Some(data)
}

#[test]
fn the_installed_data_file_reads_and_writes_back_byte_for_byte() {
    let Some(data) = inflated() else {
        return eprintln!("GENIE_DAT is not set; skipping");
    };
    let dat = DatFile::from_inflated(&data).unwrap();
    assert_eq!(dat.to_bytes().unwrap(), data);
}

#[test]
fn the_installed_data_file_has_the_values_genieutils_reads() {
    let Some(data) = inflated() else {
        return eprintln!("GENIE_DAT is not set; skipping");
    };
    let dat = DatFile::from_inflated(&data).unwrap();
    let units = match dat.version().unwrap() {
        Version(31) => 2539, // VER 8.8, build 158041
        Version(32) => 2750, // VER 8.9, build 185872
        other => panic!("no genieutils values for {other:?}"),
    };
    assert_eq!(dat.unit_headers.len(), 1701);
    let civ2 = &dat.civs[2];
    assert_eq!(civ2.units.len(), units);
    // the cow: decay 0.25 per second, default task 0 with action type 107 (the Artifact action)
    let cow = civ2.units[705].as_ref().unwrap();
    assert_eq!(
        (cow.name.as_str(), cow.resource_decay),
        ("Cow Black and White".into(), 0.25)
    );
    let bird = cow.bird.as_ref().unwrap();
    assert_eq!(bird.default_task().map(|t| t.action_kind), Some(107));
    // FLARE3: decay 1.0, a static object without the action part
    let flare = civ2.units[332].as_ref().unwrap();
    assert_eq!((flare.resource_decay, flare.bird.is_none()), (1.0, true));
}

/// A `file_version` set outside the versions the reader takes is the caller's invalid input, not
/// a format the reader lacks.
#[test]
fn file_version_set_outside_the_read_ones_is_invalid_input() {
    let Some(data) = inflated() else {
        return eprintln!("GENIE_DAT is not set; skipping");
    };
    let mut dat = DatFile::from_inflated(&data).unwrap();
    dat.file_version = *b"VER 9.9\0";
    assert_eq!(
        dat.version().unwrap_err().kind(),
        genie_error::ErrorKind::InvalidInput
    );
}
