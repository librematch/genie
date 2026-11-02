//! What a seer's sight covers: its shape ([`SightShape`]) and its circle's centre
//! ([`SightCentre`]), slot 0x1d0's and slot 0x1d8's choices.

use crate::vision::los_tables::{CENTRED, OFFSET};

/// The tiles a sight explores or lets go (slots 0x1d0 and 0x1d8, `FUN_141192560`'s `param_4`):
/// a circle of its line of sight, or its footprint as a rectangle (a foundation's, a converted
/// building's). The circle master's sight stays a circle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SightShape {
    /// 0: the circle of its line of sight around its tile.
    Circle,
    /// 1: the rectangle of its footprint.
    Footprint,
}

/// Where a sight's circle is centred (`explore_terrain`'s last argument, `FUN_141284a40`): on
/// its tile's centre (the table [`CENTRED`]) or on a corner of the tile (the table [`OFFSET`]),
/// as a building whose radii sum to an even number is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SightCentre {
    /// 0: the tile's centre.
    Tile,
    /// 1: the tile's corner.
    Corner,
}

impl SightCentre {
    /// The table of circle rows the centre reads.
    #[must_use]
    pub const fn table(self) -> &'static [&'static [(i8, i8, i8)]; 21] {
        match self {
            Self::Tile => &CENTRED,
            Self::Corner => &OFFSET,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::vision::los_tables::{CENTRED, OFFSET};
    use crate::vision::{SightCentre, SightShape};

    /// A tile's centre reads the centred table, a corner the offset one; the shapes differ.
    #[test]
    fn sight_centre_reads_its_table() {
        assert_eq!(SightCentre::Tile.table(), &CENTRED);
        assert_eq!(SightCentre::Corner.table(), &OFFSET);
        assert_ne!(SightShape::Circle, SightShape::Footprint);
    }
}
