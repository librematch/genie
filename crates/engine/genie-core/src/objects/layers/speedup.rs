//! The building that speeds an object up: an action object's (`mSpeedupBuildingId` `+0x890`,
//! `mSpeedupValue` `+0x894`) and a moving object's (`+0x808`, `+0x80c`). The save holds both
//! pairs, and each update drops its pair once the building is gone or out of range
//! (`RGE_Action_Object::update`, `0x14106ac90`; `RGE_Moving_Object::update`, `0x14114dbb0`). A
//! speed-up building sets them on the objects about it (`FUN_1411c39b0`,
//! `construction/speedup.rs`). The moving object's own speed reads its value
//! (`FUN_14114d1d0`); nothing reads the action object's.

use crate::objects::ObjectId;
use genie_diff::Diff;

/// An object's speed-up building and its value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct Speedup {
    /// The building (`+0x890`, `+0x808`), none for −1.
    pub building: Option<ObjectId>,
    /// The value (`+0x894`, `+0x80c`): the share of its master's speed a moving object gains.
    pub value: SpeedupValue,
}

impl Speedup {
    /// No building, value 0: the constructors' and the update's drop.
    pub const NONE: Self = Self {
        building: None,
        value: SpeedupValue::ZERO,
    };

    /// The speed-up a speed-up building `building` gives by `value` (`FUN_14114cb70` for a moving
    /// object: `+0x80c` the value, `+0x808` the building's id; `FUN_14106a510` for an action
    /// object: `+0x894`, `+0x890`).
    #[must_use]
    pub const fn of_building(building: ObjectId, value: SpeedupValue) -> Self {
        Self {
            building: Some(building),
            value,
        }
    }
}

/// The value of a speed-up (`+0x894`, `+0x80c`, a float).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpeedupValue(f32);

impl SpeedupValue {
    /// None (the update's drop stores 0).
    pub const ZERO: Self = Self(0.0);

    /// The value of the save's float `value`.
    #[must_use]
    pub const fn of_data(value: f32) -> Self {
        Self(value)
    }

    /// The float.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Diff for SpeedupValue {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}
