# `garrison/transport`: the Transport action

A transport sails to a point and unloads its units there. Near enough, it unloads at once
(state 6). Else it sails toward the point with a quiet `MoveTo` (state 4) and unloads where the
walk ends. The unload puts the units ashore where a drop location is free. When no unit can go
ashore, the transport sails to a random point near the old one and tries again.

- `mod.rs` holds the action's state and its view of the ship and the shore.
- `action.rs` holds the action's slots: its start, state, move, stop, work and update.
- `shore.rs` holds the points ashore and the drop location.
- `unload.rs` holds the transport in the world: the unload command and the units put ashore.
- `unit.rs` holds the unit's part: the action's update on the step and a sail to a point.

## In the game

- `RGE_Action_Transport` (action type 12, `act_tran.cpp`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`action.rs`](action.rs) | The Transport action's slots (`TransportAction`): its start, state, move, stop, work and update, its walk and its test beside the shore. |
| [`mod.rs`](mod.rs) | The Transport action (`RGE_Action_Transport`, type 12, vtable `0x142ed61e8`): a transport's sail to a point and its unload there. |
| [`shore.rs`](shore.rs) | The points ashore: a new point after none went ashore, the drop location (`find_drop_location`) and the map as the Transport reads it (`MapShore`). |
| [`tests.rs`](tests.rs) | Tests. |
| [`transport_mode.rs`](transport_mode.rs) | How a Transport was last ordered (`TransportMode`). |
| [`transport_state.rs`](transport_state.rs) | A Transport's state: the steps of its update from the sail to the unload; each sail with its `MoveTo`. |
| [`unit.rs`](unit.rs) | The unit's Transport: the action's update on the step and a sail to a point. |
| [`unload.rs`](unload.rs) | The Transport in the world: what it reads of its unit, the unload command, a move, the unload and the units put ashore. |
| [`work.rs`](work.rs) | The work command's Transport: a transport's task of type 12 (a beach, the only terrain task in the game's data) makes or retakes it. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`pathing`](../../pathing/), [`players`](../../players/), [`production`](../../production/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
