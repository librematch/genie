# `technology/upgrade`: the damage sprites

A building shows another sprite as it loses hit points, and a resource object as its storage
empties. Its master lists these damage sprites, each with the share of damage it starts at. The
check finds the object's share (of its hit points, or of its storage for a resource object) and
shows the sprite of the share's entry (`damage.rs`). The static part runs the check around a
change of the hit points; an upgrade (`../upgrade.rs`) runs it with the new master.

## In the game

- `RGE_Static_Object`'s damage sprite check (`FUN_1411859a0`) and the storage share test
  (`FUN_141193590`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`damage.rs`](damage.rs) | The damage sprites' check (`FUN_1411859a0`), which the static part runs around the hit points' change and the upgrade around the master's: the share of an object's hit points, or of its storage, that picks its master's damage sprite. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`math`](../../math/), [`objects`](../../objects/), [`players`](../../players/), [`testing`](../../testing/), [`world`](../../world/).
<!-- /uses -->
