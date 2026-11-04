//! The id of an obstruction record and of an entry of a pather's ignore list: an object's, a
//! tile's temporary record's or a map edge's.

use crate::map::Tile;
use crate::objects::{ObjectId, ObjectKey};
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

/// The top four bits of a tile's temporary record (`FUN_1410510c0`).
const TILE_BITS: u32 = 0xf000_0000;
/// The top eight bits of a map edge's record (`edge_records`, the manager's crossings).
const EDGE_BITS: u32 = 0xff00_0000;

/// An obstruction record's id (`+0x18`), as the queries return it and a pather's ignore list
/// (`FUN_140f89e40`) holds it: an object's own id, a tile's temporary record
/// (`FUN_1410510c0`: `x << 12 | y` with the top four bits set) or a map edge's crossing
/// (`x << 12 | y` with the top eight bits set). The game keeps all three as one integer, so two
/// ids are equal, and sort, by that value ([`RecordId::value`]): a tile's sorts below every
/// object's.
#[derive(Clone, Copy, Debug)]
pub enum RecordId {
    /// An object's record.
    Object(ObjectId),
    /// A tile's temporary record.
    Tile(Tile),
    /// A map edge's record at its crossing, which may lie off the map.
    Edge(Tile),
}

impl RecordId {
    /// The integer the game holds for the record.
    #[must_use]
    pub const fn value(self) -> i32 {
        match self {
            Self::Object(id) => id.get(),
            Self::Tile(t) => (t.x() << 12) | t.y() | TILE_BITS.cast_signed(),
            Self::Edge(tile) => (tile.x() << 12) | tile.y() | EDGE_BITS.cast_signed(),
        }
    }

    /// The object whose record it is, none for a tile's or an edge's.
    #[must_use]
    pub const fn object(self) -> Option<ObjectId> {
        match self {
            Self::Object(id) => Some(id),
            Self::Tile(_) | Self::Edge(_) => None,
        }
    }
}

impl From<ObjectId> for RecordId {
    fn from(id: ObjectId) -> Self {
        Self::Object(id)
    }
}

impl PartialEq for RecordId {
    fn eq(&self, other: &Self) -> bool {
        self.value() == other.value()
    }
}

impl Eq for RecordId {}

impl PartialOrd for RecordId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RecordId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value().cmp(&other.value())
    }
}

impl Hash for RecordId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value().hash(state);
    }
}

/// A lookup by a record's id reads its integer, as the game's table does with a query's hit: an
/// edge's value can be a recyclable object's id (the edge at (−1, −2) is −2), and −1 names none.
impl ObjectKey for RecordId {
    fn key(self) -> Option<ObjectId> {
        ObjectId::of_field(self.value())
    }
}

impl PartialEq<ObjectId> for RecordId {
    fn eq(&self, other: &ObjectId) -> bool {
        self.value() == other.get()
    }
}

#[cfg(test)]
mod tests {
    use crate::map::Tile;
    use crate::map::spatial::RecordId;
    use crate::objects::ObjectKey;
    use crate::test_ids::object_id;

    /// An object's record is its id; a tile's and an edge's carry the tile under their marks.
    #[test]
    fn record_value_is_the_game_integer() {
        assert_eq!(RecordId::from(object_id(7)).value(), 7);
        assert_eq!(
            RecordId::Tile(Tile::new(5, 6)).value(),
            (5 << 12) | 6 | 0xf000_0000_u32.cast_signed()
        );
        assert_eq!(
            RecordId::Edge(Tile::new(5, 6)).value(),
            (5 << 12) | 6 | 0xff00_0000_u32.cast_signed()
        );
        assert_eq!(RecordId::from(object_id(7)).object(), Some(object_id(7)));
        assert_eq!(RecordId::Tile(Tile::new(5, 6)).object(), None);
    }

    /// Two records are equal, and sort, by their values: a tile's below an object's, and an
    /// edge's off the map equal to the object whose id has its value (the game's integers).
    #[test]
    fn records_compare_by_their_values() {
        let tile = RecordId::Tile(Tile::new(5, 5));
        let object = RecordId::from(object_id(7));
        assert!(tile < object);
        let mut ids = [object, tile, RecordId::from(object_id(-2))];
        ids.sort();
        assert_eq!(ids.map(RecordId::value)[0], tile.value());
        let edge = RecordId::Edge(Tile::new(-1, -2));
        assert_eq!(edge.value(), -2);
        assert_eq!(edge, RecordId::from(object_id(-2)));
        assert_eq!(object, object_id(7));
        assert_eq!(edge.key(), Some(object_id(-2)));
        assert_eq!(RecordId::Edge(Tile::new(-1, -1)).key(), None);
    }
}
