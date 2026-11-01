# `objects`: one object and what every object runs

Every object in the world is a `Unit` (`unit/`): a villager, a building, a tree, a sheep, a
missile. This folder holds the object itself and the parts that every object has:

- `unit/` holds the `Unit` type and its update, action by action.
- `lifecycle/` steps an object from alive to dying to dead to gone.
- `masters/` holds the masters: the definition of each kind of object for each player.
- `sprite.rs` holds the object's active sprite and the clock that animates it. A hit lands on a
  frame of the attack sprite.
- `facing.rs` holds the object's angle and its facet in the sprite.
- `layers.rs` holds the object's layers below its actions (FD-045): the static layer with its
  place, animated with its animation speed, or moving with its mover. Every layer holds the
  static one. The place has one setter, which moves the world's index with it (D15).
- `actions.rs` holds the object's action list: one `Action` per kind it holds, the current one
  first.
- `release.rs` handles an object that leaves the world, and the release of a target by the
  action that held it.
- `lines.rs` reads the unit lines from `unitlines.json`: the upgrade chains of the masters.

## How the tick reaches it

The world steps each live object of a player in list order (`world/object_step.rs`). The step
calls `Unit::update` (`unit/update.rs`), which runs the object's chain of updates.

## In the game

- `RGE_Static_Object` (`stat_obj.cpp`), `RGE_Animated_Object` (`ani_obj.cpp`),
  `RGE_Moving_Object`, `RGE_Action_Object` (`act_obj.cpp`) and the action list (`act_list.cpp`).
- The sprites (`sprite.cpp`, `asprite.cpp`) and the masters (`MasterStaticObject.cpp` and the
  other master classes).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`action_state.rs`](action_state.rs) | The state of an action of any type (`ActionState`): its number. |
| [`actions.rs`](actions.rs) | The object's action list (`RGE_Action_List`, FD-045-D12): the actions it holds, its current one (the head) first, each a variant of `Action`. |
| [`damaged_timer.rs`](damaged_timer.rs) | A building's timer since its last hit. |
| [`facet.rs`](facet.rs) | A sprite's facets: their count and the facet an object faces. |
| [`facing.rs`](facing.rs) | Where an object faces: its angle (`+0x7e8`, radians on the map, 0 to 2π) and its facet (`+0xe`), counted in its current sprite's facets (`+0x140` -> `+0xa`). |
| [`frame.rs`](frame.rs) | A sprite's frame and its animation state. |
| [`hit_points.rs`](hit_points.rs) | A combat object's hit points, a unit's or a building's (`TRIBE_Combat_Object`, types 70 and 80): the heal, the gain and the loss, the regeneration and the powerup's clamp (slots 0x110, 0xb8 and 0x260, and the update's checks). |
| [`hp.rs`](hp.rs) | An object's hit points (`HitPoints`), with the game's `f32` operations, and a master's whole hit points (`MasterHitPoints`). |
| [`layers.rs`](layers.rs) | The object's class chain (FD-045-D06): the game's object classes, each layer holding the one below it, as the save reads them (`RgeMovingObject` holds `rge_animated_object`). |
| [`lines.rs`](lines.rs) | The unit lines (`resources/_common/dat/unitlines.json`): each line names the masters of one upgrade chain, its first master first. |
| [`list_index.rs`](list_index.rs) | An object's place in its player's list (`ListIndex`) and a list's places (`Places`). |
| [`mod.rs`](mod.rs) | One object and what every object has and runs: the object the engine tracks (`unit::Unit`) with its update, its actions' holdings and stops, its life and death, its release, its sprite and facing, and the masters it is made from. |
| [`object_id.rs`](object_id.rs) | An object's id as one type: the id that names an object across ticks, as the commands, their unit lists and their targets hold it. |
| [`object_list.rs`](object_list.rs) | One of a player's object lists (FD-045-D11, D20): its objects in update order and the place of each by id, which the game's table by id (`FUN_1415417a0`, `world+0x168` and `+0x178`) answers. |
| [`regeneration.rs`](regeneration.rs) | A regeneration rate: the hit points an object gains a minute. |
| [`release.rs`](release.rs) | An object's leave of the world and its release by the action that held it (slot 0x180); the update's regeneration and hit point check, and a walk's pause. |
| [`relics.rs`](relics.rs) | A building's relic counts (`Relics`). |
| [`sprite.rs`](sprite.rs) | An object's active sprite: the frame of its sprite (a `.dat` graphic) and, for a graphic that animates, the clock that advances it. |
| [`sprite_flags.rs`](sprite_flags.rs) | A sprite's sequence flags (`SpriteFlags`, the `.dat`'s `sequence_type`). |
| [`sprite_id.rs`](sprite_id.rs) | A sprite's id (`SpriteId`): the `.dat`'s graphic a master or a task shows. |
| [`teardown.rs`](teardown.rs) | The action list's deletions (`FUN_140a98620`, `FUN_140a98e20`) and each action's destructor (its slot 0): what they leave the unit's world to apply. |
| [`work_rate.rs`](work_rate.rs) | A worker's work rate (`WorkRate`), with the game's `f32` operations. |
| [`lifecycle/`](lifecycle/) | What happens to an object once it is dead: the state steps of `RGE_Static_Object::update` (158041 `0x14118ad50`) and `RGE_Animated_Object::update` (`0x141071880`), up to the update that returns 1 and so takes the object off its player's list (`RGE_Object_List::update`, `0x141166550`). |
| [`masters/`](masters/) | What the engine reads of a player's masters: the tasks a unit can do and the masters a unit changes into to do them, and how an effect changes a master: the attribute setters (slots 0x18, 0x20 and 0x28 of every master type, `Master::set_attribute`), the master copy of an upgrade (slot 8, `Master::copy_from`) and the values the slice derives from a master (`Master::follow`). |
| [`unit/`](unit/) | What the slice tracks of one object (`Unit`) and its update, action by action. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`commands`](../commands/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`orders`](../orders/), [`ownership`](../ownership/), [`pathing`](../pathing/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`unit_ai`](../unit_ai/), [`world`](../world/).
- Used by: [`combat`](../combat/), [`commands`](../commands/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`map`](../map/), [`movement`](../movement/), [`orders`](../orders/), [`ownership`](../ownership/), [`pathing`](../pathing/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
<!-- /uses -->
