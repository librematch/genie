//! The arithmetic of build 158041 as its code does it: `CVTTSS2SI` conversions, the image's own
//! math routines (`fmath`), angles, the constants the game's code computes with (`image`,
//! generated) and the map's points, positions and tiles. genie-core and genie-model compute with
//! it; it depends on no engine crate (FD-046-D28).

// The port's exceptions to the workspace's lints, genie-core's own for the code that left it
// (FD-046-D28): the game's math reproduced bit for bit keeps its arithmetic and conversions.
#![allow(
    // the game's f32 sums and products, each rounded on its own (no fused multiply-add)
    clippy::suboptimal_flops,
    // the game's conversions (cvttss2si, movsx, movzx, truncating stores), written as `as`
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    // the ported routines' math names (x, y, z, k, hx …)
    clippy::many_single_char_names,
    // a port's documentation opens with the routine and its addresses
    clippy::too_long_first_doc_paragraph
)]

mod angle;
pub mod conversions;
pub mod fmath;
pub mod geometry;
pub mod image;

pub use self::angle::Angle;
