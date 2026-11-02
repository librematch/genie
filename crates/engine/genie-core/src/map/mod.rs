//! The map: the terrain of each tile and where the world's objects stand (their places,
//! radii, footprints and the grid that finds them).

mod distance;
mod grid_size;
mod radii;
mod rect;
mod restriction;
mod restriction_id;
mod snow;
pub mod spatial;
pub mod terrain;
mod terrain_flags;
mod terrain_id;
mod terrain_mask;

pub use self::distance::{Distance, DistanceSquared, WholeDistance};
pub use self::grid_size::GridSize;
pub use self::radii::{Footprint, Radii};
pub use self::rect::{Rect, Segment};
pub use self::restriction::Restriction;
pub use self::restriction_id::RestrictionId;
pub use self::snow::Snow;
pub use self::terrain_flags::TerrainFlags;
pub use self::terrain_id::TerrainId;
pub use self::terrain_mask::TerrainMask;
pub use genie_math::geometry::Position;
pub use genie_math::geometry::{Axis, Point};
pub use genie_math::geometry::{GroundOffset, Offset};
pub use genie_math::geometry::{Tile, TileOffset, TileRect};
