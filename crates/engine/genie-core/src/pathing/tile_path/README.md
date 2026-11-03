# `pathing/tile_path`: the tile pathfinder

The tile pathfinder plans the long way of a unit over the map's tiles. The world holds one planner
for each terrain restriction in use, because a boat and a villager can cross different tiles.

Each planner keeps a grid of four levels. Level 0 has one cell per tile. Each coarser level has
one cell per 2 × 2 cells of the level below. A search runs A* over the coarsest cells that are
uniform, and goes down a level where a cell is mixed. Then it traces the path back over the tiles
and smooths it into straight stretches.

The planner is not in a save. The world builds it from the map's terrain, the restriction's
passability and the blocked tiles. A change to a building's tiles rebuilds the cells over them.

## In the game

- The planners of `path.cpp` and `pathsys.cpp`, built at load, one per terrain restriction.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`cell.rs`](cell.rs) | A planner grid's cell (`Cell`): its edges with the search's marks (`CellEdges`) and its kind (`CellKind`), the grid's two bytes; the grid (`Cells`) by a cell's place (`CellIndex`). |
| [`goal_marks.rs`](goal_marks.rs) | A search's request and goal: its ranges and the marks of the cells and tiles within them. |
| [`grid.rs`](grid.rs) | A terrain restriction's planner (`Planner`) and its four-level grid: built, read and rebuilt over a changed rectangle. |
| [`grid_level.rs`](grid_level.rs) | A level of a planner's grid (`GridLevel`). |
| [`mod.rs`](mod.rs) | The tile pathfinder: a planner per terrain restriction in use (the world's list at `world + 0x140`, made by `FUN_140f7d310`), which searches a four-level grid of the map's tiles for the path a unit's top path level asks for (`FUN_140f7c530`, called from the motion step's state-2 branch). |
| [`path_trace.rs`](path_trace.rs) | The path's points: the tiles a node's path crosses into and aims at, traced back from the goal. |
| [`plan_tile.rs`](plan_tile.rs) | The planner's places as its shorts: a tile or a cell of a level (`PlanTile`) and a rectangle of tiles (`PlanRect`). |
| [`planning.rs`](planning.rs) | The planner's entry (`FUN_140f7c530`): the search over the levels, the shortcut near the start and the search on the tiles. |
| [`search.rs`](search.rs) | The A* searches over the node cells and over the tiles: the nodes, the open list, the estimates and the expansions. |
| [`smoothing.rs`](smoothing.rs) | The smoothing of a found path (`FUN_140f7dbb0`): its straight stretches wherever the unit can walk straight. |
| [`tests.rs`](tests.rs) | Tests. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../map/), [`math`](../../math/), [`world`](../../world/).
<!-- /uses -->
