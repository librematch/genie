//! Where two values of the same type differ, field by field, each difference named by its path
//! from the values compared (`life.hp`, `actions[0].to[1]`): [`Diff`], [`differences`].
//!
//! A type derives [`Diff`]: a struct compares its fields by name, a tuple struct of one field is
//! that field, an enum differs by its variant and then compares the same variant's fields. A
//! float compares by its bits, so `0.0` and `-0.0` differ and a NaN equals the same NaN. Two
//! field attributes adjust the walk:
//!
//! - `#[diff(unsaved)]`: the game's save does not hold the field, so only [`Scope::All`]
//!   compares it.
//! - `#[diff(gap = "why")]`: a known difference from the game, its reason given: the engine
//!   does not model the field yet. [`Scope::Saved`] leaves it out, the other scopes compare it.
//! - `#[diff(eq)]`: the field compares by its `PartialEq`, as one value (a type that does not
//!   derive `Diff`).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::{Debug, Write as _};

pub use genie_diff_derive::Diff;

/// A value compared field by field ([`differences`]). Derive it (`#[derive(Diff)]`); its
/// `Debug` text shows a value where it differs.
pub trait Diff: Debug {
    /// Records in `differ` where `self` and `other` differ, under `differ`'s current path.
    fn diff(&self, other: &Self, differ: &mut Differ);
}

/// Which fields a comparison reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// What the game's save holds and the engine models: the fields marked `unsaved` or `gap`
    /// are left out. A state compared with one loaded from the game's save.
    Saved,
    /// What the game's save holds, the known gaps too: only the `unsaved` fields are left out.
    SavedWithGaps,
    /// Every field: two runs of the engine.
    All,
}

/// One place where two values differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    /// Its path from the values compared: fields by name (`life.hp`), elements and keys in
    /// brackets (`actions[0]`), a sequence's length as `len`; empty for the values themselves.
    pub path: String,
    /// The left value's text there, or `absent`.
    pub left: String,
    /// The right value's text there, or `absent`.
    pub right: String,
}

/// The text of a side that holds no value at a path.
pub const ABSENT: &str = "absent";

/// Where two values differ, in the order of their fields.
#[must_use]
pub fn differences<T: Diff + ?Sized>(left: &T, right: &T, scope: Scope) -> Vec<Difference> {
    let mut differ = Differ::new(scope);
    left.diff(right, &mut differ);
    differ.found
}

/// One step of a path.
#[derive(Clone, Debug)]
enum Step {
    Field(&'static str),
    Index(usize),
    Key(String),
}

/// A comparison under way: its scope, the path it has reached and the differences found.
#[derive(Debug)]
pub struct Differ {
    scope: Scope,
    path: Vec<Step>,
    found: Vec<Difference>,
}

impl Differ {
    /// A comparison of `scope` at the empty path.
    #[must_use]
    pub const fn new(scope: Scope) -> Self {
        Self {
            scope,
            path: Vec::new(),
            found: Vec::new(),
        }
    }

    /// The comparison reads the fields the game's save does not hold.
    #[must_use]
    pub fn compares_unsaved(&self) -> bool {
        self.scope == Scope::All
    }

    /// The comparison reads the fields marked as known gaps.
    #[must_use]
    pub fn compares_gaps(&self) -> bool {
        self.scope != Scope::Saved
    }

    /// The differences found so far.
    #[must_use]
    pub fn into_differences(self) -> Vec<Difference> {
        self.found
    }

    /// Compares the field `name`.
    pub fn field<T: Diff + ?Sized>(&mut self, name: &'static str, left: &T, right: &T) {
        self.under(Step::Field(name), |differ| left.diff(right, differ));
    }

    /// Compares the field `name` by its equality, as one value.
    pub fn equal_field<T: PartialEq + Debug + ?Sized>(
        &mut self,
        name: &'static str,
        left: &T,
        right: &T,
    ) {
        if left != right {
            self.under(Step::Field(name), |differ| differ.differs(left, right));
        }
    }

    /// Compares two values by their equality, as one value, at the current path.
    pub fn equal<T: PartialEq + Debug + ?Sized>(&mut self, left: &T, right: &T) {
        if left != right {
            self.differs(left, right);
        }
    }

    /// Records a difference at the current path, each side by its `Debug` text.
    pub fn differs<L: Debug + ?Sized, R: Debug + ?Sized>(&mut self, left: &L, right: &R) {
        self.record(format!("{left:?}"), format!("{right:?}"));
    }

    /// Records two variants of an enum at the current path, by their names.
    pub fn variants(&mut self, left: &str, right: &str) {
        self.record(left.to_owned(), right.to_owned());
    }

    /// Compares two sequences: their length, then each element both hold by its index, then
    /// each element only one holds.
    pub fn sequence<'item, T: Diff + 'item>(
        &mut self,
        left: impl ExactSizeIterator<Item = &'item T>,
        right: impl ExactSizeIterator<Item = &'item T>,
    ) {
        let (lengths, mut left, mut right) = ((left.len(), right.len()), left, right);
        if lengths.0 != lengths.1 {
            self.field("len", &lengths.0, &lengths.1);
        }
        for index in 0..lengths.0.max(lengths.1) {
            match (left.next(), right.next()) {
                (Some(left_item), Some(right_item)) => self.under(Step::Index(index), |differ| {
                    left_item.diff(right_item, differ);
                }),
                (left, right) => {
                    self.under(Step::Index(index), |differ| differ.one_side(left, right));
                }
            }
        }
    }

    /// Compares two maps key by key, in the keys' order; a key only one holds is absent on the
    /// other side.
    pub fn map<K: Ord + Debug, V: Diff>(&mut self, left: &BTreeMap<K, V>, right: &BTreeMap<K, V>) {
        let keys: BTreeSet<&K> = left.keys().chain(right.keys()).collect();
        for key in keys {
            let step = Step::Key(format!("{key:?}"));
            match (left.get(key), right.get(key)) {
                (Some(left_item), Some(right_item)) => {
                    self.under(step, |differ| left_item.diff(right_item, differ));
                }
                (left, right) => self.under(step, |differ| differ.one_side(left, right)),
            }
        }
    }

    /// A value one side holds and the other does not.
    fn one_side<T: Debug + ?Sized>(&mut self, left: Option<&T>, right: Option<&T>) {
        let text =
            |side: Option<&T>| side.map_or_else(|| ABSENT.to_owned(), |value| format!("{value:?}"));
        self.record(text(left), text(right));
    }

    /// Runs `compare` with `step` added to the path.
    fn under(&mut self, step: Step, compare: impl FnOnce(&mut Self)) {
        self.path.push(step);
        compare(self);
        let _ = self.path.pop();
    }

    fn record(&mut self, left: String, right: String) {
        let path = self.path_text();
        self.found.push(Difference { path, left, right });
    }

    fn path_text(&self) -> String {
        let mut text = String::new();
        for step in &self.path {
            let _ = match step {
                Step::Field(name) if text.is_empty() => write!(text, "{name}"),
                Step::Field(name) => write!(text, ".{name}"),
                Step::Index(index) => write!(text, "[{index}]"),
                Step::Key(key) => write!(text, "[{key}]"),
            };
        }
        text
    }
}

/// Types compared by their equality, as one value.
macro_rules! by_equality {
    ($($t:ty),* $(,)?) => {
        $(impl Diff for $t {
            fn diff(&self, other: &Self, differ: &mut Differ) {
                differ.equal(self, other);
            }
        })*
    };
}

by_equality!(
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    bool,
    char,
    (),
    str,
    String
);

/// Floats compared by their bits; two NaNs that differ show their bits.
macro_rules! by_bits {
    ($($t:ty: $width:literal),*) => {
        $(impl Diff for $t {
            fn diff(&self, other: &Self, differ: &mut Differ) {
                if self.to_bits() == other.to_bits() {
                    return;
                }
                if self.is_nan() && other.is_nan() {
                    let nan = |value: &$t| format!("NaN ({:#0width$x})", value.to_bits(), width = $width);
                    differ.record(nan(self), nan(other));
                } else {
                    differ.differs(self, other);
                }
            }
        })*
    };
}

by_bits!(f32: 10, f64: 18);

impl<T: Diff> Diff for Option<T> {
    fn diff(&self, other: &Self, differ: &mut Differ) {
        match (self, other) {
            (Some(left), Some(right)) => left.diff(right, differ),
            (None, None) => {}
            _ => differ.differs(self, other),
        }
    }
}

impl<T: Diff + ?Sized> Diff for Box<T> {
    fn diff(&self, other: &Self, differ: &mut Differ) {
        (**self).diff(other, differ);
    }
}

impl<T: Diff> Diff for [T] {
    fn diff(&self, other: &Self, differ: &mut Differ) {
        differ.sequence(self.iter(), other.iter());
    }
}

impl<T: Diff, const N: usize> Diff for [T; N] {
    fn diff(&self, other: &Self, differ: &mut Differ) {
        differ.sequence(self.iter(), other.iter());
    }
}

impl<T: Diff> Diff for Vec<T> {
    fn diff(&self, other: &Self, differ: &mut Differ) {
        differ.sequence(self.iter(), other.iter());
    }
}

impl<T: Diff> Diff for VecDeque<T> {
    fn diff(&self, other: &Self, differ: &mut Differ) {
        differ.sequence(self.iter(), other.iter());
    }
}

impl<K: Ord + Debug, V: Diff> Diff for BTreeMap<K, V> {
    fn diff(&self, other: &Self, differ: &mut Differ) {
        differ.map(self, other);
    }
}

impl<T: Ord + Debug> Diff for BTreeSet<T> {
    fn diff(&self, other: &Self, diffs: &mut Differ) {
        for value in self.symmetric_difference(other) {
            let step = Step::Key(format!("{value:?}"));
            let (left, right) = (self.get(value), other.get(value));
            diffs.under(step, |differ| differ.one_side(left, right));
        }
    }
}

/// Tuples, their fields numbered.
macro_rules! tuple {
    ($($name:ident $index:tt),+) => {
        impl<$($name: Diff),+> Diff for ($($name,)+) {
            fn diff(&self, other: &Self, differ: &mut Differ) {
                $(differ.field(stringify!($index), &self.$index, &other.$index);)+
            }
        }
    };
}

tuple!(A 0, B 1);
tuple!(A 0, B 1, C 2);
tuple!(A 0, B 1, C 2, D 3);

#[cfg(test)]
mod tests;
