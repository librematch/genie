# `unit_ai/best_target`: the best unit to attack

When a unit AI looks for a target, `bestUnitToAttack` scores the objects in reach. It weighs each
candidate by its attack category, its range and the unit AI's order history, then ranks them.

- `choice.rs` holds the search itself: the object tests, the ranges and the choice.
- `statistics.rs` holds the scores, the ranks, the sort and the order history's weights.
- `world_view.rs` holds what the search reads of the searcher and of the world.

## In the game

- `UnitAIModule::bestUnitToAttack`.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`attack_category.rs`](attack_category.rs) | `bestUnitToAttack`'s attack categories and the tables it keeps by category. |
| [`choice.rs`](choice.rs) | `bestUnitToAttack` itself (`FUN_140c44680`): the object tests, the ranges and the choice. |
| [`mod.rs`](mod.rs) | `UnitAIModule::bestUnitToAttack` (158041 `0x140c44680`) and what it keeps: the attack categories (`FUN_140c4c8e0`) by which a unit AI's order history weighs its choice. |
| [`statistics.rs`](statistics.rs) | `bestUnitToAttack`'s statistics (`Statistics`): the entries, an object's add, the scores, the ranks and the sort, and the order history's weights. |
| [`tests.rs`](tests.rs) | Tests. |
| [`values.rs`](values.rs) | The statistics' values: an entry's kill time and scores, the table's counts and the order history's weights. |
| [`world_view.rs`](world_view.rs) | What `bestUnitToAttack` reads of the unit whose AI searches, of its arguments and of the world (`AttackField`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`ownership`](../../ownership/), [`players`](../../players/), [`world`](../../world/).
<!-- /uses -->
