# `world/load/groups`: the unit groups from a save

The world's unit groups as the save holds them (FD-036): each group's motion, formation,
members and orders, from genie-save's generated reader of the build. The mapping is the only
place that knows the build's save layout of a group.

- `mod.rs` holds the groups' load (`unit_groups`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`mod.rs`](mod.rs) | The world's unit groups from a save (FD-036): genie-save's generated reader of the build into the `crate::groups` state. |
| [`tests.rs`](tests.rs) | Tests. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`groups`](../../../groups/), [`map`](../../../map/), [`math`](../../../math/), [`movement`](../../../movement/), [`objects`](../../../objects/), [`pathing`](../../../pathing/), [`players`](../../../players/), [`unit_ai`](../../../unit_ai/).
<!-- /uses -->
