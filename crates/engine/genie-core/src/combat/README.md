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
- `powerup.rs` holds an object's powerups: what an aura gives it, how long it holds, and its
  reset. `aura/` holds the auras that give them and the powerup update that runs them.

## How the tick reaches it

An object's update (`objects/unit/update.rs`) runs down the attack timer. If the Attack is the
object's first action, the update runs it. A missile is an object of its own, and its Missile action flies
it. After the combat update, every combat unit and building runs its passive abilities, then
its powerup update (`aura/`).

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
| [`ability_timer.rs`](ability_timer.rs) | A combat object's timed ability: how long it lasts and when it began. |
| [`attack_bonus.rs`](attack_bonus.rs) | What an object's powerup or stingers add to its attack (`AttackBonus`). |
| [`attack_target.rs`](attack_target.rs) | An Attack's target as its own type: the object it attacks. |
| [`class_or_master.rs`](class_or_master.rs) | The object a combat task names (`ClassOrMaster`): a class or a master, as slot 277 keeps it in the aura groups and the stinger tasks. |
| [`combat_update.rs`](combat_update.rs) | `TRIBE_Combat_Object::update` (`0x141208440`), a combat object's or building's own update (types 70 and 80) around `RGE_Combat_Object::update`. |
| [`kill_counts.rs`](kill_counts.rs) | A kill's counts in its players' attributes (`World::count_kills`): slot 22's credits of its burns, the damage's counts for its owner and `FUN_1412005b0`'s for a killer. |
| [`kill_state.rs`](kill_state.rs) | Whether an object's kill is counted. |
| [`lockdown.rs`](lockdown.rs) | How often an object is locked down. |
| [`mod.rs`](mod.rs) | Combat: attacks, hits and missiles. |
| [`powerup.rs`](powerup.rs) | An object's powerups (`+0xa24`..`+0xa54`, `+0xa98`, `+0xad4`): an aura's powerup (`World::apply_powerup`, `FUN_141200dd0`) and their reset (`World::reset_powerups`, `FUN_1412066b0`). |
| [`reload.rs`](reload.rs) | An object's reload time (slot 0x218, `FUN_141086c90`): the time between its attacks, from its master's and its own values, which technologies and effects change. |
| [`special_sprite.rs`](special_sprite.rs) | Whether a combat object shows its master's special sprite. |
| [`victim_reads.rs`](victim_reads.rs) | What a hit reads of its victim: the object's values and its relations to the attacker. |
| [`victims.rs`](victims.rs) | A hit's victim: what the hit reads of it, the objects it may reach, the hit written back, the kill and its counts. |
| [`volley.rs`](volley.rs) | A unit's strikes back and its volleys: the missiles it fires and launches. |
| [`abilities/`](abilities/) | A combat object's passive abilities: `FUN_1412072f0`, which `TRIBE_Combat_Object::update` (`0x141208440`) runs on every combat unit and building (types 70 and 80) after `RGE_Combat_Object::update`, with what that one returned (the object leaves its list); a building's update (`TRIBE_Building_Object::update`) runs its production after it. |
| [`attack/`](attack/) | The Attack action (`RGE_Action_Attack`, type 9, vtable `0x142ed47a8`) of a melee unit: a hunting villager's sub-action on a tree or on its livestock (`crate::gathering::hunt`). |
| [`aura/`](aura/) | An object's auras: its master's tasks of type 155 (`ActionKind::AURA`) in the groups that slot 277 builds of them (`TRIBE_Combat_Object::slot277`, `0x141206d50`), which the powerup update (`FUN_141204a10`, `World::powerup_update`) reads. |
| [`combat_update/`](combat_update/) | The combat update's tests. |
| [`damage/`](damage/) | The damage side of a hit: what it reads of its target and its attacker and what it changes. |
| [`missile/`](missile/) | The missile (`RGE_Missile_Object`, type 60) and its action (`RGE_Action_Missile`, type 8): a ranged attacker's swing makes one at its weapon offset (`FUN_141081020`, `spawn_point`), its action aims at the target (`FUN_140a9c600`, `aim`; `set_state` 3, `FUN_140a9de70`, `MissileAction::launch`) and flies it each update (`FUN_140a9e6e0`, `MissileAction::fly`) until it hits the target (the shooter's hit, slot 0x860) or the ground. |
| [`powerup/`](powerup/) | The powerups' tests. |
| [`stingers/`](stingers/) | An object's stingers (`mActiveStingers`, `+0xb58`): what its attackers' hits hand it (`hand_off`, `FUN_14107bdf0`) from their masters' stinger tasks (`stinger_tasks`), and the sums the stinger update rebuilds from them in each update (`World::object_stingers`, `FUN_1412092e0`). |
| [`volley/`](volley/) | The missile launch. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`construction`](../construction/), [`gathering`](../gathering/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`players`](../players/), [`technology`](../technology/), [`testing`](../testing/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
- Used by: [`commands`](../commands/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`production`](../production/), [`technology`](../technology/), [`testing`](../testing/), [`unit_ai`](../unit_ai/), [`world`](../world/).
<!-- /uses -->
