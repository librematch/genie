# `world/load`: the world's load from a save

A recording's header and every per-tick save hold the world as the game wrote it. The load
turns genie-save's records of that save into genie's world: the world's and the players'
fields, the map, the sight and the groups, and each object's row and parts, its movement, its
actions and sub-actions. The values the save does not hold come from the `.dat`'s
masters (genie-import's tables).

- `mod.rs` holds the load's shared readings, the ids of a saved list.
- `world/` holds the world and its players, and the world as the header's load leaves it.
- `map/` holds the map's terrain, the objects' places and the players' sight.
- `groups/` holds the unit groups.
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
| [`mod.rs`](mod.rs) | The world's load from a save (FD-043-01M6JPBEDA, the review's ORA-01): the world, its players, map, sight and groups, and each saved object's row, its parts, its movement, its actions and sub-actions, as genie-save reads them from a recording's header or a per-tick save. |
| [`groups/`](groups/) | The world's unit groups from a save (FD-036): genie-save's generated reader of the build into the `crate::groups` state. |
| [`map/`](map/) | The map and the players' sight from a save: the terrain, the objects as movement sees them, the visible maps and resources. |
| [`movers/`](movers/) | A saved moving object's movement: its masters' animated and moving parts, its path levels, its movement state and its `MoveTo`'s point. |
| [`object/`](object/) | A saved object as the slice's object (`object`: its row, `unit`, and its parts): its town bell, a building's gate and its construction. |
| [`object_parts/`](object_parts/) | A saved object's lifecycle fields and its unit AI. |
| [`sub_actions/`](sub_actions/) | A saved unit's reload, sprite and Attack, its Build and Fly tasks, a building's production and gather point, and the default task an object's first update starts. |
| [`top_actions/`](top_actions/) | A saved object's top action as the slice models it (`saved_actions`). |
| [`world/`](world/) | The world as a save holds it (`saved_world`): every player's objects in their saved order, its technologies, wonder time, ghosts, AI settings and history, the map, the groups and the garrisons' firepower; and as the game's load leaves it (`loaded_world`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`commands`](../../commands/), [`construction`](../../construction/), [`garrison`](../../garrison/), [`gathering`](../../gathering/), [`groups`](../../groups/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`orders`](../../orders/), [`ownership`](../../ownership/), [`pathing`](../../pathing/), [`players`](../../players/), [`production`](../../production/), [`technology`](../../technology/), [`testing`](../../testing/), [`unit_ai`](../../unit_ai/), [`vision`](../../vision/).
<!-- /uses -->
