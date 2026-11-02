//! The DE header reader against `ImHex`'s offsets for aoe2rec's pattern on one recording, and
//! against every tier-1 recording of the corpus. Needs `GENIE_REC_CORPUS`; skips without it.

use genie_rec::de;
use genie_save::generated::world::RgeMasterStaticObject;
use std::collections::BTreeMap;
use std::env::var_os;
use std::fs::{read, read_dir};
use std::path::{Path, PathBuf};

const FIXTURE: &str = include_str!("fixtures/marvinblue-426048731-header-offsets.txt");

fn corpus() -> Option<PathBuf> {
    var_os("GENIE_REC_CORPUS").map(PathBuf::from)
}

/// The expected end offset of a named section.
fn expected(section: &str) -> usize {
    FIXTURE
        .lines()
        .filter(|l| !l.starts_with('#'))
        .find_map(|l| {
            let mut f = l.split_whitespace();
            (f.next() == Some(section)).then(|| {
                usize::from_str_radix(f.nth(1).unwrap().trim_start_matches("0x"), 16).unwrap()
            })
        })
        .unwrap_or_else(|| panic!("no {section} in the fixture"))
}

/// The expected start offset of a named section.
fn expected_start(section: &str) -> usize {
    FIXTURE
        .lines()
        .filter(|l| !l.starts_with('#'))
        .find_map(|l| {
            let mut f = l.split_whitespace();
            (f.next() == Some(section)).then(|| {
                usize::from_str_radix(f.next().unwrap().trim_start_matches("0x"), 16).unwrap()
            })
        })
        .unwrap_or_else(|| panic!("no {section} in the fixture"))
}

/// The element count of a named array section.
fn expected_count(section: &str) -> usize {
    FIXTURE
        .lines()
        .filter(|l| !l.starts_with('#'))
        .find_map(|l| {
            let mut f = l.split_whitespace();
            (f.next() == Some(section)).then(|| f.nth(2).unwrap().parse().unwrap())
        })
        .unwrap_or_else(|| panic!("no count for {section} in the fixture"))
}

fn header_of(path: &Path) -> Vec<u8> {
    let data = read(path).unwrap();
    de::chapters(&data).unwrap()[0].inflate_header().unwrap()
}

#[test]
fn marvinblue_sections_end_where_imhex_says() {
    let Some(dir) = corpus() else {
        return eprintln!("GENIE_REC_CORPUS is not set; skipping");
    };
    let header =
        genie_save::parse(&header_of(&dir.join("MarvinBlue/426048731.aoe2record"))).unwrap();
    assert_eq!(
        (
            header.save_version().major,
            header.save_version().build,
            header.save_version().hundredths()
        ),
        (66, 158_041, 6660)
    );
    let ends = header.ends;
    assert_eq!(ends.lobby, expected("lobby_settings"));
    // aoe2rec's replay block ends 8 bytes into the game's map load: the map's 0xdeaddead marker
    // and its format version (RgeMap's header and version), which the pattern counts to the replay
    assert_eq!(ends.replay + 8, expected("replay"));
    assert_eq!(ends.map, expected("map_info"));
    assert_eq!(ends.players_start, expected_start("players"));

    let players = header.players();
    assert_eq!(players.len(), 3);
    // The boundaries inside a player section are checked against the game's own reads
    // (tests/de_oracle.rs); against ImHex, the section starts, ends and counts.
    for (i, p) in players.iter().enumerate() {
        let name = |part: &str| format!("player{i}{part}");
        assert_eq!(
            p.ends().unwrap().start,
            expected_start(&name("")),
            "player {i}"
        );
        assert_eq!(
            p.objects().masters.as_ref().map_or(0, Vec::len),
            expected_count(&name(".initial_objects")),
            "player {i}"
        );
        assert_eq!(
            p.live_objects().count(),
            expected_count(&name(".objects")),
            "player {i}"
        );
    }
    // Every section, Gaia's included, is read to its end, and the next one starts right there. The
    // pattern skips Gaia's tail by searching for player 1; ImHex's offset for player 1 is the check.
    assert_eq!(players[0].ends().unwrap().end, expected_start("player1"));
    for (i, p) in players.iter().enumerate().skip(1) {
        assert_eq!(
            (
                p.sleeping_objects().count(),
                p.objects().doppleganger_objects.objects().count()
            ),
            (0, 0),
            "player {i}"
        );
        assert_eq!(
            p.ends().unwrap().end,
            expected(&format!("player{i}")),
            "player {i}"
        );
    }
}

/// A player's masters by id: the static master of every class, a building's included.
#[test]
fn marvinblue_player_masters_are_found_by_id() {
    let Some(dir) = corpus() else {
        return eprintln!("GENIE_REC_CORPUS is not set; skipping");
    };
    let header =
        genie_save::parse(&header_of(&dir.join("MarvinBlue/426048731.aoe2record"))).unwrap();
    let players = header.players();
    let objects = players[2].objects();
    let static_part = |id| objects.master(id).and_then(|m| m.static_part());
    // FLARE3, a static object with a death sprite and no death spawn
    let flare = static_part(332).unwrap();
    assert_eq!(
        (flare.id(), flare.death_sprite_id, flare.death_spawn_obj_id),
        (332, 1751, -1)
    );
    // the empty town-centre annex, a building
    assert_eq!(static_part(890).map(RgeMasterStaticObject::id), Some(890));
    assert!(objects.master(i16::MAX).is_none());
}

/// An object's own master (one the object carries instead of its player's): Gaia's objects of
/// master 600 in MarvinBlue/426049890 have one.
#[test]
fn an_objects_own_master_is_its_static_master() {
    let Some(dir) = corpus() else {
        return eprintln!("GENIE_REC_CORPUS is not set; skipping");
    };
    let header =
        genie_save::parse(&header_of(&dir.join("MarvinBlue/426049890.aoe2record"))).unwrap();
    let players = header.players();
    let object = |id| {
        players[0]
            .live_objects()
            .filter_map(|o| o.static_part())
            .find(|s| s.id == id)
            .unwrap()
    };
    let own = object(11664).own_master().unwrap();
    assert_eq!(own.read_temp, 600);
    let without = players[0]
        .live_objects()
        .filter_map(|o| o.static_part())
        .find(|s| s.has_own_master_object == 0)
        .unwrap();
    assert!(without.own_master().is_none());
}

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

/// How far the corpus parses. Tier 1 (save 66.6, build 158041, the analysed binary) is strict: every
/// recording must parse. The reader is sequential, so each player section must end where the next
/// one starts. Other versions are only reported.
#[test]
fn every_corpus_header_parses() {
    let Some(dir) = corpus() else {
        return eprintln!("GENIE_REC_CORPUS is not set; skipping");
    };
    let mut paths = Vec::new();
    recordings(&dir, &mut paths);
    let mut tier1 = BTreeMap::<String, usize>::new();
    let mut others = BTreeMap::<String, usize>::new();
    let mut tier1_failures = Vec::new();
    for path in &paths {
        let data = read(path).unwrap();
        let chapter = de::chapters(&data).unwrap()[0];
        let version = chapter.save_version().unwrap();
        let is_tier1 = (version.hundredths(), version.build) == (6660, 158_041);
        let outcome = match genie_save::parse(&chapter.inflate_header().unwrap()) {
            Ok(_) => "parsed".to_string(),
            Err(e) => {
                if is_tier1 {
                    tier1_failures.push(format!(
                        "{}: {e}",
                        path.strip_prefix(&dir).unwrap().display()
                    ));
                }
                if e.kind() == genie_save::ErrorKind::Unsupported {
                    format!("unsupported: {}", e.reason())
                } else {
                    "failed".to_string()
                }
            }
        };
        *(if is_tier1 { &mut tier1 } else { &mut others })
            .entry(outcome)
            .or_default() += 1;
    }
    eprintln!("tier 1: {tier1:?}\nother versions: {others:?}");
    assert!(tier1_failures.is_empty(), "{tier1_failures:#?}");
    assert_eq!(tier1.get("parsed"), Some(&137));
}
