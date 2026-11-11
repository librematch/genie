# `unit_ai/world_view`: the unit AI's view of the world

While a unit AI runs, it is taken out of its unit. `UnitView` then gives it the unit and the
parts of the world that it uses: the objects, the planners, the random streams, the masters. What
it does to other objects goes into lists that the caller applies afterwards.

- `mod.rs` holds the views: `UnitView` (the unit and its context), `Foreign` and `OwnerView`.
- `calls.rs`, `looks.rs` and `searches.rs` hold what the unit AI asks of its view: the unit's
  calls, what it reads of the unit and of the objects it names, and its searches. Their long
  answers live in their systems as methods of `UnitView`: the attack command in
  `combat/attack/`, the build command in `construction/build/`, the hunt in `gathering/hunt/`.
- `unit_view.rs` holds what the view asks of its unit.
- `world_picker.rs` is the world as `bestUnitToAttack` reads it, and `attack_reads.rs` its
  reads: the searching unit, the found objects, the tests, the statistics and the quick path.
- `commands.rs` holds the unit AI's repair and enter commands, and the auto scout's area.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`attack_reads.rs`](attack_reads.rs) | What `bestUnitToAttack` reads of the world through the unit's picker (`WorldPicker`): the searching unit, the found objects, the attack and neutral tests, the statistics' reads and the quick path. |
| [`calls.rs`](calls.rs) | The unit's calls its unit AI makes through its view, each at once where the game's AI calls the unit's slot: the attack command, the under-attack flag, an `Effect`, the notify its owner hears, the order slots' commands (the gather and hunt commands in the AI's frame), the wait for a foundation and the stop of a soldier in a group. |
| [`commands.rs`](commands.rs) | The repair and enter commands of the unit AI and the auto scout's area. |
| [`looks.rs`](looks.rs) | What the unit AI reads through its view of its unit and of the objects it names: the unit's action list as it stands now, its patrol path, the stand and reach tests, the build answer, an object's look and its target flags, a master's class, and what a damage notify weighs (the duel, a harmless attacker, a defended unit's nearest attacker). |
| [`mod.rs`](mod.rs) | The views a unit's update and its unit AI read the world through (`UnitView`, `Step`, `Foreign`, `OwnerView`, the pickers). |
| [`searches.rs`](searches.rs) | The unit AI's searches through its view: the objects-in-sight registry's search with its pick (`Search`), the attack test of a named object, the resource, foundation and drop-site searches, the spot off a footprint, the call for help, and the auto scout's searches with its scout map. |
| [`tests.rs`](tests.rs) | Tests. |
| [`unit_view.rs`](unit_view.rs) | What the unit AI's view asks of its unit: its picker, a helper's tests and a master switch. |
| [`world_picker.rs`](world_picker.rs) | The world as `bestUnitToAttack` and the attack tests read it (`WorldPicker`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`construction`](../../construction/), [`garrison`](../../garrison/), [`gathering`](../../gathering/), [`groups`](../../groups/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`orders`](../../orders/), [`ownership`](../../ownership/), [`pathing`](../../pathing/), [`players`](../../players/), [`vision`](../../vision/), [`world`](../../world/).
<!-- /uses -->
