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
//! # fn main() -> genie_error::GenieResult<()> {
//! use genie_lang::{LangFileKind::Dll, StringKey};
//! use std::fs::File;
//! let file = File::open("./test/dlls/language_x1_p1.dll")?;
//! let lang_file = Dll.read_from(file)?;
//! assert_eq!(
//!     lang_file.get(&StringKey::from(30177u32)),
//!     Some(&String::from("Turbo Random Map - Buildings create units faster, villagers gather faster, build faster, and carry more.")));
//! assert_eq!(
//!     lang_file.get(&StringKey::from(20156u32)),
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
//! # fn main() -> genie_error::GenieResult<()> {
//! use genie_lang::{LangFileKind::Ini, StringKey};
//! use std::io::Cursor;
//! let text = br#"
//! 46523=The Uighurs will join if you kill Ornlu the wolf and return to tell the tale.
//! ; a comment
//! 46524=Uighurs: Yes, that is the pelt of the great wolf. We will join you, Genghis Khan. And to seal the agreement, we will give you the gift of flaming arrows!
//! "#;
//! let text_reader = Cursor::new(&text[..]);
//! let lang_file = Ini.read_from(text_reader)?;
//! assert_eq!(
//!     lang_file.get(&StringKey::from(46523u32)),
//!     Some(&String::from("The Uighurs will join if you kill Ornlu the wolf and return to tell the tale.")));
//! # Ok(()) }
//! ```
//!
//! ## HD key-value files
//! ```rust
//! # fn main() -> genie_error::GenieResult<()> {
//! use genie_lang::{LangFileKind::KeyValue, StringKey};
//! use std::io::Cursor;
//! let text = br#"
//! 46523 "The Uighurs will join if you kill Ornlu the wolf and return to tell the tale."
//! 46524 "Uighurs: Yes, that is the pelt of the great wolf. We will join you, Genghis Khan. And to seal the agreement, we will give you the gift of flaming arrows!"
//! 46604 "Kill the traitor, Kushluk.\n\nPrevent the tent of Genghis Khan (Wonder) from being destroyed."
//! LOBBYBROWSER_DATMOD_TITLE_FORMAT "DatMod: \"%s\""
//! "#;
//! let text_reader = Cursor::new(&text[..]);
//! let lang_file = KeyValue.read_from(text_reader)?;
//! assert_eq!(
//!     lang_file.get(&StringKey::from(46523u32)),
//!     Some(&String::from("The Uighurs will join if you kill Ornlu the wolf and return to tell the tale.")));
//! assert_eq!(
//!     lang_file.get(&StringKey::from("LOBBYBROWSER_DATMOD_TITLE_FORMAT")),
//!     Some(&String::from(r#"DatMod: "%s""#)));
//! # Ok(()) }
//! ```
//!
//! ## Creating a file from scratch
//! ```rust
//! # fn main() -> genie_error::GenieResult<()> {
//! use genie_lang::{LangFile, StringKey};
//! let mut lang_file = LangFile::new();
//! lang_file.insert(StringKey::from(46604u32), String::from("Kill the traitor, Kushluk.\n\n\
//!                       Prevent the tent of Genghis Khan (Wonder) from being destroyed."));
//! let mut out = vec![];
//! lang_file.write_to_ini(&mut out)?;
//! assert_eq!(
//!     String::from_utf8_lossy(&out),
//!     r"46604=Kill the traitor, Kushluk.\n\nPrevent the tent of Genghis Khan (Wonder) from being destroyed.
//! ");
//! lang_file.insert(StringKey::from("LOBBYBROWSER_DATMOD_TITLE_FORMAT"), String::from(r#"DatMod: "%s""#));
//! let mut out = vec![];
//! lang_file.write_to_keyval(&mut out)?;
//! let result_string = String::from_utf8_lossy(&out);
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

use genie_error::{GenieError, GenieResult};
pub use genie_support::StringKey;
use std::collections::HashMap;
use std::io::Read;
use std::str::FromStr;

mod formats;
mod map;

/// The kinds of language files the games use.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum LangFileKind {
    /// An AoK/AoC-style DLL file with language resource strings.
    Dll,
    /// A Voobly-style .ini language file.
    Ini,
    /// An HD Edition key-value language file.
    KeyValue,
}

impl LangFileKind {
    /// Reads a language file from an input reader.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails, if a DLL is not a valid PE file with string resources,
    /// or if an INI line has a non-numeric string ID.
    pub fn read_from(&self, reader: impl Read) -> GenieResult<LangFile> {
        use LangFileKind::{Dll, Ini, KeyValue};
        let mut lang_file = LangFile::new();
        let from_method = match self {
            Dll => LangFile::read_dll,
            Ini => LangFile::read_ini,
            KeyValue => LangFile::read_keyval,
        };
        from_method(&mut lang_file, reader)?;
        Ok(lang_file)
    }
}

impl FromStr for LangFileKind {
    type Err = GenieError;
    fn from_str(text: &str) -> GenieResult<Self> {
        use LangFileKind::{Dll, Ini, KeyValue};
        match &text.to_lowercase()[..] {
            "dll" => Ok(Dll),
            "ini" => Ok(Ini),
            "key-value" => Ok(KeyValue),
            _ => Err(
                GenieError::invalid_input("a language file type this reader does not know")
                    .with("kind", text),
            ),
        }
    }
}

/// A mapping of `StringKey` key to `String` values.
///
/// May be read from or written to one of the three file formats for Aoe2
/// language files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LangFile(HashMap<StringKey, String>);

#[cfg(test)]
mod error_tests {
    use super::*;
    use genie_error::ErrorKind;

    /// An unknown file kind, an INI string id that is no number and a DLL that is no PE file
    /// fail as invalid input, each with its reason.
    #[test]
    fn unknown_kind_bad_id_and_bad_dll_are_invalid_input() {
        let error = "xml".parse::<LangFileKind>().unwrap_err();
        assert_eq!(
            (error.kind(), error.reason()),
            (
                ErrorKind::InvalidInput,
                "a language file type this reader does not know"
            )
        );
        let error = LangFileKind::Ini.read_from(&b"x=y\n"[..]).unwrap_err();
        assert_eq!(
            (error.kind(), error.reason()),
            (ErrorKind::InvalidInput, "a string id that is no number")
        );
        let error = LangFileKind::Dll.read_from(&b"MZ"[..]).unwrap_err();
        assert_eq!(
            (error.kind(), error.reason()),
            (ErrorKind::InvalidInput, "a language DLL that does not read")
        );
    }
}
