# `pathing/hull_path`: the old short pather

The game keeps an older short pather for units wider than half a tile. It also measures the
length of a unit's short path to an object.

The pather first tries the quick path: a straight walk, or one detour around a single obstruction.
Else it wraps the obstructions near the walk into convex hulls. Each hull grows by the unit's
radius and merges with every hull it overlaps. An A* search then runs over the hulls' points. A
start or a goal inside a hull walks out of it first. The path found is smoothed at the end.

Every line test is the obstruction manager's. Moving objects do not count, unless the caller
lists them.

## In the game

- `MShortPather` (the HD edition's name), with `FindShortPath`, `findQuickPath`, `buildHulls` and
  `pathWithinHull`.
- `MConvexHull`: one hull.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`area.rs`](area.rs) | The search's goal and area: the goal's query, a goal at the map's edge, an end partway, the area's growth and the entrances' join. |
| [`convex.rs`](convex.rs) | A convex hull (`Hull`, `MConvexHull`): the point test, the intersection, a box's overlap, the merge and the rays to a box or out of the hull. |
| [`find.rs`](find.rs) | `FindShortPath` (`FUN_140f85580`): the bounds, the query, the quick path and the area before its rounds, and the path at their end. |
| [`mod.rs`](mod.rs) | The obstruction manager's old short pather (`MShortPather`, the names of the `AoK HD` PDB; build 158041's `FindShortPath` is `FUN_140f85580`): a search around convex hulls of the obstructions near a straight walk: the short path of a unit wider than half a tile (`FUN_140f9e540`'s dispatch, `crate::movement::short_path_for`) and the length of a unit's short path to an object (slot 0x3e8). |
| [`node_lists.rs`](node_lists.rs) | The node lists: a node added, closed, costed and linked; the manager's line tests; the path built back from its end. |
| [`obstacles.rs`](obstacles.rs) | The hulls' building from the area's obstructions: the tiles' boxes, the runs the unit cannot cross, the cliffs; a point's hull and the area's bounds. |
| [`rounds.rs`](rounds.rs) | `FindShortPath`'s rounds (`FUN_140f85580`'s loop): the walks out of the start's hull and into the goal's, then the A* over the hulls' points, again after the area grew. |
| [`setup.rs`](setup.rs) | The pather's construction (`HullPather`) and `canQuickPath` (object slot 0x3e0). |
| [`tests.rs`](tests.rs) | Tests. |
| [`walk_in.rs`](walk_in.rs) | The walk into a hull (`pathWithinHull` into a hull, `FUN_140f88ee0`). |
| [`walks.rs`](walks.rs) | The quick path (`findQuickPath`) and the walks out of a hull and into one (`pathWithinHull`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`construction`](../../construction/), [`map`](../../map/), [`math`](../../math/), [`objects`](../../objects/), [`players`](../../players/), [`world`](../../world/).
<!-- /uses -->
