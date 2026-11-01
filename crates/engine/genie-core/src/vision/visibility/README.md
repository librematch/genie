# `vision/visibility`: the visible maps

Each player has a visible map that counts, per tile, the player's objects that see the tile. A
tile the player never saw is -1, a tile it saw before is 0. The map's unified map holds, per tile,
one bit for each player that sees it and one for each player that saw it.

- `mod.rs` holds the maps and their state.
- `sight.rs` turns an object's sight on and off, and moves it with the object.
- `object_events.rs` holds the sight of a combat object's update, an ownership change and a
  construction.
- `explore.rs` explores and unexplores circles and rectangles of tiles.
- `building_links.rs` links a building's tiles to its anchor tile. A player that sees one tile
  of a building sees the building's tiles as explored.

## In the game

- `RGE_Visible_Map` and `RGE_Unified_Visible_Map` (`visible.cpp`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`building_links.rs`](building_links.rs) | A building's tile links (the map tile's `+0x19` bits) and its UMV extent entry. |
| [`explore.rs`](explore.rs) | `explore_terrain` and `unexplore_terrain` over a circle and a rectangle, and the reveal of a rectangle. |
| [`mod.rs`](mod.rs) | What each player sees: the players' `RGE_Visible_Map`s and the map's `RGE_Unified_Visible_Map` (both saved), which the objects' lines of sight keep. |
| [`object_events.rs`](object_events.rs) | The sight of a combat object's update and its registry, an ownership change and a construction's start and completion. |
| [`sight.rs`](sight.rs) | An object's sight on and off (slots 0x1d0 and 0x1d8), and after its move, its state's and its master's change. |
| [`tests.rs`](tests.rs) | Tests. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../map/), [`math`](../../math/), [`objects`](../../objects/), [`ownership`](../../ownership/), [`players`](../../players/), [`world`](../../world/).
<!-- /uses -->
