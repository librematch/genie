# `players/world_player/tech_tree`: the tech tree's data

The tech tree (`players/world_player/tech_tree.rs`) answers what a civ can build and research.
This folder holds its data: the tree's units, buildings and unique units and the queries over
them, which the game-data model takes along (FD-046).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`tech_tree_data.rs`](tech_tree_data.rs) | The tech tree's data: its units, buildings and unique units, and the queries over them (FD-046). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`world`](../../../world/).
<!-- /uses -->
