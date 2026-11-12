# `orders/work`: the work command's parts

Before a unit garrisons, the work command asks the task finder whether the unit's own task
list holds a garrison task on the target. A unit whose action is on the task already takes the
new target with that action.

- `garrison.rs` holds the unit as the finder's actor and that test.
- `retarget.rs` holds the action that takes the new target: a Hunt or a Gather retargets, a
  Build or a Repair takes the target again. `retarget/` holds its tests.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`garrison.rs`](garrison.rs) | The garrison finder's view of a unit: the task finder's actor and whether its own task list holds a garrison task on a target. |
| [`retarget.rs`](retarget.rs) | A worker's action already on the task the work found takes the new target itself (`retake_task`): a Hunt or a Gather retargets, a Build or a Repair takes the target again. |
| [`retarget/`](retarget/) | The work command's retake's tests. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`construction`](../../construction/), [`garrison`](../../garrison/), [`gathering`](../../gathering/), [`map`](../../map/), [`math`](../../math/), [`objects`](../../objects/), [`players`](../../players/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
