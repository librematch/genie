# `combat/aura`: auras and the powerup update

An aura powers up the objects around its owner. A master's tasks of type 155 describe it: each
task names a target (a class plus 900, or a master), an area, a powerup and the most objects that
count.

- `mod.rs` builds the aura groups of a master: tasks of one area form a group, tasks of one
  powerup a part. The groups keep the order of the game's hash maps (`math::msvc`).
- `update.rs` holds the powerup update (`FUN_141204a10`). It ends an object's powerups once their
  hold has passed. Then each group lists the objects near the owner, counts them, and gives each
  part's amount to the powered ones or to the owner itself.

## How the tick reaches it

An object's full update (`world/after_step.rs`) runs the powerup update of every combat unit
and building after its passive abilities.

## In the game

- `FUN_141204a10`, the powerup update of `TRIBE_Combat_Object`.
- Slot 277 (`0x141206d50`) builds the groups into `+0xa58` and `+0xaa8`; genie derives them
  from the master where they are read.
- `FUN_141288b80` is the area's query.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`mod.rs`](mod.rs) | An object's auras: its master's tasks of type 155 (`ActionKind::AURA`) in the groups that slot 277 builds of them (`TRIBE_Combat_Object::slot277`, `0x141206d50`), which the powerup update (`FUN_141204a10`, `World::powerup_update`) reads. |
| [`tests.rs`](tests.rs) | Tests: The aura groups and the powerup update. |
| [`update.rs`](update.rs) | The powerup update (`FUN_141204a10`, `World::powerup_update`): an object's powerups end once their hold has passed, and its auras power up what they find around it. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../map/), [`math`](../../math/), [`objects`](../../objects/), [`players`](../../players/), [`unit_ai`](../../unit_ai/), [`vision`](../../vision/), [`world`](../../world/).
<!-- /uses -->
