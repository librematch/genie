# `world`: the world's state and its tick

`World` (`mod.rs`) holds the whole game state: the players with their objects, the map, the
pathing state, the random streams and the masters. This folder holds that state and the tick that
moves it on. The behaviours that a tick runs live in their systems. Many of them are methods of
`World` in their own folders.

- `setup.rs` builds the world from the save in a recording's header.
- `load/` reads each saved object into the world: its row, its parts, its movement, its
  actions and sub-actions.
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
| [`action_parts.rs`](action_parts.rs) | The action part's reads and writes (FD-045-D59): an object's action layer by id in the world's table (`Parts::actions`), an empty one where the table holds none, and the updating object's layer out of the table with its row (`Parts::take_actions`). |
| [`after_step.rs`](after_step.rs) | An object's step and what follows it (`World::step_object`): what its `Step` gathered, applied to the world in the order the game's update makes them. |
| [`ai_values.rs`](ai_values.rs) | The values of a player's AI settings (`crate::world::AiSettings`). |
| [`checkpoint.rs`](checkpoint.rs) | The per-player 11-word state block that recorded games checkpoint every N syncs. |
| [`forced.rs`](forced.rs) | The forced replay (dev only): the game's state after a tick (`GameState`) and the step that forces it on the world. |
| [`frame_seed.rs`](frame_seed.rs) | The world's frame seed. |
| [`index.rs`](index.rs) | The world's table by id (`FUN_1415417a0`: `world+0x168` for ids 0 and up, `+0x178` for the recycled ones, FD-045-D49): where each object is, a player's live or sleeping slot. |
| [`lets.rs`](lets.rs) | What an object's update does to the objects its actions take and let go (slots 0x178 and 0x180) and to the building a counted builder leaves: each the world's call where the game makes it (FD-045-D07, D28; `World::take`, `World::release`, `World::leave_builders`), which runs `Lets` on a view of the whole world; the unit AI's side runs them so too, its own AI's notifies held until its call returns (`World::with_lets_for`). |
| [`mod.rs`](mod.rs) | The world: its state and its tick. |
| [`mode_680.rs`](mode_680.rs) | The game object's tagged value at `+0x680`. |
| [`motions.rs`](motions.rs) | The motion part's reads and writes (FD-045-D58): an object's animated layer and its mover, by id in the world's table (`Parts::motion`), beside the row's place. |
| [`object_step.rs`](object_step.rs) | One object's step with its `Step` (`World::run_object`) and a unit AI run outside its update. |
| [`others.rs`](others.rs) | The world's objects by id as an update reaches them (FD-045-D19, D28): `OthersRef` to read them (the owner's live list and the other players' lists), `Others` to change them while the unit AI runs, and the world's own lookups for the update itself, which holds the world (`World::locate`, `World::wake_sleeper`, `World::take_out_own`). |
| [`parts.rs`](parts.rs) | The parts only some objects hold, each in a table by id on the world (FD-045-D51), so that an object's row holds only what every object has. |
| [`parts_fixtures.rs`](parts_fixtures.rs) | A test's world with its parts: fixtures that file a test object's parts beside its row (`World::with_parts`, `World::split_layers`) and the whole objects tests push. |
| [`play_state.rs`](play_state.rs) | The world's game state. |
| [`player.rs`](player.rs) | A player (`Player`): its objects, attributes and formations, and the stop of its update. |
| [`schedule.rs`](schedule.rs) | The world update as a declared schedule (FD-045-D13): its steps in the game's order, each with the game routine it ports, and the update that runs them with an observer. |
| [`setup.rs`](setup.rs) | The world as the header save holds it (`World::new`): the next object id, the planners and the shape mesh the game's load builds. |
| [`slots.rs`](slots.rs) | The checked reach into the world's player table and the players' object lists. |
| [`step.rs`](step.rs) | What a tick's update of one object reads and writes (`Step`) and its walks. |
| [`tests.rs`](tests.rs) | Tests. |
| [`tick.rs`](tick.rs) | An update's number. |
| [`unread.rs`](unread.rs) | The objects the load could not read whole (FD-045-D61): the import's gap, not game data. |
| [`whole.rs`](whole.rs) | An object as one value (FD-045-D51): its row and its parts, as the world takes it in and gives it back whole, and the view a comparison reads (`ObjectView`). |
| [`world_update.rs`](world_update.rs) | The world update's parts (`crate::world::schedule` runs them): the command phase, the moving objects, the players' relations and each player's objects' updates. |
| [`load/`](load/) | The world's load from a save (FD-043-01M6JPBEDA, the review's ORA-01): each saved object's row, its parts, its movement, its actions and sub-actions, as genie-save reads them from a recording's header or a per-tick save. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`commands`](../commands/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`pathing`](../pathing/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`testing`](../testing/), [`unit_ai`](../unit_ai/), [`vision`](../vision/).
- Used by: [`combat`](../combat/), [`commands`](../commands/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`pathing`](../pathing/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`testing`](../testing/), [`unit_ai`](../unit_ai/), [`vision`](../vision/).
<!-- /uses -->
