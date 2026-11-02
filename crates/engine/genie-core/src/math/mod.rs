//! The bit-for-bit foundations: the game's random generators and the MSVC algorithms whose order
//! shows in the simulation, beside genie-math's f32 math and image constants, re-exported here.

pub mod msvc;
pub mod rng;

pub use genie_math::Angle;
pub use genie_math::conversions;
pub use genie_math::fmath;
pub use genie_math::image;
