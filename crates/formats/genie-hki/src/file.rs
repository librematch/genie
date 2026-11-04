//! A hotkey file ([`HotkeyInfo`]): its version and groups, compressed or not.

use crate::errors::no_group;
use crate::{Hotkey, HotkeyGroup, HotkeyInfoMetadata};
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use genie_error::GenieResult;
use genie_lang::LangFile;
use genie_support::read_list;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::{fmt, slice, vec};

/// Represents a HKI file containing hotkey settings.
#[derive(Debug, Clone, PartialEq)]
pub struct HotkeyInfo {
    /// The file version.
    pub(super) version: f32,

    /// The hotkey groups.
    pub(super) groups: Vec<HotkeyGroup>,
}

impl HotkeyInfo {
    /// Read a hotkey info structure from an uncompressed stream.
    pub(super) fn read_uncompressed<R: Read>(input: &mut R) -> GenieResult<Self> {
        let version = input.read_f32::<LE>()?;
        let num_groups = input.read_u32::<LE>()?;
        let groups = read_list(num_groups as usize, || HotkeyGroup::read_from(input))?;

        Ok(Self { version, groups })
    }

    /// Reads a hotkey info file.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, including when the data is not valid
    /// DEFLATE or ends early.
    pub fn read_from(input: impl Read) -> GenieResult<Self> {
        let mut input = DeflateDecoder::new(input);
        Self::read_uncompressed(&mut input)
    }

    /// Writes a hotkey info structure to an uncompressed stream.
    // The format stores the group count as a u32.
    #[allow(clippy::cast_possible_truncation)]
    pub(super) fn write_to_uncompressed<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_f32::<LE>(self.version)?;
        output.write_u32::<LE>(self.groups.len() as u32)?;
        for group in &self.groups {
            group.write_to(output)?;
        }
        Ok(())
    }

    /// Writes a hotkey info file.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        let mut output = DeflateEncoder::new(output, Compression::default());
        self.write_to_uncompressed(&mut output)
    }

    /// Gets the file version.
    ///
    /// ```rust
    /// # fn main() -> anyhow::Result<()> {
    /// use std::fs::File;
    /// use genie_hki::HotkeyInfo;
    /// let mut f = File::open("test/files/aoc1.hki")?;
    /// let info = HotkeyInfo::read_from(&mut f)?;
    /// assert_eq!(info.version(), 1.0);
    ///
    /// let mut f = File::open("test/files/hd0.hki")?;
    /// let info = HotkeyInfo::read_from(&mut f)?;
    /// assert_eq!(info.version(), 3.0);
    /// # Ok(()) }
    /// ```
    #[must_use]
    pub const fn version(&self) -> f32 {
        self.version
    }

    /// Returns an immutable reference to a hotkey group, if that group exists.
    ///
    /// ```rust
    /// # fn main() -> anyhow::Result<()> {
    /// use std::fs::File;
    /// use genie_hki::{HotkeyInfo, HotkeyGroupId};
    /// let mut f = File::open("test/files/aoc1.hki")?;
    /// let info = HotkeyInfo::read_from(&mut f)?;
    /// assert!(info.group(HotkeyGroupId::Villager as usize).is_some());
    /// assert!(info.group(HotkeyGroupId::Mill as usize).is_none());
    /// # Ok(()) }
    /// ```
    #[must_use]
    pub fn group(&self, group_id: usize) -> Option<&HotkeyGroup> {
        self.groups.get(group_id)
    }

    /// Returns a mutable reference to a hotkey group, if that group exists.
    pub fn group_mut(&mut self, group_id: usize) -> Option<&mut HotkeyGroup> {
        self.groups.get_mut(group_id)
    }

    /// Returns the number of hotkey groups in this info's hotkey file.
    /// ```rust
    /// # fn main() -> anyhow::Result<()> {
    /// use std::fs::File;
    /// use genie_hki::HotkeyInfo;
    ///
    /// let mut f = File::open("test/files/aoc1.hki")?;
    /// let info = HotkeyInfo::read_from(&mut f)?;
    /// assert_eq!(14, info.num_groups());
    ///
    /// let mut f = File::open("test/files/wk.hki")?;
    /// let info = HotkeyInfo::read_from(&mut f)?;
    /// assert_eq!(15, info.num_groups());
    /// # Ok(()) }
    /// ```
    #[must_use]
    pub const fn num_groups(&self) -> usize {
        self.groups.len()
    }

    /// Returns an iterator over the hotkey groups present in this info's hotkey
    /// file.
    pub fn iter(&self) -> impl Iterator<Item = &HotkeyGroup> {
        self.groups.iter()
    }

    /// Returns an iterator over mutable references to the hotkey groups of this info's hotkey
    /// file.
    pub fn iter_mut(&mut self) -> slice::IterMut<'_, HotkeyGroup> {
        self.groups.iter_mut()
    }

    /// Returns a `HotkeyInfo` struct equivalent to this `HotkeyInfo`, but with
    /// the key at index `key_index` of the group given by `group_index`
    /// unbound.
    ///
    /// # Errors
    ///
    /// An `InvalidInput` error if there is no group at `group_index` or no hotkey at
    /// `key_index` in it.
    pub fn unbind_key(&self, group_index: usize, key_index: usize) -> GenieResult<Self> {
        self.bind_key(group_index, key_index, 0, false, false, false)
    }

    /// Returns a `HotkeyInfo` struct equivalent to this `HotkeyInfo`, but with
    /// the key at index `key_index` of the group given by `group_index`
    /// bound with the given key and key modifiers.
    ///
    /// # Errors
    ///
    /// An `InvalidInput` error if there is no group at `group_index` or no hotkey at
    /// `key_index` in it.
    pub fn bind_key(
        &self,
        group_index: usize,
        key_index: usize,
        key: i32,
        ctrl: bool,
        alt: bool,
        shift: bool,
    ) -> GenieResult<Self> {
        if group_index >= self.num_groups() {
            return Err(no_group(group_index, self.num_groups()));
        }
        let mut groups = Vec::with_capacity(self.num_groups());
        for (i, grp) in self.groups.iter().enumerate() {
            let append = if i == group_index {
                grp.bind(key_index, key, ctrl, alt, shift)?
            } else {
                grp.clone()
            };
            groups.push(append);
        }
        Ok(Self { groups, ..*self })
    }

    /// Returns a map `keycode -> vec[hotkey1, hotkey2, ... hotkeyn]` mapping
    /// every used keybinding to all of the hotkeys to which it is assigned.
    ///
    /// Note hotkeys may have different behavior in different contexts, such
    /// as `A` producing an archer when an Archery Range is selected and
    /// a militia when a Barracks is selected.
    pub fn bindings_per_keycode(&self) -> HashMap<i32, Vec<Hotkey>> {
        let mut bindings = HashMap::new();
        for group in self.iter() {
            for hotkey in group.iter() {
                bindings
                    .entry(hotkey.key)
                    .or_insert_with(Vec::new)
                    .push(*hotkey);
            }
        }
        bindings
    }

    /// Returns a string representation of this `HotkeyInfo` struct using the
    /// strings from `lang_file` and the group name string keys given in `him`.
    ///
    /// # Panics
    ///
    /// Panics if the number of hotkeys in any group of this file differs from
    /// the number of hotkeys given to that group in `him`.
    #[must_use]
    pub fn string_from_lang(&self, lang_file: &LangFile, him: &HotkeyInfoMetadata) -> String {
        let groups: Vec<String> = self
            .groups
            .iter()
            .zip(him.iter())
            .map(|(grp, sk)| grp.string_from_lang(lang_file, sk))
            .collect();
        format!("Version: {}\n{}", self.version, groups.join("\n"))
    }
}

impl fmt::Display for HotkeyInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let group_string = if self.groups.is_empty() {
            String::new()
        } else {
            let groups: Vec<String> = self.groups.iter().map(ToString::to_string).collect();
            format!("\n{}", groups.join("\n"))
        };
        write!(f, "Version: {}{}", self.version, group_string)
    }
}

impl IntoIterator for HotkeyInfo {
    type Item = HotkeyGroup;
    type IntoIter = vec::IntoIter<HotkeyGroup>;
    fn into_iter(self) -> Self::IntoIter {
        self.groups.into_iter()
    }
}

impl<'a> IntoIterator for &'a HotkeyInfo {
    type Item = &'a HotkeyGroup;
    type IntoIter = slice::Iter<'a, HotkeyGroup>;
    fn into_iter(self) -> Self::IntoIter {
        self.groups.iter()
    }
}

impl<'a> IntoIterator for &'a mut HotkeyInfo {
    type Item = &'a mut HotkeyGroup;
    type IntoIter = slice::IterMut<'a, HotkeyGroup>;
    fn into_iter(self) -> Self::IntoIter {
        self.groups.iter_mut()
    }
}
