//! The game's data as genie holds it (FD-046): the players' masters, the technologies and their
//! effects, the tech tree, the unit lines, the drop sites and the graphics, with the newtypes
//! their fields hold and the operations that change them (an effect's change to a master). Plain
//! data and functions over it; the systems that run a game are genie-core's. The modules mirror
//! genie-core's paths until the components regroup them (FD-046 step 3).

// The port's exceptions to the workspace's lints, genie-core's own for the code that left it
// (FD-046-D35): the model holds build 158041's values and conversions as its code does.
#![allow(
    // the game's f32 sums and products, each rounded on its own (no fused multiply-add)
    clippy::suboptimal_flops,
    // the game's conversions (cvttss2si, movsx, movzx, truncating stores), written as `as`
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    // the decompiled routines' names (dx, dy, x0, y1 …) and flag fields
    clippy::similar_names,
    // a routine is ported whole, its documentation opens with the routine and its addresses
    clippy::too_many_lines,
    clippy::too_long_first_doc_paragraph
)]

pub use genie_error::{ErrorKind, GenieError, GenieResult};

pub mod combat;
pub mod layer;
pub mod map;
pub mod math;
pub mod model;
pub mod movement;
pub mod objects;
pub mod players;
pub mod production;
#[cfg(feature = "schema")]
mod schema;
pub mod technology;
#[cfg(test)]
mod test_ids;
pub mod unit_ai;
pub mod world;
