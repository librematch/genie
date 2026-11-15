# `construction/unpack`: the Unpack action

A packed trebuchet (class 51) unpacks into its master's transform through an Unpack on its
action list. It walks to its target or its point (state 11, its quiet `MoveTo`) until it is
within its range, or until the walk failed: then it unpacks where it stands, and fails out of
its target's reach. Its `set_state(6)` takes it off its list, turns the building into its
master's transform (the list deleted with it), puts it back at the head and shows the new
master's construction sprite. At work (state 6) its timer runs up from the unit's pack progress
toward the new master's unpack time, the train time of its first train location, by the
building's work rate in every update, and its sprite's frame follows the timer. Once the timer
reached the unpack time, the building takes its standing sprite and its annexes, and its unit
AI takes the attack order on the Unpack's target first. A stop ends the Unpack and keeps its
share done for the next Pack or Unpack. The destructor turns the building back in a played game
at work.

- `mod.rs` holds the action's state, status, timer, range, point and queue id, its reach, its
  work, its frame, its stop and its progress.
- `unit.rs` binds it to the building and the world: the update with its walk, the transform at
  work, the end with its attack order, and the destructor's part.

The load is the Unpack's one maker here. Its constructor, its start with 1 and its installs are
the task unpack-install-sites', its `set_state(11)` the task unpack-set-moving's.

## In the game

- `TRIBE_Action_Unpack` (action type 0x7d, vtable `0x142ed81a8`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`mod.rs`](mod.rs) | A building's Unpack: `TRIBE_Action_Unpack` (action type 0x7d, vtable `0x142ed81a8`), which a packed trebuchet (class 51) and `UNIT_ORDER` install on a building. |
| [`tests.rs`](tests.rs) | Tests: The Unpack's tests on player 1's packed trebuchet 700 (masters 331 and 42) and player 2's villager 800. |
| [`unit.rs`](unit.rs) | An Unpack bound to its building and the world: its update with its walk and its reach, the building's transform at work (`set_state(6)`), its work rate, its sprite's frame, its standing sprite, its annexes and its unit AI's attack, and its destructor's transform back. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`commands`](../../commands/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`orders`](../../orders/), [`players`](../../players/), [`world`](../../world/).
<!-- /uses -->
