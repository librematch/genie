//! The unit lines (`resources/_common/dat/unitlines.json`): each line names the masters of one
//! upgrade chain, its first master first. The game builds two tables from the file once, at its
//! first world load (`FUN_140fde680`, from the load `FUN_141533cb0` with Gaia's master count):
//! one entry a master (`DAT_143cc2968`, 12 bytes: the next master of its chain, its line, its
//! line's `Building` flag) and one entry a line (`DAT_143cc2988`, 200 ints by line + 399: the
//! chain's first master). The object tracker files a unit under its line's first master
//! (`FUN_141658430`), and a garrison into the buildings near its target looks for those of the
//! target's line (`FUN_14161e370`).
//!
//! 158041: the builder `FUN_140fde680`, a master's line `FUN_140fdd6c0`, a line's first master
//! `FUN_140fdd6f0`.

use crate::objects::masters::MasterId;
use serde::Deserialize;
use serde::Serialize;

/// Table 2's entries (`DAT_143cc2990`, the builder's 200).
pub const LINES: usize = 200;

/// A unit line's id (`LineID`, an int): −399 and the 199 after it index table 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LineId(i32);

impl LineId {
    /// The first line id (table 2's index 0): a line's id + 399 indexes table 2.
    pub const FIRST: Self = Self(-399);

    /// The line of the file's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// Table 2's index of the line, when within its 200 entries.
    #[must_use]
    pub fn slot(self) -> Option<usize> {
        let slot = usize::try_from(self.0.checked_sub(Self::FIRST.0)?).ok()?;
        (slot < LINES).then_some(slot)
    }
}

/// One entry of the file's `UnitLines` as the builder reads it. A key the entry leaves out is
/// `None` (`LineID`, `IDChain`) or false (`Building`) or empty (`Identifier`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UnitLine {
    /// `LineID`.
    pub line: Option<LineId>,
    /// `IDChain`: the masters, the line's first first.
    pub chain: Option<Vec<i32>>,
    /// `Building`.
    pub building: bool,
    /// `Identifier`: one string, or each of an array's.
    pub identifiers: Vec<String>,
}

/// A master's entry of table 1 (12 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LineEntry {
    /// `+0`: the master after it in its chain (none for the last, the game's −1).
    pub next: Option<MasterId>,
    /// `+4`: its line (none, the game's −1).
    pub line: Option<LineId>,
    /// `+8`: its line's `Building` flag.
    pub building: bool,
}

impl LineEntry {
    /// The entry of a master in no line (the builder's fill: −1, −1, 0).
    pub const NONE: Self = Self {
        next: None,
        line: None,
        building: false,
    };
}

/// The two tables and the identifiers' list (`DAT_143f15808`: each line's identifiers with its
/// id). Empty, as the game's when the file is missing, until [`UnitLines::build`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UnitLines {
    /// Table 1, by master id.
    pub entries: Vec<LineEntry>,
    /// Table 2, by line + 399: the line's first master (none, the game's −1).
    pub firsts: Vec<Option<MasterId>>,
    /// The identifiers, with their line, in the file's order.
    pub identifiers: Vec<(LineId, String)>,
}

impl UnitLines {
    /// `FUN_140fde680` with `master_count` (Gaia's, `player+0x20`) on the file's `lines`: table 1
    /// holds `master_count` entries of no line and table 2 200 lines of no master. Each entry
    /// with both `IDChain` and `LineID` whose line + 399 is within table 2 fills its chain from
    /// the last master to the first: a master id out of the masters' range (0 to the count, a
    /// short) is left out (and the link passes over it); each other takes its line, the `Building` flag and the master after
    /// it, and the first (index 0) becomes the line's first master. Its identifiers then join
    /// the list with its line. A later line's entry for a master replaces an earlier one's. The
    /// game logs the entries it leaves out.
    #[must_use]
    pub fn build(master_count: i32, lines: &[UnitLine]) -> Self {
        let count = usize::try_from(master_count).unwrap_or(0);
        let mut out = Self {
            entries: vec![LineEntry::NONE; count],
            firsts: vec![None; LINES],
            identifiers: Vec::new(),
        };
        for unit_line in lines {
            let (Some(line), Some(chain)) = (unit_line.line, unit_line.chain.as_ref()) else {
                // "Unit line is incorrectly defined and is missing a key variable"
                continue;
            };
            let Some(first) = line.slot().and_then(|slot| out.firsts.get_mut(slot)) else {
                // "Unit line ID is out of bounds (%d)"
                continue;
            };
            let mut next = None;
            for (index, &id) in chain.iter().enumerate().rev() {
                let Some((master, line_entry)) = i16::try_from(id)
                    .ok()
                    .filter(|_| id < master_count)
                    .and_then(MasterId::of_data)
                    .and_then(|master_id| {
                        Some((master_id, out.entries.get_mut(master_id.index())?))
                    })
                else {
                    // "Unit type ID is out of bounds (%d)"
                    continue;
                };
                if index == 0 {
                    *first = Some(master);
                }
                *line_entry = LineEntry {
                    next,
                    line: Some(line),
                    building: unit_line.building,
                };
                next = Some(master);
            }
            out.identifiers.extend(
                unit_line
                    .identifiers
                    .iter()
                    .map(|text| (line, text.clone())),
            );
        }
        out
    }

    /// `FUN_140fdd6c0`: the line of master `master` (none, the game's −1, for an id out of
    /// table 1 or in no line).
    #[must_use]
    pub fn line_of(&self, master: MasterId) -> Option<LineId> {
        self.entries
            .get(master.index())
            .and_then(|line_entry| line_entry.line)
    }

    /// `FUN_140fdd6f0`: the first master of line `line` (none, the game's −1, for a line out
    /// of table 2 or without one). Table 2 has its 200 entries once built; before, it holds
    /// none.
    #[must_use]
    pub fn first_of(&self, line: LineId) -> Option<MasterId> {
        line.slot()
            .and_then(|s| self.firsts.get(s))
            .copied()
            .flatten()
    }

    /// The master a line stands for (`FUN_141658430`'s and `FUN_14161e370`'s reading): the first
    /// of `master`'s line, or `master` itself in none (a line without a first master gives
    /// none, the game's −1).
    #[must_use]
    pub fn base(&self, master: MasterId) -> Option<MasterId> {
        // the builder files only the masters' range's ids (0 or more) as a first master
        self.line_of(master)
            .map_or(Some(master), |line| self.first_of(line))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_ids::master_id;

    fn line(id: i32, chain: &[i32], building: bool, names: &[&str]) -> UnitLine {
        UnitLine {
            line: Some(LineId::of_data(id)),
            chain: Some(chain.to_vec()),
            building,
            identifiers: names.iter().map(|text| (*text).to_owned()).collect(),
        }
    }

    #[test]
    fn chain_maps_each_master_to_its_line_its_first_and_the_next() {
        // the archer line (-299: 4, 24, 492) and the tower line (-398: 79, 234, 235)
        let lines = UnitLines::build(
            600,
            &[
                line(-299, &[4, 24, 492], false, &["archer-line"]),
                line(-398, &[79, 234, 235], true, &["watch-tower-line"]),
            ],
        );
        assert_eq!(lines.entries.len(), 600);
        assert_eq!(lines.firsts.len(), LINES);
        assert_eq!(
            lines.entries[24],
            LineEntry {
                next: Some(master_id(492)),
                line: Some(LineId::of_data(-299)),
                building: false
            }
        );
        assert_eq!(lines.entries[492].next, None);
        assert_eq!(lines.entries[4].next, Some(master_id(24)));
        assert!(lines.entries[234].building);
        assert_eq!(
            (
                lines.line_of(master_id(492)),
                lines.first_of(LineId::of_data(-299)),
                lines.base(master_id(492))
            ),
            (
                Some(LineId::of_data(-299)),
                Some(master_id(4)),
                Some(master_id(4))
            )
        );
        assert_eq!(
            (lines.base(master_id(235)), lines.base(master_id(83))),
            (Some(master_id(79)), Some(master_id(83)))
        );
        assert_eq!(
            lines.identifiers,
            vec![
                (LineId::of_data(-299), "archer-line".to_owned()),
                (LineId::of_data(-398), "watch-tower-line".to_owned())
            ]
        );
    }

    #[test]
    fn ids_out_of_the_masters_range_are_left_out_and_the_links_pass_over_them() {
        // chain 7, -3, 900, 6 with 600 masters: 6's next is -1, 7's next is 6; the first (7) is
        // in range, so it is the line's first
        let lines = UnitLines::build(600, &[line(-297, &[7, -3, 900, 6], false, &[])]);
        assert_eq!(lines.entries[7].next, Some(master_id(6)));
        assert_eq!(lines.first_of(LineId::of_data(-297)), Some(master_id(7)));
        // a chain whose index 0 is out of range leaves the line without a first master
        let lines = UnitLines::build(600, &[line(-296, &[600, 75], false, &[])]);
        assert_eq!(lines.first_of(LineId::of_data(-296)), None);
        assert_eq!(lines.line_of(master_id(75)), Some(LineId::of_data(-296)));
        assert_eq!(lines.base(master_id(75)), None);
        // an id at the master count is out (the builder's `local_208 <= id`)
        assert_eq!(lines.line_of(master_id(600)), None);
    }

    #[test]
    fn line_out_of_table_two_or_without_key_is_skipped_whole() {
        let lines = UnitLines::build(
            100,
            &[
                line(-400, &[1], false, &["low"]),
                line(-199, &[2], false, &["high"]),
                UnitLine {
                    line: Some(LineId::of_data(-299)),
                    chain: None,
                    ..UnitLine::default()
                },
                UnitLine {
                    line: None,
                    chain: Some(vec![3]),
                    ..UnitLine::default()
                },
                line(-200, &[4], false, &["last"]),
            ],
        );
        assert_eq!(
            [1, 2, 3, 4].map(|m| lines.line_of(master_id(m))),
            [None, None, None, Some(LineId::of_data(-200))]
        );
        assert_eq!(
            lines.identifiers,
            vec![(LineId::of_data(-200), "last".to_owned())]
        );
        // an empty chain still lists its identifiers
        let lines = UnitLines::build(100, &[line(-250, &[], false, &["empty"])]);
        assert_eq!(
            lines.identifiers,
            vec![(LineId::of_data(-250), "empty".to_owned())]
        );
        assert_eq!(lines.first_of(LineId::of_data(-250)), None);
    }

    #[test]
    fn later_line_replaces_an_earlier_lines_entry_of_master() {
        let lines = UnitLines::build(
            100,
            &[
                line(-290, &[10, 11], false, &[]),
                line(-280, &[12, 11], true, &[]),
            ],
        );
        assert_eq!(
            lines.entries[11],
            LineEntry {
                next: None,
                line: Some(LineId::of_data(-280)),
                building: true
            }
        );
        // the first line keeps its first master
        assert_eq!(lines.first_of(LineId::of_data(-290)), Some(master_id(10)));
    }

    #[test]
    fn unbuilt_tables_hold_no_line() {
        let lines = UnitLines::default();
        assert_eq!(
            (
                lines.line_of(master_id(4)),
                lines.first_of(LineId::of_data(-299)),
                lines.base(master_id(4))
            ),
            (None, None, Some(master_id(4)))
        );
    }
}
