# `orders/work/retarget`: the work command's retake's tests

The tests of the work command's notify once the action on top took the new target
(`../retarget.rs`). They cover the order and action of the action on top, the last retarget
time that an Attack sets, a unit without a unit AI and an empty action list.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`tests.rs`](tests.rs) | Tests: The work command's notify once the action on top took the new target (`World::notify_retaken`): the order and action of the action on top, an Attack's retarget time, a unit without a unit AI and an empty action list. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: no other system.
<!-- /uses -->
