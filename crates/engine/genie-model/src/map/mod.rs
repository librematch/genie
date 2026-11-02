//! The map's values that masters and technologies hold: distances, radii and footprints, terrain
//! and restriction ids and masks, and genie-math's points, positions and tiles.

pub mod distance;
pub mod radii;
pub mod restriction_id;
pub mod terrain_id;
pub mod terrain_mask;

pub mod restriction;
pub mod terrain;
pub mod terrain_flags;
pub use self::distance::Distance;
pub use self::distance::DistanceSquared;
pub use self::distance::WholeDistance;
pub use self::radii::Footprint;
pub use self::radii::Radii;
pub use self::restriction::Restriction;
pub use self::restriction_id::RestrictionId;
pub use self::terrain_flags::TerrainFlags;
pub use self::terrain_id::TerrainId;
pub use self::terrain_mask::TerrainMask;
pub use genie_math::geometry::Position;
pub use genie_math::geometry::{Axis, Point};
pub use genie_math::geometry::{GroundOffset, Offset};
pub use genie_math::geometry::{Tile, TileOffset, TileRect};
