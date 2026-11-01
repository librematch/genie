# `production/spawn`: a trained unit's spawn

When the make action finishes a unit, the unit needs a place beside its building. The spawn point
search walks the building's sides in segments and tries each spot through the master's placement
test: the map's edge, the terrain, the explored tiles and the objects in the way.

- `make_action.rs` holds the production list and the make action.
- `search.rs` holds the spawn point search.
- `placement_test.rs`, `terrains.rs` and `world_map.rs` hold the placement test and what it
  reads.
- `new_unit.rs` makes the unit, gives it its command queue and its unit AI.
- `gather_walk.rs` sends the new unit to its building's gather point.
- `research.rs` holds the research action.
- `objects.rs` holds the other objects that the world makes: remains, flares and a building that
  a death leaves.

## In the game

- `TRIBE_Action_Make_Obj` and its spawn point search.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`gather_walk.rs`](gather_walk.rs) | A new unit's walk to its building's gather point (`FUN_140ace9c0`). |
| [`make_action.rs`](make_action.rs) | A building's production list and its make action (`TRIBE_Action_Make_Obj`). |
| [`mod.rs`](mod.rs) | A trained unit's spawn: where the make action (`TRIBE_Action_Make_Obj`, `FUN_140ad0240`) puts the unit its building made. |
| [`new_unit.rs`](new_unit.rs) | A made unit's spawn, the command queue it takes, its unit AI and the combat unit's constructor. |
| [`objects.rs`](objects.rs) | A spot beside a building, the remains a death leaves, a plain object, the flares and a building a death makes. |
| [`placement_code.rs`](placement_code.rs) | What a building's placement test answers. |
| [`placement_test.rs`](placement_test.rs) | The master's placement test (slot 0x50): the objects near a candidate, BUILD's placement test and the spawn's. |
| [`research.rs`](research.rs) | The research action (`TRIBE_Action_Make_Tech`): its technology, its update, its end and its destructor. |
| [`search.rs`](search.rs) | The spawn point search (`FUN_1411d12d0`): the building's sides in segments, each spot's candidates (`FUN_1411c5b60`) and the corners' wall checks. |
| [`terrains.rs`](terrains.rs) | The placement test's terrains (`FUN_141123f80`): the hill mode, the placement and side terrains and the tiles beside a box. |
| [`tests.rs`](tests.rs) | Tests. |
| [`world_map.rs`](world_map.rs) | The map as the placement test reads it from the world (`WorldMap`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`commands`](../../commands/), [`garrison`](../../garrison/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`orders`](../../orders/), [`ownership`](../../ownership/), [`players`](../../players/), [`technology`](../../technology/), [`unit_ai`](../../unit_ai/), [`vision`](../../vision/), [`world`](../../world/).
<!-- /uses -->
