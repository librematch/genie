# `movement/move_to`

The `MoveTo` action: a unit walks to a target object or a point. One struct, `MoveToAction`,
holds the game's fields: its state, target, point, sprite, voice (loud or quiet), range and wait. It runs
in two roles:

- As a unit's own top action (`action.rs`): a moved or following unit's, a wandering animal's.
  It is loud.
- As a sub-action (`sub_action.rs`): another action holds it, mostly quiet. The Explore,
  the Transport, the Enter, the Artifact, the Repair and a bird's Fly hold the struct. The
  Artifact's `MoveTo` to a point and the Fly's are loud. The other owners (an Attack, a
  Build, a Gather, a Hunt) still hold its state and wait in fields of their own and run it
  with `start_walk_shown` and `walk_or_give_up` (`mod.rs`), until they move to the struct one
  by one.

The `MoveTo`'s `set_state` (`0x140aa29e0`) shows the unit's sprites: turning to state 4 its own
sprite (`+0x58`) before the motion step runs, so the top speed reads that sprite; its end
(states 1 and 13), unless it is quiet, the standing sprite. `Shown` and `Shows` carry the
unit's sprite and the Sprite stream to it.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`action.rs`](action.rs) | The `MoveTo` as a unit's own top action (`MoveToAction`): its constructors, its start and its update. |
| [`constructors.rs`](constructors.rs) | The quiet sub-action `MoveTo`s an action makes and adds to its sub-action list: what their constructors read of the unit (`MoveToReads`) and the object one walks to (`WalkTo`); to an object (`FUN_140aa0df0`), to a point (`FUN_140aa0d30`) and to none. |
| [`list.rs`](list.rs) | The sub-action list's update (`FUN_140a992b0`) for an owner action whose list holds a quiet `MoveTo`: its first action's update, 1 for an empty list ("There is no list."). |
| [`mod.rs`](mod.rs) | The `MoveTo` action (`RGE_Action_Move_To`, type 1, vtable `0x142ed6030`): a unit walks to a target object or a point. |
| [`move_wait_state.rs`](move_wait_state.rs) | A `MoveTo`'s wait state: whether it waited after the motion step gave up. |
| [`sub_action.rs`](sub_action.rs) | The `MoveTo` as an action's sub-action: its constructor (`FUN_140aa0d30`, made quiet) and its update through the owner's sub-action list (`FUN_140a992b0`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`construction`](../../construction/), [`gathering`](../../gathering/), [`map`](../../map/), [`math`](../../math/), [`objects`](../../objects/), [`players`](../../players/), [`testing`](../../testing/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
