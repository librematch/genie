//! The tests' newtypes: an object id, a unit group, a player or a master of a value the test knows is one,
//! a list of ids and an object's load, through the types' checked constructors.

use crate::commands::UnitList;
use crate::groups::GroupId;
use crate::objects::ObjectId;
use crate::objects::lifecycle::Held;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::CostAmount;
use crate::objects::masters::MasterId;
use crate::objects::masters::TaskId;
use crate::objects::masters::{COST_SLOTS, CostSlot};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::technology::TechId;
use crate::unit_ai::OrderTargetKind;
use crate::unit_ai::best_target::AttackCategory;

/// The object id `value` (not −1); const, so a test's constants name objects too.
pub(crate) const fn object_id(value: i32) -> ObjectId {
    match ObjectId::of_field(value) {
        Some(id) => id,
        None => panic!("a test's object id is not −1"),
    }
}

/// The action type `value` (0 or more).
pub(crate) const fn action_kind(value: i16) -> ActionKind {
    match ActionKind::of_data(value) {
        Some(kind) => kind,
        None => panic!("a test's action type is 0 or more"),
    }
}

/// The entries of a list of ids of `values`, −1 for none.
pub(crate) fn object_ids(values: impl IntoIterator<Item = i32>) -> Vec<Option<ObjectId>> {
    values.into_iter().map(ObjectId::of_field).collect()
}

/// The object ids `values` (none of them −1), in order.
pub(crate) fn object_id_list(values: impl IntoIterator<Item = i32>) -> Vec<ObjectId> {
    values.into_iter().map(object_id).collect()
}

/// The unit group `value` (not −1); const, so a test's constants name groups too.
pub(crate) const fn group_id(value: i32) -> GroupId {
    match GroupId::of_field(value) {
        Some(id) => id,
        None => panic!("a test's group id is not −1"),
    }
}

/// The player `number`.
pub(crate) fn player_id(number: u8) -> PlayerId {
    PlayerId::from(number)
}

/// The master `value` (0 or more).
pub(crate) fn master_id(value: i16) -> MasterId {
    MasterId::try_from(value).expect("a test's master is 0 or more")
}

/// The master `value`, or none for −1 (a field the game fills with −1).
pub(crate) fn opt_master(value: i16) -> Option<MasterId> {
    MasterId::of_data(value)
}

/// The task `value` of a test's master (0 or more); const, so a test's constants name tasks too.
pub(crate) const fn task_id(value: i16) -> TaskId {
    assert!(0 <= value, "a negative task id");
    TaskId::of_task(value)
}

/// The class `value` (0 or more).
pub(crate) fn class_id(value: i16) -> ClassId {
    ClassId::try_from(value).expect("a test's class is 0 or more")
}

/// The class `value`, or none for −1 (a unit AI's target class the game holds as −1).
pub(crate) fn class_of(value: i16) -> Option<ClassId> {
    ClassId::of_field(value).expect("a test's class is −1 or more")
}

/// A queued order's target type `value`, or none for −1.
pub(crate) fn order_kind(value: i32) -> Option<OrderTargetKind> {
    OrderTargetKind::of_field(value).expect("a test's target type is −1 or more")
}

/// A queued order's target owner `value`, or none for −1.
pub(crate) fn owner(value: i32) -> Option<PlayerId> {
    PlayerId::of_field(value).expect("a test's owner is −1 or a player")
}

/// The attack category `value` (0 to 11), or none for −1.
pub(crate) fn attack_category(value: i32) -> Option<AttackCategory> {
    AttackCategory::of_field(value).expect("a test's attack category is −1 or 0 to 11")
}

/// The attack category `value` (0 to 11).
pub(crate) fn category_of(value: i32) -> AttackCategory {
    attack_category(value).expect("a test's attack category is 0 to 11")
}

/// The technology `value` (0 or more).
pub(crate) fn tech_id(value: i16) -> TechId {
    TechId::try_from(value).expect("a test's technology is 0 or more")
}

/// The unit list of `ids`, its count theirs (fewer than `0xffff`).
pub(crate) fn unit_list(ids: Vec<Option<ObjectId>>) -> UnitList {
    UnitList::try_from(ids).expect("a test's unit list is shorter than 0xffff ids")
}

/// An object's load: `amount` of attribute `kind` (none for an object that carries nothing).
pub(crate) const fn carrying(kind: Option<AttributeId>, amount: f32) -> Held {
    Held {
        kind,
        amount: Amount::new(amount),
    }
}

/// A player's `n` attributes (at least the four resources): food, wood, stone and gold from
/// `resources`, the rest 0.
pub(crate) fn attributes(n: usize, resources: [f32; 4]) -> Vec<Amount> {
    let mut a = vec![Amount::ZERO; n.max(4)];
    for (v, r) in a.iter_mut().zip(resources) {
        *v = Amount::new(r);
    }
    a
}

/// `values` as attribute amounts, a test's expected attributes.
pub(crate) fn amounts<const N: usize>(values: [f32; N]) -> [Amount; N] {
    values.map(Amount::new)
}

/// Attribute adds of `adds`' amounts, a test's expected adds.
pub(crate) fn adds<const N: usize>(adds: [(AttributeId, f32); N]) -> [(AttributeId, Amount); N] {
    adds.map(|(kind, amount)| (kind, Amount::new(amount)))
}

/// A master's cost slots of `entries` in their order, the slots after them empty.
pub(crate) fn costs(entries: &[(AttributeId, i16, bool)]) -> [CostSlot; COST_SLOTS] {
    assert!(entries.len() <= COST_SLOTS, "a master has three cost slots");
    let mut slots = [CostSlot::default(); COST_SLOTS];
    for (slot, &(attribute, amount, paid)) in slots.iter_mut().zip(entries) {
        *slot = CostSlot::new(attribute, CostAmount::of_data(amount), paid);
    }
    slots
}
