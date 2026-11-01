# `players/history`: the player's history

Each player keeps a history: a population entry each history period (30 game seconds) and a
list of events, newest first. The events are the ages reached, castles and wonders built or
destroyed, battles and the defeat. The AI's age time reads the age events back.

- `mod.rs` holds the history, its entry and event adders and its update once a game second,
  which finds the battles.
- `values.rs` holds the history's values: the populations, the time slices, the update count,
  the attributes' tallies and the share of the player's value.

## In the game

- `TRIBE_History_Info` (TribeHistoryInfo.cpp), the player's `+0xdb8`, saved with the player.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`mod.rs`](mod.rs) | The player's history (`TRIBE_History_Info`, TribeHistoryInfo.cpp; player `+0xdb8`, saved with the player): a population entry each history period and a list of events, newest first. |
| [`values.rs`](values.rs) | The history's values: an entry's kind and populations, a time slice, a count of the history's updates, an attribute's tally and a share of the player's value. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`math`](../../math/), [`world`](../../world/).
<!-- /uses -->
