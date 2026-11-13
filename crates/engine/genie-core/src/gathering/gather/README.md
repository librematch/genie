# `gathering/gather`: the Gather action

A forager sent to a resource walks to it (state 4, a quiet `MoveTo`), faces it (state 6) and
gathers (state 7). With a full load it walks to a drop site (state 8) and comes back to the
resource. Under the game's farm setting, a farm has four stops, and the farmer walks to a random
point on the farm for each stop.

- `mod.rs` holds the action's state and its constants.
- `start.rs` builds and starts the action, and holds its retarget and its halt.
- `update.rs` is one update in the walking, facing and gathering states.
- `walks.rs` builds the quiet `MoveTo`s of `set_state`: the one component, `MoveToAction`, to an
  object or to a point.
- `work.rs` holds the face, the work, the drop-off's update and the random waits.
- `point_walks.rs` holds the walks to a point, the searches and the waits for a farm.
- `unit.rs` is the gatherer's part of its update: the drop-off, the search and the update in the
  world.

## In the game

- `RGE_Action_Gather` (action type 5, `act_gath.cpp`, `tact_gth.cpp`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`gather_state.rs`](gather_state.rs) | A Gather's state: the steps of its update from the walk to the drop, each walk with its `MoveTo`. |
| [`mod.rs`](mod.rs) | The Gather action (`RGE_Action_Gather`, type 5, vtable `0x142ed4f28`): a forager sent to a resource walks to it (state 4, a quiet `MoveTo`), faces it (6), gathers (7), takes the load to a drop site (8) and walks back to the resource (4). |
| [`point_walks.rs`](point_walks.rs) | The walks to a point and the waits: back to search (state 11), back to reseed (0xf2), for a farm (0xf3), the search (3) and a move's walk (15). |
| [`start.rs`](start.rs) | The Gather's constructor and start, its retarget, the walk, the drop and a move's target, the idle and the halt; a farm's stops and their productivity. |
| [`tests.rs`](tests.rs) | Tests. |
| [`unit.rs`](unit.rs) | A builder's and a gatherer's drop-off and a gatherer's update and search. |
| [`update.rs`](update.rs) | One update (slot 6, `0x140a8e1d0`) in states 4, 6 and 7: the walk's `MoveTo`, the units in the way and the teardown. |
| [`walks.rs`](walks.rs) | The `MoveTo`s the Gather's `set_state` makes (`0x140a8cd40`): what their constructors read of the unit, and the two the Gather uses, one to an object and one to a point. |
| [`work.rs`](work.rs) | The face (state 6), the work (7), the drop-off's update (8) and a random wait. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`construction`](../../construction/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`orders`](../../orders/), [`players`](../../players/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
