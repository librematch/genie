//! The language file as a map of string keys to strings ([`LangFile`]).

use crate::LangFile;
use genie_support::StringKey;
use std::collections::BTreeMap;
use std::collections::btree_map::{Entry, IntoIter, Iter, IterMut, Keys, Values, ValuesMut};
use std::fmt;
use std::iter::FromIterator;
use std::mem::take;
use std::ops::Index;

impl LangFile {
    /// Creates an empty language file.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_lang::LangFile;
    ///
    /// let lang_file = LangFile::new();
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Returns `true` if this language file contains no key-value pairs,
    /// `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// assert!(lang_file.is_empty());
    /// lang_file.insert(StringKey::from(0u32), String::from(""));
    /// assert!(!lang_file.is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of key-value pairs in this language file.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// assert_eq!(0, lang_file.len());
    /// lang_file.insert(StringKey::from(0u32), String::from(""));
    /// assert_eq!(1, lang_file.len());
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Removes all key-value pairs from this Language file.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(0u32), String::from(""));
    /// lang_file.clear();
    /// assert!(lang_file.is_empty());
    /// ```
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Clears this Language file, returning all key-value pairs as an iterator, in key order.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(1u32), String::from("b"));
    /// lang_file.insert(StringKey::from(0u32), String::from("a"));
    ///
    /// for (key, value) in lang_file.drain().take(1) {
    ///     assert_eq!(key, StringKey::from(0u32));
    ///     assert_eq!(value, "a");
    /// }
    /// assert!(lang_file.is_empty());
    /// ```
    pub fn drain(&mut self) -> IntoIter<StringKey, String> {
        take(&mut self.0).into_iter()
    }

    /// Returns `true` if the map contains a value for key `string_key`, `false` if not.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(0u32), String::from(""));
    /// assert!(lang_file.contains_key(&StringKey::from(0u32)));
    /// assert!(!lang_file.contains_key(&StringKey::from(1u32)));
    /// ```
    #[must_use]
    pub fn contains_key(&self, string_key: &StringKey) -> bool {
        self.0.contains_key(string_key)
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(0u32), String::from(""));
    /// assert_eq!(Some(&String::from("")), lang_file.get(&StringKey::from(0u32)));
    /// assert_eq!(None, lang_file.get(&StringKey::from(1u32)));
    /// ```
    #[must_use]
    pub fn get(&self, string_key: &StringKey) -> Option<&String> {
        self.0.get(string_key)
    }

    /// Returns a mutable reference to the value corresponding to the key.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(0u32), String::from("a"));
    /// if let Some(s) = lang_file.get_mut(&StringKey::from(0u32)) { s.push('A'); }
    /// assert_eq!(lang_file.get(&StringKey::from(0u32)).map(String::as_str), Some("aA"));
    /// ```
    pub fn get_mut(&mut self, string_key: &StringKey) -> Option<&mut String> {
        self.0.get_mut(string_key)
    }

    /// Returns the given key's corresponding entry in the language file.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(0u32), String::from("a"));
    /// let s0 = lang_file.entry(StringKey::from(0u32))
    ///                   .or_insert(String::from("Hello"));
    /// s0.push('A');
    /// let s1 = lang_file.entry(StringKey::from(1u32))
    ///                   .or_insert(String::from("Hello"));
    /// s1.push('A');
    /// assert_eq!(lang_file.get(&StringKey::from(0u32)).map(String::as_str), Some("aA"));
    /// assert_eq!(lang_file.get(&StringKey::from(1u32)).map(String::as_str), Some("HelloA"));
    /// ```
    pub fn entry(&mut self, key: StringKey) -> Entry<'_, StringKey, String> {
        self.0.entry(key)
    }

    /// Inserts a key-value pair into this language file.
    ///
    /// Returns `None`. if the language file did not have the key present.
    ///
    /// If the key was present, the value is updated, and the old value is
    /// returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(0u32), String::from("a"));
    /// assert!(!lang_file.is_empty());
    ///
    /// lang_file.insert(StringKey::from(0u32), String::from("b"));
    /// assert_eq!(Some(String::from("b")),
    ///            lang_file.insert(StringKey::from(0u32), String::from("c")));
    /// ```
    pub fn insert(&mut self, string_key: StringKey, text: String) -> Option<String> {
        self.0.insert(string_key, text)
    }

    /// Removes a key-value pair from the map, returning the value at the key
    /// if the key was previously in the map.
    /// Returns `None` if the key was not in the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(0u32), String::from(""));
    /// assert_eq!(Some(String::from("")),
    ///            lang_file.remove(&StringKey::from(0u32)));
    /// assert_eq!(None, lang_file.remove(&StringKey::from(0u32)));
    /// ```
    pub fn remove(&mut self, string_key: &StringKey) -> Option<String> {
        self.0.remove(string_key)
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// In other words, removes all pairs `(key, value)` such that `keep(&key, &mut value)`
    /// returns `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(0u32), String::from("zero"));
    /// lang_file.insert(StringKey::from("a"), String::from("a"));
    /// lang_file.insert(StringKey::from(1u32), String::from("one"));
    /// lang_file.insert(StringKey::from("2"), String::from("two"));
    /// lang_file.retain(|key, _| match key {
    ///     StringKey::Num(_) => true,
    ///     StringKey::Name(_) => false,
    /// });
    /// assert_eq!(3, lang_file.len());
    /// ```
    pub fn retain<F: FnMut(&StringKey, &mut String) -> bool>(&mut self, keep: F) {
        self.0.retain(keep);
    }

    /// An iterator visiting all key-value pairs in key order.
    /// Each element borrows the file: `(&StringKey, &String)`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(0u32), String::from("zero"));
    /// lang_file.insert(StringKey::from("a"), String::from("a"));
    /// lang_file.insert(StringKey::from(1u32), String::from("one"));
    /// lang_file.insert(StringKey::from("2"), String::from("two"));
    ///
    /// for (key, value) in lang_file.iter() { println!("key: {}, val: {}", key, value); }
    /// ```
    pub fn iter(&self) -> Iter<'_, StringKey, String> {
        self.0.iter()
    }

    /// Returns an iterator that visits all key-values pairs in key
    /// order, with mutable references to the values.
    /// Each element borrows the file: `(&StringKey, &mut String)`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(0u32), String::from("zero"));
    /// lang_file.insert(StringKey::from("a"), String::from("a"));
    /// lang_file.insert(StringKey::from(1u32), String::from("one"));
    /// lang_file.insert(StringKey::from("2"), String::from("two"));
    ///
    /// for (_, value) in lang_file.iter_mut() { value.push('A'); }
    /// for (key, value) in &lang_file { println!("key: {}, val: {}", key, value); }
    /// ```
    pub fn iter_mut(&mut self) -> IterMut<'_, StringKey, String> {
        self.0.iter_mut()
    }

    /// Returns an iterator that visits all keys in key order.
    /// Each element borrows the file: `&StringKey`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(0u32), String::from("zero"));
    /// lang_file.insert(StringKey::from("a"), String::from("a"));
    /// lang_file.insert(StringKey::from(1u32), String::from("one"));
    /// lang_file.insert(StringKey::from("2"), String::from("two"));
    ///
    /// for key in lang_file.keys() { println!("key: {}", key); }
    /// ```
    pub fn keys(&self) -> Keys<'_, StringKey, String> {
        self.0.keys()
    }

    /// Returns an iterator that visits all values in key order.
    /// Each element borrows the file: `&String`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(0u32), String::from("zero"));
    /// lang_file.insert(StringKey::from("a"), String::from("a"));
    /// lang_file.insert(StringKey::from(1u32), String::from("one"));
    /// lang_file.insert(StringKey::from("2"), String::from("two"));
    ///
    /// for value in lang_file.values() { println!("value: {}", value); }
    /// ```
    pub fn values(&self) -> Values<'_, StringKey, String> {
        self.0.values()
    }

    /// Returns an iterator visiting all values mutable in key order.
    /// Each element borrows the file: `&mut String`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use genie_lang::{LangFile, StringKey};
    ///
    /// let mut lang_file = LangFile::new();
    /// lang_file.insert(StringKey::from(0u32), String::from("zero"));
    /// lang_file.insert(StringKey::from("a"), String::from("a"));
    /// lang_file.insert(StringKey::from(1u32), String::from("one"));
    /// lang_file.insert(StringKey::from("2"), String::from("two"));
    ///
    /// for value in lang_file.values_mut() { value.push('A'); }
    /// for value in lang_file.values() { println!("{}", value); }
    /// ```
    pub fn values_mut(&mut self) -> ValuesMut<'_, StringKey, String> {
        self.0.values_mut()
    }
}

impl IntoIterator for LangFile {
    type Item = (StringKey, String);
    type IntoIter = IntoIter<StringKey, String>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'file> IntoIterator for &'file LangFile {
    type Item = (&'file StringKey, &'file String);
    type IntoIter = Iter<'file, StringKey, String>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'file> IntoIterator for &'file mut LangFile {
    type Item = (&'file StringKey, &'file mut String);
    type IntoIter = IterMut<'file, StringKey, String>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

impl fmt::Display for LangFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let strs: Vec<String> = self
            .0
            .iter()
            .map(|(string_key, text)| format!("{string_key}: {text}"))
            .collect();
        write!(formatter, "{}", strs.join("\n"))
    }
}

impl Extend<(StringKey, String)> for LangFile {
    fn extend<T: IntoIterator<Item = (StringKey, String)>>(&mut self, iter: T) {
        self.0.extend(iter);
    }
}

impl FromIterator<(StringKey, String)> for LangFile {
    fn from_iter<T: IntoIterator<Item = (StringKey, String)>>(iter: T) -> Self {
        let mut lang_file = Self::default();
        lang_file.extend(iter);
        lang_file
    }
}

impl Index<&StringKey> for LangFile {
    type Output = String;
    fn index(&self, key: &StringKey) -> &String {
        self.0.index(key)
    }
}
