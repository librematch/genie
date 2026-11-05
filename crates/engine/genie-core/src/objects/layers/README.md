# `objects/layers`: an object's class chain

An object is the game's class chain (FD-045-D06): the static object, the animated one, the
moving one, then the action, combat and building layers, each as the save reads it. This folder
holds the layers as structs.

`mod.rs` holds the static layer with its place, which is on every object's row. `motion.rs` holds
the animated layer (types 20 to 29) or the moving one with its animated layer inside (types 30
and up): the object's motion part (`Motion`, in the world's tables, FD-045-D58). `action.rs`
holds the action layer (`RGE_Action_Object`), the action part (`Action`, FD-045-D59).
`combat.rs` holds the combat layer (`RGE_Combat_Object` and `TRIBE_Combat_Object`) and
`building.rs` the building layer (`TRIBE_Building_Object`). These three are parts too: an object
without an entry reads an empty layer or the game's defaults (below type 40, 50 or 80).

## In the game

- `RGE_Static_Object`, `RGE_Animated_Object`, `RGE_Moving_Object`, `RGE_Action_Object`,
  `RGE_Combat_Object`, `TRIBE_Combat_Object`, `TRIBE_Building_Object`: the save's classes
  `RgeStaticObject` to `TribeBuildingObject`.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`action.rs`](action.rs) | `RGE_Action_Object` (types 40 and up, FD-045-D06): the action layer, the save's action list (`RgeActionObject`'s `actions`), then what genie keeps for the default task its update starts (`RGE_Action_Object::update`, `0x14106ac90`), and the reads of its top action. |
| [`building.rs`](building.rs) | `TRIBE_Building_Object` (type 80, FD-045-D06): the building layer, its fields in the save's order (`TribeBuildingObject`), then the two the save does not hold. |
| [`combat.rs`](combat.rs) | `RGE_Combat_Object` and `TRIBE_Combat_Object` (types 50 and up, FD-045-D06): the combat layer, its fields in the save's order (`RgeCombatObject`, then `TribeCombatObject`). |
| [`command_count.rs`](command_count.rs) | An action object's command count (`command_flag`, `+0x888`, a byte the save holds): one more for every work and move its slots 0x128, 0x140 and 0x138 hand on (`FUN_14106c590`, `FUN_141070c90`, `FUN_14106d340`), 0 from the constructor (`FUN_14106f9e0`). |
| [`default_task.rs`](default_task.rs) | The default task of an object's action layer (FD-045-D48, D60): what its master's default task starts once its action list is empty in state 2 (`RGE_Action_Object::update`, `0x14106ac90`, slot 0x7f0), as the load or the object's constructor found it. |
| [`idle_time.rs`](idle_time.rs) | A villager's idle time (`RGE_Action_Object` `+0x898` to `+0x8a0`): what its update counts while it may be interrupted and adds to its owner's statistics once it dies (`RGE_Action_Object::update`, `0x14106ac90`). |
| [`mod.rs`](mod.rs) | The object's class chain (FD-045-D06): the game's object classes, each layer holding the one below it, as the save reads them (`RgeMovingObject` holds `rge_animated_object`). |
| [`motion.rs`](motion.rs) | The layers between an object's place and its action layer (FD-045-D58): the animated object's (types 20 to 29) and the moving object's (30 and up), which holds the animated one, as the object's `Motion` in the world's table, and `MotionMut`, the one a routine changes beside the object's row. |
| [`speedup.rs`](speedup.rs) | The building that speeds an action object up (`mSpeedupBuildingId` `+0x890`, `mSpeedupValue` `+0x894`): the save holds both, the update drops them once the building is gone or out of range (`RGE_Action_Object::update`, `0x14106ac90`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`construction`](../../construction/), [`garrison`](../../garrison/), [`gathering`](../../gathering/), [`map`](../../map/), [`movement`](../../movement/), [`orders`](../../orders/), [`ownership`](../../ownership/), [`pathing`](../../pathing/), [`players`](../../players/), [`testing`](../../testing/), [`world`](../../world/).
<!-- /uses -->
