//! FD-016's probe: read each file with its genie crate and, where the crate writes, check that it
//! writes the file back byte for byte. One JSON object per file on stdout:
//! `{"file", "read", "roundtrip" (true, false or null), "error"}`.
//!
//! Usage: `cargo run --release -p genie-formats --example probe -- KIND FILE...`, KIND one of
//! dat-de, json-table, scenario, campaign, palette, strings-keyvalue (scripts/formats.toml's
//! `probe`).

use flate2::read::DeflateDecoder;
use std::io::{Cursor, Read};

/// What the probe found for one file.
struct Outcome {
    read: bool,
    roundtrip: Option<bool>,
    error: Option<String>,
}

fn failed(e: impl std::fmt::Display) -> Outcome {
    Outcome {
        read: false,
        roundtrip: None,
        error: Some(e.to_string()),
    }
}

fn dat_de(bytes: &[u8]) -> Outcome {
    let mut data = Vec::new();
    if let Err(e) = DeflateDecoder::new(bytes).read_to_end(&mut data) {
        return failed(e);
    }
    match genie_dat::de::DatFile::from_inflated(&data) {
        Ok(dat) => Outcome {
            read: true,
            roundtrip: Some(dat.to_bytes().is_ok_and(|b| b == data)),
            error: None,
        },
        Err(e) => failed(e),
    }
}

fn scenario(bytes: &[u8]) -> Outcome {
    match genie_scx::Scenario::read_from(Cursor::new(bytes)) {
        Ok(s) => {
            let mut out = Vec::new();
            let written = s.write_to(&mut out);
            Outcome {
                read: true,
                roundtrip: Some(written.is_ok() && out == bytes),
                error: written.err().map(|e| format!("write: {e}")),
            }
        }
        Err(e) => failed(e),
    }
}

fn campaign(bytes: &[u8]) -> Outcome {
    let mut c = match genie_cpx::Campaign::from(Cursor::new(bytes)) {
        Ok(c) => c,
        Err(e) => return failed(e),
    };
    // the archive's header and index, then every scenario in it
    for i in 0..c.len() {
        if let Err(e) = c.by_index(i) {
            return failed(format!("scenario {i}: {e}"));
        }
    }
    let mut out = Vec::new();
    let written = c.write_to(&mut out);
    Outcome {
        read: true,
        roundtrip: Some(written.is_ok() && out == bytes),
        error: written.err().map(|e| format!("write: {e}")),
    }
}

fn palette(bytes: &[u8]) -> Outcome {
    match jascpal::Palette::read_from(Cursor::new(bytes)) {
        Ok(p) => {
            let mut out = Vec::new();
            let written = p.write_to(&mut out);
            Outcome {
                read: true,
                roundtrip: Some(written.is_ok() && out == bytes),
                error: written.err().map(|e| format!("write: {e}")),
            }
        }
        Err(e) => failed(e),
    }
}

fn strings_keyvalue(bytes: &[u8]) -> Outcome {
    match genie_lang::LangFileType::KeyValue.read_from(Cursor::new(bytes)) {
        Ok(lang) => {
            let mut out = Vec::new();
            let written = lang.write_to_keyval(&mut out);
            Outcome {
                read: true,
                roundtrip: Some(written.is_ok() && out == bytes),
                error: written.err().map(|e| format!("write: {e}")),
            }
        }
        Err(e) => failed(e),
    }
}

/// A typed JSON table: read it, write it, and compare the written file's value with the original's
/// (the installed files differ in indentation, so bytes are not compared).
fn json_table<T>(bytes: &[u8]) -> Outcome
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(e) => return failed(e),
    };
    let table: T = match genie_json::from_str(text) {
        Ok(t) => t,
        Err(e) => return failed(e),
    };
    let same = genie_json::to_string(&table).ok().and_then(|w| {
        let a: serde_json::Value = serde_json::from_str(text).ok()?;
        let b: serde_json::Value = serde_json::from_str(&w).ok()?;
        Some(a == b)
    });
    Outcome {
        read: true,
        roundtrip: Some(same == Some(true)),
        error: None,
    }
}

/// The JSON tables genie-json types, by file name.
fn json(file: &str, bytes: &[u8]) -> Outcome {
    let name = file.rsplit('/').next().unwrap_or(file);
    match name {
        "linkedTechs.json" => json_table::<genie_json::LinkedTechs>(bytes),
        "dropsites.json" => json_table::<genie_json::DropSites>(bytes),
        "futuravailableunits.json" | "paphosfutureavailableunits.json" => {
            json_table::<genie_json::FutureUnits>(bytes)
        }
        "maps.json" => json_table::<genie_json::Maps>(bytes),
        "buttons.json" => json_table::<genie_json::Buttons>(bytes),
        _ => failed("no typed reader for this table"),
    }
}

/// A JSON string literal.
fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let kind = args.next().expect("a probe kind");
    let probe: fn(&str, &[u8]) -> Outcome = match kind.as_str() {
        "json-table" => json,
        "dat-de" => |_, b| dat_de(b),
        "scenario" => |_, b| scenario(b),
        "campaign" => |_, b| campaign(b),
        "palette" => |_, b| palette(b),
        "strings-keyvalue" => |_, b| strings_keyvalue(b),
        other => panic!("unknown probe kind {other}"),
    };
    for file in args {
        let outcome = match std::fs::read(&file) {
            // an old reader may panic on a version it does not know; that is a failed read
            Ok(bytes) => std::panic::catch_unwind(|| probe(&file, &bytes)).unwrap_or_else(|p| {
                failed(format!(
                    "panic: {}",
                    p.downcast_ref::<String>()
                        .cloned()
                        .or(p.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_default()
                ))
            }),
            Err(e) => failed(e),
        };
        let roundtrip = outcome
            .roundtrip
            .map_or("null".to_string(), |b| b.to_string());
        let error = outcome.error.map_or("null".to_string(), |e| quote(&e));
        println!(
            "{{\"file\": {}, \"read\": {}, \"roundtrip\": {roundtrip}, \"error\": {error}}}",
            quote(&file),
            outcome.read
        );
    }
}
