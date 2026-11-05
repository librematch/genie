# `gathering`: gathering resources

Villagers and fishing ships gather food, wood, gold and stone, and carry it to a drop site. This
folder holds every part of that work:

- `gather/` holds the Gather action: walk to the resource, gather, carry the load to a drop site,
  and walk back.
- `hunt/` holds the Hunt action: kill the animal or fell the tree first, then gather it.
- `make.rs` holds a farm's food: the Make action that farms, fish traps and pastures run, and
  its part in the farm's update.
- `gather_search.rs` and `closest_resource.rs` search for a resource to gather.
- `drop_sites.rs` and `drop_off.rs` find the drop site and walk the gatherer there.
- `farm_queue.rs` holds the farms and fish traps that a player paid for in advance.
- `pasture.rs` puts a new pasture's livestock on its edges.
- `mule_cart.rs` holds the mule cart's resource searches.

## How the tick reaches it

The object's update runs the gatherer's action. Each action folder has a `unit.rs`: the unit's
part of the update, which binds the action to the world.

## In the game

- `RGE_Action_Gather` (action type 5, `act_gath.cpp`, and `tact_gth.cpp`).
- `TRIBE_Action_Hunt` (action type 110, `tact_hnt.cpp`).
- `RGE_Action_Make` (action type 21, `act_make.cpp`).
- `tact_she.cpp`: the livestock.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`action_end.rs`](action_end.rs) | Whether a gathering action ends at its drop-off for the unit AI's queued orders. |
| [`closest_resource.rs`](closest_resource.rs) | `ClosestResourceObject` (`FUN_140c49760`): where it searches from and what it finds; a herded sheep. |
| [`decay_timer.rs`](decay_timer.rs) | A combat object's decay timer, which a salvage pile's decay runs. |
| [`drop_off.rs`](drop_off.rs) | A gatherer's walk after a drop and the drop sites it walks to. |
| [`drop_site_id.rs`](drop_site_id.rs) | A drop site's id as its own type: the object a worker carries its load to. |
| [`drop_sites.rs`](drop_sites.rs) | The drop-site searches (`FUN_1411671e0`): from a unit, or from a point without one. |
| [`farm_queue.rs`](farm_queue.rs) | The mill's and the dock's queues: the farms and pastures (`farm_queue_count`, `+0x141c`) and the fish traps (`fishTrapQueueCount`, `+0x1420`) a player paid for ahead, which GAME's sub-commands 0xd, 0x15 and 0x11 buy and 0xe, 0x16 and 0x12 give back. |
| [`farm_stops.rs`](farm_stops.rs) | A gatherer's stops on a farm. |
| [`gather_search.rs`](gather_search.rs) | The searches for a resource to gather: a builder's search for more work around the building it completed (`FUN_1415cc4e0`, over the tiles `FUN_1415cc1a0` reaches) and the unit AI's `ClosestResourceObject` (`FUN_140c49760`, slot 0x80), which the builder's Build asks next (`0x140ab0370` case 0xdf). |
| [`gather_target.rs`](gather_target.rs) | A Gather's target as its own type: the resource it works. |
| [`hunt_target.rs`](hunt_target.rs) | A Hunt's target as its own type: the animal it hunts or leads. |
| [`make.rs`](make.rs) | A farm's food: `RGE_Action_Make` (action type 21, vtable `RGE_Action_Make::vftable`), the default task of the farms (50, 1187), the fish trap (199) and the pastures (1893, 1897). |
| [`make_state.rs`](make_state.rs) | A Make's state: making food. |
| [`mod.rs`](mod.rs) | Gathering resources: the Gather and Hunt actions with each gatherer's part of its update, a farm's food, the searches for a resource, the drop sites and drop-offs, the mill's and the dock's queues and the pastures. |
| [`mule_cart.rs`](mule_cart.rs) | The mule cart: the resources a finished cart searches (its completion, `FUN_1411c6bd0`'s tail) and its unit AI's own search (slot 0x1e8), then the work command toward the one it found. |
| [`pasture.rs`](pasture.rs) | A pasture's livestock (`FUN_1411c8410`, `SetupPasture`): the completion's first part (`FUN_1411c6bd0`) of a pasture (masters 1893 and 1897, class 49) whose build points reached its build time puts its owner's livestock on points along its edges, each holding its share of the pasture's food. |
| [`pasture_id.rs`](pasture_id.rs) | A pasture's id as its own type: the building whose livestock a unit is in or tends. |
| [`queue_count.rs`](queue_count.rs) | The count of farms or fish traps a queue paid for. |
| [`salvage_amounts.rs`](salvage_amounts.rs) | What a salvage pile holds of six attributes. |
| [`gather/`](gather/) | The Gather action (`RGE_Action_Gather`, type 5, vtable `0x142ed4f28`): a forager sent to a resource walks to it (state 4, a quiet `MoveTo`), faces it (6), gathers (7), takes the load to a drop site (8) and walks back to the resource (4). |
| [`gather_search/`](gather_search/) | The resource search's passes. |
| [`hunt/`](hunt/) | The Hunt action (`TRIBE_Action_Hunt`, type 110, vtable `0x142ed70f8`): a villager sent to a tree (or an animal) with a gather task attacks it first (state 12, an `Attack` sub-action); once it is dead the villager walks to it (state 4, a `MoveTo` sub-action), faces it (6) and gathers (7), then carries the load to a drop site (8). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`construction`](../construction/), [`groups`](../groups/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`pathing`](../pathing/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`testing`](../testing/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
- Used by: [`combat`](../combat/), [`commands`](../commands/), [`construction`](../construction/), [`garrison`](../garrison/), [`groups`](../groups/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`testing`](../testing/), [`unit_ai`](../unit_ai/), [`world`](../world/).
<!-- /uses -->
