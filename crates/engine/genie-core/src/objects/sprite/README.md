# `objects/sprite`: a sprite's definition

The sprites (`objects/sprite.rs`) advance an object's frame and say when its sprite ends. This
folder holds the part of it that is the game's data: a sprite's definition from the `.dat` and
the speed it gives a mover, which the game-data model takes along (FD-046).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`graphic.rs`](graphic.rs) | A sprite's definition and the speed it gives a mover (FD-046: the model's part of the sprites). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`movement`](../../movement/), [`world`](../../world/).
<!-- /uses -->
