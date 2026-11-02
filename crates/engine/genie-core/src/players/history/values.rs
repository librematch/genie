//! The history's values: an entry's kind and populations, a time slice, a count of the
//! history's updates, an attribute's tally and a share of the player's value.

use crate::math::conversions::{cvttss2si, cvttss2si_64};
use crate::math::image::history::{BATTLE_RECORD, BATTLE_START};
use crate::players::Amount;

/// An entry's kind (`add_new_history_entry`'s first argument): the update's population entry
/// is kind 0; an entry of any other kind keeps its slot zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EntryKind(u8);

impl EntryKind {
    /// 0: the civilian and military population, the only kind the update adds.
    pub const POPULATION: Self = Self(0);

    /// The kind of byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// A population an entry holds (`history[i]`, a short): the civilian or the military
/// population at a period's end, through `CVTTSS2SI` and its low 16 bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Population(i16);

impl Population {
    /// No population: a kind other than [`EntryKind::POPULATION`], or a negative sum.
    pub const ZERO: Self = Self(0);

    /// The population of the save's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The population of the attribute value `count` as the update hands it over: `CVTTSS2SI`
    /// (a NaN or an overflow is `i32::MIN`), then its low 16 bits.
    #[must_use]
    pub fn of_count(count: f32) -> Self {
        Self(cvttss2si(count) as i16)
    }

    /// The short, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

/// An entry of the history (`history[i]`): the civilian and the military population.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct HistoryEntry {
    /// The civilian population (attribute 37).
    pub civilian: Population,
    /// The military population (attribute 40).
    pub military: Population,
}

/// A time slice: the history's count of entries, as an event's `time_slice` (`+0x4`) holds it
/// when it happened, and the entries' buffer (`max_number_of_entries`, `+0x20`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimeSlice(i32);

impl TimeSlice {
    /// The time slice of the save's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The time slice of `count` entries (the game's 32-bit count).
    #[must_use]
    pub const fn of_count(count: usize) -> Self {
        Self(count as i32)
    }

    /// This one `more` slices on, wrapping as the game's int add.
    #[must_use]
    pub const fn wrapping_add(self, more: Self) -> Self {
        Self(self.0.wrapping_add(more.0))
    }

    /// The int, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

/// A number of the history's updates, one a game second: the period (the `.dat`'s
/// `time_slice`, `world+0x8b8`) and the runs since the last one (`update_history_count`,
/// `+0x28`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HistoryUpdates(i32);

impl HistoryUpdates {
    /// The number of the save's or the `.dat`'s int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// One update more, wrapping as the game's int add.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// This number less `period`, wrapping as the game's int sub.
    #[must_use]
    pub const fn wrapping_sub(self, period: Self) -> Self {
        Self(self.0.wrapping_sub(period.0))
    }

    /// The int, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl genie_diff::Diff for HistoryUpdates {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// An attribute's whole value as the history tallies it (`CVTTSS2SI`): kills, razings and the
/// value of units and buildings, at a period's end, since it or summed over a battle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tally(i32);

impl Tally {
    /// Nothing tallied.
    pub const ZERO: Self = Self(0);

    /// The tally of the save's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The tally of the attribute value `amount`: `CVTTSS2SI` into 32 bits.
    #[must_use]
    pub fn of_amount(amount: Amount) -> Self {
        Self(cvttss2si(amount.get()))
    }

    /// The tally of the attribute value `amount`: `CVTTSS2SI` into 64 bits, the low 32 kept.
    #[must_use]
    pub fn of_amount_64(amount: Amount) -> Self {
        Self(cvttss2si_64(amount.get()) as i32)
    }

    /// This tally and `other`, wrapping as the game's int add.
    #[must_use]
    pub const fn wrapping_add(self, other: Self) -> Self {
        Self(self.0.wrapping_add(other.0))
    }

    /// This tally less `other`, wrapping as the game's int sub.
    #[must_use]
    pub const fn wrapping_sub(self, other: Self) -> Self {
        Self(self.0.wrapping_sub(other.0))
    }

    /// Its low 16 bits as an unsigned short (`MOVZX`): a count the battle sums.
    #[must_use]
    pub const fn low_short(self) -> Self {
        Self(self.0 as u16 as i32)
    }

    /// This tally over `whole`, both as floats.
    #[must_use]
    pub fn ratio(self, whole: Self) -> f32 {
        self.0 as f32 / whole.0 as f32
    }

    /// The int, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

/// A share of the player's value (`running_ave_BV_percent`, `+0xe0`, and a period's): the
/// kills' and razings' value over the player's own.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Share(f32);

impl Share {
    /// No share.
    pub const ZERO: Self = Self(0.0);

    /// The share of the save's float `value`.
    #[must_use]
    pub const fn of_data(value: f32) -> Self {
        Self(value)
    }

    /// The share `part` of `whole` is ([`Tally::ratio`]).
    #[must_use]
    pub fn of(part: Tally, whole: Tally) -> Self {
        Self(part.ratio(whole))
    }

    /// This share and `other`, as the game adds the floats.
    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }

    /// Whether a period's own share starts or goes on with a battle: [`BATTLE_START`] or more,
    /// as doubles.
    #[must_use]
    pub fn battling(self) -> bool {
        f64::from(self.0) >= BATTLE_START
    }

    /// Whether a battle's summed share makes it an event: [`BATTLE_RECORD`] or more, as
    /// doubles.
    #[must_use]
    pub fn recorded(self) -> bool {
        f64::from(self.0) >= BATTLE_RECORD
    }

    /// The float, as the save holds it.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A population is the count's `CVTTSS2SI` cut to 16 bits; a NaN gives 0.
    #[test]
    fn population_is_the_truncated_count_low_short() {
        assert_eq!(Population::of_count(7.9), Population::of_data(7));
        assert_eq!(Population::of_count(70_000.0).get(), 4464);
        assert_eq!(Population::of_count(f32::NAN), Population::ZERO);
        assert_eq!(Population::of_count(-2.5).get(), -2);
        assert_eq!(EntryKind::of_data(0), EntryKind::POPULATION);
        assert_eq!(EntryKind::of_data(3).get(), 3);
    }

    /// Time slices and update counts wrap as the game's ints.
    #[test]
    fn time_slices_and_update_counts_wrap() {
        assert_eq!(TimeSlice::of_count(3), TimeSlice::of_data(3));
        let max = TimeSlice::of_data(i32::MAX);
        assert_eq!(max.wrapping_add(TimeSlice::of_data(1)).get(), i32::MIN);
        assert!(TimeSlice::of_data(2) < TimeSlice::of_data(60));
        assert_eq!(HistoryUpdates::of_data(i32::MAX).next().get(), i32::MIN);
        let period = HistoryUpdates::of_data(30);
        assert_eq!(HistoryUpdates::of_data(31).wrapping_sub(period).get(), 1);
        assert!(HistoryUpdates::of_data(29) < period);
    }

    /// A tally truncates the attribute (32 or 64 bits), wraps and keeps a count's low short.
    #[test]
    fn tally_truncates_wraps_and_keeps_the_low_short() {
        assert_eq!(Tally::of_amount(Amount::new(12.9)), Tally::of_data(12));
        assert_eq!(Tally::of_amount(Amount::new(f32::NAN)).get(), i32::MIN);
        // 2^32 + 5 through 64 bits keeps the low 32: 0
        assert_eq!(
            Tally::of_amount_64(Amount::new(4_294_967_296.0)),
            Tally::ZERO
        );
        assert_eq!(Tally::of_amount_64(Amount::new(-3.5)).get(), -3);
        assert_eq!(
            Tally::of_data(i32::MAX)
                .wrapping_add(Tally::of_data(1))
                .get(),
            i32::MIN
        );
        assert_eq!(Tally::of_data(5).wrapping_sub(Tally::of_data(7)).get(), -2);
        assert_eq!(Tally::of_data(-1).low_short().get(), 65_535);
        assert_eq!(Tally::of_data(70_000).low_short().get(), 4464);
        assert!((Tally::of_data(1).ratio(Tally::of_data(4)) - 0.25).abs() < f32::EPSILON);
    }

    /// A share adds as floats and compares with the battle's thresholds as doubles.
    #[test]
    fn share_adds_and_meets_the_battle_thresholds() {
        let share = Share::of(Tally::of_data(3), Tally::of_data(25));
        assert_eq!(share.get(), 3.0 / 25.0);
        // 0.12 as a float lies below the double's 0.12
        assert!(!share.battling());
        assert!(Share::of_data(0.13).battling());
        assert!(!Share::of_data(0.14).recorded());
        assert!(Share::of_data(0.1).plus(Share::of_data(0.06)).recorded());
        assert_eq!(Share::ZERO.plus(Share::of_data(0.5)), Share::of_data(0.5));
    }
}
