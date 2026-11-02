//! The tests' object and group ids, through the types' checked constructors.

use genie_core::groups::GroupId;
use genie_core::objects::ObjectId;
use genie_core::objects::masters::ActionKind;
use genie_core::objects::masters::ClassId;
use genie_core::objects::masters::MasterId;

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

/// Build 158041's male villager (master 83), the tests' unit.
pub(crate) const VILLAGER_MALE: MasterId = MasterId::named(83);
/// Build 158041's female villager (master 293).
pub(crate) const VILLAGER_FEMALE: MasterId = MasterId::named(293);

/// The player attributes the tests name, by build 158041's numbers (the engine reads them from
/// the build layer, FD-046-D48).
pub(crate) mod attribute {
    use genie_core::players::attribute::AttributeId;

    /// Food.
    pub(crate) const FOOD: AttributeId = AttributeId::named(0);
    /// Wood.
    pub(crate) const WOOD: AttributeId = AttributeId::named(1);
    /// Stone.
    pub(crate) const STONE: AttributeId = AttributeId::named(2);
    /// Gold.
    pub(crate) const GOLD: AttributeId = AttributeId::named(3);
    /// The population headroom.
    pub(crate) const HEADROOM: AttributeId = AttributeId::named(4);
}
