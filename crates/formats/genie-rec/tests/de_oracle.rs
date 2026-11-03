//! The DE header reader against the game's own reads (FD-014 load oracle).
//!
//! The oracle (`aoe2de-exe-re/load_oracle`, `just load-oracle`) runs build 158041's header load in an
//! emulator and writes one trace per recording: the size and call site of every stream read, in
//! order. Each read of this reader must nest inside one read of the game, or cover whole reads of the
//! game; a read that overlaps a game read partially is a misparse. Needs `GENIE_REC_CORPUS` (the
//! recordings) and `GENIE_REC_TRACES` (the `.trace` files, named after the path below the corpus);
//! skips without them.

use binrw::io::{Cursor, Read, Seek, SeekFrom};
use flate2::read::ZlibDecoder;
use genie_rec::de::chapters;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::env::var_os;
use std::fmt::Write as _;
use std::fs::{read as fs_read, read_dir};
use std::io::Error;
use std::path::{Path, PathBuf};

/// A cursor that records the position and length of every read.
struct LoggingReader<'a> {
    inner: Cursor<&'a [u8]>,
    reads: Vec<(u64, u64)>,
}

impl Read for LoggingReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let pos = self.inner.position();
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.reads.push((pos, n as u64));
        }
        Ok(n)
    }
}

impl Seek for LoggingReader<'_> {
    fn seek(&mut self, to: SeekFrom) -> Result<u64, Error> {
        self.inner.seek(to)
    }
}

/// The game's reads of one header, in order from offset 0: sizes, call sites and the call sites one
/// frame up (RVAs). The trace holds more frames; this check needs two.
struct Trace {
    sizes: Vec<u32>,
    sites: Vec<u32>,
    callers: Vec<u32>,
}

fn read_trace(path: &Path) -> Trace {
    let data = fs_read(path).unwrap();
    assert_eq!(&data[..8], b"GRTRACE3", "{}: not a trace", path.display());
    let word_at = |b: &[u8], i: usize| u32::from_le_bytes(b[4 * i..4 * i + 4].try_into().unwrap());
    let (count, width) = (
        word_at(&data[8..], 0) as usize,
        word_at(&data[8..], 1) as usize,
    );
    let mut rows = Vec::new();
    // The row count is checked against `rows.len()` below.
    let _ = ZlibDecoder::new(&data[16..])
        .read_to_end(&mut rows)
        .unwrap();
    let step = 1 + width;
    assert_eq!(
        rows.len(),
        count * step * 4,
        "{}: truncated",
        path.display()
    );
    let column = |c: usize| (0..count).map(|i| word_at(&rows, i * step + c)).collect();
    Trace {
        sizes: column(0),
        sites: if width > 0 { column(1) } else { vec![0; count] },
        callers: if width > 1 { column(2) } else { vec![0; count] },
    }
}

/// Where a read of ours overlaps a read of the game partially.
#[derive(Debug, PartialEq)]
struct Crossing {
    ours: (u64, u64),
    /// The game read it crosses: offset, size, index in the trace.
    theirs: (u64, u64, usize),
}

/// The first of `ours` (position, length) that neither nests inside one read of `game_sizes` (read
/// back to back from offset 0) nor covers whole game reads; also a read past the game's last byte.
fn first_crossing(ours: &[(u64, u64)], game_sizes: &[u32]) -> Option<Crossing> {
    crossings(ours, game_sizes).into_iter().next()
}

/// Every read of `ours` that crosses a game read (see `first_crossing`). After a misparse the two
/// readers can fall back into step, so later crossings are separate findings.
fn crossings(ours: &[(u64, u64)], game_sizes: &[u32]) -> Vec<Crossing> {
    let mut found = Vec::new();
    let mut starts = Vec::with_capacity(game_sizes.len() + 1);
    let mut at = 0u64;
    for &s in game_sizes {
        starts.push(at);
        at += u64::from(s);
    }
    let end = at;
    starts.push(end);
    let is_boundary = |x: u64| starts.binary_search(&x).is_ok();
    for &(pos, len) in ours {
        let stop = pos + len;
        let i = match starts.binary_search(&pos) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let theirs = || {
            let i = i.min(game_sizes.len().saturating_sub(1));
            (starts[i], u64::from(game_sizes[i]), i)
        };
        if stop > end {
            found.push(Crossing {
                ours: (pos, len),
                theirs: theirs(),
            });
            break;
        }
        let inside = stop <= starts[i + 1];
        let covers = pos == starts[i] && is_boundary(stop);
        if !inside && !covers {
            found.push(Crossing {
                ours: (pos, len),
                theirs: theirs(),
            });
        }
    }
    found
}

#[test]
fn reads_that_nest_or_cover_whole_game_reads_agree() {
    let game = [4u32, 2, 8];
    // inside one read, a whole read, two whole reads, a peek of one byte
    assert_eq!(
        first_crossing(&[(0, 2), (2, 2), (4, 2), (6, 8), (0, 6), (6, 1)], &game),
        None
    );
}

#[test]
fn read_that_straddles_game_boundary_is_crossing() {
    let game = [4u32, 2, 8];
    assert_eq!(
        first_crossing(&[(0, 4), (2, 4)], &game),
        Some(Crossing {
            ours: (2, 4),
            theirs: (0, 4, 0)
        })
    );
    // starts at a boundary but ends inside a game read
    assert_eq!(
        first_crossing(&[(4, 5)], &game),
        Some(Crossing {
            ours: (4, 5),
            theirs: (4, 2, 1)
        })
    );
    // past the last byte the game read
    assert_eq!(
        first_crossing(&[(10, 6)], &game),
        Some(Crossing {
            ours: (10, 6),
            theirs: (6, 8, 2)
        })
    );
}

/// Both readers' reads within `radius` bytes of `at`, with the bytes: the context of a crossing.
fn context(header: &[u8], ours: &[(u64, u64)], trace: &Trace, at: u64, radius: u64) -> String {
    let window = |pos: u64, len: u64| pos + len + radius > at && pos < at + radius;
    let hex = |pos: u64, len: u64| {
        let end = (pos + len).min(header.len() as u64).min(pos + 12);
        let range = usize::try_from(pos).unwrap()..usize::try_from(end).unwrap();
        let b: Vec<String> = header[range].iter().map(|b| format!("{b:02x}")).collect();
        b.join(" ")
    };
    let mut out = String::from("  game reads (offset +size bytes, caller RVA):\n");
    let mut pos = 0u64;
    for (i, &size) in trace.sizes.iter().enumerate() {
        let len = u64::from(size);
        if window(pos, len) {
            writeln!(
                out,
                "    {pos:>8} +{len:<4} {:<36} {:#x}",
                hex(pos, len),
                trace.callers[i]
            )
            .unwrap();
        }
        pos += len;
        if pos > at + radius {
            break;
        }
    }
    out += "  our reads:\n";
    for &(pos, len) in ours.iter().filter(|&&(p, l)| window(p, l)) {
        writeln!(out, "    {pos:>8} +{len:<4} {}", hex(pos, len)).unwrap();
    }
    out
}

fn env_dir(name: &str) -> Option<PathBuf> {
    var_os(name).map(PathBuf::from)
}

fn recordings(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            recordings(&path, out);
        } else if path.extension().is_some_and(|e| e == "aoe2record") {
            out.push(path);
        }
    }
}

/// The trace file name of a recording: its path below the corpus, `__` for separators.
fn trace_name(path: &Path, corpus: &Path) -> String {
    let rel = path.strip_prefix(corpus).unwrap();
    let parts: Vec<_> = rel
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    format!("{}.trace", parts.join("__"))
}

// One end-to-end check over the corpus, with its report in one place.
#[allow(clippy::too_many_lines)]
#[test]
fn the_reader_agrees_with_the_games_reads_on_every_traced_recording() {
    let (Some(corpus), Some(traces)) = (env_dir("GENIE_REC_CORPUS"), env_dir("GENIE_REC_TRACES"))
    else {
        eprintln!("GENIE_REC_CORPUS or GENIE_REC_TRACES not set; skipping");
        return;
    };
    let mut paths = Vec::new();
    recordings(&corpus, &mut paths);
    paths.sort();
    let (mut checked, mut crossed, mut stops) = (0, Vec::new(), Vec::new());
    let mut failures = Vec::new();
    let mut first_context = None;
    let (mut read_by_us, mut read_by_game) = (0u64, 0u64);
    // crossings per caller of the crossed game read: which routines this reader misreads
    let mut by_caller = BTreeMap::<u32, (usize, usize)>::new();
    for path in paths {
        let trace_path = traces.join(trace_name(&path, &corpus));
        if !trace_path.exists() {
            continue;
        }
        let data = fs_read(&path).unwrap();
        let header = chapters(&data).unwrap()[0].inflate_header().unwrap();
        let trace = read_trace(&trace_path);
        let mut reader = LoggingReader {
            inner: Cursor::new(&header),
            reads: Vec::new(),
        };
        let parsed = genie_save::parse_reader(&mut reader);
        if let Err(e) = parsed {
            // the reads before the failure still say where this reader left the game's
            if failures.is_empty() {
                let at = reader.reads.last().map_or(0, |&(p, l)| p + l);
                failures.push(format!(
                    "{}: {e:?}\n{}",
                    path.display(),
                    context(&header, &reader.reads, &trace, at, 48)
                ));
            } else {
                failures.push(format!("{}: {e:?}", path.display()));
            }
        }
        checked += 1;
        let total = trace.sizes.iter().map(|&s| u64::from(s)).sum::<u64>();
        read_by_us += reader.inner.position();
        read_by_game += total;
        stops.push((reader.inner.position(), total, path.clone()));
        let all = crossings(&reader.reads, &trace.sizes);
        let mut callers_here = BTreeSet::new();
        for c in &all {
            let e = by_caller.entry(trace.callers[c.theirs.2]).or_default();
            e.0 += 1;
            if callers_here.insert(trace.callers[c.theirs.2]) {
                e.1 += 1;
            }
        }
        if let Some(c) = all.into_iter().next() {
            if first_context.is_none() {
                first_context = Some(context(&header, &reader.reads, &trace, c.ours.0, 24));
            }
            let (site, caller) = (trace.sites[c.theirs.2], trace.callers[c.theirs.2]);
            crossed.push(format!(
                "{}: our read {:?} crosses the game's read at {:#x} (+{}, site {:#x} from {:#x}; RVAs)",
                path.display(),
                c.ours,
                c.theirs.0,
                c.theirs.1,
                site,
                caller
            ));
        }
    }
    eprintln!(
        "{checked} traced recordings; this reader reads {read_by_us} of the game's {read_by_game} bytes"
    );
    stops.sort_by_key(|(at, total, _)| total - at);
    for (at, total, path) in stops.iter().take(3) {
        eprintln!("  stops at {at:#x} of {total:#x}: {}", path.display());
    }
    let mut ranked: Vec<_> = by_caller.into_iter().collect();
    ranked.sort_by_key(|(_, (n, _))| Reverse(*n));
    eprintln!("  crossed game reads come from {} callers", ranked.len());
    // GENIE_REC_ALL_CALLERS lists every caller, not only the first twelve
    let shown = if var_os("GENIE_REC_ALL_CALLERS").is_some() {
        ranked.len()
    } else {
        12
    };
    for (caller, (n, recs)) in ranked.iter().take(shown) {
        eprintln!("  {n:>6} crossings in {recs:>3} recordings at game reads from {caller:#x}");
    }
    assert!(checked > 0, "no trace in {}", traces.display());
    if !failures.is_empty() {
        eprintln!(
            "{} recordings fail to parse, e.g.\n  {}",
            failures.len(),
            failures[0]
        );
    }
    assert!(
        crossed.is_empty() && failures.is_empty(),
        "{} recordings cross the game's reads:\n{}\nfirst one:\n{}",
        crossed.len(),
        crossed
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n"),
        first_context.unwrap_or_default()
    );
}
