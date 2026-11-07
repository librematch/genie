# `players/world_player`: the player's own update

Every player has an update of its own, apart from its objects. The world runs it after the
player's groups and before the player's objects. Gaia's update only counts its time.

The other players' update runs a loop once for each whole second: the history, the explored
share, the kill ratio, the defeat check, and, while the player plays, its resource trickles and
its mills' food. After the objects, the player's scout map takes the tiles that its map explored.
A monastery's own update gives its player the goods of its relics through the same adds.

- `update.rs` holds the update and the per-second loop.
- `income.rs` holds the trickles, the mills' food, a monastery's relic goods and the attribute
  adds.
- `counts.rs` counts the town centres and the masters that the player can train or upgrade.
- `tech_tree.rs` holds the game's tech tree table.

## In the game

- `WorldPlayer::update` and `WorldPlayerGaia::update`, which both end in
  `WorldPlayerBase::update`.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`counts.rs`](counts.rs) | The counts the update reads: the town centres, the trainable and the upgradable masters, and where a master trains. |
| [`income.rs`](income.rs) | The income: the trickles, the mills' food, a monastery's relic goods, the attribute adds and their sharing, and the update's tail. |
| [`mod.rs`](mod.rs) | The world player's own update (`WorldPlayer::update`, `0x14163b100`, slot 0x100) and Gaia's (`WorldPlayerGaia::update`, `0x141681cd0`, its slot 0x100). |
| [`tech_tree.rs`](tech_tree.rs) | The game's tech tree table (`TechTree`, `FUN_140c7e080`) and its lookups. |
| [`tests.rs`](tests.rs) | Tests. |
| [`update.rs`](update.rs) | The world player's update: the loop's seconds, the explored share, the defeat check and what keeps a player in the game. |
| [`tech_tree/`](tech_tree/) | The tech tree's data. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../map/), [`math`](../../math/), [`objects`](../../objects/), [`technology`](../../technology/), [`world`](../../world/).
<!-- /uses -->
