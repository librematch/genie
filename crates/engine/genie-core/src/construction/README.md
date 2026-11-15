# `construction`: building and repairing

Villagers build foundations into buildings and repair what is damaged. This folder holds that
work and a building's own update:

- `build/` holds the Build action and a foundation's construction.
- `repair/` holds the Repair action and its payment.
- `wait_solidify.rs` holds a villager's wait until a foundation's construction starts.
- `housing/` holds a building's Housing, which `UNIT_ORDER`'s case 3 installs: it pays its wood,
  runs down its timer by the building's work rate and then gives the building and its owner more
  headroom.
- `pack/` holds a trebuchet's Pack: its timer runs up by the building's work rate to its
  master's pack time, and the building then turns into its packed master, its unit AI told to
  attack the Pack's target first.
- `unpack/` holds a packed trebuchet's Unpack: it walks within its range of its target or its
  point, turns the building into its unpacked master, runs its timer up by the building's work
  rate to that master's unpack time, and then gives the building its standing sprite and its
  annexes, its unit AI told to attack the Unpack's target first.
- `building_update.rs` holds a building's own update: a gate's and a farm's update, the
  completion, the whole seconds (a monastery's relic goods, which `players/world_player/income.rs`
  adds, and the heal of the units inside), the hit points' regeneration and the death blast.
- `speedup.rs` holds a speed-up building's work on the objects about it, with which its whole
  seconds start: the moving speed-up, the work-rate speed-up or the heal its player's attributes
  255 to 261 give.
- `gates.rs` holds the gates: which units are in a gate, and the gates that open for their
  player's update and close after it.

## How the tick reaches it

The world opens a player's gates before the player's objects update, and closes them after
(the schedule's `OpenGates` and `CloseGates`, `world/schedule.rs`). The object's update runs a
building's own steps first, then the builder's or repairer's action.

## In the game

- `TRIBE_Action_Build` (action type 101, `tact_bld.cpp`).
- `TRIBE_Action_Repair` (action type 106, `tact_rep.cpp`).
- `TRIBE_Action_Wait_Solidify` (action type 0x9a).
- `TRIBE_Action_Housing` (action type 0x7b, vtable `0x142ed7008`).
- `TRIBE_Action_Pack` (action type 0x7c, vtable `0x142ed7978`).
- `TRIBE_Action_Unpack` (action type 0x7d, vtable `0x142ed81a8`).
- `TRIBE_Building_Object` (`t_b_obj.cpp`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`annexes.rs`](annexes.rs) | A main building's annexes (`+0xbc0`..`+0xbd8`): its four slots, each an annex or none. |
| [`builder_count.rs`](builder_count.rs) | The builders or repairers on an object. |
| [`building_update.rs`](building_update.rs) | A gate's and a farm's update, a building's completion and seconds, the idle swap and the death blast. |
| [`footprint_waits.rs`](footprint_waits.rs) | A builder's waits for an occupied footprint. |
| [`foundation_id.rs`](foundation_id.rs) | A foundation's id as its own type: the building a builder builds and its footprint clears. |
| [`gates.rs`](gates.rs) | The gates: the units in a gate, a player's gate list and passes, and the gates opened before and closed after a player's update. |
| [`main_building_id.rs`](main_building_id.rs) | A main building's id as its own type: the building whose annexes are its parts. |
| [`mod.rs`](mod.rs) | Building and repairing: the Build and Repair actions with each worker's part of its update, the wait for a foundation, a building's own update, Housing, Pack and Unpack, and the gates. |
| [`outcomes.rs`](outcomes.rs) | What a foundation's work answers: whether its construction goes on (`BuildProgress`) and whether it began (`ConstructionStart`). |
| [`repair_target.rs`](repair_target.rs) | A Repair's target as its own type: the object it repairs. |
| [`speedup.rs`](speedup.rs) | A speed-up building's work on the objects about it (`FUN_1411c39b0`, `FUN_1411c4a00`): a building that its player's attribute 255 names gives each allied object of its player's object target on the tiles of its reach a moving speed-up, a work-rate speed-up or a heal. |
| [`speedup_tests.rs`](speedup_tests.rs) | Tests: A speed-up building's work on the objects about it (`FUN_1411c39b0`, `FUN_1411c4a00`): the tiles of its reach, its three effects on the allied objects its player's object target names, and the call in its whole seconds. |
| [`wait_solidify.rs`](wait_solidify.rs) | A villager's wait for a foundation: `TRIBE_Action_Wait_Solidify` (action type 0x9a, vtable `0x142ed82f8`). |
| [`wait_solidify_state.rs`](wait_solidify_state.rs) | A Wait Solidify's state: waiting for its foundation to turn solid. |
| [`build/`](build/) | A builder's Build action (`TRIBE_Action_Build`, type 101, vtable `0x142ed6628`) up to the builder's walk: the install's start takes state 4 with a `MoveTo` sub-action (`RGE_Action_Move_To`, type 1) toward the foundation, or waits for work (state 3) where the constructor took none; a builder already at the foundation works in its first update (state 6). |
| [`building_update/`](building_update/) | The building update's tests. |
| [`gates/`](gates/) | The gates' tests. |
| [`housing/`](housing/) | A building's Housing: `TRIBE_Action_Housing` (action type 0x7b, vtable `0x142ed7008`), which `UNIT_ORDER`'s case 3 installs on a building (`World::housing_order`), and the work command's factory on a task of its type (`FUN_140aca4b0` case 0x7b). |
| [`pack/`](pack/) | A building's Pack: `TRIBE_Action_Pack` (action type 0x7c, vtable `0x142ed7978`), which a trebuchet (class 54) and `UNIT_ORDER`'s case 1 install on a building. |
| [`repair/`](repair/) | A repairer's Repair action (`TRIBE_Action_Repair`, type 106, vtable `0x142ed7b88`): the action walks to its target (state 4, a `MoveTo` sub-action within the unit's weapon range), faces it (state 6) and repairs it each update (state 7) until the target holds its most hit points or the player cannot pay for more; a target out of reach makes it wait (state 10) or fail (state 13). |
| [`unpack/`](unpack/) | A building's Unpack: `TRIBE_Action_Unpack` (action type 0x7d, vtable `0x142ed81a8`), which a packed trebuchet (class 51) and `UNIT_ORDER` install on a building. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`commands`](../commands/), [`gathering`](../gathering/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`pathing`](../pathing/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`testing`](../testing/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
- Used by: [`combat`](../combat/), [`commands`](../commands/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`map`](../map/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`pathing`](../pathing/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`testing`](../testing/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
<!-- /uses -->
