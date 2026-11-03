# `garrison`: units inside other objects

Units go into buildings and transports, and come out again. This folder holds both directions:

- `shelter.rs` holds the garrison order and `SEEK_SHELTER`: each unit goes to a building that
  takes it in, the nearest first.
- `takes_in.rs` tests whether a building takes a unit in, and how much room it has.
- `enter.rs`, `walk_in.rs` and `meeting.rs` hold the Enter action: the command, the walk to the
  building or transport, and the point where they meet.
- `unit.rs` is the unit's part: a unit entering and its update while it is inside another
  object.
- `unload.rs` and `let_out.rs` hold UNLOAD, `GO_BACK_TO_WORK` and a unit's leave.
- `firepower.rs` holds a unit's attack rate and firepower, and its share of a building's volleys.
- `transport/` holds the Transport action: a transport's sail and its unload ashore.

## In the game

- `RGE_Action_Enter` (`act_entr.cpp`) and `RGE_Action_Transport` (action type 12,
  `act_tran.cpp`).
- `UNIT_ORDER`'s garrison and the building's test whether it takes a unit in.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`class_flags.rs`](class_flags.rs) | A class's garrison flags (`ClassFlags`, the class-flags table's word). |
| [`container_id.rs`](container_id.rs) | A container's id as its own type: the object a unit is inside, enters or leaves. |
| [`damage_rate.rs`](damage_rate.rs) | The damage an object deals per second (`DamageRate`). |
| [`enter.rs`](enter.rs) | The enter command and the Enter's construction, install and start, at once or with the transport's move. |
| [`enter_state.rs`](enter_state.rs) | An Enter's state: walking to a building or transport, waiting, inside; each walk with its `MoveTo`. |
| [`firepower.rs`](firepower.rs) | A unit's attack rate and firepower, and its share of a building's volleys (`FUN_141213520`). |
| [`inside_count.rs`](inside_count.rs) | How many units an object holds inside (`InsideCount`). |
| [`let_out.rs`](let_out.rs) | A unit's leave of the building or the transport it is inside (`FUN_1411e7220`), or its place on the map. |
| [`meeting.rs`](meeting.rs) | Where an Enter meets its target (`FUN_140a85f30`): the landing tile, the walled-off test, the meeting point and the reach. |
| [`mod.rs`](mod.rs) | Units inside other objects: the garrison order, the Enter action, the unloads and the Transport action. |
| [`outcomes.rs`](outcomes.rs) | What a unit's way out of a building (`LetOut`), its way in (`EnterOutcome`) and its way back to work after the town bell (`WorkReturn`) answer. |
| [`raider_countdown.rs`](raider_countdown.rs) | A combat object's raider countdown. |
| [`scope.rs`](scope.rs) | Which shelters a garrison takes (`GarrisonScope`) and how far a unit goes to one (`ShelterReach`). |
| [`shelter.rs`](shelter.rs) | The garrison order and `SEEK_SHELTER`: each unit to a building that takes it in, nearest first (`Shelters`). |
| [`takes_in.rs`](takes_in.rs) | A building's test whether it takes a unit in (`FUN_14120eb10`), its room and what it holds. |
| [`tests.rs`](tests.rs) | Tests. |
| [`unit.rs`](unit.rs) | A unit entering a building or a transport, and its update inside. |
| [`unload.rs`](unload.rs) | UNLOAD and `GO_BACK_TO_WORK`: the units inside a building or a transport come out. |
| [`walk_in.rs`](walk_in.rs) | The Enter action (`EnterAction`, `RGE_Action_Enter`): its walk to its target, its wait and its update. |
| [`transport/`](transport/) | The Transport action (`RGE_Action_Transport`, type 12, vtable `0x142ed61e8`): a transport's sail to a point and its unload there. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`commands`](../commands/), [`construction`](../construction/), [`gathering`](../gathering/), [`groups`](../groups/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`pathing`](../pathing/), [`players`](../players/), [`production`](../production/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
- Used by: [`commands`](../commands/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`testing`](../testing/), [`unit_ai`](../unit_ai/).
<!-- /uses -->
