# `gathering/hunt/unit`: the hunter's object found

A hunter's search ends with an object or with nothing. An object found becomes the hunt's
target. A hunt that fails at once shows the standing sprite.

- `found.rs` holds the target taken and the failed hunt's sprite.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`found.rs`](found.rs) | A hunter's object found: it becomes the hunt's target, and the sprite of a hunt that fails at once. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../../combat/), [`math`](../../../math/), [`movement`](../../../movement/), [`objects`](../../../objects/), [`players`](../../../players/), [`unit_ai`](../../../unit_ai/), [`world`](../../../world/).
<!-- /uses -->
