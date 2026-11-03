# `movement/step`

The parts of the motion step (`../step.rs`) that end a walk. `Mover::walk_ends` checks a walk
before it steers: a walk to a target in range of it counts the waypoint reached, and a walk to
a point with a range ends where its top path ends. `Mover::loop_end` ends the motion step of a
looping path that is back at its first waypoint: the unit steers, controls its speed and stops
while its level waits for a path.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`ends.rs`](ends.rs) | The walk's ends before its steer and the motion step's end on a loop. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../map/), [`math`](../../math/), [`world`](../../world/).
<!-- /uses -->
