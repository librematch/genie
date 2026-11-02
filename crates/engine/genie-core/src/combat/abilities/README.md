# `combat/abilities`: a combat object's passive abilities

After its combat update, every combat unit and building walks its master's task list and acts on
four kinds of task:

- Generation (task type 151): the object makes a resource over time for its player
  (`generation.rs`). The Feitoria and the Keshik have it, for example.
- Movement damage (type 152): a unit that moves hits the enemies around it on a timer
  (`movement_damage.rs`).
- Terrain look (type 159): the object takes another look, speed factor and attack rate while it
  stands on a given terrain (`looks_and_rewards.rs`).
- Removal reward (type 161): a dead object that leaves its list gives its player a resource
  (`looks_and_rewards.rs`).

The routine runs whatever tasks the masters hold. The `.dat` file of build 158041 has only
generation tasks, and the civilisations' bonuses add more.

## In the game

- The passive-ability routine that `TRIBE_Combat_Object::update` runs after
  `RGE_Combat_Object::update`.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`generation.rs`](generation.rs) | The generation tasks (action type 151): their test, what they make and the player's store. |
| [`looks_and_rewards.rs`](looks_and_rewards.rs) | The terrain look tasks (action type 159) and the removal reward tasks (161). |
| [`mod.rs`](mod.rs) | A combat object's passive abilities: `FUN_1412072f0`, which `TRIBE_Combat_Object::update` (`0x141208440`) runs on every combat unit and building (types 70 and 80) after `RGE_Combat_Object::update`, with what that one returned (the object leaves its list); a building's update (`TRIBE_Building_Object::update`) runs its production after it. |
| [`movement_damage.rs`](movement_damage.rs) | The movement damage tasks (action type 152): their timer and reach, the bystanders they strike and the hit. |
| [`tests.rs`](tests.rs) | Tests. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`construction`](../../construction/), [`gathering`](../../gathering/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`players`](../../players/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
