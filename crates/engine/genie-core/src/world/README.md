# `world`: the world's state and its tick

`World` (`mod.rs`) holds the whole game state: the players with their objects, the map, the
pathing state, the random streams and the masters. This folder holds that state and the tick that
moves it on. The behaviours that a tick runs live in their systems. Many of them are methods of
`World` in their own folders.

- `setup.rs` builds the world from the save in a recording's header.
- `schedule.rs` is one tick as a declared schedule: the command phase, each player's steps and
  the steps after them, each with the game routine it ports. Read it first: it calls every
  system in the game's order.
- `world_update.rs` holds the tick's parts: the command phase and the update of a player's
  objects.
- `object_step.rs` runs one object's update with its `Step`.
- `after_step.rs` applies what the update collected, in the order of the game's update.
- `step.rs` holds the `Step`: what one object's update reads and writes of the world.
- `player.rs` holds a player: its objects, its attributes and its formations.
- `settings.rs` holds the game options that the simulation reads.
- `checkpoint.rs` computes the 11-word state block of each player. A recording checkpoints that
  block at a fixed interval of syncs, and genie compares its own block with it.
- `forced.rs` is a tool for development. It puts the game's own state after a tick into the
  world, so that the next tick starts from the game's state.

## In the game

- `BaseWorld::update` and `TRIBE_World` (`world.cpp`, `tworld.cpp`, `World.cpp`,
  `BaseWorld.cpp`).
- `WorldPlayerBase::update` (`WorldPlayerBase.cpp`): a player's objects in list order.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`after_step.rs`](after_step.rs) | An object's step and what follows it (`World::step_object`): what its `Step` gathered, applied to the world in the order the game's update makes them. |
| [`ai_values.rs`](ai_values.rs) | The values of a player's AI settings (`crate::world::AiSettings`). |
| [`checkpoint.rs`](checkpoint.rs) | The per-player 11-word state block that recorded games checkpoint every N syncs. |
| [`clock.rs`](clock.rs) | The world's clock in the game's milliseconds: a moment of it (`WorldTime`) and a span (`Millis`). |
| [`forced.rs`](forced.rs) | The forced replay (dev only): the game's state after a tick (`GameState`) and the step that forces it on the world. |
| [`frame_seed.rs`](frame_seed.rs) | The world's frame seed. |
| [`lets.rs`](lets.rs) | What an object's update does to the objects its actions take and let go (slots 0x178 and 0x180) and to the building a counted builder leaves: each the world's call where the game makes it (FD-045-D07, D28; `World::take`, `World::release`, `World::leave_builders`), which runs `Lets` on a view of the whole world; the unit AI's side runs them so too, its own AI's notifies held until its call returns (`World::with_lets_for`). |
| [`mod.rs`](mod.rs) | The world: its state and its tick. |
| [`mode_680.rs`](mode_680.rs) | The game object's tagged value at `+0x680`. |
| [`object_step.rs`](object_step.rs) | One object's step with its `Step` (`World::run_object`) and a unit AI run outside its update. |
| [`options.rs`](options.rs) | The game options' values: the game mode, the ages, the visibility, the resource level, the sub-mode's rules, the content mode and the treaty. |
| [`others.rs`](others.rs) | The world's objects by id as an update reaches them (FD-045-D19, D28): `OthersRef` to read them (the owner's live list and the other players' lists), `Others` to change them while the unit AI runs, and the world's own lookups for the update itself, which holds the world (`World::locate`, `World::wake_sleeper`, `World::take_out_own`). |
| [`play_state.rs`](play_state.rs) | The world's game state. |
| [`player.rs`](player.rs) | A player (`Player`): its objects, attributes and formations, and the stop of its update. |
| [`schedule.rs`](schedule.rs) | The world update as a declared schedule (FD-045-D13): its steps in the game's order, each with the game routine it ports, and the update that runs them with an observer. |
| [`settings.rs`](settings.rs) | The game options the simulation reads: the object `FUN_140c87b90` returns (`GameOptions`, 0x1088 bytes), which the header saves and its load `FUN_141305310` fills. |
| [`setup.rs`](setup.rs) | The world as the header save holds it (`World::new`): the next object id, the planners and the shape mesh the game's load builds. |
| [`slots.rs`](slots.rs) | The checked reach into the world's player table and the players' object lists. |
| [`step.rs`](step.rs) | What a tick's update of one object reads and writes (`Step`) and its walks. |
| [`tests.rs`](tests.rs) | Tests. |
| [`tick.rs`](tick.rs) | An update's number. |
| [`world_update.rs`](world_update.rs) | The world update's parts (`crate::world::schedule` runs them): the command phase, the moving objects, the players' relations and each player's objects' updates. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`commands`](../commands/), [`construction`](../construction/), [`gathering`](../gathering/), [`groups`](../groups/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`pathing`](../pathing/), [`players`](../players/), [`technology`](../technology/), [`unit_ai`](../unit_ai/), [`vision`](../vision/).
- Used by: [`combat`](../combat/), [`commands`](../commands/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`map`](../map/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`pathing`](../pathing/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`unit_ai`](../unit_ai/), [`vision`](../vision/).
<!-- /uses -->
