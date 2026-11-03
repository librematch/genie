# `technology/effect`: a research's completion and its effects

When a research completes, its effect runs. An effect is a list of commands, and each command
has a kind: change an attribute of a master, change a resource, upgrade a master, enable or
disable a technology, spawn an object, change a cost.

- `completion.rs` holds the completion: the effect, the technologies that depend on it, and the
  disable and enable commands.
- `commands.rs` runs each command by its kind, for every party it reaches.
- `attribute_modifier.rs` changes the masters and what their objects follow of the change.
- `object_slots.rs` changes an existing object by an attribute.
- `resources.rs` changes the player's resources and attributes.
- `tech_change.rs` changes a technology's state.
- `upgrade.rs` upgrades a master and its objects.

## In the game

- `TRIBE_Effects::do_effect` (`teffects.cpp`) and the player's attribute modifier.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`attribute_modifier.rs`](attribute_modifier.rs) | The player's attribute modifier (slot 0x148, `FUN_1416121b0`): the hooks around it, the masters' change and what their objects and an Attack follow of it. |
| [`commands.rs`](commands.rs) | The effects' `do_effect` (`FUN_141462440`, then the base `FUN_140ec2ac0`): each command by its kind for every party (`Party`). |
| [`completion.rs`](completion.rs) | A research's completion (`FUN_1413cdcd0`): its effect, the technologies that depend on it, a built building's technologies, the cost modifier and the disable and enable commands. |
| [`mod.rs`](mod.rs) | A research's completion and the effects it gives: `FUN_1413cdcd0` (the technology done, its effect, the technologies that depend on it, the player's statistics), the effects' `TRIBE_Effects::do_effect` (`FUN_141462440`, then the base `FUN_140ec2ac0`) with every kind for every party, and what they reach: the player's attribute modifier (slots 0x150, 0x158, 0x160 -> slot 0x148, `FUN_1416121b0`: the hooks `FUN_141614f40` around `FUN_14165a000`, the objects' slots 0x78, 0x80, 0x88 and the masters' 0x18, 0x20, 0x28 of `crate::objects::masters`), the resource commands (slots 0x1b0, 0x1b8, the player's attribute set and add), the technology changes (`'e'`, `'f'`, `'g'`, kind 8), the enables, upgrades, spawns and cost entries. |
| [`object_slots.rs`](object_slots.rs) | An object's change by an attribute (its slots 0x78, 0x80 and 0x88): the static object's, the combat unit's and the building's. |
| [`resources.rs`](resources.rs) | The player's resources and attributes: the modify and multiply slots, the store, the setters and the side effects of allied vision and the farms' food. |
| [`tech_change.rs`](tech_change.rs) | A technology's change (kind 8): its state, a forced research and its research time. |
| [`tests.rs`](tests.rs) | Tests. |
| [`upgrade.rs`](upgrade.rs) | A master's upgrade (slot 0x1a0) and its objects, the objects an effect spawns, the objects' kinds and a negative cost's entries. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`construction`](../../construction/), [`garrison`](../../garrison/), [`map`](../../map/), [`math`](../../math/), [`objects`](../../objects/), [`ownership`](../../ownership/), [`players`](../../players/), [`production`](../../production/), [`vision`](../../vision/), [`world`](../../world/).
<!-- /uses -->
