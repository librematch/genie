# `players/game_status`: who plays, who won, who lost

A player is playing, has won or has lost. RESIGN and the world's victory check after every tick
change that status. A defeated player's objects stop: villagers, fishing boats and explorers
stand still, buildings stop and give back their queues, and every unit AI drops its orders.

- `victory.rs` holds RESIGN, the status setter and the victory check.
- `stops.rs` stops a defeated player's objects.
- `cancels.rs` cancels a building's queue and refunds it. `UNIT_ORDER`'s cancels use it too.

## In the game

- The world's victory check (`TRIBE_World`) and the status setter of `WorldPlayer`.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`cancels.rs`](cancels.rs) | A building's queue cancelled: the whole queue, its units and researches refunded, and `UNIT_ORDER`'s cancel of one, five or some. |
| [`mod.rs`](mod.rs) | The players' game status (`+0x89`: playing, won, lost) and what changes it: RESIGN (`FUN_140d25af0`), the world's victory check after every update (`TRIBE_World` slot 0x68, `FUN_1415c9480`, whose tail `FUN_14152e3d0` decides who won), and the status setter (`WorldPlayer` slot 1, `FUN_141639860`) with a defeated player's stop of its objects (`FUN_141613ed0`): its villagers, fishing boats and explorers stop and stand, its buildings stop and give back their queues, and every unit AI's orders go. |
| [`stops.rs`](stops.rs) | A defeated player's objects stop (`FUN_141613ed0`): each object's stop and its top action's halt by kind. |
| [`tests.rs`](tests.rs) | Tests. |
| [`victory.rs`](victory.rs) | The game's end and the players' status: RESIGN, the status setter with its defeat event and the world's victory check. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`commands`](../../commands/), [`construction`](../../construction/), [`gathering`](../../gathering/), [`map`](../../map/), [`math`](../../math/), [`objects`](../../objects/), [`ownership`](../../ownership/), [`production`](../../production/), [`technology`](../../technology/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
