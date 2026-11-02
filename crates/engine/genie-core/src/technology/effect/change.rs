//! An effect's change to a value: set, add or multiply (FD-046).

/// How slot 0x148 changes an attribute (its second argument): set (0, slot 0x150), add (1, slot
/// 0x158) or multiply (2, slot 0x160); the masters' slots 0x18, 0x20, 0x28 and the objects' slots
/// 0x78, 0x80, 0x88 follow it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// The attribute takes the amount.
    Set,
    /// The amount adds to it.
    Add,
    /// It is multiplied by the amount.
    Multiply,
}
