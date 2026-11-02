# `unit_ai/task_finder`: which task a unit does on a target

A unit that gets a target needs a task: gather from it, build it, repair it, attack it, drop
resources at it. The task finder picks it from the master's task list.

The finder tries the special tasks first, each for its own kind of target: a drop-off, the
player's foundation, a garrison, a repair, a conversion, a heal. Then it searches in general: a
task for the target's combat level, else its master, else its class. Last, it tries a task for the
terrain under the target's point. The work command asks the unit's own master first, then the
player's other masters of the task swap group.

- `mod.rs` holds the finder, its first and last steps and its tests of a task on a target.
- `target_steps.rs` holds the steps on a target: the drop-off, the foundation, the garrison into
  it and its repair.
- `world.rs` holds what the finder reads of the world: relations, drop sites, a target's state.

## In the game

- `RGE_Task_List` and `TRIBE_Task_List` (`task.cpp`, `task_lst.cpp`, `ttsk_lst.cpp`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`mod.rs`](mod.rs) | The task finder (`TRIBE_Task_List` slot 2, `FUN_1412784d0`, with the `RGE_Task_List` finder `FUN_1411a8ba0` and its test of a task on a target, slot 4 `FUN_1411a8620`): which task of a master's task list a unit does on a target object. |
| [`target_steps.rs`](target_steps.rs) | The task finder's steps on a target: the drop-off, its player's foundation, the garrison into it and its repair. |
| [`world.rs`](world.rs) | What the task finder reads of the world: the relations, the drop sites, a wait's progress, a target's state and the search of slot 0x820. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`construction`](../../construction/), [`garrison`](../../garrison/), [`gathering`](../../gathering/), [`map`](../../map/), [`objects`](../../objects/), [`orders`](../../orders/), [`players`](../../players/), [`world`](../../world/).
<!-- /uses -->
