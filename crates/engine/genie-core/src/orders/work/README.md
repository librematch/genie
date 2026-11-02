# `orders/work`: the garrison finder's view

Before a unit garrisons, the work command asks the task finder whether the unit's own task
list holds a garrison task on the target.

- `garrison.rs` holds the unit as the finder's actor and that test.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`garrison.rs`](garrison.rs) | The garrison finder's view of a unit: the task finder's actor and whether its own task list holds a garrison task on a target. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`garrison`](../../garrison/), [`map`](../../map/), [`objects`](../../objects/), [`players`](../../players/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
