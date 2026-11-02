//! The generated command decoders against the tier-1 corpus (`GENIE_REC_CORPUS`, save 66.6, build
//! 158041): every command packet decodes into its command's layout and writes back byte for byte.
//! Skips without the corpus.

use genie_rec::de::{self, Op, command::Command};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn recordings(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if path.is_dir() {
            recordings(&path, out);
        } else if path.extension().is_some_and(|e| e == "aoe2record") {
            out.push(path);
        }
    }
}

#[test]
fn built_command_decodes_its_fields() {
    // BUILD (0x66): player 2, 1 builder (id 1234), a house (master 70) at (10.5, 20.5)
    let mut packet = vec![0x66, 2, 0, 0, 1, 0, 0, 0];
    packet.extend(10.5f32.to_le_bytes());
    packet.extend(20.5f32.to_le_bytes());
    packet.extend(70i32.to_le_bytes());
    packet.extend((-1i32).to_le_bytes());
    packet.extend([0; 8]);
    packet.extend(1234i32.to_le_bytes());
    let size = u16::try_from(packet.len() - 4).unwrap();
    packet[2..4].copy_from_slice(&size.to_le_bytes());

    let Command::Build(build) = Command::decode(&packet) else {
        panic!("not decoded as BUILD: {:?}", Command::decode(&packet));
    };
    assert_eq!(
        (build.commp_id, build.num, build.x, build.y),
        (2, 1, 10.5, 20.5)
    );
    assert_eq!(
        (build.oid, build.qid, build.ids.as_slice()),
        (70, -1, &[1234][..])
    );
    assert_eq!(Command::Build(build).encode().unwrap(), packet);
}

#[test]
fn packet_that_fits_no_layout_stays_raw() {
    let short = [0x66, 2, 0, 0];
    assert_eq!(Command::decode(&short), Command::Unknown(short.to_vec()));
    assert_eq!(Command::decode(&short).encode().unwrap(), short);
}

#[test]
fn every_tier_1_command_decodes_and_writes_back() {
    let Ok(dir) = std::env::var("GENIE_REC_CORPUS") else {
        eprintln!("GENIE_REC_CORPUS is not set; skipping");
        return;
    };
    let mut paths = Vec::new();
    recordings(Path::new(&dir), &mut paths);
    paths.sort();
    let (mut packets, mut unknown) = (0usize, BTreeMap::<u8, usize>::new());
    for path in &paths {
        let data = std::fs::read(path).unwrap();
        let Ok(chapters) = de::chapters(&data) else {
            continue;
        };
        let Ok(version) = chapters[0].save_version() else {
            continue;
        };
        if (version.hundredths(), version.build) != (6660, 158_041) {
            continue;
        }
        for chapter in &chapters {
            for op in chapter.ops() {
                let Ok(Op::Command { packet, .. }) = op else {
                    continue;
                };
                packets += 1;
                let command = Command::decode(packet);
                if let Command::Unknown(_) = command {
                    *unknown.entry(packet[0]).or_default() += 1;
                }
                assert_eq!(
                    command.encode().unwrap(),
                    packet,
                    "{}: {command:?}",
                    path.display()
                );
            }
        }
    }
    eprintln!("{packets} packets; not decoded per command byte: {unknown:x?}");
    assert!(packets > 0);
    assert!(unknown.is_empty(), "packets no layout fits: {unknown:x?}");
}
