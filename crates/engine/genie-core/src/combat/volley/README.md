# `combat/volley`: the missile launch

A unit's fire and its volley share one launch. The launch makes the missile from its master
(from the pool when it holds one), draws its sprite and facet, puts it on the map and starts
its action.

- `launch.rs` holds the launch.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`launch.rs`](launch.rs) | A unit's missile launch: the missile's making and start, which its fire and its volley share. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`players`](../../players/), [`vision`](../../vision/), [`world`](../../world/).
<!-- /uses -->
