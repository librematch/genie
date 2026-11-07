//! A player's culture: set for every byte but 0, and its byte as the save holds it.

use crate::players::Culture;

/// Every culture but 0 is set, and the byte comes back as the save holds it.
#[test]
fn every_culture_but_zero_is_set() {
    assert!(!Culture::NONE.is_set() && !Culture::default().is_set());
    assert!(Culture::of_data(1).is_set() && Culture::of_data(255).is_set());
    assert_eq!(Culture::of_data(11).get(), 11);
}
