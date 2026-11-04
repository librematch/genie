# `pathing/short_path`: the short pather

The short pather plans the stretch that a unit walks next. It is an any-angle search around the
shapes of the obstruction mesh (`pathing/mesh.rs`).

All coordinates are fixed point, with 2^22 units per tile. The mesh stores each shape grown by
half a tile. A unit of radius `r` tests each shape shrunk by half a tile less `r`.

The search walks the outlines of the polygons that the shapes form. Raycasts over the mesh's
tiles test which points see each other. The files follow the search:

- `call.rs`: the call and the pather's state between searches.
- `start.rs`: the start node.
- `main_loop.rs`: the main loop and a node's expansion.
- `nodes.rs`: the open list and the node tests.
- `outline.rs`: the polygons' outlines.
- `walk.rs`: a new polygon's walk along its outline.
- `raycast.rs`: the raycasts and the sight test.
- `geometry.rs`: the fixed-point geometry.

## In the game

- The short pather of `mpathsys.cpp`, which the motion step's path request calls.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`axis_dir.rs`](axis_dir.rs) | The direction of an axis walk of the short pather. |
| [`call.rs`](call.rs) | The short pather's call (`Call`, `Query`) and its state between searches (`Pather`): the terrain's shapes, the start test and the core. |
| [`geometry.rs`](geometry.rs) | The fixed-point geometry: a shrunk rectangle's corners and sides, a contact's point, the segment and ray hits. |
| [`main_loop.rs`](main_loop.rs) | The search's run: its main loop and a popped node's expansion by its type. |
| [`mod.rs`](mod.rs) | The short pather (`FUN_140f9e540` -> `FUN_140fa53d0`): a fixed-point any-angle search around the shapes of the obstruction manager's `Mesh`, for the path a unit walks next. |
| [`nodes.rs`](nodes.rs) | The search's nodes: the open list, the goal and foot tests, a polygon corner's test and the clip to the box. |
| [`outline.rs`](outline.rs) | The polygons' outlines: the tracer, the border walk, the edge that holds a point and the walk of a new polygon. |
| [`raycast.rs`](raycast.rs) | The searches' walks over the mesh's tiles (`Ctx`): a point's holder, the raycast, the axis sweeps and the sight test. |
| [`start.rs`](start.rs) | The start's node, the hits' points and the two legs of an L. |
| [`tests.rs`](tests.rs) | Tests. |
| [`token.rs`](token.rs) | Callback A's token: what a ray from a polygon corner ignores. |
| [`walk.rs`](walk.rs) | A new polygon's walk (`FUN_140fa7b50`): from a point on its closing edge along the outline both ways at once, its corners nodes toward the goal. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../map/), [`math`](../../math/), [`objects`](../../objects/), [`world`](../../world/).
<!-- /uses -->
