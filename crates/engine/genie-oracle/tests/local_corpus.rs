//! The oracle against the workspace's local corpus: the savegame recordings of the desktop
//! (save 66.6, build 162286, game v101.103.31214), standing in for the tier-1 corpus, which is not
//! on any reachable host (see specs/corpus-rebaseline.md). Needs `GENIE_REC_CORPUS` and `GENIE_DAT`;
//! skips without them.

mod common;

use common::{corpus, masters};
use genie_oracle::simulate_word9;

/// The slice's heavy work overflows the default 2 MiB test-thread stack on a real recording
/// (a world save is ~16 MB inflated); run it in a thread with room for it.
fn with_big_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(work)
        .unwrap()
        .join()
        .unwrap()
}

/// `MP Replay v101.103.31214.0 @2026.01.08 163608 (6)`: its lanes p7 and p8 are idle up to the
/// first checkpoint and the slice reproduces their word 9 there; the other lanes stop at the first
/// order's movement (tick 75, player 1). Pinned, so that a model change or a reader change is
/// noticed without the tier-1 corpus.
#[test]
fn local_corpus_idle_lanes_reproduce_at_checkpoint_1() {
    let (Some(dir), Some(masters)) = (corpus(), masters()) else {
        return eprintln!("GENIE_REC_CORPUS / GENIE_DAT are not set; skipping");
    };
    let path = with_big_stack(move || {
        let mut found = None;
        for entry in walk(dir) {
            let name = entry.file_name().unwrap().to_string_lossy().to_string();
            if name.contains("@2026.01.08 163608") {
                found = Some(entry);
                break;
            }
        }
        found.unwrap_or_else(|| panic!("the '@2026.01.08 163608' recording is not below GENIE_REC_CORPUS"))
    });
    let data = std::fs::read(&path).unwrap();
    let word9 = with_big_stack(move || simulate_word9(&data, Some(&masters)).unwrap());
    for id in [7u32, 8] {
        let w = word9.iter().find(|w| w.id == id).unwrap();
        assert_eq!(w.unsupported, None, "p{id}");
        assert_eq!(w.simulated, w.checkpoint, "p{id}");
    }
    let p1 = word9.iter().find(|w| w.id == 1).unwrap();
    let (tick, object, why) = p1.unsupported.unwrap();
    assert_eq!((tick, object), (75, 0));
    assert_eq!(why.0, "an order moves its units");
}

fn walk(dir: std::path::PathBuf) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(walk(path));
        } else {
            out.push(path);
        }
    }
    out
}
