//! The DE data file against the installed game's: `GENIE_DAT` names an `empires2_x2_p1.dat`
//! (compressed, or inflated); skips without it. The values come from genieutils-py on the same file.

use flate2::read::DeflateDecoder;
use genie_dat::de::{DatFile, Version};
use std::io::Read;

/// The file's inflated bytes: compressed as the game ships it, or already inflated (starting with
/// its version string, as the load oracle's VER 8.8 rewrite).
fn inflated() -> Option<Vec<u8>> {
    let raw = std::fs::read(std::env::var_os("GENIE_DAT")?).unwrap();
    if raw.starts_with(b"VER ") {
        return Some(raw);
    }
    let mut data = Vec::new();
    DeflateDecoder::new(&raw[..])
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
    // VER 8.9 as installed, or the load oracle's VER 8.8 rewrite of it
    assert!([Version(31), Version(32)].contains(&dat.version()));
    assert_eq!(dat.unit_headers.len(), 1701);
    let civ2 = &dat.civs[2];
    assert_eq!(civ2.units.len(), 2750);
    // the cow: decay 0.25 per second, default task 0 with action type 107 (the Artifact action)
    let cow = civ2.units[705].as_ref().unwrap();
    assert_eq!(
        (cow.name.as_str(), cow.resource_decay),
        ("Cow Black and White".into(), 0.25)
    );
    let bird = cow.bird.as_ref().unwrap();
    assert_eq!(bird.default_task().map(|t| t.action_type), Some(107));
    // FLARE3: decay 1.0, a static object without the action part
    let flare = civ2.units[332].as_ref().unwrap();
    assert_eq!((flare.resource_decay, flare.bird.is_none()), (1.0, true));
}
