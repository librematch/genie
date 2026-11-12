# `orders/gather`: the work command's Hunt and the Gather's retarget

A work command on a hunted animal starts a Hunt. The work command's notify sets the unit AI's
hunt order, and the action list starts the Hunt. A gather or work command on a Gather on top
hands the new target to the Gather's slot 0x48, which walks to a point, keeps its target, builds
its player's foundation, refuses a building or takes the target.

- `hunt.rs` holds the Hunt's start.
- `retarget.rs` holds the Gather's slot 0x48; `retarget/` holds its tests.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`hunt.rs`](hunt.rs) | A work command's Hunt: its start from the factory and the Hunt action's own start. |
| [`retarget.rs`](retarget.rs) | The Gather's slot 0x48 on a command's new target: a walk to a point without one, the target it keeps, its player's foundation it builds, a building other than a farm it refuses, else the Gather's retarget. |
| [`retarget/`](retarget/) | The Gather's retarget's tests. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`construction`](../../construction/), [`gathering`](../../gathering/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`players`](../../players/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
