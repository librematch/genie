//! The groups' metadata ([`HotkeyInfoMetadata`]) and a keycode's string id.

use genie_lang::StringKey;
use std::{slice, vec};

/// Returns `Ok(id)`, where `id` is the id number of the string giving the text
/// representation of `keycode` in a language file.
/// Returns `None` if `keycode` is not represented in a language file.
#[must_use]
pub const fn keycode_id(keycode: i32) -> Option<i32> {
    match keycode {
        112 => Some(19545),
        113 => Some(19546),
        114 => Some(19547),
        115 => Some(19548),
        116 => Some(19549),
        117 => Some(19550),
        118 => Some(19551),
        119 => Some(19552),
        120 => Some(19553),
        // Note F10 is reserved for opening the menu and cannot be reassigned
        122 => Some(19555),
        123 => Some(19556),
        124 => Some(19557),
        125 => Some(19558),
        126 => Some(19559),
        127 => Some(19560),
        128 => Some(19561),
        129 => Some(19562),
        130 => Some(19563),
        131 => Some(19564),
        132 => Some(19565),
        133 => Some(19566),
        134 => Some(19567),
        135 => Some(19568),
        253 => Some(19712),
        254 => Some(19711),
        255 => Some(19710),
        _ => None,
    }
}

/// A list of information about hotkey groups in a hotkey file.
///
/// The length is the number of groups in the file.
/// Each `StringKey` in the list is the key of the string that names the group.
/// The key is stored at the group's offset index in the hotkey file.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct HotkeyInfoMetadata(pub(super) Vec<StringKey>);

impl HotkeyInfoMetadata {
    /// Returns an empty `HotkeyInfoMetadata` struct.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_hki::HotkeyInfoMetadata;
    ///
    /// let him = HotkeyInfoMetadata::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a group to the metadata.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_hki::HotkeyInfoMetadata;
    /// use genie_lang::StringKey;
    ///
    /// let mut him = HotkeyInfoMetadata::new();
    /// him.add(StringKey::from(0u32));
    /// ```
    pub fn add(&mut self, sk: StringKey) {
        self.0.push(sk);
    }

    /// Returns the number of groups described by this metadata.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_hki::HotkeyInfoMetadata;
    /// use genie_lang::StringKey;
    ///
    /// let mut him = HotkeyInfoMetadata::new();
    /// assert_eq!(0, him.len());
    /// him.add(StringKey::from(0u32));
    /// assert_eq!(1, him.len());
    /// ```
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if this metadata does not contain any groups.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_hki::HotkeyInfoMetadata;
    /// use genie_lang::StringKey;
    ///
    /// let mut him = HotkeyInfoMetadata::new();
    /// assert!(him.is_empty());
    /// him.add(StringKey::from(0u32));
    /// assert!(!him.is_empty());
    /// ```
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns `Some(sk)`, where `sk` is the string key of the group at the
    /// given `index`.
    /// Returns `None` if no group is located at `index`, that is,
    /// if `index >= self.len()`.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_hki::HotkeyInfoMetadata;
    /// use genie_lang::StringKey;
    ///
    /// let mut him = HotkeyInfoMetadata::new();
    /// assert_eq!(None, him.get(0));
    /// him.add(StringKey::from(0u32));
    /// assert_eq!(Some(&StringKey::from(0u32)), him.get(0));
    /// ```
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&StringKey> {
        self.0.get(index)
    }

    /// Returns an iterator over the hotkey group metadata contained in this
    /// hotkey file's data.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_hki::HotkeyInfoMetadata;
    /// use genie_lang::StringKey;
    ///
    /// let mut him = HotkeyInfoMetadata::new();
    /// him.add(StringKey::from(0u32));
    /// him.add(StringKey::from(1u32));
    /// let mut iter = him.iter();
    /// assert_eq!(Some(&StringKey::from(0u32)), iter.next());
    /// assert_eq!(Some(&StringKey::from(1u32)), iter.next());
    /// assert_eq!(None, iter.next());
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = &StringKey> {
        self.0.iter()
    }
}

impl From<Vec<StringKey>> for HotkeyInfoMetadata {
    fn from(sks: Vec<StringKey>) -> Self {
        Self(sks)
    }
}

impl IntoIterator for HotkeyInfoMetadata {
    type Item = StringKey;
    type IntoIter = vec::IntoIter<StringKey>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'metadata> IntoIterator for &'metadata HotkeyInfoMetadata {
    type Item = &'metadata StringKey;
    type IntoIter = slice::Iter<'metadata, StringKey>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

// TODO Would like to move this to some kind of configuration file format
// then other mods can specify which hotkeys they use and what strings their
// groups should use
// and the group names can be overwritten in the string files
/// Returns a `HotkeyInfoMetadata` struct that represents the info metadata for
/// the default Aoe2 hotkeys (both `UserPatch` and HD).
#[must_use]
pub fn default_him() -> HotkeyInfoMetadata {
    let mut hgm = HotkeyInfoMetadata::new();
    hgm.add(StringKey::from(20000_u32)); // UnitCommands
    hgm.add(StringKey::from(20001_u32)); // GameCommands
    hgm.add(StringKey::from(20002_u32)); // Scroll
    hgm.add(StringKey::from(20003_u32)); // Villager
    hgm.add(StringKey::from(20004_u32)); // TownCenter
    hgm.add(StringKey::from(20007_u32)); // Dock
    hgm.add(StringKey::from(20008_u32)); // Barracks
    hgm.add(StringKey::from(20009_u32)); // ArcheryRange
    hgm.add(StringKey::from(20010_u32)); // Stable
    hgm.add(StringKey::from(20011_u32)); // SiegeWorkshop
    hgm.add(StringKey::from(20012_u32)); // Monastery
    hgm.add(StringKey::from(20013_u32)); // Market
    hgm.add(StringKey::from(20014_u32)); // MilitaryUnits
    hgm.add(StringKey::from(20015_u32)); // Castle
    hgm.add(StringKey::from(20017_u32)); // Mill
    hgm
}
