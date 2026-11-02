//! Read, then write: the DE header gives back its bytes (FD-014: the proof that the reader
//! accounts for every byte). Needs `GENIE_REC_CORPUS`, a directory of recordings; checks the tier-1
//! ones (save 66.6, build 158041) and skips without it.

use binrw::io::Cursor;
use genie_rec::de;
use std::env::var;
use std::fs::{read, read_dir};
use std::path::{Path, PathBuf};

fn recordings(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in read_dir(dir).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if path.is_dir() {
            recordings(&path, out);
        } else if path.extension().is_some_and(|e| e == "aoe2record") {
            out.push(path);
        }
    }
}

/// The header written back.
fn written(h: &genie_save::Header) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    genie_save::write(h, &mut out).unwrap();
    out.into_inner()
}

/// The first offset where `ours` differs from `theirs`, or where the shorter one ends.
fn first_difference(ours: &[u8], theirs: &[u8]) -> Option<usize> {
    let common = ours.iter().zip(theirs).position(|(a, b)| a != b);
    common.or_else(|| (ours.len() != theirs.len()).then(|| ours.len().min(theirs.len())))
}

#[test]
fn tier_1_headers_write_back_their_bytes() {
    let Ok(dir) = var("GENIE_REC_CORPUS") else {
        eprintln!("GENIE_REC_CORPUS is not set; skipping the round trip");
        return;
    };
    let mut paths = Vec::new();
    recordings(Path::new(&dir), &mut paths);
    paths.sort();
    let (mut checked, mut differ) = (0, Vec::new());
    for path in &paths {
        let data = read(path).unwrap();
        let Ok(chapters) = de::chapters(&data) else {
            continue;
        };
        let Ok(version) = chapters[0].save_version() else {
            continue;
        };
        if (version.hundredths(), version.build) != (6660, 158_041) {
            continue;
        }
        let inflated = chapters[0].inflate_header().unwrap();
        let header = genie_save::parse(&inflated).unwrap();
        if let Some(at) = first_difference(&written(&header), &inflated) {
            differ.push(format!("{}: first difference at {at:#x}", path.display()));
        }
        checked += 1;
    }
    eprintln!("{checked} tier-1 headers written back");
    assert!(
        differ.is_empty(),
        "{} headers differ:\n{}",
        differ.len(),
        differ.join("\n")
    );
}

#[test]
fn the_first_difference_is_where_the_bytes_or_the_lengths_part() {
    assert_eq!(first_difference(b"abc", b"abc"), None);
    assert_eq!(first_difference(b"abd", b"abc"), Some(2));
    assert_eq!(first_difference(b"ab", b"abc"), Some(2));
}
