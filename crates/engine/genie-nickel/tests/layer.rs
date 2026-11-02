//! Build 158041's layer from its Nickel source is the layer genie's tests build from the
//! generated fixture (FD-046-D23's round trip).

#[path = "../../genie-core/src/testing/build_158041.rs"]
mod build_158041;

use genie_nickel::layer::build_layer;

#[test]
fn the_nickel_layer_is_the_fixtures() {
    assert_eq!(build_layer(158_041).unwrap(), build_158041::build_158041());
}

#[test]
fn builds_genie_has_no_layer_of_fail() {
    assert!(build_layer(1).is_err());
}

/// The committed contracts parse, and build 158041's layer source passes their `BuildLayer`
/// contract.
#[test]
fn the_layer_source_passes_the_committed_contracts() {
    let contracts = include_str!("../../../../contracts/genie.ncl");
    let layer = include_str!("../../../../builds/158041.ncl");
    let checked = format!("let C = {contracts} in ({layer} | C.BuildLayer)");
    let result = genie_nickel::evaluate(&checked, "158041.ncl").map(drop);
    assert!(result.is_ok(), "{:?}", result.err().map(|e| e.to_string()));
}
