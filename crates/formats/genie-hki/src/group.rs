//! A group of hotkeys ([`HotkeyGroup`]): read, written, bound and unbound.

use crate::Hotkey;
use crate::errors::no_hotkey;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_error::GenieResult;
use genie_lang::{LangFile, StringKey};
use genie_support::read_list;
use std::io::{Read, Write};
use std::{fmt, slice, vec};

/// Represents a group of `Hotkey`s.
///
/// Different groups may have different numbers of hotkeys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyGroup {
    /// The hotkeys in this group, ordered by the order they appear in a
    /// hotkey file.
    pub(super) hotkeys: Vec<Hotkey>,
}

impl HotkeyGroup {
    /// Reads a hotkey group from an input stream.
    pub(crate) fn read_from<R: Read>(input: &mut R) -> GenieResult<Self> {
        let num_hotkeys = input.read_u32::<LE>()?;
        let hotkeys = read_list(num_hotkeys as usize, || Hotkey::read_from(input))?;

        Ok(Self { hotkeys })
    }

    /// Writes a hotkey group to an output stream.
    // The format stores the hotkey count as a u32.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_u32::<LE>(self.hotkeys.len() as u32)?;
        for hotkey in &self.hotkeys {
            hotkey.write_to(output)?;
        }
        Ok(())
    }

    /// Returns an immutable reference to a single hotkey, if that hotkey is
    /// present in this `HotkeyGroup`.
    #[must_use]
    pub fn hotkey(&self, index: usize) -> Option<&Hotkey> {
        self.hotkeys.get(index)
    }

    /// Get a mutable reference to a single hotkey.
    /// This way, you can edit or replace the mapping.
    pub fn hotkey_mut(&mut self, index: usize) -> Option<&mut Hotkey> {
        self.hotkeys.get_mut(index)
    }

    /// Returns a hotkey group equivalent to this group but with the hotkey
    /// at `index` unbound so that the key is `0` and all modifier keys are
    /// `false`.
    ///
    /// # Errors
    ///
    /// An `InvalidInput` error if the group has no hotkey at `index`.
    pub fn unbind(&self, index: usize) -> GenieResult<Self> {
        self.bind(index, 0, false, false, false)
    }

    /// Returns a hotkey group equivalent to this group but with the hotkey
    /// at `index` bound with the given key and modifier keys.
    ///
    /// # Errors
    ///
    /// An `InvalidInput` error if the group has no hotkey at `index`.
    pub fn bind(
        &self,
        index: usize,
        key: i32,
        ctrl: bool,
        alt: bool,
        shift: bool,
    ) -> GenieResult<Self> {
        if index >= self.num_hotkeys() {
            return Err(no_hotkey(index, self.num_hotkeys()));
        }
        let hotkeys = self
            .hotkeys
            .iter()
            .enumerate()
            .map(|(position, &hk)| {
                if position == index {
                    hk.key(key).ctrl(ctrl).alt(alt).shift(shift)
                } else {
                    hk
                }
            })
            .collect();
        Ok(Self { hotkeys })
    }

    /// Returns the number of hotkeys in this `HotkeyGroup`.
    /// ```rust
    /// # fn main() -> anyhow::Result<()> {
    /// use std::fs::File;
    /// use genie_hki::HotkeyInfo;
    /// let mut file = File::open("test/files/aoc1.hki")?;
    /// let info = HotkeyInfo::read_from(&mut file)?;
    /// // Villager hotkeys are group 3 in AoC and HD Edition.
    /// let group = info
    ///     .group(3)
    ///     .ok_or_else(|| anyhow::anyhow!("the file holds no villager hotkeys"))?;
    /// assert_eq!(28, group.num_hotkeys());
    /// # Ok(()) }
    /// ```
    #[must_use]
    pub const fn num_hotkeys(&self) -> usize {
        self.hotkeys.len()
    }

    /// Returns an iterator over this group's hotkeys.
    pub fn iter(&self) -> impl Iterator<Item = &Hotkey> {
        self.hotkeys.iter()
    }

    /// Returns an iterator over mutable references to this group's hotkeys.
    pub fn iter_mut(&mut self) -> slice::IterMut<'_, Hotkey> {
        self.hotkeys.iter_mut()
    }

    /// Returns a string representation of this hotkey group, using the strings
    /// from `lang_file` and the group name string key `sk`.
    #[must_use]
    pub fn string_from_lang(&self, lang_file: &LangFile, sk: &StringKey) -> String {
        let group_name = lang_file
            .get(sk)
            .map_or_else(String::new, |name| format!("{name} ({sk}):\n  "));
        let hotkeys: Vec<String> = self
            .hotkeys
            .iter()
            .map(|hki| hki.string_from_lang(lang_file))
            .collect();
        format!("{}{}", group_name, hotkeys.join("\n  "))
    }
}

impl fmt::Display for HotkeyGroup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let group_string = if self.hotkeys.is_empty() {
            String::from(" no hotkeys")
        } else {
            let hotkeys: Vec<String> = self.hotkeys.iter().map(ToString::to_string).collect();
            format!("\n  {}", hotkeys.join("\n  "))
        };
        write!(formatter, "Group:{group_string}")
    }
}

impl IntoIterator for HotkeyGroup {
    type Item = Hotkey;
    type IntoIter = vec::IntoIter<Hotkey>;
    fn into_iter(self) -> Self::IntoIter {
        self.hotkeys.into_iter()
    }
}

impl<'group> IntoIterator for &'group HotkeyGroup {
    type Item = &'group Hotkey;
    type IntoIter = slice::Iter<'group, Hotkey>;
    fn into_iter(self) -> Self::IntoIter {
        self.hotkeys.iter()
    }
}

impl<'group> IntoIterator for &'group mut HotkeyGroup {
    type Item = &'group mut Hotkey;
    type IntoIter = slice::IterMut<'group, Hotkey>;
    fn into_iter(self) -> Self::IntoIter {
        self.hotkeys.iter_mut()
    }
}
