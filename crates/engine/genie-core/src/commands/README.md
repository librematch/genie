# `commands`: the players' commands

A command is a player's input, as a recording stores it. The tick runs its commands first, before
any object updates. `World::run` (`run.rs`) dispatches each command to its handler: the
building commands, the unit commands and the players' each have their own dispatch there.

- Orders to units (INTERACT, MOVE, FOLLOW, GUARD, STOP, PATROL and the others) go on to
  `orders/`.
- `BUILD` places a foundation: the placement test and the payment (`placement.rs`), the new
  building (`new_building.rs`), its ground (`ground.rs`) and its builders (`builders.rs`).
- `BUILD_WALL` places a line of wall foundations (`wall.rs`).
- `QUEUE` and `RESEARCH` fill a building's production queue (`production.rs`,
  `building_commands.rs`).
- `CANCEL_BUILD` deletes an object (`delete.rs`).
- `GAME` runs a sub-command: diplomacy, the farm queues, treason and the others (`game.rs`).

`Command` (`mod.rs`) is the decoded command. A command that genie does not model yet is
`Command::Unmodelled`, and it stops its player.

## In the game

- `RGE_Command` and `TRIBE_Command` (`command.cpp`, `tcommand.cpp`), with `do_commands` and
  `do_command`'s cases.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`all_back_to_work.rs`](all_back_to_work.rs) | `ALL_BACK_TO_WORK` (0x8c): a town centre's villagers back to work. |
| [`attack_move.rs`](attack_move.rs) | `ATTACK_MOVE` (0x21): the player's units attack-move through waypoints. |
| [`auto_scout.rs`](auto_scout.rs) | `AUTO_SCOUT` (0x26): the player's units scout by themselves. |
| [`build.rs`](build.rs) | BUILD (0x66, `FUN_140e7a400`): a player's foundation and the builders it sends to it. |
| [`builders.rs`](builders.rs) | BUILD's builders: their task, the queue and the direct path, the wait for the foundation and the build command. |
| [`building_commands.rs`](building_commands.rs) | The commands of a building: SET GATHER POINT, the command queue and RESEARCH. |
| [`building_queue.rs`](building_queue.rs) | `BUILDING_COMMAND_QUEUE` (0x2d): a step the player's objects queue. |
| [`delete.rs`](delete.rs) | A player's deletion of one of its objects: `CANCEL_BUILD` (0x6a, `TRIBE_Command::do_command` case 0x6a), which the game's delete key sends. |
| [`flare.rs`](flare.rs) | FLARE (0x73): a flare for the player's allies. |
| [`form_formation.rs`](form_formation.rs) | `FORM_FORMATION` (0x17): the player's units form a formation. |
| [`game.rs`](game.rs) | GAME (0x67): `TRIBE_Command::do_command`'s case (`0x140e77b00`), which lets a player's sub-command for another player through only with the game's cheats, and its switch on the sub-command (`FUN_140e7c330`, `World::game`). |
| [`gate.rs`](gate.rs) | GATE (0x72): a player's gate lock. |
| [`gather_point.rs`](gather_point.rs) | SET GATHER POINT (0x78): the gather point of the player's buildings. |
| [`gather_point_state.rs`](gather_point_state.rs) | A gather point's state: a point SET GATHER POINT made, or a byte SET GATHER STATE gave. |
| [`go_back_to_work.rs`](go_back_to_work.rs) | `GO_BACK_TO_WORK` (0x80) and `GO_BACK_TO_WORK2` (0x86): the player's buildings let their villagers back out. |
| [`ground.rs`](ground.rs) | A new building's ground: the units on its foundation sent off, its terrain, the footprint clearing and its place. |
| [`mod.rs`](mod.rs) | The commands (`Command`) and their dispatch (`World::run`): the handlers of the players' orders, the buildings' placement and the production queue's. |
| [`new_building.rs`](new_building.rs) | A new building: BUILD's foundation and the building constructor, with its combat setup, its storage and its annexes. |
| [`order.rs`](order.rs) | The orders of units (INTERACT, WORK, MOVE, RETREAT, FOLLOW, GUARD): their units, the check, the reset and the order's run. |
| [`packet_values.rs`](packet_values.rs) | Values the command packets carry: a command's queue id, a trade's lots, a BUILD's sprite override and a tribute's or trade's resource word. |
| [`patrol.rs`](patrol.rs) | PATROL (0x15): the player's units patrol through waypoints. |
| [`placement.rs`](placement.rs) | BUILD's tests before its foundation: a farm on rice terrain, the pending debit, the builders, the placement test and the payment. |
| [`production.rs`](production.rs) | QUEUE: a building's production queue, its load, its units and technologies, and a new entry's costs. |
| [`queue.rs`](queue.rs) | QUEUE (0x81): units of one master at the player's buildings. |
| [`research.rs`](research.rs) | RESEARCH (0x65): a technology at the player's buildings. |
| [`resign.rs`](resign.rs) | RESIGN (0x0b): a player resigns or drops. |
| [`run.rs`](run.rs) | A command's run in the command phase (`World::run`), a human player's order and the order units' reset. |
| [`seek_shelter.rs`](seek_shelter.rs) | `SEEK_SHELTER` (0x2c): the player's villagers seek shelter. |
| [`set_gather_state.rs`](set_gather_state.rs) | SET GATHER STATE (0x82): the gather point of a player's buildings is the building itself. |
| [`stop.rs`](stop.rs) | STOP (0x01): the player's units stop. |
| [`tests.rs`](tests.rs) | Tests: The command enum's issuing player and the lane a failed run stops. |
| [`town_bell.rs`](town_bell.rs) | `TOWN_BELL` (0x7f): a town centre's bell. |
| [`trade.rs`](trade.rs) | `SELL_COMMODITY` (0x7a) and `BUY_COMMODITY` (0x7b): the player trades at the market. |
| [`transform_object.rs`](transform_object.rs) | `TRANSFORM_OBJECT` (0x29): an object of the player takes another master. |
| [`tribute.rs`](tribute.rs) | GIVE ATTRIBUTES (0xc4): the player's tribute to another. |
| [`unit_ai_state.rs`](unit_ai_state.rs) | `UNIT_AI_STATE` (0x12): the stance of the player's units. |
| [`unit_order.rs`](unit_order.rs) | `UNIT_ORDER` (0x75): an order of the player's units by its action. |
| [`units.rs`](units.rs) | The units a command's packet lists: its count and the ids after the packet's fixed part. |
| [`unload.rs`](unload.rs) | UNLOAD (0x6f): units let out of the player's buildings and transports. |
| [`unmodelled.rs`](unmodelled.rs) | A recorded command the slice does not run: it keeps its packet and why. |
| [`wall.rs`](wall.rs) | A player's line of wall foundations: `BUILD_WALL` (0x69, `FUN_140e7b2e0`), which dragging a wall sends. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
- Used by: [`construction`](../construction/), [`garrison`](../garrison/), [`objects`](../objects/), [`orders`](../orders/), [`players`](../players/), [`production`](../production/), [`world`](../world/).
<!-- /uses -->
