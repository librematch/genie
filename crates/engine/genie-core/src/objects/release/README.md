# `objects/release`: the release's tests

The tests of an object's release and leave (`../release.rs`). They cover slot 0x180 by master
type (the static object's sleep, the combat object's notify 699, a building's attacking units),
the leave of the world, the update's regeneration and hit point powerup check, and a walk's
pause.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`tests.rs`](tests.rs) | Tests: A target's release by the action that held it, an object's leave of the world, the update's regeneration and hit point check, and a walk's pause. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: no other system.
<!-- /uses -->
