//! The build layers genie knows, from their Nickel sources (FD-046-D08, D09's step 2):
//! `builds/<build>.ncl`, generated from the build's symbol table by `harness image layer`.

use crate::de::from_expr;
use crate::evaluate;
use genie_error::GenieError;
use genie_model::layer::BuildLayer;

/// Build 158041's layer source.
const BUILD_158041: &str = include_str!("../../../../builds/158041.ncl");

/// The layer of `build`, evaluated from its Nickel source.
///
/// # Errors
///
/// A [`GenieError`] for a build genie has no layer of, or a source that does not evaluate into a
/// layer.
pub fn build_layer(build: u32) -> Result<BuildLayer, GenieError> {
    let (source, name) = match build {
        158_041 => (BUILD_158041, "builds/158041.ncl"),
        _ => {
            return Err(
                GenieError::invalid_input("a build genie has no layer of").with("build", build)
            );
        }
    };
    from_expr(&evaluate(source, name)?)
}
