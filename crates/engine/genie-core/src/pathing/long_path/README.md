# `pathing/long_path`: the long pather

The long pather is a best-first search over the map's tiles, from a unit's tile to a goal tile
or to a target's footprint. The reachability test of a point runs it for a goal more than a tile
away, and it can write the path it found.

A tile's cost is the distance walked to it plus its squared distance to the goal, both in
quarter tiles; a change of direction costs 1 more. The open tiles wait in a binary heap by cost,
which the game sifts in its own way. The search ends at the goal, after too many expansions, or
when the heap runs dry; then the path runs to the tile that came nearest the goal.

## In the game

- `FUN_140f9eb70`, run on the global pather at `0x1439e6910`; the path's trace `FUN_140f9d770`.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`cost.rs`](cost.rs) | The long pather's costs (`LongCost`, in quarter tiles squared) and its open tiles' heap (`Heap`), sifted the game's way. |
| [`mod.rs`](mod.rs) | The long pather (`FUN_140f9eb70`, run on the global at `0x1439e6910`): a best-first search over the map's tiles from a unit's tile to a goal tile, which the reachability test of a point (`FUN_140fa0370`, `crate::movement::Mover::reaches`) runs for a goal more than a tile from the unit. |
| [`step_dir.rs`](step_dir.rs) | The long pather's step directions (`StepDir`, the eight tiles around one in the order the search tries them) and a set of them (`StepDirs`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`construction`](../../construction/), [`map`](../../map/), [`math`](../../math/), [`players`](../../players/), [`testing`](../../testing/), [`world`](../../world/).
<!-- /uses -->
