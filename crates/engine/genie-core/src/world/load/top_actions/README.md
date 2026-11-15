# `world/load/top_actions`: an object's action list

A saved object's action list as the world models it: each action in the list's order, the head
first, with the sub-action it carries.

- `mod.rs` holds the list's load, each action's by its kind, and a saved `MoveTo`'s wait.
- `gathering.rs` holds the gathering actions: the Hunt, the Gather and a farm's Make.
- `packing.rs` holds the siege's own actions: the Pack and the Unpack.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`gathering.rs`](gathering.rs) | The gathering actions of a saved object's top action: the Hunt, the Gather and a farm's Make (`crate::world::load::top_actions::saved_actions`). |
| [`mod.rs`](mod.rs) | A saved object's action list as the slice models it (`saved_actions`), each action by its kind. |
| [`packing.rs`](packing.rs) | The siege's own actions of a saved object's top action: the Pack and the Unpack (`crate::world::load::top_actions::saved_actions`). |
| [`tests.rs`](tests.rs) | Tests: Each saved action the slice imports, a list of several in its order, and the saved actions it refuses. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../../combat/), [`commands`](../../../commands/), [`construction`](../../../construction/), [`garrison`](../../../garrison/), [`gathering`](../../../gathering/), [`map`](../../../map/), [`movement`](../../../movement/), [`objects`](../../../objects/), [`ownership`](../../../ownership/), [`players`](../../../players/).
<!-- /uses -->
