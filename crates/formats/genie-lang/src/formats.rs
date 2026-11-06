//! The three file formats: DLLs, INI files and HD Edition's key-value files, read and
//! written, with the escapes of the text formats.

use crate::LangFile;
use byteorder::{LE, ReadBytesExt};
use encoding_rs::{UTF_16LE, WINDOWS_1252};
use encoding_rs_io::DecodeReaderBytesBuilder;
use genie_error::{GenieError, GenieResult};
use genie_support::StringKey;
use pelite::pe32::{Pe, PeFile};
use pelite::resources::Directory;
use pelite::resources::Name;
use std::io::{BufRead, BufReader, Read, Write};

/// A DLL whose strings do not read.
fn unreadable_dll(error: pelite::Error) -> GenieError {
    GenieError::invalid_input("a language DLL that does not read").with_source(error)
}

/// The first string id of a string table's block: block n holds the 16 ids from (n - 1) × 16
/// on, as Windows' `LoadString` numbers them.
///
/// Returns an error for block 0 and for a block whose first id is past the last id.
fn first_string_id(block: u32) -> GenieResult<u32> {
    block
        .checked_sub(1)
        .and_then(|blocks_before| blocks_before.checked_mul(16))
        .ok_or_else(|| {
            GenieError::invalid_input("a string block id outside the string ids")
                .with("block", block)
        })
}

impl LangFile {
    /// Reads a language file from a .DLL.
    /// This function eagerly loads all the strings into memory.
    ///
    /// Returns an error if the file does not read.
    pub(super) fn read_dll(&mut self, mut input: impl Read) -> GenieResult<()> {
        let mut bytes = vec![];
        let _ = input.read_to_end(&mut bytes)?;
        let pe = PeFile::from_bytes(&bytes).map_err(unreadable_dll)?;
        self.load_pe_file(pe)
    }

    /// TODO specify
    pub(super) fn load_pe_file(&mut self, pe: PeFile<'_>) -> GenieResult<()> {
        let resources = pe.resources().map_err(unreadable_dll)?;
        for root_dir_entry in resources.root().map_err(unreadable_dll)?.entries() {
            if root_dir_entry.name() == Ok(Name::Id(6))
                && let Some(directory) = root_dir_entry.entry().map_err(unreadable_dll)?.dir()
            {
                self.load_pe_directory(directory)?;
            }
        }
        Ok(())
    }

    /// TODO specify
    pub(super) fn load_pe_directory(&mut self, directory: Directory<'_>) -> GenieResult<()> {
        for entry in directory.entries() {
            let base_index = if let Name::Id(id) = entry.name().map_err(unreadable_dll)? {
                first_string_id(id)?
            } else {
                continue;
            };
            if let Some(subdir) = entry.entry().map_err(unreadable_dll)?.dir() {
                for data_entry in subdir.entries() {
                    if let Some(data) = data_entry.entry().map_err(unreadable_dll)?.data() {
                        self.load_pe_data(base_index, data.bytes().map_err(unreadable_dll)?)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Reads one block of a string table: each string is its count of UTF-16 code units,
    /// then the units. The strings take the ids from `first_index` on, one each: an empty
    /// string only counts its id, a string that does not decode is left out.
    ///
    /// Returns an error if a string reaches past the end of the block, or past the last id.
    pub(super) fn load_pe_data(&mut self, first_index: u32, mut data: &[u8]) -> GenieResult<()> {
        let mut indexes = first_index..=u32::MAX;
        while !data.is_empty() {
            let index = indexes.next().ok_or_else(|| {
                GenieError::invalid_input("a string id past the last id").with("first", first_index)
            })?;
            // The length is a count of UTF-16 code units.
            let units = data.read_u16::<LE>()?;
            let (encoded, rest) =
                data.split_at_checked(usize::from(units) * 2)
                    .ok_or_else(|| {
                        GenieError::invalid_input("a string past the end of the strings table")
                            .with("index", index)
                    })?;
            data = rest;
            if encoded.is_empty() {
                continue;
            }
            let (string, _enc, failed) = UTF_16LE.decode(encoded);
            if !failed {
                // A later string with the same index replaces the earlier one.
                let _ = self.0.insert(StringKey::from(index), string.to_string());
            }
        }
        Ok(())
    }

    /// Reads a language file from a .INI file, like the ones used by Voobly and
    /// the aoc-language-ini mod.
    ///
    /// This function eagerly loads all the strings into memory.
    /// At this time, the encoding of the language.ini file is assumed to be
    /// Windows codepage 1252.
    ///
    /// Returns an error if the file does not read.
    pub(super) fn read_ini(&mut self, input: impl Read) -> GenieResult<()> {
        let input = DecodeReaderBytesBuilder::new()
            .encoding(Some(WINDOWS_1252))
            .build(input);
        let input = BufReader::new(input);
        for line in input.lines() {
            self.load_ini_line(&line?)?;
        }
        Ok(())
    }

    /// Parses a single line from an ini language file.
    ///
    /// The key value pair stored in the line is inserted to `self`, if parsed
    /// successfully.
    /// Returns an error if the line's string ID is not a number.
    pub(super) fn load_ini_line(&mut self, line: &str) -> GenieResult<()> {
        if line.starts_with(';') {
            return Ok(());
        }

        let mut split = line.splitn(2, '=');
        let Some(id) = split.next() else {
            return Ok(());
        };
        let Some(value) = split.next() else {
            return Ok(());
        };

        let id: u32 = id.parse().map_err(|parse_int_error| {
            GenieError::invalid_input("a string id that is no number")
                .with("id", id)
                .with_source(parse_int_error)
        })?;
        let value = unescape(value.chars(), false);
        // A later line with the same ID replaces the earlier one.
        let _ = self.0.insert(StringKey::from(id), value);
        Ok(())
    }

    /// Reads a language file from an HD Edition-style key-value file.
    ///
    /// This function loads eagerly all the strings into memory.
    pub(super) fn read_keyval(&mut self, input: impl Read) -> GenieResult<()> {
        let input = BufReader::new(input);
        for line in input.lines() {
            self.load_keyval_line(&line?);
        }
        Ok(())
    }

    /// Parses an HD Edition string line.
    ///
    /// The key value pair stored in the line is inserted to `self`, if parsed
    /// successfully. Lines that do not parse are skipped.
    ///
    /// This is incomplete, unquoting and un-escaping is not yet done.
    pub(super) fn load_keyval_line(&mut self, line: &str) {
        let line = line.trim();
        if line.starts_with("//") || line.is_empty() {
            return;
        }

        let mut iter = line.chars();
        let id: String = iter
            .by_ref()
            .take_while(|&character| !char::is_whitespace(character))
            .collect();
        let string_key = StringKey::from(id);

        // TODO unquoting and un-escaping
        let mut iter = iter.skip_while(|&character| char::is_whitespace(character));
        let value = if iter.next() == Some('"') {
            unescape(iter, true)
        } else {
            return;
        };
        // A later line with the same key replaces the earlier one.
        let _ = self.0.insert(string_key, value);
    }

    /// Writes this language file to an output writer using the ini format.
    ///
    /// Only numeric keys are written; the format has no named keys.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to_ini<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        // TODO warning if there are string ids
        for (id, string) in self.iter().filter(|(id, _)| id.is_numeric()) {
            output.write_all(format!("{}={}\n", id, escape(string, false)).as_bytes())?;
        }
        Ok(())
    }

    /// Writes this language file to an output writer using the key-value
    /// format.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to_keyval<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        for (id, string) in self {
            output.write_all(format!("{} \"{}\"\n", id, escape(string, true)).as_bytes())?;
        }
        Ok(())
    }
}

// TODO specify
pub(super) fn unescape(escaped: impl Iterator<Item = char>, quoted: bool) -> String {
    let mut unescaped = String::new();
    let mut prev = 'x'; // Innocuous character
    for current in escaped {
        // NOTE this does not support escapes like "\\n", which should
        // print out "\n" literally, instead we get "\" followed by a newline.
        // Could be solved by making `prev` an Option
        match (prev, current) {
            ('\\', '\\') => unescaped.push('\\'),
            ('\\', 'n') => unescaped.push('\n'),
            ('\\', 'r') => unescaped.push('\r'),
            ('\\', 't') => unescaped.push('\t'),
            ('\\', '"') if quoted => unescaped.push('"'),
            (_, '"') if quoted => break,
            (_, '\\') => {} // Might be escape, wait for one more
            ('\\', escaped_character) => {
                // Previous character was escape,
                // but this is not part of a sequence
                unescaped.push('\\');
                unescaped.push(escaped_character);
            }
            (_, plain) => unescaped.push(plain),
        }
        prev = current;
    }
    unescaped
}

// TODO specify
pub(super) fn escape(source: &str, quoted: bool) -> String {
    let mut escaped = String::new();
    for character in source.chars() {
        match character {
            '\\' => escaped.push_str(r"\\"),
            '\n' => escaped.push_str(r"\n"),
            '\r' => escaped.push_str(r"\r"),
            '\t' => escaped.push_str(r"\t"),
            '"' if quoted => escaped.push_str(r#"\""#),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LangFileKind;
    use genie_error::ErrorKind;
    use std::fs::File;

    /// A file is written in key order, the numbers in turn and then the names, whatever order
    /// its strings went in: the same bytes in every run.
    #[test]
    fn keyval_file_is_written_in_key_order() {
        let lang: LangFile = [
            (StringKey::from(2u32), String::from("two")),
            (StringKey::from("b"), String::from("bee")),
            (StringKey::from(0u32), String::from("zero")),
            (StringKey::from("a"), String::from("ay")),
            (StringKey::from(1u32), String::from("one")),
        ]
        .into_iter()
        .collect();
        let mut out = Vec::new();
        lang.write_to_keyval(&mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "0 \"zero\"\n1 \"one\"\n2 \"two\"\na \"ay\"\nb \"bee\"\n"
        );
    }

    /// The game's DLL's string tables: block n's strings take the ids from (n - 1) × 16 on.
    #[test]
    fn dll_string_tables_give_their_strings_by_id() {
        let file = File::open("test/dlls/language_x1_p1.dll").unwrap();
        let lang = LangFileKind::Dll.read_from(file).unwrap();
        assert_eq!(
            lang.get(&StringKey::from(30177u32)).map(String::as_str),
            Some(
                "Turbo Random Map - Buildings create units faster, villagers gather faster, \
                 build faster, and carry more."
            )
        );
        assert_eq!(lang.len(), 29);
    }

    /// A string table block: an empty string only counts its id, a string that does not
    /// decode as UTF-16 is left out, a later one with the same id replaces the earlier.
    #[test]
    fn string_block_counts_empty_strings_and_skips_bad_ones() {
        let mut lang = LangFile::new();
        let data = [
            0, 0, 2, 0, b'h', 0, b'i', 0, 1, 0, 0x00, 0xd8, 1, 0, b'x', 0,
        ];
        lang.load_pe_data(16, &data).unwrap();
        assert_eq!(lang.get(&StringKey::from(16u32)), None);
        assert_eq!(
            lang.get(&StringKey::from(17u32)).map(String::as_str),
            Some("hi")
        );
        assert_eq!(lang.get(&StringKey::from(18u32)), None);
        assert_eq!(
            lang.get(&StringKey::from(19u32)).map(String::as_str),
            Some("x")
        );
        lang.load_pe_data(19, &[1, 0, b'y', 0]).unwrap();
        assert_eq!(
            lang.get(&StringKey::from(19u32)).map(String::as_str),
            Some("y")
        );
    }

    /// A string whose count of code units reaches past the end of its block is an error, not
    /// a panic: the strings before it are kept.
    #[test]
    fn string_past_block_end_is_an_error() {
        let mut lang = LangFile::new();
        let data = [1, 0, b'a', 0, 3, 0, b'h', 0];
        let error = lang.load_pe_data(16, &data).unwrap_err();
        assert_eq!(
            (error.kind(), error.reason()),
            (
                ErrorKind::InvalidInput,
                "a string past the end of the strings table"
            )
        );
        assert_eq!(error.context().collect::<Vec<_>>(), [("index", "17")]);
        assert_eq!(
            lang.get(&StringKey::from(16u32)).map(String::as_str),
            Some("a")
        );
    }

    /// A string after the one with the last id is an error, not an overflow: each string,
    /// an empty one too, takes the next id.
    #[test]
    fn string_past_last_id_is_an_error() {
        let mut lang = LangFile::new();
        let data = [1, 0, b'a', 0, 0, 0];
        let error = lang.load_pe_data(u32::MAX, &data).unwrap_err();
        assert_eq!(
            (error.kind(), error.reason()),
            (ErrorKind::InvalidInput, "a string id past the last id")
        );
        assert_eq!(
            error.context().collect::<Vec<_>>(),
            [("first", "4294967295")]
        );
        assert_eq!(
            lang.get(&StringKey::from(u32::MAX)).map(String::as_str),
            Some("a")
        );
    }

    /// Block n of a string table holds the ids from (n - 1) × 16 on; block 0 and a block past
    /// the last id hold none.
    #[test]
    fn string_block_gives_its_first_id() {
        assert_eq!(first_string_id(1).unwrap(), 0);
        assert_eq!(first_string_id(2).unwrap(), 16);
        assert_eq!(first_string_id(1 << 28).unwrap(), u32::MAX - 15);
        for block in [0, (1 << 28) + 1] {
            let error = first_string_id(block).unwrap_err();
            assert_eq!(
                (error.kind(), error.reason()),
                (
                    ErrorKind::InvalidInput,
                    "a string block id outside the string ids"
                )
            );
            assert_eq!(
                error.context().collect::<Vec<_>>(),
                [("block", block.to_string().as_str())]
            );
        }
    }

    /// The escapes of the text formats: backslash, newline, carriage return and tab, a quote
    /// only in a quoted text; a quoted text ends at its closing quote; a backslash pair reads
    /// as one backslash; a backslash before another letter stays.
    #[test]
    fn the_text_escapes_read_back_what_they_wrote() {
        let text = "b\nc\rd\te\"f";
        assert_eq!(escape(text, true), r#"b\nc\rd\te\"f"#);
        assert_eq!(escape(text, false), r#"b\nc\rd\te"f"#);
        assert_eq!(unescape(escape(text, true).chars(), true), text);
        assert_eq!(escape("a\\b", false), r"a\\b");
        assert_eq!(unescape(r"a\\".chars(), false), "a\\");
        assert_eq!(unescape(r#"say "hi" there"#.chars(), true), "say ");
        assert_eq!(unescape(r#"say "hi""#.chars(), false), r#"say "hi""#);
        assert_eq!(unescape(r"\q\n".chars(), false), "\\q\n");
    }
}
