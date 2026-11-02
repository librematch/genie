# `genie-core/src`: the simulation core

This is the source of `genie-core`, genie's deterministic simulation core. It plays a recorded
game of Age of Empires II: Definitive Edition (build 158041) tick by tick and gives the same
state as the game, bit for bit.

The code is sorted into the engine's systems, one folder each. A folder's `README.md` explains its
system. [`docs/ENGINE_SYSTEMS.md`](../../../../docs/ENGINE_SYSTEMS.md) explains how the systems
work together: one tick, one object's update, how the systems depend on each other, and how to
find a behaviour.

Start with these files:

- [`world/schedule.rs`](world/schedule.rs): one tick of the world, its steps in the game's order.
- [`objects/unit/update.rs`](objects/unit/update.rs): one object's update.
- [`world/mod.rs`](world/mod.rs): the `World` type, which holds the whole game state.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`lib.rs`](lib.rs) | Deterministic simulation core of the genie engine reimplementation. |
| [`test_ids.rs`](test_ids.rs) | The tests' newtypes: an object id, a unit group, a player or a master of a value the test knows is one, a list of ids and an object's load, through the types' checked constructors. |
| [`trace.rs`](trace.rs) | What the engine tells a `tracing` subscriber while it runs, for debugging. |
| [`combat/`](combat/) | Combat: attacks, hits and missiles. |
| [`commands/`](commands/) | The commands (`Command`) and their dispatch (`World::run`): the handlers of the players' orders, the buildings' placement and the production queue's. |
| [`construction/`](construction/) | Building and repairing: the Build and Repair actions with each worker's part of its update, the wait for a foundation, a building's own update and the gates. |
| [`garrison/`](garrison/) | Units inside other objects: the garrison order, the Enter action, the unloads and the Transport action. |
| [`gathering/`](gathering/) | Gathering resources: the Gather and Hunt actions with each gatherer's part of its update, a farm's food, the searches for a resource, the drop sites and drop-offs, the mill's and the dock's queues and the pastures. |
| [`groups/`](groups/) | Unit groups and formations (FD-036): the game's `BUnitGroup` (`group.cpp`) with its motion (`BMotion`) and formations (`BFormation`). |
| [`map/`](map/) | The map: the terrain of each tile and where the world's objects stand (their places, radii, footprints and the grid that finds them). |
| [`math/`](math/) | The bit-for-bit foundations: the game's random generators and the MSVC algorithms whose order shows in the simulation, beside genie-math's f32 math and image constants, re-exported here. |
| [`movement/`](movement/) | A unit's movement (`RGE_Moving_Object`, MovingObject.cpp in 158041): the velocity step that moves it along its forward vector, and the motion step that steers it along its path. |
| [`objects/`](objects/) | One object and what every object has and runs: the object the engine tracks (`unit::Unit`) with its update, its actions' holdings and stops, its life and death, its release, its sprite and facing, and the masters it is made from. |
| [`orders/`](orders/) | The orders a command gives its units: INTERACT and its splitter into loose units and groups, the moves, patrols and follows, the work, gather, hunt and build orders, the drop-offs, the town bell and the Empire Wars start. |
| [`ownership/`](ownership/) | Objects that change hands: the Artifact action of livestock, which a unit of another player takes over, and an ownership change with what it carries along (a building's annexes, its garrison, the attributes it counts). |
| [`pathing/`](pathing/) | Finding a way: the planners of the paths that units walk, from the tile pathfinder over the whole map to the short pathers around the obstructions near a unit. |
| [`players/`](players/) | The players beside their objects: their game status (playing, won, lost), diplomacy, tribute, treason, the market, their history, their object trackers, and the world player's own update. |
| [`production/`](production/) | Training units and researching technologies: a building's production queue, the actions that work it off, and a trained unit's spawn. |
| [`technology/`](technology/) | Technologies and upgrades: the players' technologies, a research's effects and an object's upgrade to another master. |
| [`testing/`](testing/) | What genie-core's tests build their game data from: each build's layer as the generated fixture states it (FD-046-D08), so a test needs no Nickel, and the classes the tests name. |
| [`unit_ai/`](unit_ai/) | The unit AI (`UnitAIModule`): what a unit decides by itself. |
| [`vision/`](vision/) | What each player sees: the visible maps its objects' sight keeps, the vision masks that share sight among allies, the objects-in-sight registry, the fog's ghosts and the newly explored tiles. |
| [`world/`](world/) | The world: its state and its tick. |
<!-- /files -->

## How it connects

<!-- uses -->
The crate root declares the systems. Each folder's README names what it uses.
<!-- /uses -->
