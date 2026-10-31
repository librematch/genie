# `combat`: attacks, hits and missiles

Combat is what happens between an attacker and its target. This folder holds every part of it:

- `attack/` holds the Attack action: the walk to the target, the swing and the reload.
- `damage/` computes what a hit does: the armor, the attack bonuses, the charges, the blasts, and
  the hit points that the target loses.
- `missile/` holds the missiles: where a ranged attacker's missile starts, how it aims and flies,
  and what it hits.
- `volley.rs` holds a unit's volleys and its strikes back: the missiles it fires and launches.
- `victims.rs` holds a hit's victim: what the hit reads of it, the objects it can reach, the kill
  and its counts.
- `abilities/` holds a combat object's passive abilities: resources it generates, damage it does
  while it moves, the look it takes on a terrain, and the reward for its removal.

## How the tick reaches it

An object's update (`objects/unit/update.rs`) runs down the attack timer. If the Attack is the
object's first action, the update runs it. A missile is an object of its own, and its Missile action flies
it. After the combat update, every combat unit and building runs its passive abilities.

## In the game

- `RGE_Action_Attack` (action type 9, `act_atak.cpp`).
- `RGE_Missile_Object` (object type 60, `misl_obj.cpp`) and `RGE_Action_Missile` (action type 8,
  `act_misl.cpp`).
- `RGE_Combat_Object` and `TRIBE_Combat_Object` (`t_c_obj.cpp`), and the combat master
  (`TribeMasterCombatObject.cpp`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`charge.rs`](charge.rs) | An object's charge (`Charge`) and how fast it fills (`ChargeRate`), with the game's `f32` operations. |
| [`mod.rs`](mod.rs) | Combat: attacks, hits and missiles. |
| [`victim_reads.rs`](victim_reads.rs) | What a hit reads of its victim: the object's values and its relations to the attacker. |
| [`victims.rs`](victims.rs) | A hit's victim: what the hit reads of it, the objects it may reach, the hit written back, the kill and its counts. |
| [`volley.rs`](volley.rs) | A unit's strikes back and its volleys: the missiles it fires and launches. |
| [`abilities/`](abilities/) | A combat object's passive abilities: `FUN_1412072f0`, which `TRIBE_Combat_Object::update` (`0x141208440`) runs on every combat unit and building (types 70 and 80) after `RGE_Combat_Object::update`, with what that one returned (the object leaves its list); a building's update (`TRIBE_Building_Object::update`) runs its production after it. |
| [`attack/`](attack/) | The Attack action (`RGE_Action_Attack`, type 9, vtable `0x142ed47a8`) of a melee unit: a hunting villager's sub-action on a tree or on its livestock (`crate::gathering::hunt`). |
| [`damage/`](damage/) | The damage side of a hit: what it reads of its target and its attacker and what it changes. |
| [`missile/`](missile/) | The missile (`RGE_Missile_Object`, type 60) and its action (`RGE_Action_Missile`, type 8): a ranged attacker's swing makes one at its weapon offset (`FUN_141081020`, `spawn_point`), its action aims at the target (`FUN_140a9c600`, `aim`; `set_state` 3, `FUN_140a9de70`, `MissileAction::launch`) and flies it each update (`FUN_140a9e6e0`, `MissileAction::fly`) until it hits the target (the shooter's hit, slot 0x860) or the ground. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`construction`](../construction/), [`gathering`](../gathering/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`players`](../players/), [`production`](../production/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
- Used by: [`commands`](../commands/), [`construction`](../construction/), [`gathering`](../gathering/), [`groups`](../groups/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`unit_ai`](../unit_ai/), [`world`](../world/).
<!-- /uses -->
