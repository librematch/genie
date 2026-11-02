# `orders/gather`: the work command's Hunt

A work command on a hunted animal starts a Hunt. The work command's notify sets the unit AI's
hunt order, and the action list starts the Hunt.

- `hunt.rs` holds the Hunt's start.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`hunt.rs`](hunt.rs) | A work command's Hunt: its start from the factory and the Hunt action's own start. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`gathering`](../../gathering/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`players`](../../players/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
