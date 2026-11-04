//! The DE reader against a corpus of real recordings. Set `GENIE_REC_CORPUS` to a directory of
//! `.aoe2record` files; without it the test only reports that it skipped. The expected totals
//! describe the workspace's tier-1 corpus (save 66.6, build 158041): 137 recordings. On the 132 of
//! them that aoe2rec 0.10.2 also reads, the totals equal aoe2rec's in every category (checked
//! 2026-10-15); aoe2rec rejects a recording that ends right after a sync, and aoc-mgz cannot parse
//! the headers of three others.

use genie_rec::de::Op;
use genie_rec::de::chapters;
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

#[derive(Debug, Default, PartialEq, Eq)]
struct Totals {
    recordings: usize,
    commands: usize,
    syncs: usize,
    checkpoints: usize,
    views: usize,
    chats: usize,
}

#[test]
fn tier_1_recordings_read_and_round_trip() {
    let Ok(dir) = var("GENIE_REC_CORPUS") else {
        eprintln!("GENIE_REC_CORPUS is not set; skipping the corpus test");
        return;
    };
    let mut paths = Vec::new();
    recordings(Path::new(&dir), &mut paths);
    paths.sort();

    let mut totals = Totals::default();
    let (mut tier1, mut problems) = (0, Vec::new());
    for path in &paths {
        let data = read(path).unwrap();
        let Ok(chapters) = chapters(&data) else {
            continue;
        };
        let Ok(version) = chapters[0].save_version() else {
            continue;
        };
        if (version.hundredths(), version.build) != (6660, 158_041) {
            continue;
        }
        tier1 += 1;
        let mut rec = Totals {
            recordings: 1,
            ..Totals::default()
        };
        let mut failed = false;
        for chapter in &chapters {
            let (mut written, mut interval, mut syncs) = (Vec::new(), 0, 0);
            for (i, op) in chapter.ops().enumerate() {
                let op = match op {
                    Ok(op) => op,
                    Err(e) => {
                        problems.push(format!("{}: {e}", path.display()));
                        failed = true;
                        break;
                    }
                };
                match &op {
                    Op::Pregame(p) => interval = p.checkpoint_interval as usize,
                    Op::Command { .. } => rec.commands += 1,
                    Op::Sync { .. } => {
                        rec.syncs += 1;
                        syncs += 1;
                    }
                    Op::Checkpoint { .. } => {
                        rec.syncs += 1;
                        rec.checkpoints += 1;
                        syncs += 1;
                        if chapter.index == 0 && syncs % interval != 0 {
                            problems
                                .push(format!("{}: checkpoint at sync {syncs}", path.display()));
                        }
                    }
                    Op::View(_) => rec.views += 1,
                    Op::Chat { .. } => rec.chats += 1,
                    // an op a later version of the reader adds: the census must learn to count it
                    other => problems.push(format!(
                        "{}: an op the census does not count: {other:?}",
                        path.display()
                    )),
                }
                if chapter.index == 0 && i == 0 && !matches!(op, Op::Pregame(_)) {
                    problems.push(format!(
                        "{}: chapter 0 does not start with Pregame",
                        path.display()
                    ));
                }
                op.write(&mut written).expect("a read op writes back");
            }
            if !failed && written != chapter.body {
                problems.push(format!(
                    "{}: chapter {} does not round-trip",
                    path.display(),
                    chapter.index
                ));
            }
        }
        if !failed {
            totals.recordings += rec.recordings;
            totals.commands += rec.commands;
            totals.syncs += rec.syncs;
            totals.checkpoints += rec.checkpoints;
            totals.views += rec.views;
            totals.chats += rec.chats;
        }
    }
    eprintln!("tier 1: {tier1} recordings, {totals:?}");
    assert!(problems.is_empty(), "{problems:#?}");
    let expected = Totals {
        recordings: 137,
        commands: 692_263,
        syncs: 13_480_324,
        checkpoints: 26_892,
        views: 13_103_140,
        chats: 3_662,
    };
    assert_eq!(totals, expected);
}
