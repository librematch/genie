//! The bit-for-bit foundations: the game's f32 math, its random generators, the MSVC
//! algorithms whose order shows in the simulation, and the image's constants.

mod angle;
pub mod conversions;
pub mod fmath;
pub mod image;
pub mod msvc;
pub mod rng;

pub use self::angle::Angle;
