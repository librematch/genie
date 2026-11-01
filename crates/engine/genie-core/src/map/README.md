# `map`: the terrain and where the objects stand

The map holds the terrain of each tile and the places of the world's objects. Movement, pathing,
vision and the unit AI's searches read it.

- `terrain.rs` holds the map's tiles: their terrain, the terrain to restore, the layer, and what
  the `.dat` file says of each terrain.
- `spatial/` holds where every live object is: its position, its radii, its footprint, and the
  record that the game's obstruction manager keeps for it. The grid of the records answers the
  queries: which objects are near a point, which block a walk, which a unit can see.

## In the game

- `RGE_Map` (`map.cpp`, `tmap.cpp`): the tiles, 0x40 bytes each.
- The obstruction manager: the records of the objects' shapes, in a grid of levels.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`distance.rs`](distance.rs) | A distance on the map in tiles (`Distance`) and its square (`DistanceSquared`), each with the game's `f32` operations. |
| [`grid_size.rs`](grid_size.rs) | The size of a grid of tiles as one type (`GridSize`): its width and height, apart from a tile on it. |
| [`mod.rs`](mod.rs) | The map: the terrain of each tile and where the world's objects stand (their places, radii, footprints and the grid that finds them). |
| [`offset.rs`](offset.rs) | A displacement on the map as one type (`Offset`): x, y and z, apart from a place (`crate::map::Position`) and the game's vectors (x, height, y, `crate::math::fmath::BVector`); and one on the ground (`GroundOffset`): x and y, apart from a point (`crate::map::Point`). |
| [`point.rs`](point.rs) | A point on the ground as one type (`Point`): x and y, apart from a place with its height (`crate::map::Position`) and an object's radii (`crate::map::Footprint`). |
| [`position.rs`](position.rs) | A place on the map as one type (`Position`): x, y and z, apart from the game's vectors (x, height, y, `crate::math::fmath::BVector`). |
| [`radii.rs`](radii.rs) | An object's size as two types: its three radii (`Radii`, x, y and z, as its master holds them) and their part on the ground (`Footprint`, x and y), apart from a place (`crate::map::Position`) and a displacement (`crate::map::Offset`). |
| [`rect.rs`](rect.rs) | A box on the map's ground as one type (`Rect`): two corners, x0 and y0 then x1 and y1, apart from a point (`crate::map::Point`) and a walk between two points (`Segment`). |
| [`restriction.rs`](restriction.rs) | A terrain restriction's values (`Restriction`): one a terrain, which terrains an object of the restriction may stand on and the terrain bonus's damage multipliers. |
| [`restriction_id.rs`](restriction_id.rs) | A terrain restriction's id: which terrains an object may stand on. |
| [`terrain.rs`](terrain.rs) | The map's terrain per tile (`RGE_Map`'s tiles, 0x40 bytes each: `+0x9` the terrain, `+0xa` the terrain to restore, `+0xc` the layer) and what the `.dat` says of each terrain, as far as the slice reads them: a completed building's snow check and its foundation terrain (`FUN_1411dbb70`, `FUN_1411ddd90`). |
| [`terrain_flags.rs`](terrain_flags.rs) | A terrain's flags (`TerrainFlags`, the `.dat`'s `is_water` byte). |
| [`terrain_id.rs`](terrain_id.rs) | A terrain's id (`TerrainId`): a map tile's terrain and the `.dat`'s terrain records' index. |
| [`terrain_mask.rs`](terrain_mask.rs) | The terrain classes an object may stand on, as the obstruction manager's tile words test them. |
| [`tile.rs`](tile.rs) | A tile of the map as one type (`Tile`): x and y in whole tiles, apart from a point on the ground (`crate::map::Point`) and a step from one tile to another (`TileOffset`). |
| [`spatial/`](spatial/) | Where the world's objects are, as movement reads them: each live object's position, radii and class, and the obstruction record the game keeps for it in its obstruction manager (`DAT_143f0a088`, queried by `FUN_141051900`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`construction`](../construction/), [`groups`](../groups/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`pathing`](../pathing/), [`players`](../players/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
- Used by: [`combat`](../combat/), [`commands`](../commands/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`ownership`](../ownership/), [`pathing`](../pathing/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
<!-- /uses -->
