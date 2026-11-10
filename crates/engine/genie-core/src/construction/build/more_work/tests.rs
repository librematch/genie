//! The reach of a builder's search for more work: the building's line of sight held to the
//! tuning's floor and cap, as the Build's case 0xdf compares them.

use crate::construction::build::more_work::more_work_reach;
use crate::map::Distance;
use crate::testing::tuning;
use crate::world::layer::generated::GatherSearchTuning;

/// The reach of line of sight `tiles` under build 158041's tuning (floor 4, cap 20).
fn reach(tiles: f32) -> Distance {
    more_work_reach(Distance::new(tiles), &tuning().gather_search)
}

#[test]
fn reach_below_the_floor_is_four() {
    assert_eq!(reach(3.5), Distance::new(4.0));
    assert_eq!(reach(0.0), Distance::new(4.0));
    assert_eq!(reach(f32::NEG_INFINITY), Distance::new(4.0));
}

#[test]
fn reach_between_the_floor_and_the_cap_is_the_line_of_sight() {
    assert_eq!(reach(12.5), Distance::new(12.5));
    assert_eq!(reach(20.0), Distance::new(20.0));
}

#[test]
fn reach_above_the_cap_is_the_cap() {
    assert_eq!(reach(20.5), Distance::new(20.0));
    assert_eq!(reach(f32::INFINITY), Distance::new(20.0));
}

#[test]
fn reach_at_the_floor_is_the_line_of_sight() {
    // a floor above the Build's 4.0 tells the line of sight from the 4.0 apart
    let raised = GatherSearchTuning {
        reach_floor: 6.0,
        ..tuning().gather_search
    };
    assert_eq!(
        more_work_reach(Distance::new(6.0), &raised),
        Distance::new(6.0)
    );
    assert_eq!(
        more_work_reach(Distance::new(5.5), &raised),
        Distance::new(4.0)
    );
}

#[test]
fn nan_line_of_sight_passes_both_compares() {
    // `CMOVBE` and the `JA` not taken keep an unordered line of sight, its bits as they stand
    let nan = f32::from_bits(0xffc0_0001);
    assert_eq!(reach(nan).get().to_bits(), 0xffc0_0001);
}
