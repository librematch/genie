# `map/spatial`: where the objects are

`Spatial` holds each live object's position, radii and class, and the record that the game's
obstruction manager keeps for it. Many systems ask it questions, so this folder has one file per
group of queries.

- The footprints of buildings and their occupants (`footprint.rs`).
- The obstruction grid's cells and its line queries (`grid.rs`).
- The objects on the map: their tiles, their moves, their freezing (`objects.rs`).
- The queries of the old short pather (`pather.rs`).
- The unit AI's searches for the closest object and the best unit to attack (`search.rs`).
- Sight, ghosts and ownership changes as the map sees them (`sight.rs`).

The game files its records in a grid of levels. `Spatial` keeps them by object id and asks the
grid only which records a query visits. A record in a cell outside the query's tiles is not
found, as in the game.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`footprint.rs`](footprint.rs) | A footprint's spots, occupants and clearing, foreign marks, clear points and obstruction candidates. |
| [`grid.rs`](grid.rs) | The obstruction grid's cells and visits, the scout map, alliances and the line queries. |
| [`mod.rs`](mod.rs) | Where the world's objects are, as movement reads them: each live object's position, radii and class, and the obstruction record the game keeps for it in its obstruction manager (`DAT_143f0a088`, queried by `FUN_141051900`). |
| [`objects.rs`](objects.rs) | The objects on the map: their tiles and records, their freezing and the known checks, their moves inside and out, and the segment tests. |
| [`pather.rs`](pather.rs) | The old short pather's queries, the map's edge records and the segment tests. |
| [`record_id.rs`](record_id.rs) | The id of an obstruction record and of an entry of a pather's ignore list: an object's, a tile's temporary record's or a map edge's. |
| [`search.rs`](search.rs) | A position's change, the sight check and the unit AI's searches: the closest object and the best unit to attack. |
| [`sight.rs`](sight.rs) | The artifact's owner search, an ownership change, vision and sight, ghosts and the standable spots. |
| [`tests.rs`](tests.rs) | Tests. |
| [`tile_sight.rs`](tile_sight.rs) | What a player knows of a tile. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`construction`](../../construction/), [`groups`](../../groups/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`ownership`](../../ownership/), [`pathing`](../../pathing/), [`players`](../../players/), [`unit_ai`](../../unit_ai/), [`vision`](../../vision/), [`world`](../../world/).
<!-- /uses -->
