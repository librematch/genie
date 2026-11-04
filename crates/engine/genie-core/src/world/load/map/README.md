# `world/load/map`: the map and the sight from a save

The map's terrain per tile with the `.dat`'s terrain records, the objects as movement sees them
(their places, footprints and obstructions), and what each player sees: its visible map, the
unified map and the visible resources.

- `mod.rs` holds the terrain (`map_terrain`), the objects' places (`spatial`) and the sight
  (`saved_sight`, `saved_resources`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`mod.rs`](mod.rs) | The map and the players' sight from a save: the terrain, the objects as movement sees them, the visible maps and resources. |
| [`tests.rs`](tests.rs) | Tests. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../../map/), [`objects`](../../../objects/), [`ownership`](../../../ownership/), [`players`](../../../players/), [`vision`](../../../vision/).
<!-- /uses -->
