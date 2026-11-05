# `groups/update`: a group's update

The world runs the update of each group before the objects of the group's player. This folder
holds that update.

- `entry.rs` runs a player's groups, deletes a group whose update ends it, and checks its members.
- `awake.rs` is the update of an awake group: its lead, the motion, the formation's turn and
  front, and a new layout.
- `moves.rs` runs the group's current command: a move, the members' moves and steps, the orders
  that are due and the idle timer.
- `attack.rs` runs the attack command (9): it ends when the target is gone, lets the formation
  categories attack when they are in reach, and follows a target that moved.
- `release.rs` sends the units of a category at the target one by one.
- `send.rs` sends a member that is within reach at the target of its group's order: the gate
  part it strikes, its speed and its retarget timer.
- `checks.rs` holds what the update asks of the world: whether the units keep apart, whether a
  target is still there, whether a member is idle.

The idle timer draws from the world's random stream. That is the only draw of the world stream
outside the tick's own frame seeds.

## In the game

- `BUnitGroup::update` (`group.cpp`) and the group's destructor.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`attack.rs`](attack.rs) | The attack command's update (FD-036): `FUN_14103f4e0`, which `BUnitGroup::update` runs while the group's current command is an attack (9). |
| [`awake.rs`](awake.rs) | An awake group's update: its lead, what follows the motion, the formation's turn and front, the lines' ranges and the relayout. |
| [`checks.rs`](checks.rs) | What the update asks of the world: the units kept apart, an interruption, rest, a target still to act on, the relics, a member's idleness, the sprites shown and the lead. |
| [`entry.rs`](entry.rs) | The update's entry: a player's groups, a group's update and its deletion, the members' check and a hibernating group. |
| [`mod.rs`](mod.rs) | A group's update (FD-036): `BUnitGroup::update` (`FUN_14103d910`), which the world runs for the groups a player owns before the player's objects, and the deletion of a group whose update returns 0 (the world list's unlink, then `BUnitGroup`'s destructor, `FUN_140fe1920`). |
| [`moves.rs`](moves.rs) | The current command's update: a move, the members' moves and steps, the order queue's due orders and the idle timer. |
| [`release.rs`](release.rs) | A group's units sent at the target one by one (FD-036): `FUN_14103d460` (the log's "`UnlockedAttackWithoutPlan`"), which the attack update (`World::attack_update`) runs for the units of a category in reach. |
| [`send.rs`](send.rs) | A member sent at the target of its group's order (FD-036): the branch of `FUN_14102bbd0` for a member within reach, with the gate rule that picks the part of a gate it strikes (`FUN_14100dbe0`), the unit's speed (slot 0x208, `FUN_141202c70`) and its weapon reach (slot 0x1e8, `FUN_141086330`). |
| [`sprites.rs`](sprites.rs) | The sprites the group update shows its members: standing, walking and running, each from the member's master and its action. |
| [`tests.rs`](tests.rs) | Tests. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`construction`](../../construction/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`orders`](../../orders/), [`pathing`](../../pathing/), [`players`](../../players/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
