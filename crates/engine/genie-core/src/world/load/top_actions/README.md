# `world/load/top_actions`: an object's top action

A saved object's top action as the world models it, with the sub-action it carries.

- `mod.rs` holds the top action's load and a saved `MoveTo`'s wait.
- `gathering.rs` holds the gathering actions: the Hunt, the Gather and a farm's Make.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`gathering.rs`](gathering.rs) | The gathering actions of a saved object's top action: the Hunt, the Gather and a farm's Make (`crate::world::load::top_actions::saved_actions`). |
| [`mod.rs`](mod.rs) | A saved object's top action as the slice models it (`saved_actions`). |
| [`tests.rs`](tests.rs) | Tests: Each saved top action the slice imports, and the saved actions it refuses. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../../combat/), [`construction`](../../../construction/), [`garrison`](../../../garrison/), [`gathering`](../../../gathering/), [`map`](../../../map/), [`movement`](../../../movement/), [`objects`](../../../objects/), [`ownership`](../../../ownership/), [`players`](../../../players/).
<!-- /uses -->
