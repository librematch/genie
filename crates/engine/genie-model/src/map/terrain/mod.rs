//! What the `.dat` says of each terrain: its record as the map's terrain reads see it
//! ([`TerrainKind`]) and its class bit in the restrictions' masks ([`terrain_class`]).

pub mod terrain_kind;

pub use self::terrain_kind::TerrainKind;
pub use self::terrain_kind::terrain_class;
