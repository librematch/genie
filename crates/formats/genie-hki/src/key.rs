//! One hotkey ([`Hotkey`]): its key, its string and its modifiers, read and written.

use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_lang::{LangFile, StringKey};
use std::io::{Read, Write};
use std::{fmt, io};

/// The information about a single hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hotkey {
    /// Keycode that activates this hotkey.
    ///
    /// You can use a crate like [keycodes](https://docs.rs/keycodes/0.1.0/)
    /// to compare this to named virtual keys like `keycodes::KEY_RETURN`.
    ///
    /// If `string_id` is `-1`, this `key` must be `0`.
    pub key: i32,

    /// The string ID for this hotkey's label. -1 if this hotkey is unused.
    pub string_id: i32,

    /// Whether the Ctrl key needs to be held to activate this hotkey.
    pub ctrl: bool,

    /// Whether the Alt key needs to be held to activate this hotkey.
    pub alt: bool,

    /// Whether the Shift key needs to be held to activate this hotkey.
    pub shift: bool,

    /// Not sure what this does yet? Actually may be unused…
    pub(super) mouse: i8,
}

impl Default for Hotkey {
    fn default() -> Self {
        Self {
            key: 0,
            string_id: -1,
            ctrl: false,
            alt: false,
            shift: false,
            mouse: 0,
        }
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}{}{}{}",
            self.string_id,
            if self.ctrl { "Ctrl-" } else { "" },
            if self.alt { "Alt-" } else { "" },
            if self.shift { "Shift-" } else { "" },
            self.key
        )
    }
}

impl Hotkey {
    /// Returns a hotkey equivalent to this one but with the key set to `key`.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_hki::Hotkey;
    ///
    /// let hki = Hotkey::default();
    /// let hki2 = hki.key(5);
    /// assert_eq!(0, hki.key);
    /// assert_eq!(5, hki2.key);
    /// ```
    #[must_use]
    pub const fn key(self, key: i32) -> Self {
        Self { key, ..self }
    }

    /// Returns a hotkey equivalent to this one but with the string key set
    /// to `string_id`.
    /// If the id is `-1`, the returned key also has a `key` field set to `0`.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_hki::Hotkey;
    ///
    /// let hki = Hotkey::default();
    /// let hki2 = hki.string_id(5);
    /// assert_eq!(-1, hki.string_id);
    /// assert_eq!(5, hki2.string_id);
    ///
    /// let hki3 = hki2.key(5);
    /// let hki4 = hki3.string_id(-1);
    /// assert_eq!(5, hki3.key);
    /// assert_eq!(0, hki4.key);
    /// ```
    #[must_use]
    pub const fn string_id(self, string_id: i32) -> Self {
        if string_id == -1 {
            Self {
                string_id,
                key: 0,
                ..self
            }
        } else {
            Self { string_id, ..self }
        }
    }

    /// Returns a hotkey equivalent to this one but with `ctrl` determining
    /// whether control must be held when pressing the key.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_hki::Hotkey;
    ///
    /// let hki = Hotkey::default();
    /// let hki2 = hki.ctrl(true);
    /// assert!(!hki.ctrl);
    /// assert!(hki2.ctrl);
    /// ```
    #[must_use]
    pub const fn ctrl(self, ctrl: bool) -> Self {
        Self { ctrl, ..self }
    }

    /// Returns a hotkey equivalent to this one but with `alt` determining
    /// whether alt must be held when pressing the key.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_hki::Hotkey;
    ///
    /// let hki = Hotkey::default();
    /// let hki2 = hki.alt(true);
    /// assert!(!hki.alt);
    /// assert!(hki2.alt);
    /// ```
    #[must_use]
    pub const fn alt(self, alt: bool) -> Self {
        Self { alt, ..self }
    }

    /// Returns a hotkey equivalent to this one but with `shift` determining
    /// whether shift must be held when pressing the key.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_hki::Hotkey;
    ///
    /// let hki = Hotkey::default();
    /// let hki2 = hki.shift(true);
    /// assert!(!hki.shift);
    /// assert!(hki2.shift);
    /// ```
    #[must_use]
    pub const fn shift(self, shift: bool) -> Self {
        Self { shift, ..self }
    }

    /// Reads a hotkey from an input stream.
    pub(crate) fn read_from<R: Read>(input: &mut R) -> io::Result<Self> {
        let key = input.read_i32::<LE>()?;
        let string_id = input.read_i32::<LE>()?;
        let ctrl = input.read_u8()? != 0;
        let alt = input.read_u8()? != 0;
        let shift = input.read_u8()? != 0;
        let mouse = input.read_i8()?;

        Ok(Self {
            key,
            string_id,
            ctrl,
            alt,
            shift,
            mouse,
        })
    }

    /// Writes a hotkey to an output stream.
    pub(crate) fn write_to<W: Write>(&self, output: &mut W) -> io::Result<()> {
        output.write_i32::<LE>(self.key)?;
        output.write_i32::<LE>(self.string_id)?;
        output.write_u8(u8::from(self.ctrl))?;
        output.write_u8(u8::from(self.alt))?;
        output.write_u8(u8::from(self.shift))?;
        output.write_i8(self.mouse)?;
        Ok(())
    }

    /// Returns a string representation of this hotkey, using the strings from
    /// `lang_file`.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_hki::Hotkey;
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(5u32), String::from("A"));
    /// let hotkey = Hotkey::default().key(65).string_id(5).ctrl(true);
    /// assert_eq!("A (5): ctrl-65", hotkey.string_from_lang(&lang_file));
    ///
    /// let default = Hotkey::default();
    /// assert_eq!("-1: 0", default.string_from_lang(&lang_file));
    /// ```
    #[must_use]
    pub fn string_from_lang(&self, lang_file: &LangFile) -> String {
        let ctrl = if self.ctrl { "ctrl-" } else { "" };
        let alt = if self.alt { "ctrl-" } else { "" };
        let shift = if self.shift { "ctrl-" } else { "" };

        // Only positive ids name a string.
        let name = u32::try_from(self.string_id)
            .ok()
            .filter(|&id| id > 0)
            .and_then(|id| lang_file.get(&StringKey::from(id)));

        name.map_or_else(
            || format!("{}: {}{}{}{}", self.string_id, ctrl, alt, shift, self.key),
            |s| {
                format!(
                    "{} ({}): {}{}{}{}",
                    s, self.string_id, ctrl, alt, shift, self.key
                )
            },
        )
    }
}
