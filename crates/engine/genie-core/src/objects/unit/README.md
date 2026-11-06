# `objects/unit`: the object and its update

`Unit` (`mod.rs`) is the object's row: the static object the engine tracks, its id, place, life
cycle, sprite and garrison. Its other layers are parts in the world's tables (FD-045-D51): the
motion, the action layer, the combat and building layers and the unit AI. The action layer's list
holds each action as its kind's variant, so an object that does not build has no Build state.

`update.rs` is the object's update. It follows the game's chain of classes in order: a building's
own steps, the velocity step, the life cycle, the sprite, the actions and the unit AI. Each step
of an action is a method in the action's system:

- `combat/attack/unit.rs`: the attack timers and the Attack.
- `movement/unit.rs`: the velocity step, the `MoveTo`, the Explore and the Fly.
- `ownership/artifact.rs`: the Artifact.
- `construction/build/unit.rs` and `construction/repair/unit.rs`: the Build and the Repair.
- `garrison/unit.rs`: the inside update, the Enter and the transport.
- `gathering/make.rs`, `gathering/gather/unit.rs` and `gathering/hunt/hunter.rs`: the Make, the
  Gather and the Hunt.

The other files of this folder:

- `life_step.rs` holds the life cycle's step in the update: the hit points, the states and what
  each new state sets off.
- `default_task.rs` holds the master's default task, which an idle unit starts.
- `answers.rs` holds what the object and its actions answer: its taker, its gait, whether its
  top action may be interrupted and the target it attacks. The top action itself is the action
  layer's (`objects/layers/action.rs`).
- `stop.rs` tears down an object's actions and shows its standing sprite.
- `death.rs` holds an object's death and a foundation's death.

## In the game

- The `update` of each class in the chain: `RGE_Static_Object`, `RGE_Animated_Object`,
  `RGE_Moving_Object`, `RGE_Action_Object`, `RGE_Combat_Object`, `TRIBE_Combat_Object` and
  `TRIBE_Building_Object`.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`answers.rs`](answers.rs) | What a unit and its action layer answer: their taker, its context, its gait, whether its top action may be interrupted and the target it attacks. |
| [`death.rs`](death.rs) | An object's death (`set_state(3)` and the static part of it) and a foundation's death. |
| [`default_task.rs`](default_task.rs) | The master's default task, which an idle unit starts in its update: a bird's Fly, a farm's Make or the Artifact of livestock. |
| [`idle_time.rs`](idle_time.rs) | What `RGE_Action_Object::update` (`0x14106ac90`) does after the action list: a villager's idle statistics, then the drop of a speed-up building that is gone or out of range. |
| [`life_step.rs`](life_step.rs) | The life cycle's step in a unit's update: the hit points, the states, and what each new state sets off. |
| [`mod.rs`](mod.rs) | What the slice tracks of one object (`Unit`) and its update, action by action. |
| [`stop.rs`](stop.rs) | A unit's stops: its actions torn down or cleared, its standing sprite, an action's effects applied. |
| [`tests.rs`](tests.rs) | Tests. |
| [`update.rs`](update.rs) | A unit's update on its owner's turn, action by action. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`construction`](../../construction/), [`gathering`](../../gathering/), [`groups`](../../groups/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`orders`](../../orders/), [`ownership`](../../ownership/), [`pathing`](../../pathing/), [`players`](../../players/), [`technology`](../../technology/), [`unit_ai`](../../unit_ai/), [`vision`](../../vision/), [`world`](../../world/).
<!-- /uses -->
