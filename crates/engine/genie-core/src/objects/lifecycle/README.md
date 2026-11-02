# `objects/lifecycle`: an object's life and death

An object's state goes from alive to dying to dead to gone. This folder holds the state steps of
that way, as the game's static and animated objects run them.

The states are the save's `object_state`: 2 alive, 3 dying, 4 playing its dying sprite, 5 dead,
7 its resource going back, 8 gone. The update that returns 1 takes the object off its player's
list.

`death_parts.rs` holds the parts of a death: a farm's bonus, the combat object's refund, a
building's storage, the military and civilian counts.

## In the game

- `RGE_Static_Object::update` and `RGE_Animated_Object::update`: the state steps.
- `RGE_Object_List::update`: the list that removes an object whose update returns 1.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`death_parts.rs`](death_parts.rs) | A death's parts: a farm's bonus, the visible resources forgotten, the combat object's death and refund, a built building's death and storage, and the military and civilian counts. |
| [`master_data.rs`](master_data.rs) | What an object's life reads of its master (FD-046: the model's part of the lifecycle). |
| [`mod.rs`](mod.rs) | What happens to an object once it is dead: the state steps of `RGE_Static_Object::update` (158041 `0x14118ad50`) and `RGE_Animated_Object::update` (`0x141071880`), up to the update that returns 1 and so takes the object off its player's list (`RGE_Object_List::update`, `0x141166550`). |
| [`object_state.rs`](object_state.rs) | An object's state: where it stands between its placing and its leaving. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`construction`](../../construction/), [`garrison`](../../garrison/), [`map`](../../map/), [`math`](../../math/), [`pathing`](../../pathing/), [`players`](../../players/), [`world`](../../world/).
<!-- /uses -->
