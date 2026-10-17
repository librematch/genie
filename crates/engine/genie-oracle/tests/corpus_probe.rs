//! Throwaway probe: run the oracle's first-checkpoint comparison and word-9 simulation on every
//! recording below `GENIE_REC_CORPUS` (no tier-1 filter), and report where the slice stops.

mod common;

use common::{corpus, masters};
use genie_oracle::{compare_first_checkpoint, simulate_word9};
use genie_rec::de;
use std::path::{Path, PathBuf};

#[test]
fn probe_corpus() {
    let (Some(dir), masters) = (corpus(), masters()) else {
        eprintln!("GENIE_REC_CORPUS / GENIE_DAT are not set; skipping");
        return;
    };
    let mut paths = Vec::new();
    fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.extension().is_some_and(|e| e == "aoe2record") {
                out.push(path);
            }
        }
    }
    collect(Path::new(&dir), &mut paths);
    paths.sort();

    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                println!("{name}: read error {e}");
                continue;
            }
        };
        let version = de::chapters(&data)
            .ok()
            .and_then(|c| c[0].save_version().ok())
            .map(|v| format!("{}.{} build {}", v.major, v.minor, v.build))
            .unwrap_or_else(|| "?".into());
        let compared = match compare_first_checkpoint(&data) {
            Ok(c) => c,
            Err(e) => {
                println!("{name} [{version}]: compare error {e}");
                continue;
            }
        };
        let word9 = match simulate_word9(&data, masters.as_ref()) {
            Ok(w) => w,
            Err(e) => {
                println!("{name} [{version}]: simulate error {e}");
                continue;
            }
        };
        let (mut exact, mut off, mut unsup) = (0, 0, 0);
        let mut exact_lanes = Vec::new();
        let mut first = String::new();
        for w in &word9 {
            let c = compared.iter().find(|c| c.id == w.id);
            let save_differs = c.is_some_and(|c| {
                genie_oracle::SAVE_WORDS
                    .iter()
                    .any(|&k| c.from_save[k] != c.checkpoint[k])
            });
            if let Some((tick, object, why)) = &w.unsupported {
                unsup += 1;
                if first.is_empty() {
                    first = format!("p{}: tick {tick}, object {object}: {}", w.id, why.0);
                }
            } else if save_differs {
                // the save words differ from the checkpoint: the checkpoint is beyond the save's reach
                unsup += 1;
                if first.is_empty() {
                    first = format!("p{}: save words differ from checkpoint", w.id);
                }
            } else if w.simulated == w.checkpoint {
                exact += 1;
                exact_lanes.push(w.id);
            } else {
                off += 1;
                if first.is_empty() {
                    first = format!("p{}: off by {}", w.id, w.checkpoint as i64 - w.simulated as i64);
                }
            }
        }
        println!(
            "{name} [{version}]: exact {exact} off {off} unsupported {unsup} ; exact lanes {exact_lanes:?} ; first: {first}"
        );
    }
}
