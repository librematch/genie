//! The tests' newtypes of a value the test knows is one, through the types' checked
//! constructors: genie-core's `test_ids` helpers that genie-model's tests use (a copy until a
//! shared test crate holds them, FD-044).

use crate::objects::masters::CostAmount;
use crate::objects::masters::MasterId;
use crate::objects::masters::{COST_SLOTS, CostSlot};
use crate::players::attribute::AttributeId;

/// The master `value` (0 or more).
pub(crate) fn master_id(value: i16) -> MasterId {
    MasterId::try_from(value).expect("a test's master is 0 or more")
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
