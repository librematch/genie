//! The game's feature flags for experimental bug fixes: the load turns them on for a saved 1
//! alone.

use crate::world::FeatureFlags;

/// Only a saved 1 turns the flags on.
#[test]
fn only_saved_one_turns_the_flags_on() {
    assert_eq!(FeatureFlags::of_saved(1), FeatureFlags::On);
    for saved in [0, 2, 255] {
        assert_eq!(FeatureFlags::of_saved(saved), FeatureFlags::Off);
    }
    assert_eq!(FeatureFlags::default(), FeatureFlags::Off);
}
