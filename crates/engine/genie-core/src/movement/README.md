# `movement`: walking the way

A unit's movement has two steps in every update, and this folder holds both:

- The velocity step moves the unit along its forward vector by its speed. It runs first in the
  update, with the speed and the direction that the last update left.
- The motion step steers the unit along its path. A walking action calls it later in the same
  update. It turns the unit toward its waypoint and sets the acceleration for the next update.

`Mover` (`mod.rs`) holds a unit's movement state. The motion step has one file for each stage:

- `step.rs`: the loop that runs the stages, and the velocity step.
- `request.rs` and `request_plans.rs`: the path request to the planners (`request_plans.rs` the
  planners of a unit with a footprint, one after the other).
- `new_path.rs`: a new path level.
- `walk.rs`: the walk, with its steering and its speed.
- `waypoint.rs`: a reached waypoint and the next one.
- `path_end.rs`: the end of the path.

This folder also holds the actions that are walks:

- `MoveTo` (`move_to/`): a walk to an object or a point. Many actions use it as a sub-action;
  a unit's own `MoveTo` is in `move_to/action.rs`.
- Explore (`explore.rs`): the auto scout's walk to the next unexplored area.
- Fly (`fly.rs`): a bird's flights and rests.

`unit.rs` is the unit's part: the velocity step in the unit's update, and the update of its
own `MoveTo`, its Explore and its Fly.

`ai_moves.rs` holds the moves that a unit AI asks of its unit.

## How the tick reaches it

The object's update (`objects/unit/update.rs`) runs the velocity step of every moving object.
If the object's action walks, the action runs the motion step. The planners that it asks for a path are in
`pathing/`.

## In the game

- `RGE_Moving_Object` (`MovingObject.cpp`): the velocity step and the motion step.
- `motion.cpp`, `waypoint.cpp`: the path levels and the waypoints.
- `RGE_Action_Move_To` (`act_move.cpp`), `RGE_Action_Explore` (`act_expl.cpp`) and the Fly action
  (`act_bird.cpp`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`acceleration.rs`](acceleration.rs) | An acceleration in tiles per second squared. |
| [`ai_moves.rs`](ai_moves.rs) | A unit's moves its unit AI asks: to a point (slot 0x340, the head action's slot 0x40 may take the point) and to an object (slot 0x348), a move that gave up, and the standing sprite of a `MoveTo` that ends at its start. |
| [`explore.rs`](explore.rs) | The Explore action (`RGE_Action_Explore`, type 4, made by `FUN_140a87d30`) and the auto scout's search for a point to explore (`FUN_140c65490`, the unit AI's "unexploredArea"). |
| [`explore_state.rs`](explore_state.rs) | An Explore's state: waiting for a point or walking to it. |
| [`fly.rs`](fly.rs) | A bird's Fly action (type 10, vtable `0x142ed4c88`; update `0x140a84b40`, `set_state` `0x140a84a50`): the bird rests (state 2), picks a point to fly to from the Action stream (state 3) and flies there with a `MoveTo` sub-action (state 4) until its flight time runs out or the `MoveTo` ends; then it rests for its task's wait time, or picks the next flight at once. |
| [`fly_state.rs`](fly_state.rs) | A Fly's state: a bird resting, choosing its flight and flying. |
| [`long_walk.rs`](long_walk.rs) | A step's reach of its waypoint, the long pather's walker and the units whose tiles it lowers. |
| [`mod.rs`](mod.rs) | A unit's movement (`RGE_Moving_Object`, MovingObject.cpp in 158041): the velocity step that moves it along its forward vector, and the motion step that steers it along its path. |
| [`move_algorithm.rs`](move_algorithm.rs) | A moving master's move algorithm (`MoveAlgorithm`, `move_algorithm`). |
| [`new_path.rs`](new_path.rs) | A new level's path: no path, its link under the walked level, the short pather's ignore list and a blocked start. |
| [`path_counts.rs`](path_counts.rs) | A path level's link to its parent and the plans it may still try; a mover's wait delays and substitute count. |
| [`path_end.rs`](path_end.rs) | The reach test, a `MoveTo`'s path end that follows its target, the top path and the pauses. |
| [`path_flags.rs`](path_flags.rs) | A path's flags (`PathFlags`, `+0xc`) and a path level's (`LevelFlags`, `+0x3c`). |
| [`path_points.rs`](path_points.rs) | Path points, the gaps between footprints and the position setter's tests. |
| [`path_state.rs`](path_state.rs) | A path level's state: whether it asks for a path, walks it or is done with it. |
| [`request.rs`](request.rs) | The motion step's path request: the long pather's plan, the walled-off stretches and the goal's reach. |
| [`request_plans.rs`](request_plans.rs) | The path request's planners for a unit with a footprint: the tile pathfinder's first plan, the short pather's tail, the long pather for a far waypoint and the tile pathfinder again. |
| [`speed.rs`](speed.rs) | A speed on the map in tiles per second (`Speed`), with the game's `f32` operations. |
| [`step.rs`](step.rs) | The velocity step, a new object's setup, the position setter and the motion step's loop. |
| [`tests.rs`](tests.rs) | Tests. |
| [`turn_rate.rs`](turn_rate.rs) | How fast a unit or a group turns (`TurnRate`, radians per second), with the game's `f32` operations. |
| [`unit.rs`](unit.rs) | The movement actions in a unit's update: the velocity step, the unit's own `MoveTo`, its Explore and a bird's Fly. |
| [`walk.rs`](walk.rs) | The walk: its kept hits, the obstruction query's ends, the facets, the steering and the speed control. |
| [`waypoint.rs`](waypoint.rs) | A reached waypoint and what follows it: the next waypoint, a level's end and the moving state. |
| [`waypoint_index.rs`](waypoint_index.rs) | A waypoint's place in a level's path. |
| [`waypoint_level.rs`](waypoint_level.rs) | A path level's place in a unit's path data. |
| [`move_to/`](move_to/) | The `MoveTo` action (`RGE_Action_Move_To`, type 1, vtable `0x142ed6030`): a unit walks to a target object or a point. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`map`](../map/), [`math`](../math/), [`objects`](../objects/), [`ownership`](../ownership/), [`pathing`](../pathing/), [`players`](../players/), [`production`](../production/), [`unit_ai`](../unit_ai/), [`world`](../world/).
- Used by: [`combat`](../combat/), [`commands`](../commands/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`map`](../map/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`pathing`](../pathing/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`unit_ai`](../unit_ai/), [`world`](../world/).
<!-- /uses -->
