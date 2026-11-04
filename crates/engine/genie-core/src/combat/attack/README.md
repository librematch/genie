# `combat/attack`: the Attack action

The Attack action brings a unit to its target and strikes it. A unit that starts away from its
target walks there with a `MoveTo` sub-action. In reach, it swings: the hit lands on a frame of
the attack sprite, and the unit waits out its reload between swings. A target that dies ends the
swing and the action.

The files follow the action's life:

- `constructor.rs` builds the action and starts it.
- `update.rs` is one update of the action.
- `states.rs` is the action's `set_state`: the walk, the teardown, a lost target, the end.
- `walk_and_swing.rs` holds the codes of the walk to the target and the swing with its hit.
- `walks.rs` holds the `MoveTo`s `set_state` makes (states 4, 5 and 11) and the update of the
  sub-action list that runs them.
- `ready_to_attack.rs` tests the range and the sight, and turns the unit to its target.
- `hits.rs` holds the hit itself, and `sprites.rs` the swing's sprites and the attack timer.
- `work_slots.rs` holds the slots that set a target and run the work.
- `unit.rs` is the unit's part of its update: the Attack's update with the strike and what
  follows, or the end of an Attack that lost its target.
- `unit_ai_side.rs` holds the attack that a unit AI commands: a new Attack, a forced attack, and
  the readiness and sight tests that the unit AI asks.

The damage of the hit is in `combat/damage/`.

## In the game

- `RGE_Action_Attack` (action type 9, `act_atak.cpp`), with `readyToAttack` and `work`.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`attack_state.rs`](attack_state.rs) | An Attack's state: the steps of its update between the start and the end, each walk with its `MoveTo`. |
| [`constructor.rs`](constructor.rs) | The Attack's constructor (`FUN_140a7c210`) and its start (slot 5): its sprites and frames and what it reads of its unit and its target. |
| [`hits.rs`](hits.rs) | A hit: where it comes from (`Hit`), what the swing does after it, the strike (`FUN_141084230`) and the blast so far. |
| [`mod.rs`](mod.rs) | The Attack action (`RGE_Action_Attack`, type 9, vtable `0x142ed47a8`) of a melee unit: a hunting villager's sub-action on a tree or on its livestock (`crate::gathering::hunt`). |
| [`ready_to_attack.rs`](ready_to_attack.rs) | readyToAttack (`FUN_140a7f9d0`): the ready state, the follow, the range and sight tests and the facing at its end. |
| [`sprites.rs`](sprites.rs) | The swing's and the arrival's sprites, the attack timer and the unit's turn to its target. |
| [`states.rs`](states.rs) | The Attack's `set_state` (`0x140a820c0`): the move, the teardown and prelude, a lost target, one out of sight, the finish and the halt. |
| [`tests.rs`](tests.rs) | Tests. |
| [`unit.rs`](unit.rs) | The unit's own Attack in its update: a lost target's end, and the strike with what follows. |
| [`unit_ai_side.rs`](unit_ai_side.rs) | The attack the unit AI commands through its view while it runs (`UnitView::attack`): the new Attack, a forced attack again on the unit's own target, the target as a hit reads it, the start's readiness, the target's sight and a villager's swap. |
| [`update.rs`](update.rs) | One update (slot 6, `0x140a82ae0`): a target gone, a disarmed building, the walks to where the target was seen. |
| [`walk_and_swing.rs`](walk_and_swing.rs) | State 4's walk to the target and state 7's swing with its hit and its trample. |
| [`walks.rs`](walks.rs) | The `MoveTo`s the Attack's `set_state` makes (`0x140a820c0`, cases 4, 5 and 11) and the update of its sub-action list (`FUN_140a992b0`). |
| [`work_slots.rs`](work_slots.rs) | The swing's state (7), slot 9 (`RGE_Action_Attack::work`) and slot 0x68 (`set_target_obj`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`construction`](../../construction/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`players`](../../players/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
