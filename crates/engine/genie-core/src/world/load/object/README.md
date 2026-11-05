# `world/load/object`: a saved object

A saved object becomes a row of its player's list and the parts the world files beside it:
its town bell, a building's gate and its construction.

- `mod.rs` holds the object's row and its parts.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`building.rs`](building.rs) | A saved building's parts: its lists of objects, its annex slots, its gate and its construction. |
| [`mod.rs`](mod.rs) | A saved object as the slice's object (`object`: its row, `unit`, and its parts): its town bell, a building's gate and its construction. |
| [`tests.rs`](tests.rs) | Tests. |
| [`town_bell.rs`](town_bell.rs) | A saved combat object's town bell (`town_bell`) and the parts that hold its unit AI's order and action values (`ai_parts`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../../combat/), [`construction`](../../../construction/), [`garrison`](../../../garrison/), [`gathering`](../../../gathering/), [`groups`](../../../groups/), [`map`](../../../map/), [`movement`](../../../movement/), [`objects`](../../../objects/), [`orders`](../../../orders/), [`ownership`](../../../ownership/), [`players`](../../../players/), [`unit_ai`](../../../unit_ai/).
<!-- /uses -->
