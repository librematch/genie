//! The world save of Age of Empires II: Definitive Edition (save 66.x and later), read and written
//! to its last byte.
//!
//! The game saves its world in one format: the save-game file header ([`SaveGameFileHeader`]: the
//! version, the build, the lobby's `GameOptions`, the save's name and seed), then the world
//! (`World::save`, read back by `World::load_world`), the map and every player's objects included.
//! Three containers hold it:
//!
//! - a recorded game: every chapter starts with one, deflated (genie-rec reads the chapters);
//! - a saved game (`.aoe2spgame`, `.aoe2mpgame`);
//! - the tick oracle's snapshots (workspace FD-015), which the game writes through `World::save`.
//!
//! The layout is generated from build 158041's load routines (workspace `aoe2de-exe-re/layout`,
//! FD-014) into [`generated::world`]. Every field's boundary is checked against the game's own
//! reads, and a parsed save writes back byte for byte (genie-rec's `tests/de_oracle.rs` and
//! `tests/de_roundtrip.rs`, over the recordings' chapters). The generated version gates compare
//! the raw 16.16 save version ([`Header::raw_version`]).
//!
//! The errors are [`GenieError`]s. Bytes that are not a save this reader takes (too old, cut
//! short, not the layout) are [`ErrorKind::InvalidInput`]; a part of the layout the generator
//! does not derive yet [`ErrorKind::Unsupported`], its place in the context (`at`, a byte offset
//! into the save); a failed write [`ErrorKind::Io`].

// the 16.16 version's halves and the stream offsets (a save is far below 4 GiB) are cut on purpose
#![allow(clippy::cast_possible_truncation)]
#![warn(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::unimplemented,
    clippy::todo
)]

pub mod generated;
mod layout_shapes;
mod objects;
mod player;

pub use generated::world::{
    BaseWorld, GameOptions, Object, ObjectList, Player, PlayerInfo, PlayerObjects, RgeMap, RgeTile,
    SaveGameFileHeader, World, WorldPlayer,
};
pub use layout_shapes::{
    LenString16, LenString32, LenVec32, Positioned, STRING_SIGNATURE, SigString,
};
pub use layout_shapes::{Unsupported, unsupported_in};
pub use player::{PlayerEnds, PlayerSection};

use binrw::BinRead;
use binrw::io::{Read, Seek, SeekFrom};

/// The oldest save version this reader supports (66.0).
pub const MIN_HUNDREDTHS: u32 = 6600;

use binrw::io::{Cursor, Write};
use genie_error::GenieResult;
pub use genie_error::{ErrorKind, GenieError};

/// The version fields at the start of a save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveVersion {
    /// The whole part, 66 for save 66.6.
    pub major: u16,
    /// A binary fraction: 39321 / 65536 is the ".6" of save 66.6.
    pub minor: u16,
    /// The engine build (`de.build`), the UI build + 0x20000.
    pub build: u32,
}

impl SaveVersion {
    /// The save version in hundredths, e.g. 6660 for 66.6.
    #[must_use]
    pub fn hundredths(&self) -> u32 {
        // the engine rounds the fraction to two decimals (66.6 -> 6660)
        u32::from(self.major) * 100 + (u32::from(self.minor) * 100 + 32768) / 65536
    }
}

/// Where the sections of the save end, for checks against other readers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SectionEnds {
    /// The save-game file header ends (aoe2rec's lobby settings); the world starts (aoe2rec's
    /// `ai_config` is the world's first field, `loadAIRules`, and the AI scripts it announces).
    pub lobby: usize,
    /// `BaseWorld`'s fields before the map (aoe2rec's `Replay`).
    pub replay: usize,
    /// The map ends.
    pub map: usize,
    /// The first player section starts here.
    pub players_start: usize,
}

/// A world save: the save-game file header, then the world (`World::load_world`), both generated
/// from the game's load code, to the save's last byte.
///
/// Its default is all zeros: a test's save, which [`parse`] never gives.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Header {
    /// The save-game file header: version, build, the lobby's options.
    pub file_header: SaveGameFileHeader,
    /// The world, to the save's last byte.
    pub world: World,
    /// Where its sections end.
    pub ends: SectionEnds,
}

impl Header {
    /// The save version and build: 66.6 and 158041 for the corpus's tier 1.
    #[must_use]
    pub const fn save_version(&self) -> SaveVersion {
        let raw = self.raw_version();
        SaveVersion {
            major: (raw >> 16) as u16,
            minor: raw as u16,
            build: self.file_header.game_build_number,
        }
    }

    /// The version as the game compares it: 16.16 fixed point, e.g. `0x429999` for 66.6 (158041
    /// `DAT_143cd8948`, which the playback entry sets from this field). The generated readers'
    /// version gates take this value.
    #[must_use]
    pub const fn raw_version(&self) -> u32 {
        self.file_header.new_save_game_version
    }

    /// The player sections, Gaia first, with where each starts ([`PlayerSection::ends`] gives
    /// where it ends).
    ///
    /// A player of a type no creator handles is left out; [`parse`] accepts none, so a parsed
    /// save lists them all.
    #[must_use]
    pub fn players(&self) -> Vec<PlayerSection<'_>> {
        self.world
            .base_world
            .players
            .iter()
            .filter_map(|p| PlayerSection::new(p.pos as usize, &p.val).ok())
            .collect()
    }
}

/// The bytes a value writes back: as many as it read.
pub(crate) fn written_len<T: binrw::BinWrite>(value: &T) -> GenieResult<usize>
where
    for<'a> T::Args<'a>: Default,
{
    let mut out = Cursor::new(Vec::new());
    value
        .write_le(&mut out)
        .map_err(|e| GenieError::io("a value that does not write back").with("binrw", e))?;
    Ok(out.into_inner().len())
}

/// Read a world save (a recording's chapter header once inflated).
///
/// # Errors
///
/// [`ErrorKind::InvalidInput`] for bytes that are not a save of 66.0 or later or do not end
/// where the world does, [`ErrorKind::Unsupported`] for a part the reader does not derive yet.
pub fn parse(save: &[u8]) -> GenieResult<Header> {
    parse_reader(&mut Cursor::new(save))
}

/// Read a world save from any seekable reader, e.g. one that logs its reads. The world reads to
/// the save's last byte; bytes left after it are an error.
///
/// # Errors
///
/// As [`parse`], and for a reader that fails.
pub fn parse_reader<R: Read + Seek>(r: &mut R) -> GenieResult<Header> {
    // The save version (16.16) at offset 12, peeked: the file header's own gates are decided for the
    // supported versions, and it brings the version the world's gates compare.
    let start = position(r)?;
    let mut peek = [0u8; 16];
    r.read_exact(&mut peek).map_err(|e| read_error(&e.into()))?;
    let raw = u32::from_le_bytes([peek[12], peek[13], peek[14], peek[15]]);
    let v = SaveVersion {
        major: (raw >> 16) as u16,
        minor: raw as u16,
        build: 0,
    }
    .hundredths();
    if v < MIN_HUNDREDTHS {
        return Err(GenieError::invalid_input("a save older than 66.0").with("version", v));
    }
    let _ = r
        .seek(SeekFrom::Start(start as u64))
        .map_err(|e| read_error(&e.into()))?;
    let file_header = SaveGameFileHeader::read_args(r, (raw,)).map_err(|e| read_error(&e))?;
    let lobby_end = position(r)?;
    let world =
        World::read_args(r, (file_header.new_save_game_version,)).map_err(
            |e| match unsupported_in(&e) {
                Some((unsupported, at)) => unsupported.with("at", format_args!("{at:#x}")),
                None => read_error(&e),
            },
        )?;
    let here = position(r)?;
    let end = r
        .seek(SeekFrom::End(0))
        .map_err(|e| read_error(&e.into()))? as usize;
    if here != end {
        return Err(
            GenieError::invalid_input("a world that ends before the save's end")
                .with("world end", here)
                .with("bytes after it", end - here),
        );
    }
    for p in &world.base_world.players {
        // the game's WorldPlayerFactory.cpp: "InvalidPlayerType: %d"
        let _ = PlayerSection::new(p.pos as usize, &p.val)?;
    }
    let map = &world.base_world.map;
    let ends = SectionEnds {
        lobby: lobby_end,
        replay: map.pos as usize,
        map: map.pos as usize + written_len(&map.val)?,
        players_start: world
            .base_world
            .players
            .first()
            .map_or(here, |p| p.pos as usize),
    };
    Ok(Header {
        file_header,
        world,
        ends,
    })
}

/// Write a save as `parse` read it, in read order. Read, then write gives back the save byte for
/// byte (genie-rec's `tests/de_roundtrip.rs`).
///
/// # Errors
///
/// [`ErrorKind::Io`] when the writer fails or a value cannot be written back.
pub fn write<W: Write + Seek>(h: &Header, w: &mut W) -> GenieResult<()> {
    use binrw::BinWrite;
    (|| {
        h.file_header.write_le(w)?;
        h.world.write_le(w)
    })()
    .map_err(|e| GenieError::io("a save that does not write").with("binrw", e))
}

fn position<R: Seek>(r: &mut R) -> GenieResult<usize> {
    Ok(r.stream_position().map_err(|e| read_error(&e.into()))? as usize)
}

#[track_caller]
fn read_error(e: &binrw::Error) -> GenieError {
    GenieError::invalid_input("a save that does not read").with("binrw", e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::world::AiExpertState;

    #[test]
    fn the_save_version_rounds_its_fraction_to_hundredths() {
        let v = SaveVersion {
            major: 66,
            minor: 39321,
            build: 158_041,
        };
        assert_eq!(v.hundredths(), 6660);
    }

    #[test]
    fn save_older_than_66_is_refused_before_its_header_is_read() {
        let mut save = b"VER 9.4\0".to_vec();
        save.extend((-1.0f32).to_le_bytes());
        save.extend(0x0041_0000u32.to_le_bytes());
        let e = parse(&save).unwrap_err();
        assert_eq!(e, GenieError::invalid_input("a save older than 66.0"));
        assert_eq!(e.context().collect::<Vec<_>>(), [("version", "6500")]);
    }

    /// Save 66.6, build 158041's own version: its fraction in sixteen bits.
    const SAVE_66_6: u32 = 0x0042_9999;

    /// A save of the build's own version, every generated part its default.
    fn default_save() -> Header {
        let mut h = Header::default();
        h.file_header.new_save_game_version = SAVE_66_6;
        h
    }

    fn bytes(h: &Header) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        write(h, &mut out).unwrap();
        out.into_inner()
    }

    /// The generated parts' defaults are a save the reader reads back, byte for byte: its
    /// sections end where the writer put them.
    #[test]
    fn default_save_reads_back_byte_for_byte() {
        let save = bytes(&default_save());
        let read = parse(&save).unwrap();
        assert_eq!(bytes(&read), save);
        assert_eq!(read.save_version().hundredths(), 6660);
        assert_eq!(read.ends.lobby, written_len(&read.file_header).unwrap());
        assert!(read.ends.lobby < read.ends.replay && read.ends.replay < read.ends.map);
        assert_eq!(read.ends.players_start, save.len());
        assert_eq!(read.players(), []);
    }

    #[test]
    fn bytes_after_the_world_are_an_error() {
        let mut save = bytes(&default_save());
        save.push(0);
        let e = parse(&save).unwrap_err();
        assert_eq!(
            e,
            GenieError::invalid_input("a world that ends before the save's end")
        );
        assert_eq!(
            e.context().collect::<Vec<_>>(),
            [
                ("world end", &*(save.len() - 1).to_string()),
                ("bytes after it", "1")
            ]
        );
    }

    /// A save cut inside its file header or its world does not read.
    #[test]
    fn cut_save_does_not_read() {
        let save = bytes(&default_save());
        for len in [20, save.len() - 1] {
            let e = parse(&save[..len]).unwrap_err();
            assert_eq!(
                e,
                GenieError::invalid_input("a save that does not read"),
                "{len}"
            );
        }
        assert!(parse(&save[..10]).is_err());
    }

    /// A player whose type has no class is the game's own error.
    #[test]
    fn player_of_no_known_kind_is_invalid() {
        let mut h = default_save();
        let world = &mut h.world.base_world;
        world.num_players = 1;
        world.player_time_delta = vec![0];
        world.victory_conditions = vec![generated::world::RgeVictoryConditions::default()];
        world.players = vec![Positioned {
            pos: 0,
            val: Player {
                kind: 4,
                ..Player::default()
            },
        }];
        let e = parse(&bytes(&h)).unwrap_err();
        assert_eq!(e, GenieError::invalid_input("InvalidPlayerType"));
        assert_eq!(e.context().next(), Some(("type", "4")));
    }

    /// The state a version `v` AI writes: each part the version enables, zeroed.
    fn ai_state(v: f32) -> AiExpertState {
        use crate::generated::world::{AiExpertStateEntry1, TimerManager};
        let zeroed = |n: usize| Some(vec![0u32; n]);
        let nine = (v >= 0.14).then_some(0u32);
        AiExpertState {
            ai_version: v,
            ai_version_2: (v >= 0.13).then_some(0.0),
            stat_0: nine,
            stat_1: nine,
            stat_2: nine,
            stat_3: nine,
            stat_4: nine,
            stat_5: nine,
            stat_6: nine,
            stat_7: nine,
            stat_8: nine,
            timer_manager: (v >= 0.12).then(|| Box::new(TimerManager::default())),
            table_c070: if v >= 0.09 { zeroed(256) } else { None },
            table_b070: if v >= 0.08 { zeroed(256) } else { None },
            table_b470: if v >= 0.08 { zeroed(256) } else { None },
            unnamed_list_at_0x140b0eaad: (v >= 0.08)
                .then(|| vec![AiExpertStateEntry1::default(); 8]),
        }
    }

    /// A world with AI scripts: one string, one list with one rule and its elements, and the
    /// state a version-0.14 AI writes.
    fn save_with_ai_scripts() -> Header {
        use crate::generated::world::{AiExpertEngine, AiGroupTable, AiList, AiRule, AiString};
        let mut h = default_save();
        h.world.load_ai_rules = 1;
        h.world.ai_scripts = Some(Box::new(AiExpertEngine {
            max_strings: 2,
            num_strings: 1,
            strings: vec![AiString {
                length: 3,
                text: b"abc".to_vec(),
            }],
            max_lists: 2,
            lists: vec![
                AiList {
                    in_use: 1,
                    num_rules: 1,
                    rules: Some(vec![AiRule {
                        // byte 13 counts the rule elements that follow
                        rule: [
                            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        ],
                        list_rule_elements: vec![[3; 24]],
                        ..AiRule::default()
                    }]),
                    ..AiList::default()
                },
                AiList::default(),
            ],
            group_tables: vec![
                AiGroupTable {
                    num_groups: 2,
                    groups: Some(vec![7, 9]),
                    ..AiGroupTable::default()
                },
                AiGroupTable::default(),
            ],
            ..AiExpertEngine::default()
        }));
        h.world.ai_state = Some(Box::new(ai_state(0.14)));
        h
    }

    /// The world's AI scripts (`AIExpertEngine::load`) and their state read back byte for byte,
    /// each in-use list with its rules and its group ids.
    #[test]
    fn the_worlds_ai_scripts_read_back_byte_for_byte() {
        let save = bytes(&save_with_ai_scripts());
        let read = parse(&save).unwrap();
        assert_eq!(bytes(&read), save);
        let ai = read.world.ai_scripts.as_deref().unwrap();
        assert_eq!(ai.strings[0].text, b"abc");
        assert_eq!(ai.lists[0].rules.as_ref().map(Vec::len), Some(1));
        assert_eq!(ai.lists[1].rules, None);
        assert_eq!(ai.group_tables[0].groups.as_deref(), Some(&[7, 9][..]));
        assert_eq!(read.world.ai_state.as_deref().unwrap().ai_version, 0.14);
    }

    /// An AI state older than 0.14 has none of the nine statistics, and one older than 0.08 none
    /// of the tables: the state is as long as its version says.
    #[test]
    fn the_ai_states_parts_follow_its_version() {
        let state_len = |v: f32| {
            let mut h = save_with_ai_scripts();
            h.world.ai_state = Some(Box::new(ai_state(v)));
            let with = bytes(&h).len();
            h.world.ai_state = None;
            with - bytes(&h).len()
        };
        // the version alone, then each part: 0.08 two word tables and the byte table, 0.09 a
        // fourth table, 0.12 the timer manager (8 x 50 words), 0.13 a float, 0.14 nine words
        assert_eq!(state_len(0.02), 4);
        assert_eq!(state_len(0.08), 4 + 2 * 1024 + 8 * 256);
        assert_eq!(state_len(0.09), state_len(0.08) + 1024);
        assert_eq!(state_len(0.12), state_len(0.09) + 8 * 50 * 4);
        assert_eq!(state_len(0.13), state_len(0.12) + 4);
        assert_eq!(state_len(0.14), state_len(0.13) + 9 * 4);
    }
}
