//! genie-lang reads language files into a map of UTF-8 strings.
//! All three major language file types used by Age of Empires versions are
//! supported: DLLs, INI files, and HD Edition's key-value format.
//!
//! DLLs are used by the original games.
//! INI files are used for Voobly mods, and can be used with a standard
//! `AoC` installation through the aoc-language-ini mod.
//!
//! ## DLLs
//! ```rust
//! # fn main() -> anyhow::Result<()> {
//! use genie_lang::{LangFileType::Dll, StringKey};
//! use std::{convert::TryFrom, fs::File};
//! let f = File::open("./test/dlls/language_x1_p1.dll")?;
//! let lang_file = Dll.read_from(f)?;
//! assert_eq!(
//!     lang_file.get(&StringKey::try_from(30177).unwrap()),
//!     Some(&String::from("Turbo Random Map - Buildings create units faster, villagers gather faster, build faster, and carry more.")));
//! assert_eq!(
//!     lang_file.get(&StringKey::try_from(20156).unwrap()),
//!     Some(&String::from("<b>Byzantines<b> \n\
//!           Defensive civilization \n\
//!           · Buildings +10% HPs Dark, +20% Feudal, \n +30% Castle, +40% Imperial Age \n\
//!           · Camels, skirmishers, Pikemen, Halberdiers cost -25% \n\
//!           · Fire ships +20% attack \n\
//!           · Advance to Imperial Age costs -33% \n\
//!           · Town Watch free \n\n\
//!           <b>Unique Unit:<b> Cataphract (cavalry) \n\n\
//!           <b>Unique Tech:<b> Logistica (Cataphracts cause trample damage) \n\n\
//!           <b>Team Bonus:<b> Monks +50% heal speed")));
//! # Ok(()) }
//! ```
//!
//! ## INI files
//! ```rust
//! # fn main() -> anyhow::Result<()> {
//! use genie_lang::{LangFileType::Ini, StringKey};
//! use std::{convert::TryFrom, io::Cursor};
//! let text = br#"
//! 46523=The Uighurs will join if you kill Ornlu the wolf and return to tell the tale.
//! ; a comment
//! 46524=Uighurs: Yes, that is the pelt of the great wolf. We will join you, Genghis Khan. And to seal the agreement, we will give you the gift of flaming arrows!
//! "#;
//! let f = Cursor::new(&text[..]);
//! let lang_file = Ini.read_from(f)?;
//! assert_eq!(
//!     lang_file.get(&StringKey::try_from(46523).unwrap()),
//!     Some(&String::from("The Uighurs will join if you kill Ornlu the wolf and return to tell the tale.")));
//! # Ok(()) }
//! ```
//!
//! ## HD key-value files
//! ```rust
//! # fn main() -> anyhow::Result<()> {
//! use genie_lang::{LangFileType::KeyValue, StringKey};
//! use std::{convert::TryFrom, io::Cursor};
//! let text = br#"
//! 46523 "The Uighurs will join if you kill Ornlu the wolf and return to tell the tale."
//! 46524 "Uighurs: Yes, that is the pelt of the great wolf. We will join you, Genghis Khan. And to seal the agreement, we will give you the gift of flaming arrows!"
//! 46604 "Kill the traitor, Kushluk.\n\nPrevent the tent of Genghis Khan (Wonder) from being destroyed."
//! LOBBYBROWSER_DATMOD_TITLE_FORMAT "DatMod: \"%s\""
//! "#;
//! let f = Cursor::new(&text[..]);
//! let lang_file = KeyValue.read_from(f)?;
//! assert_eq!(
//!     lang_file.get(&StringKey::try_from(46523).unwrap()),
//!     Some(&String::from("The Uighurs will join if you kill Ornlu the wolf and return to tell the tale.")));
//! assert_eq!(
//!     lang_file.get(&StringKey::from("LOBBYBROWSER_DATMOD_TITLE_FORMAT")),
//!     Some(&String::from(r#"DatMod: "%s""#)));
//! # Ok(()) }
//! ```
//!
//! ## Creating a file from scratch
//! ```rust
//! # fn main() -> anyhow::Result<()> {
//! use genie_lang::{LangFile, StringKey};
//! use std::{convert::TryFrom, str};
//! let mut lang_file = LangFile::new();
//! lang_file.insert(StringKey::try_from(46604).unwrap(), String::from("Kill the traitor, Kushluk.\n\n\
//!                       Prevent the tent of Genghis Khan (Wonder) from being destroyed."));
//! let mut out = vec![];
//! lang_file.write_to_ini(&mut out)?;
//! assert_eq!(
//!     str::from_utf8(&out)?,
//!     r"46604=Kill the traitor, Kushluk.\n\nPrevent the tent of Genghis Khan (Wonder) from being destroyed.
//! ");
//! lang_file.insert(StringKey::from("LOBBYBROWSER_DATMOD_TITLE_FORMAT"), String::from(r#"DatMod: "%s""#));
//! let mut out = vec![];
//! lang_file.write_to_keyval(&mut out)?;
//! let result_string = str::from_utf8(&out)?;
//! println!("{}", result_string);
//! assert!(
//!     result_string == r#"46604 "Kill the traitor, Kushluk.\n\nPrevent the tent of Genghis Khan (Wonder) from being destroyed."
//! LOBBYBROWSER_DATMOD_TITLE_FORMAT "DatMod: \"%s\""
//! "#
//!     ||
//!     result_string == r#"LOBBYBROWSER_DATMOD_TITLE_FORMAT "DatMod: \"%s\""
//! 46604 "Kill the traitor, Kushluk.\n\nPrevent the tent of Genghis Khan (Wonder) from being destroyed."
//! "#);
//! # Ok(()) }
//! ```

#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(rust_2018_idioms)]
#![deny(unsafe_code)]
#![warn(unused)]

pub use genie_support::{StringKey, TryFromStringKeyError};
use std::collections::HashMap;
use std::io::{Error as IoError, Read};
use std::num::ParseIntError;
use std::str::FromStr;

mod formats;
mod map;

#[cfg(test)]
use std::io;

/// Errors that may occur when loading a language file.
///
/// For DLL files, `PeError` and `IoError` can occur.
/// For INI and HD Edition files, `ParseIntError` and `IoError` can occur.
/// Both the INI and HD Edition parsers silently ignore invalid lines.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// An error occurred while reading strings from the DLL.
    /// It probably does not contain any or is malformed.
    #[error(transparent)]
    PeError(#[from] pelite::Error),
    /// An error occurred while reading data from the file.
    #[error(transparent)]
    IoError(#[from] IoError),
    /// An error occurred while parsing a numeric string ID into an integer
    /// value.
    #[error(transparent)]
    ParseIntError(#[from] ParseIntError),
}

/// An error when parsing a string to a language file.
///
/// The field contains the string that could not be parsed.
#[derive(Debug, thiserror::Error)]
#[error("{}", self.0)]
pub struct ParseLangFileTypeError(String);

/// Aoe2 supports three types of language files
#[derive(Debug, Clone, Copy)]
pub enum LangFileType {
    /// An AoK/AoC-style DLL file with language resource strings.
    Dll,
    /// A Voobly-style .ini language file.
    Ini,
    /// An HD Edition key-value language file.
    KeyValue,
}

impl LangFileType {
    /// Reads a language file from an input reader.
    ///
    /// # Errors
    ///
    /// Returns a `LoadError` if reading fails, if a DLL is not a valid PE file with string
    /// resources, or if an INI line has a non-numeric string ID.
    pub fn read_from(&self, r: impl Read) -> Result<LangFile, LoadError> {
        use LangFileType::{Dll, Ini, KeyValue};
        let mut lang_file = LangFile::new();
        let from_method = match self {
            Dll => LangFile::read_dll,
            Ini => LangFile::read_ini,
            KeyValue => LangFile::read_keyval,
        };
        from_method(&mut lang_file, r)?;
        Ok(lang_file)
    }
}

impl FromStr for LangFileType {
    type Err = ParseLangFileTypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use LangFileType::{Dll, Ini, KeyValue};
        match &s.to_lowercase()[..] {
            "dll" => Ok(Dll),
            "ini" => Ok(Ini),
            "key-value" => Ok(KeyValue),
            _ => Err(ParseLangFileTypeError(String::from(s))),
        }
    }
}

/// A mapping of `StringKey` key to `String` values.
///
/// May be read from or written to one of the three file formats for Aoe2
/// language files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LangFile(HashMap<StringKey, String>);

impl From<LoadError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: LoadError) -> Self {
        let to = match &e {
            LoadError::PeError(_) => Self::invalid_input("a language DLL that does not read"),
            LoadError::IoError(_) => Self::io("a read or write that failed"),
            LoadError::ParseIntError(_) => Self::invalid_input("a string id that is no number"),
        };
        to.with_source(e)
    }
}

impl From<ParseLangFileTypeError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: ParseLangFileTypeError) -> Self {
        Self::invalid_input("a language file type this reader does not know").with_source(e)
    }
}

#[cfg(test)]
mod genie_error_tests {
    use super::*;
    #[cfg(test)]
    use std::error::Error;

    /// The errors convert into `GenieError`s with their kinds and reasons, the original as
    /// source.
    #[test]
    fn language_errors_convert_into_genie_errors() {
        let e: genie_error::GenieError = LoadError::IoError(io::Error::other("x")).into();
        assert_eq!(
            (e.kind(), e.reason()),
            (genie_error::ErrorKind::Io, "a read or write that failed")
        );
        assert!(Error::source(&e).is_some());
        let e: genie_error::GenieError = ParseLangFileTypeError("x".to_owned()).into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::InvalidInput,
                "a language file type this reader does not know"
            )
        );
        assert!(Error::source(&e).is_some());
    }
}
