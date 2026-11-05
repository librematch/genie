# `movement/motion_bound`: the motion bound's tests

The tests of genie's bound on how far a unit may move and act (`../motion_bound.rs`, no routine
of the game): the frozen check reads it for what a stopped player's unit may have done since it
froze. They build masters, auras and stingers by hand over the test build's model.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`tests.rs`](tests.rs) | Tests: The bound of a unit's speed and reach: the largest sprite factor and speed-up, each aura source once, a stinger that raises the speed, a siege unit's infantry, a charger's run, the range auras, and a speed that is not a number. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: no other system.
<!-- /uses -->
