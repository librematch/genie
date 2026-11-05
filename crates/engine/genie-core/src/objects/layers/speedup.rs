//! The building that speeds an action object up (`mSpeedupBuildingId` `+0x890`,
//! `mSpeedupValue` `+0x894`): the save holds both, the update drops them once the building is
//! gone or out of range (`RGE_Action_Object::update`, `0x14106ac90`). No routine of build
//! 158041 sets them, and none reads the value.

use crate::objects::ObjectId;
use genie_diff::Diff;

/// An action object's speed-up building and its value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct Speedup {
    /// `+0x890`: the building, none for −1.
    pub building: Option<ObjectId>,
    /// `+0x894`: the value.
    pub value: SpeedupValue,
}

impl Speedup {
    /// No building, value 0: the constructors' and the update's drop.
    pub const NONE: Self = Self {
        building: None,
        value: SpeedupValue::ZERO,
    };
}

/// The value of a speed-up (`+0x894`, a float).
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
}

impl Diff for SpeedupValue {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}
