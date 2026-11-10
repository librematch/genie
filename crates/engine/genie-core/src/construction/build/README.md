# `construction/build`: the Build action

A builder walks to its foundation with a `MoveTo` sub-action (state 4). At the foundation it faces
it and joins the work (state 7). A builder that stands on the footprint of a foundation that no
one started walks off it first (state 223) and comes back. Each update of work adds to the
foundation's construction. The first work starts the construction, and the work that reaches the
build time completes the building.

- `mod.rs` holds the action's state, the construction's state and their constants.
- `update.rs` starts the action and runs its state in each update.
- `states.rs` holds the states that the update and the unit's stop set.
- `walk_ends.rs` holds the ends of the walk: arrived, failed, or the foundation lost.
- `placing.rs` holds the placement on a foundation that no one started, and the walk off the
  footprint.
- `construction.rs` holds a foundation's construction: its work, its start and its completion.
- `clearing.rs` removes the objects on a new building's footprint.
- `drop_off.rs` sends a builder that carries resources to a drop site first.
- `more_work.rs` finds a resource near the finished building for the builder.
- `unit.rs` is the builder's part of its update: what the Build reads of the world, the
  action's update, and what its step changes.
- `world_side.rs` holds the construction's parts that run on the whole world.
- `unit_ai_side.rs` holds what the unit AI asks of a build.

## In the game

- `TRIBE_Action_Build` (action type 101, `tact_bld.cpp`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`build_state.rs`](build_state.rs) | A Build's state: the steps of its update from the walk to the finished building, each walk with its `MoveTo`. |
| [`clearing.rs`](clearing.rs) | The footprint clearing (`FUN_1415c3620`) of a building: the objects on its tiles that go. |
| [`completion.rs`](completion.rs) | A foundation's completion on the world (`FUN_1411dbb70` from state 0 to 2): each of its parts is the call the game makes, where the game makes it. |
| [`construction.rs`](construction.rs) | A foundation's construction (`Construction`): its start and its work's share of hit points, with the completion's storage, ground and counts (`crate::world::World::complete_building`). |
| [`drop_off.rs`](drop_off.rs) | State 8: a builder that carries something walks to a drop site first (`DropSearch`). |
| [`mod.rs`](mod.rs) | A builder's Build action (`TRIBE_Action_Build`, type 101, vtable `0x142ed6628`) up to the builder's walk: the action starts in state 4 with a `MoveTo` sub-action (`RGE_Action_Move_To`, type 1) toward the foundation; a builder already at the foundation works in its first update (state 6). |
| [`more_work.rs`](more_work.rs) | A builder's search for more work once its Build ended without queued orders (the Build's case 0xdf): a resource around the building it finished, as the unit AI's `ClosestResourceObject` names it. |
| [`placing.rs`](placing.rs) | The placement on a foundation no builder started (state 7), a builder's joining and its work, the walk off the footprint (state 223) and back. |
| [`states.rs`](states.rs) | The states the update and the unit's stop set: the prelude, the work's end and what follows it, the restart, the idle, a retarget, the halt and the end; the builder count and the load the work's end drops. |
| [`step_tests.rs`](step_tests.rs) | Tests: The Build's update on the step (`BuildAction::update` through `Unit::build_part`), on World fixtures: the state of the builder, its foundation and the world after each update. |
| [`tests.rs`](tests.rs) | Tests. |
| [`unit.rs`](unit.rs) | The builder's part of its update: what the Build action reads as it starts (its foundation, the builder and its unit AI's orders), the action's update on the Build copied out of the action list, and the work, which runs on the world with the builder lent: the foundation's work, its starts and its completion. |
| [`unit_ai_side.rs`](unit_ai_side.rs) | What the unit AI asks of a build while it runs (through its view, `UnitView`): whether the build command takes a foundation, the builder's search for another foundation, and a unit's spot off a foundation's footprint. |
| [`update.rs`](update.rs) | The Build action's start (state 4 toward the foundation) and its update, which runs the action's state; the AI's build orders whose foundation is gone. |
| [`walk_ends.rs`](walk_ends.rs) | The ends of the builder's walk: at its end, failed, its foundation lost; the walk to a point (state 11). |
| [`walks.rs`](walks.rs) | The Build's `MoveTo`s (`0x140aae200` cases 4, 8, 11 and 223) through `MoveToReads`: the one to its foundation, and the sub-action list's update. |
| [`world_side.rs`](world_side.rs) | The construction's parts that run on the whole world (`crate::world::World`), outside a builder's update: a building's work and completion, its storage's leave, the built object's stacking and its new objects. |
| [`more_work/`](more_work/) | The search for more work's tests. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`commands`](../../commands/), [`gathering`](../../gathering/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`orders`](../../orders/), [`pathing`](../../pathing/), [`players`](../../players/), [`production`](../../production/), [`testing`](../../testing/), [`unit_ai`](../../unit_ai/), [`vision`](../../vision/), [`world`](../../world/).
<!-- /uses -->
