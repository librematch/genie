//! Objects that change hands: the Artifact action of livestock, which a unit of another player
//! takes over, and an ownership change with what it carries along (a building's annexes, its
//! garrison, the attributes it counts).

pub mod artifact;
mod artifact_state;
mod artifact_sub;
pub(crate) mod capture;
mod capture_kind;
pub(crate) mod change;

pub use self::capture_kind::CaptureKind;

#[cfg(test)]
mod tests;
