# `world/load`: the world's load from a save

A recording's header and every per-tick save hold the world as the game wrote it. The load
turns genie-save's records of that save into genie's world: each object's row and parts, its
movement, its actions and sub-actions. The values the save does not hold come from the `.dat`'s
masters (genie-import's tables).

- `mod.rs` holds the load's shared readings, the ids of a saved list.
- `object/` holds a saved object as a row of the world and its parts.
- `object_parts/` holds an object's lifecycle and its unit AI.
- `movers/` holds a moving object's movement and its path levels.
- `top_actions/` holds an object's top action.
- `sub_actions/` holds a unit's sub-actions, its tasks and a building's production.
- `fixtures.rs` holds the tests' save records.

## In the game

- The playback's open loads the world (`load_world`, slot 7 of the world): each object's
  `load` reads its class's fields in order (FD-002's spine).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`fixtures.rs`](fixtures.rs) | Save parts for the load's tests, built from the generated layouts' defaults: genie holds no game saves (FD-044), so a test builds the few fields its routine reads and leaves the rest zero. |
| [`mod.rs`](mod.rs) | The world's load from a save (FD-043-01M6JPBEDA, the review's ORA-01): each saved object's row, its parts, its movement, its actions and sub-actions, as genie-save reads them from a recording's header or a per-tick save. |
| [`movers/`](movers/) | A saved moving object's movement: its masters' animated and moving parts, its path levels, its movement state and its `MoveTo`'s point. |
| [`object/`](object/) | A saved object as the slice's object (`object`: its row, `unit`, and its parts): its town bell, a building's gate and its construction. |
| [`object_parts/`](object_parts/) | A saved object's lifecycle fields and its unit AI. |
| [`sub_actions/`](sub_actions/) | A saved unit's reload, sprite and Attack, its Build and Fly tasks, a building's production and gather point, and the default task an object's first update starts. |
| [`top_actions/`](top_actions/) | A saved object's top action as the slice models it (`saved_actions`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`commands`](../../commands/), [`construction`](../../construction/), [`garrison`](../../garrison/), [`gathering`](../../gathering/), [`groups`](../../groups/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`orders`](../../orders/), [`ownership`](../../ownership/), [`pathing`](../../pathing/), [`players`](../../players/), [`production`](../../production/), [`technology`](../../technology/), [`testing`](../../testing/), [`unit_ai`](../../unit_ai/).
<!-- /uses -->
