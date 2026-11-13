# `garrison`: units inside other objects

Units go into buildings and transports, and come out again. This folder holds both directions:

- `shelter.rs` holds the garrison order and `SEEK_SHELTER`: each unit goes to a building that
  takes it in, the nearest first.
- `takes_in.rs` tests whether a building takes a unit in, and how much room it has.
- `enter.rs`, `walk_in.rs` and `meeting.rs` hold the Enter action: the command, the walk to the
  building or transport, and the point where they meet. `walk_point.rs` holds its walks to a
  point: to a target without room (state 11), and without a target. `restart.rs` starts it
  again when the action list's deletion leaves it at the head.
- `unit.rs` is the unit's part: a unit entering and its update while it is inside another
  object. `holder.rs` is the container's part of an enter and a leave: its list, its volleys
  and, in a building, its counts of the villagers and raiders inside (`counts.rs`) and of the
  relics and livestock (`relics.rs`: a relic also counts in its player's relics held, and its
  leave takes back the technologies paid in more relics than are left).
- `nested.rs` holds what a unit with units inside brings in and takes out: a container of type
  70 or more counts the units its units hold (`+0x9a8`), and its slot 0x498 (`garrisoned` in
  `takes_in.rs`) adds them to the units inside.
- `unload.rs` and `let_out.rs` hold UNLOAD, `GO_BACK_TO_WORK` and a unit's leave.
- `firepower.rs` holds a unit's attack rate and firepower, and its share of a building's volleys.
- `heal.rs` holds what a building's whole seconds do for its units inside: each heals, whoever's
  it is, and the villagers inside repair the building at its owner's cost.
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
| [`counts.rs`](counts.rs) | A building's counts of the units inside it (`GarrisonCounts`). |
| [`counts_tests.rs`](counts_tests.rs) | Tests: A building's counts of the units inside it, its villagers (`+0xba8`) and its raiders (`+0xc2c`): as units enter and leave, as the load links them in again, and in its slot 0x498. |
| [`damage_rate.rs`](damage_rate.rs) | The damage an object deals per second (`DamageRate`). |
| [`enter.rs`](enter.rs) | The enter command and the Enter's construction, install and start, at once or with the transport's move. |
| [`enter_state.rs`](enter_state.rs) | An Enter's state: walking to a building or transport, waiting, inside; each walk with its `MoveTo`. |
| [`firepower.rs`](firepower.rs) | A unit's attack rate and firepower, and its share of a building's volleys (`FUN_141213520`). |
| [`heal.rs`](heal.rs) | A building's heal of its units inside and its repair by the villagers inside: the end of `TRIBE_Building_Object::update`'s whole seconds (`0x1411c9000`). |
| [`heal_tests.rs`](heal_tests.rs) | Tests: A building's whole seconds with units inside (`TRIBE_Building_Object::update`, `0x1411c9000`): the heal of every unit inside by its own master and type, the armor a full heal gives back, the repair by the villagers inside, and a foundation completed in its update, which runs none. |
| [`holder.rs`](holder.rs) | A container's own part of a unit's enter and leave (its slots 0x1c0 and 0x1c8): its list, its volleys and a building's counts. |
| [`inside_count.rs`](inside_count.rs) | How many units an object holds inside (`InsideCount`). |
| [`let_out.rs`](let_out.rs) | A unit's leave of the building or the transport it is inside (`FUN_1411e7220`), or its place on the map. |
| [`meeting.rs`](meeting.rs) | Where an Enter meets its target (`FUN_140a85f30`): the landing tile, the walled-off test, the meeting point and the reach. |
| [`mod.rs`](mod.rs) | Units inside other objects: the garrison order, the Enter action, the unloads and the Transport action. |
| [`nested.rs`](nested.rs) | The units inside the units inside a container (`+0x9a8`, `NestedCount`): what a unit holds comes in with it (slot 0x1a0) and goes out with it (slot 0x1a8). |
| [`nested_tests.rs`](nested_tests.rs) | Tests: What the units inside a unit come in and go out with (`+0x9a8`, `NestedCount`): the container's count as a unit that holds others enters (slot 0x1a0, `FUN_141212e10`) and leaves (slot 0x1a8, `FUN_141213120`), and slot 0x498 that adds it (`FUN_141213500`). |
| [`outcomes.rs`](outcomes.rs) | What a unit's way out of a building (`LetOut`), its way in (`EnterOutcome`) and its way back to work after the town bell (`WorkReturn`) answer. |
| [`raider_countdown.rs`](raider_countdown.rs) | A combat object's raider countdown. |
| [`relics.rs`](relics.rs) | A building's relics and livestock inside it (`FUN_1411cb850`, `FUN_1411d6ad0`): their counts, the relics its player holds and the researched technologies paid in relics. |
| [`relics_tests.rs`](relics_tests.rs) | Tests: A building's relics and livestock (`FUN_1411cb850`, `FUN_1411d6ad0`): their counts as they go in and out, the relics its player holds, the technologies updated as a relic comes in and those paid in relics that go back as one leaves. |
| [`restart.rs`](restart.rs) | The Enter's restart under a deleted head (its slot 0x28 with 0): its start again, by where the unit is and what its target has room for. |
| [`scope.rs`](scope.rs) | Which shelters a garrison takes (`GarrisonScope`) and how far a unit goes to one (`ShelterReach`). |
| [`shelter.rs`](shelter.rs) | The garrison order and `SEEK_SHELTER`: each unit to a building that takes it in, nearest first (`Shelters`). |
| [`siege_tests.rs`](siege_tests.rs) | Tests: A siege unit's count of the infantry inside it, and a ranged unit's reload as it leaves one. |
| [`takes_in.rs`](takes_in.rs) | A building's test whether it takes a unit in (`FUN_14120eb10`), its room and what it holds. |
| [`tests.rs`](tests.rs) | Tests. |
| [`unit.rs`](unit.rs) | A unit entering a building or a transport, and its update inside. |
| [`unload.rs`](unload.rs) | UNLOAD and `GO_BACK_TO_WORK`: the units inside a building or a transport come out. |
| [`walk_in.rs`](walk_in.rs) | The Enter action (`EnterAction`, `RGE_Action_Enter`): its walk to its target, its wait and its update. |
| [`walk_point.rs`](walk_point.rs) | The Enter's walks to a point (`EnterAction`): its walk to its target's place (state 11) and its walk without a target. |
| [`transport/`](transport/) | The Transport action (`RGE_Action_Transport`, type 12, vtable `0x142ed61e8`): a transport's sail to a point and its unload there. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`commands`](../commands/), [`construction`](../construction/), [`groups`](../groups/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`pathing`](../pathing/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
- Used by: [`commands`](../commands/), [`map`](../map/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`testing`](../testing/), [`unit_ai`](../unit_ai/), [`world`](../world/).
<!-- /uses -->
