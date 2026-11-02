//! A train or research button: as a QUEUE command and a queue entry hold it ([`QueueButton`]),
//! and as a train or research location holds it ([`LocationButton`]).

/// A train or research button as a QUEUE command (`button_location_id`) and a queue entry
/// (`mButtonLocationId`, `+0x28`) hold it: an int, −1 for none. A location matches its low byte
/// (`FUN_1411cbc40`). Every value is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct QueueButton(i32);

impl QueueButton {
    /// −1: no button (the first location's train time).
    pub const NONE: Self = Self(-1);

    /// The button of the command's or the save's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// Whether location button `button` is this one: its low byte against the location's.
    #[must_use]
    pub const fn names(self, button: LocationButton) -> bool {
        self.0 as u8 == button.0
    }

    /// The int, as the command and the save hold it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

/// A train or research location's button (`buttonLocationID`, a byte of the `.dat` or of a
/// technology change): 0 for none. Every byte is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LocationButton(u8);

impl LocationButton {
    /// 0: no button.
    pub const NONE: Self = Self(0);

    /// The button of the data's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl genie_diff::Diff for QueueButton {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

impl genie_diff::Diff for LocationButton {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::production::{LocationButton, QueueButton};

    /// A queue's button names a location's by its low byte.
    #[test]
    fn queue_button_names_location_by_its_low_byte() {
        assert!(QueueButton::of_data(0x103).names(LocationButton::of_data(3)));
        assert!(!QueueButton::NONE.names(LocationButton::of_data(3)));
        assert!(QueueButton::NONE.names(LocationButton::of_data(0xff)));
        assert_eq!(QueueButton::default().get(), 0);
        assert_eq!(LocationButton::NONE.get(), 0);
    }
}
