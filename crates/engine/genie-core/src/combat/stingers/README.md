# `combat/stingers`: what a hit leaves on its target

A stinger is a timed change a hit leaves on an object: a burn that takes hit points per minute,
a slow that takes speed. A master's tasks of type 157 describe it: each task names a target (a
class plus 900, or a master), what it changes, its value, how long it lasts, and whether it goes
to the target or to the attacker.

- `mod.rs` holds the types: a master's stinger tasks, an active stinger and its saved form.
- `hand_off.rs` holds the hand-off (`FUN_14107bdf0`): a hit hands each stinger task that names
  its target to the target, or to the attacker itself.
- `update.rs` holds the stinger update (`FUN_1412092e0`). It zeroes an object's stinger sums,
  then rebuilds them from its active stingers and drops the ended ones.
- `credits.rs` holds slot 22's kill credits (`FUN_141205c80`). An object that dies with
  stingers counts its kill for the players of its burns' attackers, each once, and marks its
  kill counted (`+0x954`).

## How the tick reaches it

A hit on a combat unit (`take_unit`) or a building (`take_building`) runs the hand-off when the
target keeps at least 1 hit point; the hit's write-back adds the stingers at the world time. An
object's full update (`world/after_step.rs`) runs the stinger update of every combat unit and
building after its powerup update. A loss below 1 hit point (slot 22: a hit's `lose`, the
regeneration's `Unit::lose_hp`) runs the credits; the hit's write-back or the regeneration
counts them in the world.

## In the game

- `FUN_14107bdf0`, the hand-off, and `FUN_141200830`, the push onto `mActiveStingers` (`+0xb58`).
- `FUN_14107ddb0` (slot 277) keeps a master's stinger tasks in `+0x938`; genie derives them from
  the master where the hand-off reads them.
- `FUN_1412092e0`, the stinger update of `TRIBE_Combat_Object`.
- `FUN_141205c80`, slot 22, the loss; its credits gather the attackers and their players in two
  hash sets keyed by address, whose order no run repeats and nothing depends on.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`credits.rs`](credits.rs) | Slot 22's kill credits (`FUN_141205c80` below 1 hit point): an object that dies with stingers counts its kill for the players of the attackers whose stingers burn it. |
| [`hand_off.rs`](hand_off.rs) | A hit's stingers (`FUN_14107bdf0`): the attacker's stinger tasks that name its target, handed to the target or to the attacker. |
| [`mod.rs`](mod.rs) | An object's stingers (`mActiveStingers`, `+0xb58`): what its attackers' hits hand it (`hand_off`, `FUN_14107bdf0`) from their masters' stinger tasks (`stinger_tasks`), and the sums the stinger update rebuilds from them in each update (`World::object_stingers`, `FUN_1412092e0`). |
| [`tests.rs`](tests.rs) | Tests: An object's stingers: its master's stinger tasks, a hit's hand-off, the save's entries and the stinger update. |
| [`update.rs`](update.rs) | The stinger update (`FUN_1412092e0`): the sums `+0xb38`..`+0xb53` rebuilt from the object's active stingers in each update. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`players`](../../players/), [`world`](../../world/).
<!-- /uses -->
