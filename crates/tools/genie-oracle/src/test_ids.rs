//! The tests' object and group ids, through the types' checked constructors.

use genie_core::groups::GroupId;
use genie_core::objects::ObjectId;
use genie_core::objects::masters::ActionKind;
use genie_core::objects::masters::ClassId;

/// The object id `value` (not −1).
pub(crate) const fn object_id(value: i32) -> ObjectId {
    match ObjectId::of_field(value) {
        Some(id) => id,
        None => panic!("a test's object id is not −1"),
    }
}

/// The unit group `value` (not −1).
pub(crate) const fn group_id(value: i32) -> GroupId {
    match GroupId::of_field(value) {
        Some(id) => id,
        None => panic!("a test's group id is not −1"),
    }
}

/// The action type `value` (0 or more).
pub(crate) const fn action_type(value: i16) -> ActionKind {
    match ActionKind::of_data(value) {
        Some(kind) => kind,
        None => panic!("a test's action type is 0 or more"),
    }
}

/// The class `value` (0 or more).
pub(crate) fn class_id(value: i16) -> ClassId {
    ClassId::try_from(value).expect("a test's class is 0 or more")
}
