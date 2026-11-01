# `objects/layers`: an object's class chain

An object is the game's class chain (FD-045-D06): the static object, the animated one, the
moving one, then the action, combat and building layers, each as the save reads it. This folder
holds the layers as structs.

`mod.rs` holds the layers below the action layer, the object's `Body`: the static layer with its
place (every object), the animated layer (types 20 and up) and the moving one (types 30 and up).
`combat.rs` holds the combat layer (`RGE_Combat_Object` and `TRIBE_Combat_Object`): every object
holds it for now, with the game's defaults below type 50. `building.rs` holds the building layer
(`TRIBE_Building_Object`), likewise on every object, with the game's defaults below type 80.

## In the game

- `RGE_Static_Object`, `RGE_Animated_Object`, `RGE_Moving_Object`, `RGE_Combat_Object`,
  `TRIBE_Combat_Object`, `TRIBE_Building_Object`: the save's classes `RgeStaticObject` to
  `TribeBuildingObject`.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`building.rs`](building.rs) | `TRIBE_Building_Object` (type 80, FD-045-D06): the building layer, its fields in the save's order (`TribeBuildingObject`), then the two the save does not hold. |
| [`combat.rs`](combat.rs) | `RGE_Combat_Object` and `TRIBE_Combat_Object` (types 50 and up, FD-045-D06): the combat layer, its fields in the save's order (`RgeCombatObject`, then `TribeCombatObject`). |
| [`mod.rs`](mod.rs) | The object's class chain (FD-045-D06): the game's object classes, each layer holding the one below it, as the save reads them (`RgeMovingObject` holds `rge_animated_object`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`commands`](../../commands/), [`construction`](../../construction/), [`garrison`](../../garrison/), [`map`](../../map/), [`movement`](../../movement/), [`orders`](../../orders/), [`ownership`](../../ownership/), [`pathing`](../../pathing/), [`players`](../../players/), [`production`](../../production/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
