# `groups/steer`: the members' moves to their slots

Each member of a formation walks to its slot. This folder holds that part of the group's update.

- `to_slots.rs` moves the members to their slots. It stops a free member and steers a member
  toward its slot.
- `slot_steps.rs` tries a slot that is due, steers on a clear line and sets a member's state,
  speed, facing, place and own move.
- `detour.rs` finds a way round an obstruction on the line to the slot. It asks the short
  pather, the long pather and the tile planner.

## In the game

- The part of `BUnitGroup::update` that brings each member of a formation to its slot, with the
  member's steering.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`detour.rs`](detour.rs) | The way round a record on the line to the slot: the short pather, the long pather and the tile planner. |
| [`mod.rs`](mod.rs) | The members' moves to their slots (FD-036): the part of `BUnitGroup::update` (`FUN_14103d910`, `0x14103e8e8`..`0x14103efba`) that brings each member of a formation to its slot, with the member's steering (`FUN_141025fb0`) and the setter of a member's own move (`FUN_141039f60`). |
| [`slot_steps.rs`](slot_steps.rs) | A due slot's try, the steering on a clear line, and the setters of a member's state, speed, facing, place and own move. |
| [`tests.rs`](tests.rs) | Tests. |
| [`to_slots.rs`](to_slots.rs) | The members' moves to their slots, a free member's stop, the line test and a member's steering toward its slot (`FUN_141025fb0`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`pathing`](../../pathing/), [`players`](../../players/), [`world`](../../world/).
<!-- /uses -->
