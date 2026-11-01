# `production`: training units and researching technologies

A building's production queue holds the units and technologies that the player ordered there.
The building works off the first entry with an action of its production list: the make action
trains a unit, the research action researches a technology.

- `mod.rs` holds the queue, the make and research actions' state, the costs, the refunds and the
  population.
- `spawn/` holds the trained unit's spawn: where it appears beside the building and what it does
  first.

The `QUEUE` and `RESEARCH` commands are in `commands/`. The cancels are in
`players/game_status/`.

## How the tick reaches it

A building's update runs its production action after its combat update.

## In the game

- `TRIBE_Action_Make_Obj` and `TRIBE_Action_Make_Tech` (`tact_mob.cpp`).
- The queue of `TRIBE_Building_Object` (`t_b_obj.cpp`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`button.rs`](button.rs) | A train or research button: as a QUEUE command and a queue entry hold it (`QueueButton`), and as a train or research location holds it (`LocationButton`). |
| [`costs.rs`](costs.rs) | A queue entry's costs: the test and the payment of a unit's or a technology's costs, and their refunds when an entry is cancelled (`FUN_141630530` and the cancels' calls). |
| [`hotkey.rs`](hotkey.rs) | A train location's hotkey. |
| [`make_obj_state.rs`](make_obj_state.rs) | A unit's make action's state: making the unit its building trains. |
| [`make_tech_state.rs`](make_tech_state.rs) | A research's state: researching a technology at its building. |
| [`mod.rs`](mod.rs) | Training units and researching technologies: a building's production queue, the actions that work it off, and a trained unit's spawn. |
| [`spawn/`](spawn/) | A trained unit's spawn: where the make action (`TRIBE_Action_Make_Obj`, `FUN_140ad0240`) puts the unit its building made. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`commands`](../commands/), [`garrison`](../garrison/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`players`](../players/), [`technology`](../technology/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
- Used by: [`combat`](../combat/), [`commands`](../commands/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`movement`](../movement/), [`objects`](../objects/), [`players`](../players/), [`technology`](../technology/).
<!-- /uses -->
