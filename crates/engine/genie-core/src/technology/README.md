# `technology`: technologies and upgrades

A player researches technologies. A technology costs resources and time, and its effect changes
the player's masters, resources or other technologies.

- `mod.rs` holds the technologies of the `.dat` file and each player's state of each: the costs,
  the research test and the payment.
- `effect/` holds a research's completion and the effects it gives.
- `upgrade.rs` holds an object's upgrade to another master, as the upgrade of its master runs it.

## In the game

- `TRIBE_Player_Tech` (`tech.cpp`, `techtree.cpp`).
- `TRIBE_Effects` (`effects.cpp`, `teffects.cpp`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`command_args.rs`](command_args.rs) | The arguments of the technology commands: how a cost or a research time changes (`ChangeMode`), what kind 8 changes (`ModifyMode`) and at which location (`ModifyLocation`), and the technology a disable names (`DisabledTech`). |
| [`effect_command.rs`](effect_command.rs) | An effect's commands by kind: the `.dat`'s 12 bytes each (a kind and four arguments) as `TRIBE_Effects::do_effect` (`FUN_141462440`) and its base (`FUN_140ec2ac0`) read them, parsed once where the `.dat` is read. |
| [`mod.rs`](mod.rs) | Technologies and upgrades: the players' technologies, a research's effects and an object's upgrade to another master. |
| [`research_state.rs`](research_state.rs) | A player's technology's state: whether it may be researched, is being researched or is researched. |
| [`tech_id.rs`](tech_id.rs) | A technology's id as one type: the place of a technology in the tech tree and in a player's technologies, the `.dat`'s technology record index. |
| [`tech_type.rs`](tech_type.rs) | A technology's type as one type: its record's `+0x3c`, the `.dat`'s `type`. |
| [`upgrade.rs`](upgrade.rs) | An object's upgrade to another master: slot 0x98 (`FUN_141201a60` for a combat object, `FUN_1411c4660` for a building), down the classes' overrides (`FUN_14107cb70`, `FUN_14106a530`, `FUN_14114cb80`) to the static object's `FUN_141186580`, as the upgrade of its master runs it (`FUN_141657d10`, `World::upgrade_objects`); and the damage sprites' check (`FUN_1411859a0`), which the static part runs around the hit points' change. |
| [`effect/`](effect/) | A research's completion and the effects it gives: `FUN_1413cdcd0` (the technology done, its effect, the technologies that depend on it, the player's statistics), the effects' `TRIBE_Effects::do_effect` (`FUN_141462440`, then the base `FUN_140ec2ac0`) with every kind for every party, and what they reach: the player's attribute modifier (slots 0x150, 0x158, 0x160 -> slot 0x148, `FUN_1416121b0`: the hooks `FUN_141614f40` around `FUN_14165a000`, the objects' slots 0x78, 0x80, 0x88 and the masters' 0x18, 0x20, 0x28 of `crate::objects::masters`), the resource commands (slots 0x1b0, 0x1b8, the player's attribute set and add), the technology changes (`'e'`, `'f'`, `'g'`, kind 8), the enables, upgrades, spawns and cost entries. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`players`](../players/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
- Used by: [`commands`](../commands/), [`construction`](../construction/), [`gathering`](../gathering/), [`objects`](../objects/), [`orders`](../orders/), [`players`](../players/), [`production`](../production/), [`world`](../world/).
<!-- /uses -->
