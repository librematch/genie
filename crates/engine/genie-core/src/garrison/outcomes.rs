//! What a unit's way out of a building ([`LetOut`]), its way in ([`EnterOutcome`]) and its
//! way back to work after the town bell ([`WorkReturn`]) answer.

/// Whether a unit left the building it was in ([`crate::world::World::let_out`], `FUN_1411e7220`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum LetOut {
    /// It stands on the spot beside the building.
    Out,
    /// No spot was free, nor a point inside the building's footprint: the game's 0.
    Stayed,
}

/// Whether a unit got into a building ([`crate::objects::unit::Unit::enter_building`], `FUN_140a867b0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum EnterOutcome {
    /// The building took it in.
    Inside,
    /// The building had no room.
    NoRoom,
}

/// Whether a unit went back to work after the town bell ([`crate::world::World::back_to_work`], `FUN_1412081e0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum WorkReturn {
    /// It left the building, or went to its point outside.
    Went,
    /// It stayed: no spot was free, or outside it had no bell flag or point.
    Stayed,
}
