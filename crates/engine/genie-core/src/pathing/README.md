# `pathing`: finding a way

A unit that walks needs a path. The game plans it on several scales, and this folder holds all
of the planners:

- The tile pathfinder (`tile_path/`) plans the long way over the map's tiles. It searches a grid
  of four levels, from 8 × 8 tiles down to one tile.
- The long pather (`long_path/`) is a best-first search over the map's tiles, from a unit's tile
  to a goal tile. The motion step uses it for a path level, and the reachability test of a point
  runs it too.
- The short pather (`short_path/`) plans the next stretch of the walk. It searches around the
  shapes of the obstruction mesh in fixed point, at any angle.
- The old hull pather (`hull_path/`) plans the short path of a unit wider than half a tile. It
  searches around convex hulls of the obstructions near the walk.

`mesh.rs` holds the obstruction mesh's rectangles. `zones.rs` numbers the patches of connected
tiles for each terrain restriction.

`Pathing` (`mod.rs`) holds the planning state that is not in a save. A world started from a save
builds it from the map, as the game does at load.

## How the tick reaches it

If the path levels of a walking unit need a path, its motion step (`movement/`) asks for one.
The unit then walks the path's waypoints. A unit group's motion (`groups/motion/`) and its
members' steering (`groups/steer/`) ask the same planners.

## In the game

- `path.cpp`, `pathsys.cpp`, `mpathsys.cpp`: the planners and the pathers.
- `MShortPather`: the old short pather (the HD edition's name).
- `RGE_Zone_Map` (`zone_map.cpp`): the zones.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`fixed_distance.rs`](fixed_distance.rs) | A length in the pathing's fixed point (`FixedDistance`) and a squared one (`FixedDistanceSquared`). |
| [`fixed_point.rs`](fixed_point.rs) | A point in the pathing's fixed point (`FixedPoint`). |
| [`fixed_rect.rs`](fixed_rect.rs) | A rectangle in the pathing's fixed point (`FixedRect`). |
| [`mesh.rs`](mesh.rs) | The obstruction manager's shape mesh (`*(*(DAT_143f0a088 + 8))`): the rectangles the short pather searches around, in fixed point (2^22 units per tile). |
| [`mod.rs`](mod.rs) | Finding a way: the planners of the paths that units walk, from the tile pathfinder over the whole map to the short pathers around the obstructions near a unit. |
| [`plan_budget_values.rs`](plan_budget_values.rs) | A player's path budget's values: its caps, its plans and a walker's waits. |
| [`shape_id.rs`](shape_id.rs) | A shape's id in the obstruction manager's mesh. |
| [`shape_side.rs`](shape_side.rs) | A side of a shape in the obstruction manager's mesh. |
| [`short_found.rs`](short_found.rs) | What a short path search returns (`ShortFound`). |
| [`zones.rs`](zones.rs) | The map's zones (`RGE_Zone_Map`, made by `FUN_141410680`, labelled by `FUN_141413190`): every tile gets the number of the patch of tiles it belongs to, a patch being the tiles a 4-way flood reaches over tiles that are, like its first, passable or not for a terrain restriction (its value for the tile's terrain, map tile `+0x9`, above 0). |
| [`hull_path/`](hull_path/) | The obstruction manager's old short pather (`MShortPather`, the names of the `AoK HD` PDB; build 158041's `FindShortPath` is `FUN_140f85580`): a search around convex hulls of the obstructions near a straight walk: the short path of a unit wider than half a tile (`FUN_140f9e540`'s dispatch, `crate::movement::short_path_for`) and the length of a unit's short path to an object (slot 0x3e8). |
| [`long_path/`](long_path/) | The long pather (`FUN_140f9eb70`, run on the global at `0x1439e6910`): a best-first search over the map's tiles from a unit's tile to a goal tile, which the reachability test of a point (`FUN_140fa0370`, `crate::movement::Mover::reaches`) runs for a goal more than a tile from the unit. |
| [`short_path/`](short_path/) | The short pather (`FUN_140f9e540` -> `FUN_140fa53d0`): a fixed-point any-angle search around the shapes of the obstruction manager's `Mesh`, for the path a unit walks next. |
| [`tile_path/`](tile_path/) | The tile pathfinder: a planner per terrain restriction in use (the world's list at `world + 0x140`, made by `FUN_140f7d310`), which searches a four-level grid of the map's tiles for the path a unit's top path level asks for (`FUN_140f7c530`, called from the motion step's state-2 branch). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`construction`](../construction/), [`map`](../map/), [`math`](../math/), [`objects`](../objects/), [`players`](../players/), [`testing`](../testing/), [`world`](../world/).
- Used by: [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`map`](../map/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`unit_ai`](../unit_ai/), [`world`](../world/).
<!-- /uses -->
