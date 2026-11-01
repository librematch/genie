# `players`: the players beside their objects

A player has more than its objects. This folder holds the rest:

- `game_status/` holds the game status (playing, won, lost), resignation and the victory check.
- `world_player/` holds the player's own update: the per-second economy, the trickles and the
  defeat check.
- `diplomacy.rs` holds a change of stance toward another player, and what follows from it.
- `tribute.rs` holds a tribute of resources to another player.
- `treason.rs` holds treason: the allies see the enemy kings.
- `market.rs` holds the market's prices and the trades.
- `history.rs` holds the player's history: the population over time and the events.
- `tracker.rs` holds the object tracker: the player's objects by master and class.

## In the game

- `WorldPlayer` and `WorldPlayerGaia` (`player.cpp`, `tplayer.cpp`).
- `TRIBE_History_Info` (`t_histry.cpp`).
- The victory conditions (`victory.cpp`, `tvictory.cpp`, `VictoryConditions.cpp`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`amount.rs`](amount.rs) | An amount of a player attribute (`Amount`), with the game's `f32` operations. |
| [`amount_rate.rs`](amount_rate.rs) | How fast an amount changes (`AmountRate`, an attribute's amount per second), with the game's `f32` operations. |
| [`attribute.rs`](attribute.rs) | The indices of the player's attributes (`player+0x8`'s array of `f32`): the one definition of the attributes the engine reads and writes by number. |
| [`by_player.rs`](by_player.rs) | One value per player, looked up by the player (`ByPlayer`). |
| [`civ_id.rs`](civ_id.rs) | A civilisation's number. |
| [`diplomacy.rs`](diplomacy.rs) | A player's diplomacy change (the world player's slot 0x30, `FUN_141639690`, which GAME's sub-command 0 runs): its stance toward another player (`diplomacy[i]`, `+0xe12`), the relation it makes (`relation[i]`, `+0x260`) and what its queries make of that player's objects (`unit_diplomacy[i]`, `+0x278`); its objects hear of it (slot 0x120: units inside a building or siege tower of a player no longer allied leave it); the players' sight masks and tile lists are made again; then the world's teams (`FUN_1415e23f0`) and the units the players reveal (`FUN_14152bcb0`). |
| [`market.rs`](market.rs) | The market: the world's commodity prices (`+0x868`..`+0x8a0`, saved) and the two commands that trade at them, `TRIBE_COMMAND_SELL_COMMODITY` (0x7a, `FUN_141639310`) and `TRIBE_COMMAND_BUY_COMMODITY` (0x7b, `FUN_141618390`). |
| [`mod.rs`](mod.rs) | The players beside their objects: their game status (playing, won, lost), diplomacy, tribute, treason, the market, their history, their object trackers, and the world player's own update. |
| [`player_id.rs`](player_id.rs) | A player's number as one type: the player a command comes from or names, and the index of the world's players. |
| [`player_set.rs`](player_set.rs) | A set of players, one bit each (`PlayerSet`). |
| [`player_type.rs`](player_type.rs) | A player's type as one type: `player+0x10`, the save's class byte. |
| [`query_relation.rs`](query_relation.rs) | A search's relation number: which players' objects `ClosestObject` asks for or passes over. |
| [`relation.rs`](relation.rs) | How a player stands to another as one type (`Relation`), and a player's row of them (`Relations`, `relation[i]`, `player+0x260`). |
| [`stance.rs`](stance.rs) | A player's stance toward another as one type: `diplomacy[i]` (`+0xe12`). |
| [`tracker.rs`](tracker.rs) | The player's object tracker (`player+0x18`, the "Object Tracker"): lists of the player's object ids by key, in the order the ids came in. |
| [`treason.rs`](treason.rs) | Treason (GAME's sub-command 0xa, the old Spies): a player with a castle pays gold to have its allies see the kings of the players it is not allied with, a flare at each king for each of them. |
| [`tribute.rs`](tribute.rs) | A player's tribute: `TRIBE_COMMAND_GIVE_ATTRIBUTES` (0xc4, `TRIBE_Command::do_command`'s case, `FUN_140e7d040`). |
| [`unit_relation.rs`](unit_relation.rs) | What a player's queries make of another player's objects as one type, and the tables by it. |
| [`ai_module/`](ai_module/) | A computer player's AI module (`player+0x7a8`, `TribeMainDecisionAIModule`, 0x28fb8 bytes). |
| [`game_status/`](game_status/) | The players' game status (`+0x89`: playing, won, lost) and what changes it: RESIGN (`FUN_140d25af0`), the world's victory check after every update (`TRIBE_World` slot 0x68, `FUN_1415c9480`, whose tail `FUN_14152e3d0` decides who won), and the status setter (`WorldPlayer` slot 1, `FUN_141639860`) with a defeated player's stop of its objects (`FUN_141613ed0`): its villagers, fishing boats and explorers stop and stand, its buildings stop and give back their queues, and every unit AI's orders go. |
| [`history/`](history/) | The player's history (`TRIBE_History_Info`, TribeHistoryInfo.cpp; player `+0xdb8`, saved with the player): a population entry each history period and a list of events, newest first. |
| [`world_player/`](world_player/) | The world player's own update (`WorldPlayer::update`, `0x14163b100`, slot 0x100) and Gaia's (`WorldPlayerGaia::update`, `0x141681cd0`, its slot 0x100). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`ownership`](../ownership/), [`production`](../production/), [`technology`](../technology/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
- Used by: [`combat`](../combat/), [`commands`](../commands/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`map`](../map/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`pathing`](../pathing/), [`production`](../production/), [`technology`](../technology/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
<!-- /uses -->
