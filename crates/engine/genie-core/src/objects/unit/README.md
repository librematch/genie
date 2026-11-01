# `objects/unit`: the object and its update

`Unit` (`mod.rs`) holds what the engine tracks of one object: its id, its position, its life
cycle, its movement, its actions, its unit AI and its sprite. Each kind of action has its own
field, so an object that does not build has no Build state.

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
- `top.rs` answers what the object's actions hold: the top action, its target and its gait.
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
| [`death.rs`](death.rs) | An object's death (`set_state(3)` and the static part of it) and a foundation's death. |
| [`default_task.rs`](default_task.rs) | The master's default task, which an idle unit starts in its update: a bird's Fly, a farm's Make or the Artifact of livestock. |
| [`life_step.rs`](life_step.rs) | The life cycle's step in a unit's update: the hit points, the states, and what each new state sets off. |
| [`mod.rs`](mod.rs) | What the slice tracks of one object (`Unit`) and its update, action by action. |
| [`stop.rs`](stop.rs) | A unit's stops: its actions torn down or cleared, its standing sprite, an action's effects applied. |
| [`tests.rs`](tests.rs) | Tests. |
| [`top.rs`](top.rs) | What a unit's actions hold: their taker, its top action, target and gait. |
| [`update.rs`](update.rs) | A unit's update on its owner's turn, action by action. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`commands`](../../commands/), [`construction`](../../construction/), [`garrison`](../../garrison/), [`gathering`](../../gathering/), [`groups`](../../groups/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`orders`](../../orders/), [`ownership`](../../ownership/), [`pathing`](../../pathing/), [`players`](../../players/), [`production`](../../production/), [`technology`](../../technology/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
