//! A shape's id in the obstruction manager's mesh.

use std::fmt::{Display, Error, Formatter};

/// A shape's id in the mesh ([`crate::pathing::mesh::Mesh`]): its slot, the place of its box and
/// its sides in the mesh's tables. An object keeps its shape's id (`+0x64c`); a freed slot links
/// the next free one through its box's first word, −1 for none ([`ShapeId::link`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShapeId(u32);

impl ShapeId {
    /// The shape of slot `slot`, for the tests and the game's dumps.
    #[must_use]
    pub const fn new(slot: u32) -> Self {
        Self(slot)
    }

    /// Its slot's number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Its place in the mesh's tables.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// The free list's link a freed slot's first word holds (`FUN_140fdc250`): the next free
    /// slot, −1 for none.
    #[must_use]
    pub const fn link(next: Option<Self>) -> i32 {
        match next {
            Some(s) => s.0 as i32,
            None => -1,
        }
    }

    /// The next free slot a freed slot's first word links (`FUN_140fdb8d0`), none for −1.
    #[must_use]
    pub const fn of_link(word: i32) -> Option<Self> {
        match word {
            -1 => None,
            w => Some(Self(w as u32)),
        }
    }
}

impl Display for ShapeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use crate::pathing::ShapeId;

    /// A free slot's link is the next free slot's number, −1 for none, and reads back.
    #[test]
    fn free_slot_link_is_the_next_slot_or_minus_one() {
        let s = ShapeId::new(7);
        assert_eq!((s.get(), s.index(), s.to_string()), (7, 7, "7".to_owned()));
        assert_eq!([Some(s), None].map(ShapeId::link), [7, -1]);
        assert_eq!([7, -1].map(ShapeId::of_link), [Some(s), None]);
    }
}
