# `objects/masters`: the definitions of the objects

A master is the definition of one kind of object for one player: its hit points, speed, armor,
attacks, costs, sprites and tasks. Each player has its own copy of every master. An effect
changes the copies of the players that it reaches.

The values are game data. They come from the player's masters in the save, and from the `.dat`
file for what the save does not hold.

- `mod.rs` holds the `Master` type and its tasks.
- `lookup.rs` finds a player's master by id, and the master and task that a work command takes.
- An effect changes a master through the attribute setters. The game has one setter per master
  type, and `dispatch.rs` picks it:
  - `static_setters.rs`: the static, animated, moving and action masters.
  - `combat_setters.rs`: the combat and missile masters (`RGE_`).
  - `tribe_setters.rs`: the combat and building masters of Age of Empires II's layer
    (`TRIBE_`).
- `derived.rs` updates the values that follow from a changed master.
- `attribute.rs` names the attributes that an effect changes.

## In the game

- The master classes (`MasterStaticObject.cpp` and the classes above it), with their attribute
  setters and their copy for an upgrade.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`class_mask.rs`](class_mask.rs) | A set of classes as the game tests one: a bit mask over the classes below a bound. |
| [`derived.rs`](derived.rs) | The values the slice derives from a master after a change (`follow`): the sprites' speed and facets, a moving unit's values. |
| [`lookup.rs`](lookup.rs) | A player's masters by id (`Masters`) and the work command's choice of master and task. |
| [`mod.rs`](mod.rs) | What the engine reads of a player's masters: the tasks a unit can do and the masters a unit changes into to do them, and how an effect changes a master: the attribute setters (slots 0x18, 0x20 and 0x28 of every master type, `Master::set_attribute`), the master copy of an upgrade (slot 8, `Master::copy_from`) and the values the slice derives from a master (`follow`). |
| [`tests.rs`](tests.rs) | Tests. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`construction`](../../construction/), [`map`](../../map/), [`movement`](../../movement/), [`players`](../../players/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
