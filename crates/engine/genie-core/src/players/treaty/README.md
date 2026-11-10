# `players/treaty`: the world's treaty monitor

A treaty keeps the players from fighting for some minutes. While it holds, the players who would
be enemies stand in relation 4, and each player's town centres have a no-build zone about them.
The world holds one monitor for it, which the save does not hold: the load makes it from the
save's treaty byte.

- `mod.rs` holds the monitor: its clock, its state, the zones, and the world's two queries.
- `update.rs` holds the starts, the end and the update with its countdown, the world update's
  step that runs it, and the load's branch that makes it.
- `zones.rs` holds the zones' walk over the town centres, a town centre's sweep of the
  foundations about it, the no-build test, and the clear of a player's first zone when its town
  centre foundation goes.

The zones need the game's screen. genie plays as a game client: the load draws none, and every
update while the treaty holds draws those still missing.

Other systems read the monitor as it stands. The ally test and the sharing of resources count
a neutral allied while the treaty holds. A diplomacy change to an enemy makes relation 4 while
it holds. The delete of a town centre foundation clears its owner's first zone. RESEARCH of
Castle Age in game mode 16 marks the monitor, and its next update ends the treaty.

## In the game

- `TreatyMode` (HD's names): its constructor, `StartTreaty`, `EndTreaty`, `Update`,
  `AttemptToDrawBorders` and `IsWithinNoBuildZone`.
- `World::load_world`'s treaty branch and `World::update`'s treaty step.
- `FUN_1414615d0`, a player's zone clear, which DE adds.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`mod.rs`](mod.rs) | The world's treaty monitor (`world+0x820`, HD's `TreatyMode`, 0x160 bytes): a treaty's countdown, and the no-build zones about the players' town centres while it holds. |
| [`tests.rs`](tests.rs) | Tests. |
| [`update.rs`](update.rs) | The treaty monitor's starts, its end and its update (`FUN_141461620`, `FUN_141461710`, `FUN_141461310`, `FUN_1414618a0`), the world update's step that runs it, and the load's branch that makes it. |
| [`zones.rs`](zones.rs) | The treaty monitor's zones: their walk over the players' town centres (`FUN_141461c00`), a town centre's sweep of the foundations about it (`FUN_1411c7ae0`), the no-build test (`FUN_141461400`) and the clear of a player's first zone (`FUN_1414615d0`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`construction`](../../construction/), [`map`](../../map/), [`math`](../../math/), [`objects`](../../objects/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
