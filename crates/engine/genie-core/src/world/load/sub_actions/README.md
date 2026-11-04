# `world/load/sub_actions`: a unit's sub-actions and tasks

A saved unit's reload, sprite and Attack, its Build and Fly tasks, a building's production and
gather point, and the default task an object's first update starts.

- `mod.rs` holds the sub-actions' and the tasks' load.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`mod.rs`](mod.rs) | A saved unit's reload, sprite and Attack, its Build and Fly tasks, a building's production and gather point, and the default task an object's first update starts. |
| [`tests.rs`](tests.rs) | Tests. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../../combat/), [`commands`](../../../commands/), [`construction`](../../../construction/), [`gathering`](../../../gathering/), [`map`](../../../map/), [`movement`](../../../movement/), [`objects`](../../../objects/), [`ownership`](../../../ownership/), [`players`](../../../players/), [`production`](../../../production/), [`technology`](../../../technology/).
<!-- /uses -->
