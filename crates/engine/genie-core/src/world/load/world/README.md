# `world/load/world`: the world from a save

A save holds the world as the game wrote it. Its load turns the world's and the players' fields
into genie's world: the players' technologies, wonder times, ghosts, AI settings and history,
the map, the sight, the unit groups, the market and the garrisons' firepower. A recording's
header then needs what the game's own load builds, which the save does not hold.

- `mod.rs` holds the world as the save holds it (`saved_world`).
- `options.rs` holds the game options; `garrisons.rs` the garrisons' firepower.
- `players.rs` holds a player's AI settings, history, AI module and technologies.
- `market.rs` holds the world's market.
- `loaded.rs` holds the world as the header's load leaves it (`loaded_world`): the
  objects-in-sight registry, the hunted animals' origins, the watched lists.

## In the game

- The playback's open loads the world (`load_world`), then each player's and each object's
  parts in the save's order.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`garrisons.rs`](garrisons.rs) | The garrisons' firepower after a load (`garrison_firepower`): each unit inside a building of its own in that building's list, its share in the building's volleys. |
| [`loaded.rs`](loaded.rs) | The world as a recording's header load leaves it (`loaded_world`): what the game's load builds that the save does not hold. |
| [`market.rs`](market.rs) | The world's market as the save holds it (`market`). |
| [`mod.rs`](mod.rs) | The world as a save holds it (`saved_world`): every player's objects in their saved order, its technologies, wonder time, ghosts, AI settings and history, the map, the groups and the garrisons' firepower; and as the game's load leaves it (`loaded_world`). |
| [`options.rs`](options.rs) | The game options the simulation reads (`settings`) from the header's `GameOptions`. |
| [`players.rs`](players.rs) | A player's saved parts the world's load reads: its AI settings, its history, its AI module and its technologies' state. |
| [`tests.rs`](tests.rs) | Tests. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../../combat/), [`garrison`](../../../garrison/), [`gathering`](../../../gathering/), [`map`](../../../map/), [`movement`](../../../movement/), [`objects`](../../../objects/), [`pathing`](../../../pathing/), [`players`](../../../players/), [`production`](../../../production/), [`technology`](../../../technology/), [`testing`](../../../testing/), [`unit_ai`](../../../unit_ai/), [`vision`](../../../vision/).
<!-- /uses -->
