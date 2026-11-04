# `ownership`: objects that change hands

An object can change its owner. A unit of another player takes over livestock, a monk converts a
unit, a player loses a building with its garrison.

- `artifact.rs` holds the Artifact action and its part in the animal's update. It is the default
  task of livestock: it checks whether a unit of another player is close enough to take the
  animal over.
- `capture.rs` holds captures and takeovers: by sight, by the unit AI, and the search for more
  livestock after a capture.
- `change.rs` holds the ownership change itself, with what it moves along: a building's
  foundation, annexes and garrison, and the attributes and storage it counts.

## In the game

- `TRIBE_Action_Artifact` (action type 107, `tact_art.cpp`), `tact_cap.cpp`, `tact_cnv.cpp`.
- The object's ownership change (slot 0x70).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`artifact.rs`](artifact.rs) | The Artifact action (type 107): the default task of livestock, which keeps checking whether a unit of another player is close enough to take the animal over (the search is the caller's, `crate::map::spatial::Spatial::artifact_owner`). |
| [`artifact_state.rs`](artifact_state.rs) | An Artifact's state: checking for a new owner and waiting between checks. |
| [`artifact_sub.rs`](artifact_sub.rs) | An Artifact's sub-action: one at most, a move's `MoveTo` or an enter command's Enter. |
| [`capture.rs`](capture.rs) | Captures and takeovers: an animal or artifact a player takes over, its stop, the unit AI's ownership change, a takeover by sight, a unit's ownership change, and the scan for more livestock after a capture. |
| [`capture_kind.rs`](capture_kind.rs) | How another player takes an object over. |
| [`change.rs`](change.rs) | An ownership change (slot 0x70, `FUN_14120f800`): to Gaia, a building's with its foundation, annexes and garrison, and the attributes and storage it moves. |
| [`mod.rs`](mod.rs) | Objects that change hands: the Artifact action of livestock, which a unit of another player takes over, and an ownership change with what it carries along (a building's annexes, its garrison, the attributes it counts). |
| [`tests.rs`](tests.rs) | Tests. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`garrison`](../garrison/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`players`](../players/), [`testing`](../testing/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
- Used by: [`commands`](../commands/), [`construction`](../construction/), [`garrison`](../garrison/), [`map`](../map/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`testing`](../testing/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
<!-- /uses -->
