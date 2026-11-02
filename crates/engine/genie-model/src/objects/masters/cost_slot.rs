//! A master's cost (`+0x880`): three slots in their places, an empty one among them.

use crate::objects::masters::CostAmount;
use crate::players::attribute::AttributeId;
use genie_diff::Diff;

/// The number of a master's cost slots (`+0x880`, six bytes each).
pub const COST_SLOTS: usize = 3;

/// One of a master's cost slots (`+0x880`: the attribute, the amount and the paid flag, six
/// bytes): the `.dat`'s `resource_costs` and the save's `build_inventory` entry in its place. An
/// empty slot holds attribute −1 (none); the readers pass over it, and the slots after it keep
/// their places (the queue's gold slot, the upgrade effect's removal and addition).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CostSlot {
    /// The attribute it costs, none for an empty slot (−1).
    pub attribute: Option<AttributeId>,
    /// How much.
    pub amount: CostAmount,
    /// The paid flag (`+0x884`): the payment takes it; an unpaid entry (the population) is
    /// only tested.
    pub paid: bool,
}

impl CostSlot {
    /// A slot of `attribute`, `amount` and the paid flag.
    #[must_use]
    pub const fn new(attribute: AttributeId, amount: CostAmount, paid: bool) -> Self {
        Self {
            attribute: Some(attribute),
            amount,
            paid,
        }
    }

    /// The slot's entry `(attribute, amount, paid)`, none for an empty slot.
    #[must_use]
    pub const fn entry(self) -> Option<(AttributeId, CostAmount, bool)> {
        match self.attribute {
            Some(attribute) => Some((attribute, self.amount, self.paid)),
            None => None,
        }
    }
}

/// The entries of `slots` with an attribute, in their order (the readers that pass over the
/// empty ones).
pub fn entries(slots: &[CostSlot]) -> impl Iterator<Item = (AttributeId, CostAmount, bool)> + '_ {
    slots.iter().filter_map(|s| s.entry())
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::CostAmount;
    use crate::objects::masters::cost_slot::{CostSlot, entries};
    use crate::test_ids::attribute;
    use crate::test_ids::costs;

    /// An empty slot keeps its place: the entries pass over it, the slot after it stays third.
    #[test]
    fn an_empty_slot_keeps_its_place_and_the_entries_pass_over_it() {
        let villager = [
            CostSlot::new(attribute::FOOD, CostAmount::of_data(50), true),
            CostSlot::default(),
            CostSlot::new(attribute::HEADROOM, CostAmount::of_data(1), false),
        ];
        assert_eq!(villager[1].entry(), None);
        assert_eq!(
            entries(&villager)
                .map(|(a, n, p)| (a, n.get(), p))
                .collect::<Vec<_>>(),
            [(attribute::FOOD, 50, true), (attribute::HEADROOM, 1, false)]
        );
        assert_eq!(
            costs(&[(attribute::WOOD, 25, true)]),
            [
                CostSlot::new(attribute::WOOD, CostAmount::of_data(25), true),
                CostSlot::default(),
                CostSlot::default()
            ]
        );
    }
}
