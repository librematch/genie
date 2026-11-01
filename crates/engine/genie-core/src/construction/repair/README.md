# `construction/repair`: the Repair action

A repairer walks to its target within its weapon range (state 4), faces it (state 6) and repairs
it in every update (state 7). When the target has its full hit points, or the player cannot pay for more, it stops. A target out of reach makes it wait (state 10) or fail (state 13).

The work of one update is the unit's work rate times the task's work value times the update's
seconds. Each repairer after the first adds half of that. The repair pays a share of the
target's cost.

- `mod.rs` holds the action's state and its view of the world.
- `action.rs` holds the action's start, update and states.
- `payment.rs` holds what a repair heals and what it costs.
- `unit.rs` is the repairer's part of its update.

## In the game

- `TRIBE_Action_Repair` (action type 106, `tact_rep.cpp`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`action.rs`](action.rs) | The Repair action's start, update and states (`RepairAction`): the walk, the face, the wait, the work, a new target, a move and the halt. |
| [`mod.rs`](mod.rs) | A repairer's Repair action (`TRIBE_Action_Repair`, type 106, vtable `0x142ed7b88`): the action walks to its target (state 4, a `MoveTo` sub-action within the unit's weapon range), faces it (state 6) and repairs it each update (state 7) until the target holds its most hit points or the player cannot pay for more; a target out of reach makes it wait (state 10) or fail (state 13). |
| [`payment.rs`](payment.rs) | What a repair reaches and pays: a building's and a combat unit's slot 0x8c8 by the repairer's player. |
| [`repair_state.rs`](repair_state.rs) | A Repair's state: the steps of its update from the walk to the work. |
| [`tests.rs`](tests.rs) | Tests. |
| [`unit.rs`](unit.rs) | The repair part of the update and the unit's attackers and seen target. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`ownership`](../../ownership/), [`pathing`](../../pathing/), [`players`](../../players/), [`production`](../../production/), [`technology`](../../technology/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
