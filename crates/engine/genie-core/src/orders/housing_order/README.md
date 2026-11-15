# `orders/housing_order`: the Housing order's tests

The tests of `UNIT_ORDER`'s Housing (`../housing_order.rs`) on the house of
`construction/housing`'s tests: the install and its payment, the guard by what the building holds
and by its work, a computer player's production, the fault of an object that is no building, and
the command's run. The work command's Housing (the factory's case 0x7b) has its tests here too:
the queue id −1, the unit AI's notify, the fault without a unit AI, the retake of a Housing on
top, and the move of an action object, whose `RGE_Action_List` makes no Housing.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`tests.rs`](tests.rs) | Tests: `UNIT_ORDER`'s Housing and the work command's on the house of `construction::housing`'s tests, and an action object's Housing task on `world::tests`'s mule cart. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: no other system.
<!-- /uses -->
