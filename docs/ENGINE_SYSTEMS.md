# The engine's systems

`genie-core` is genie's simulation core. It reimplements the simulation of Age of Empires II:
Definitive Edition, build 158041, bit for bit. It has no graphics, sound or input: other crates
add those. This page explains how the core is organised. It describes one tick of the world, one
object's update, the nineteen systems the code is sorted into, and how to find a behaviour.

Each folder of [`crates/engine/genie-core/src/`](../crates/engine/genie-core/src/) has a
`README.md`. It explains the system or the part in that folder and lists its files.

## Terms

| Term | Meaning |
| --- | --- |
| world | The whole game state: the players, their objects, the map, the pathing state, the random streams. The type is `World` (`world/`). |
| tick | One update of the world. A tick runs the players' commands, then every player's objects. The game calls it `BaseWorld::update`. |
| player | One side of the game. Player 0 is Gaia: the animals, trees, mines and relics. |
| object | Anything in the world: a villager, a soldier, a building, a tree, a sheep, a missile. |
| `Unit` | The type that holds one object, of any kind (`objects/unit/`). The name comes from the slice's first ports. |
| master | The definition of an object's kind for one player: its hit points, speed, tasks, costs. The `.dat` file gives the masters, and each player's copy changes with its technologies. |
| action | What an object does now: walk to a point (`MoveTo`), attack, gather, build, repair. An object holds a list of actions and runs the first one. |
| unit AI | The part of a unit that decides by itself: it picks targets, answers attacks and takes orders. |
| command | A player's input, as a recording stores it: an order to units, a building placement, a research. |
| order | What a command, or a unit AI, tells a unit to do. An order usually ends in a new action. |
| task | One entry of a master's task list: what the master can do to a kind of target (gather wood, build, repair). |
| port | The Rust reimplementation of one routine of the game. Its documentation names the routine. |
| `RGE_`, `TRIBE_` | The prefixes of the game's classes. `RGE_` classes are the Genie engine's base. `TRIBE_` classes are Age of Empires II's layer on top of it. |
| stop | A player whose update reaches a part that is not ported yet stops. Its objects freeze and the stop names the missing part (`GenieError::unsupported`). |

## One tick

`World::tick_with` (`world/schedule.rs`) runs one tick as a declared schedule (FD-045-D13): the
steps before the players', then each player's steps player by player (Gaia first, then players
1 to 8), then the steps after them. The steps run in the game's order, because every random draw
and every list order shows in the result. `World::tick_observed` runs the same steps and shows
the world after each one; the harness compares that world with the game's state at the same
point (`harness steps`).

The schedule, generated from `world/schedule.rs` (`just schedule`):

<!-- schedule -->
| # | Step | Runs | What it does |
| ---: | --- | --- | --- |
| 1 | `Commands` | once | The command phase: the update's count and the world's time go on, the streams take the frame seed and the players' commands run (`BaseWorld::update` up to `RGE_Command__do_commands`, before `FUN_141552990`). |
| 2 | `Clock` | for each player | The player's time goes on by the update's milliseconds (`BaseWorld::update`, `world+0x18c`). |
| 3 | `OpenGates` | for each player | The gates of the players allied with the player open for its units (slot 0x318). |
| 4 | `StartWork` | for each player | In the first update of an Empire Wars game the player's idle villagers go to work (`BaseWorld::update` with the world's `+0x6c6`). |
| 5 | `Groups` | for each player | The player's unit groups update (`FUN_14103d910`); before them the pathing takes the tiles whose static marks changed (genie's own cache). |
| 6 | `WorldPlayer` | for each player | The player's own update up to its objects: a world player's seconds or Gaia's time (`WorldPlayer::update`, slot 0x100), then its plans start over (`WorldPlayerBase::update`, `+0x67c`). |
| 7 | `Objects` | for each player | The player's objects update in their list's order (`FUN_141166550` on the player's list). |
| 8 | `NewTiles` | for each player | The tiles the player's map explored give their resources (`Process_New_Tiles`, `FUN_1412899a0`). |
| 9 | `ScoutMap` | for each player | A world player's scout map takes the tiles of its tile list (`WorldPlayer::update` after `WorldPlayerBase::update`). |
| 10 | `CloseGates` | for each player | The gates the player opened close again (slot 0x320). |
| 11 | `ClearTiles` | for each player | The player's tile list empties (`FUN_141661030` on `+0x180`). |
| 12 | `Draw` | once | The world stream draws the seed that `Reseed` gives the streams (`rand15` at `0x141551aca`); before it the frozen objects' unknown places grow by the update (genie's own, for the forced replay). |
| 13 | `Ghosts` | once | Every player's ghosts update, Gaia first (`FUN_141166550` on each player's `+0x40`). |
| 14 | `Watched` | once | Every player's watched list, Gaia first (`FUN_1410905e0` on each player's `+0x78`). |
| 15 | `Reseed` | once | All streams take the drawn seed (`SeedAll`, `0x140efe4f0`). |
| 16 | `Turn` | once | The turn passes to the next player, and the time of the player whose turn ends starts over (`BaseWorld::update`, `world+0x188`). |
| 17 | `Victory` | once | The victory check (`TRIBE_World` slot 0x68, `FUN_1415c9480`). |
| 18 | `FrameSeed` | once | The world stream draws the next update's frame seed (`rand15` at `0x141551bd7`). |
| 19 | `LastRandoms` | once | The multiplayer sync's draws of four streams (`setLastRandoms`, `0x140d3ce20`). |
| 20 | `Treaty` | once | The treaty monitor's update while the update has time (`World::update` at `0x1415e8e90`, `World::treaty_step`). |
| 21 | `Vision` | once | The players' vision masks again (`World::update`'s slot 0x78, `FUN_1415e92b0`). |
<!-- /schedule -->

In the `Objects` step every live object of the player updates, in list order
(`world/object_step.rs`, then `Unit::update`); the next section lists what one update does.
Objects that died, fell asleep or changed owner leave the list. The swap-remove of the list
skips the object that moves into the free place until the next tick, as in the game.

Only one player's unit AIs run their full update in a tick: the player whose turn it is. The
other players' unit AIs only take their orders and notifications.

## One object's update

`Unit::update` (`objects/unit/update.rs`) is the object's update. It follows the game's chain of
classes: the static object, the animated object, the moving object, the action object, the combat
object and the building.

1. A building's gate, its completion and its seconds (`construction/building_update.rs`).
2. The attack timer and the volley under way run down (`combat/attack/unit.rs`).
3. A moving object takes its velocity step: it moves along its forward vector by its speed
   (`movement/unit.rs`).
4. The life cycle steps: a dead object plays its dying sprite, leaves its remains and goes
   (`objects/unit/life_step.rs`, `objects/lifecycle/`).
5. The sprite's clock goes on (`objects/sprite.rs`).
6. An idle unit starts its master's default task (`objects/unit/default_task.rs`).
7. The action list runs its first action. Each action's part of the update is a method in the
   action's system:
   - `ownership/artifact.rs`: the Artifact.
   - `construction/build/unit.rs` and `construction/repair/unit.rs`: the Build and the Repair.
   - `garrison/unit.rs`: the inside update and the Enter.
   - `movement/unit.rs`: the `MoveTo`, the Explore and the Fly.
   - `gathering/make.rs`, `gathering/gather/unit.rs` and `gathering/hunt/hunter.rs`: the Make,
     the Gather and the Hunt.
   - `combat/attack/unit.rs`: the Attack.

   A walking action calls the motion step, which plans the path (`pathing/`) and steers the unit
   along it (`movement/`).
8. A combat object's passive abilities run (`combat/abilities/`), and a building's production
   (`production/`).
9. The unit AI runs (`unit_ai/`).

The update reads and writes the world through its `Step` (`world/step.rs`). A call on another
object takes effect at once. What the update does to the object itself waits in the `Step`, and
`World::step_object` (`world/after_step.rs`) applies it after the update: the object's own
kills, releases, works and ownership change.

## The systems

Each top-level folder is one system. The table names the game's classes in it and the game's
source files it comes from. The file names are those that the HD edition's debug symbols name
(8.3 names such as `act_atak.cpp`). They are matched by their names only.

| Folder | What it holds | The game's classes | The game's files |
| --- | --- | --- | --- |
| [`world/`](../crates/engine/genie-core/src/world/) | The world's state and its tick: the setup from a recording's header, the world update, a player, one object's step and its context, the game options, the checkpoint block | `BaseWorld`, `TRIBE_World`, `WorldPlayerBase` | `world.cpp`, `tworld.cpp`, `World.cpp`, `BaseWorld.cpp`, `WorldPlayerBase.cpp` |
| [`objects/`](../crates/engine/genie-core/src/objects/) | One object and what every object runs: its update, its actions' stops, its life and death, its sprite and facing, its masters | `RGE_Static_Object`, `RGE_Animated_Object`, `RGE_Moving_Object`, `RGE_Action_Object`, `RGE_Master_*` | `stat_obj.cpp`, `ani_obj.cpp`, `act_obj.cpp`, `act_list.cpp`, `sprite.cpp`, `asprite.cpp`, `MasterStaticObject.cpp` |
| [`commands/`](../crates/engine/genie-core/src/commands/) | The players' commands and their dispatch: placements, walls, queues, deletes, GAME | `RGE_Command`, `TRIBE_Command` | `command.cpp`, `tcommand.cpp` |
| [`orders/`](../crates/engine/genie-core/src/orders/) | The orders a command gives its units: INTERACT and its splitter, moves, patrols, follows, work and gather orders, the town bell, the Empire Wars start | (in `TRIBE_Command` and the unit AI) | (in `command.cpp` and the unit AI's files) |
| [`unit_ai/`](../crates/engine/genie-core/src/unit_ai/) | The unit AI: its orders and notifies, its target choice, its idle checks, the task finder and its view of the world | `UnitAIModule` and its types, `RGE_Task_List`, `TRIBE_Task_List` | `UnitAIModule.cpp`, `aiuaimod.cpp`, `taiuaimd.cpp`, `task.cpp`, `task_lst.cpp`, `ttsk_lst.cpp` |
| [`groups/`](../crates/engine/genie-core/src/groups/) | Unit groups and their formations, their motion and their members' moves to their slots | `BUnitGroup`, `BFormation`, `BMotion` | `group.cpp`, `formtn.cpp`, `motion.cpp` |
| [`pathing/`](../crates/engine/genie-core/src/pathing/) | Finding a way: the tile pathfinder, the long pather, the short pather, the old hull pather, the obstruction mesh, the zones | the planners, `MShortPather`, `RGE_Zone_Map` | `path.cpp`, `pathsys.cpp`, `mpathsys.cpp`, `zone_map.cpp` |
| [`movement/`](../crates/engine/genie-core/src/movement/) | Walking the way: the velocity step, the motion step, waypoints, and the actions that are walks (`MoveTo`, Explore, a bird's Fly) | `RGE_Moving_Object`, `RGE_Action_Move_To`, `RGE_Action_Explore`, the Fly action (type 10) | `MovingObject.cpp`, `motion.cpp`, `waypoint.cpp`, `act_move.cpp`, `act_expl.cpp`, `act_bird.cpp` |
| [`map/`](../crates/engine/genie-core/src/map/) | The map: the terrain per tile and where the objects stand | `RGE_Map`, the obstruction manager | `map.cpp`, `tmap.cpp` |
| [`vision/`](../crates/engine/genie-core/src/vision/) | What each player sees: the visible maps, the vision masks, the objects-in-sight registry, the fog's ghosts, the explored tiles | `RGE_Visible_Map`, `RGE_Unified_Visible_Map`, `Visible_Unit_Manager`, `RGE_Doppleganger_Object` | `visible.cpp`, `vis_unit.cpp`, `VisibleUnitManager.cpp`, `dpl_obj.cpp` |
| [`combat/`](../crates/engine/genie-core/src/combat/) | Attacks, hits and missiles: the Attack action, damage, passive abilities, missiles and volleys, a hit's victims | `RGE_Action_Attack`, `RGE_Missile_Object`, `RGE_Action_Missile`, `TRIBE_Combat_Object` | `act_atak.cpp`, `act_misl.cpp`, `misl_obj.cpp`, `t_c_obj.cpp`, `TribeMasterCombatObject.cpp` |
| [`gathering/`](../crates/engine/genie-core/src/gathering/) | Gathering resources: the Gather and Hunt actions, a farm's food, the resource searches, drop sites, the mill's queues, pastures, the mule cart | `RGE_Action_Gather`, `TRIBE_Action_Hunt`, `RGE_Action_Make` | `act_gath.cpp`, `tact_gth.cpp`, `tact_hnt.cpp`, `act_make.cpp`, `tact_she.cpp` |
| [`construction/`](../crates/engine/genie-core/src/construction/) | Building and repairing: the Build and Repair actions, the wait for a foundation, a building's own update, the gates | `TRIBE_Action_Build`, `TRIBE_Action_Repair`, `TRIBE_Action_Wait_Solidify`, `TRIBE_Building_Object` | `tact_bld.cpp`, `tact_rep.cpp`, `t_b_obj.cpp` |
| [`garrison/`](../crates/engine/genie-core/src/garrison/) | Units inside other objects: garrisons, entering, unloading, transports | `RGE_Action_Enter`, `RGE_Action_Transport` | `act_entr.cpp`, `act_tran.cpp` |
| [`production/`](../crates/engine/genie-core/src/production/) | A building's production queue and a trained unit's spawn | `TRIBE_Action_Make_Obj`, `TRIBE_Action_Make_Tech` | `t_b_obj.cpp`, `tact_mob.cpp` |
| [`technology/`](../crates/engine/genie-core/src/technology/) | Technologies, their effects, and an object's upgrade to another master | `TRIBE_Player_Tech`, `TRIBE_Effects` | `tech.cpp`, `techtree.cpp`, `effects.cpp`, `teffects.cpp` |
| [`ownership/`](../crates/engine/genie-core/src/ownership/) | Objects that change hands: captures, the Artifact action of livestock, ownership changes | `TRIBE_Action_Artifact` | `tact_cap.cpp`, `tact_art.cpp`, `tact_cnv.cpp` |
| [`players/`](../crates/engine/genie-core/src/players/) | The players beside their objects: game status, diplomacy, tribute, treason, the treaty, the market, history, the object tracker, the world player's update | `WorldPlayer`, `WorldPlayerGaia`, `TRIBE_History_Info`, `TreatyMode` | `player.cpp`, `tplayer.cpp`, `t_histry.cpp`, `victory.cpp`, `tvictory.cpp`, `VictoryConditions.cpp` |
| [`math/`](../crates/engine/genie-core/src/math/) | The bit-for-bit foundations: the game's f32 math, its random generators, the MSVC algorithms whose order shows, the image's constants | the random generators, `BVector` | `math3d.cpp`, the C runtime |

`lib.rs` declares the systems. `trace.rs` holds what the engine tells a
[`tracing`](https://docs.rs/tracing) subscriber while it runs.

## How the systems depend on each other

The systems do not form strict layers. Two of them are hubs that call into every behaviour:
`world` runs the tick, and `objects` runs an object's update. A `Unit` holds the state of each
kind of action, so `objects` names every system that has an action.

A behaviour that needs the whole world is a method of `World`, and it lives in its own system, not
in `world/`. For example, `World::upgrade_object` is in `technology/upgrade.rs`. So `world/`
holds only the world's state and the tick, and most systems name `crate::world`.

At the bottom, `math` names no other system, and `pathing` names only `map` and `math`.
`map/spatial/` answers the queries of many systems, so it names their types.

Each folder's `README.md` lists the systems that its code names and the systems that name it.
`scripts/readmes.py` generates the lists from the code. Comments and test files do not count.

## Finding a behaviour

- **By system.** Start at the system's folder and its `README.md`. The table of files gives the
  first sentence of each file's documentation.
- **By the game's name.** Every port names the game's routine in its documentation: a class and
  method where the debug symbols give one (`RGE_Action_Attack::work`), else the routine's address
  in build 158041 (`0x140a82ae0`). Search for the name or the address:
  `grep -rn "0x140a82ae0" crates/engine/genie-core/src`.
- **By a stop.** A stop names the missing part in its reason. Search for the reason's text: every
  `GenieError::unsupported("…")` holds it.
- **By a trace.** `trace.rs` lists the targets that the engine logs. [DEBUGGING.md](DEBUGGING.md)
  shows how to play a recording and find where it goes wrong.

## Determinism

genie must give the same bits as the game on every host, WebAssembly included. Four parts of the
code keep it so:

- **The math** (`math/fmath/`). The game's `f32` routines (`sinf`, `atan2f`, `powf`, …) are
  ported from its machine code, with their constants and their operation order. A sum or a
  product is rounded on its own, never fused. A float becomes an integer as the x86 instruction
  does it (`math/conversions.rs`), not as Rust's `as` does.
- **The random streams** (`math/rng.rs`). The game has eleven `WELL512a` streams. All take the
  frame seed at the start of every tick, so the draws of one tick do not reach the next. A port
  draws from the same stream as the game, in the same order.
- **The order of lists.** The objects update in their list's order. A removal swaps the last
  object into the free place, as the game's list does. A sort runs MSVC's introsort
  (`math/msvc.rs`), because the order of equal elements depends on the algorithm.
- **The game data.** The values come from the recording's save and the game's `.dat` file, never
  from constants in the code. The exe's facts about specific ids (roles, slots) and its tuning
  values (ranges, times, factors) are the build layer, `builds/158041.ncl`, which the model
  holds (`Model::roles`, `slots`, `tuning`). `math/image.rs` holds the image's arithmetic
  constants that the routines read. A generator writes both from the build's symbol table.

## Conventions

The rules for new code are in [CODING_STYLE.md](../CODING_STYLE.md). This section names the ones
that shape the engine's source.

- **Names.** Folders and files have plain English names for what they hold. Game addresses and
  class abbreviations stay in the documentation, not in paths or identifiers.
- **One routine, one port.** A port follows its routine whole: every branch, its order of
  operations, its random draws. A part that is not ported yet returns
  `GenieError::unsupported`, and the player stops there. A new port adds no such part: it ports
  every branch.
- **File length.** A hand-written file holds at most 700 lines of production code
  (`scripts/file_size.py`). Tests do not count.
- **An action's folder.** An action has a folder of its own in its system, for example
  `gathering/hunt/`. `mod.rs` holds the action's state and its constants. The other files follow
  the action's slots and states: its start, its update, its `set_state`. `unit.rs` is the
  unit's part of the update, which binds the action to the world. `unit_ai_side.rs` holds what
  the unit AI asks of the action. `tests.rs` holds the action's tests.
- **The folder READMEs.** The prose is written by hand. `scripts/readmes.py SRC` fills the
  generated sections (the table of files, the systems used) between their markers.
  `scripts/readmes.py SRC --check` fails on a missing or stale README. The pre-commit hook and CI
  run the check.
