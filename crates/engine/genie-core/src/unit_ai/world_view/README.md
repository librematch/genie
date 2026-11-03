# `unit_ai/world_view`: the unit AI's view of the world

While a unit AI runs, it is taken out of its unit. `UnitView` then gives it the unit and the
parts of the world that it uses: the objects, the planners, the random streams, the masters. What
it does to other objects goes into lists that the caller applies afterwards.

- `mod.rs` holds the views: `UnitView` (the unit and its context), `Foreign` and `OwnerView`.
- `side.rs` implements `UnitSide` for `UnitView`. Its long answers live in their systems as
  methods of `UnitView`: the attack command in `combat/attack/`, the build command in
  `construction/build/`, the hunt in `gathering/hunt/`.
- `unit_view.rs` holds what the view asks of its unit.
- `world_picker.rs` is the world as `bestUnitToAttack` reads it.
- `commands.rs` holds the unit AI's repair and enter commands, and the auto scout's area.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`commands.rs`](commands.rs) | The repair and enter commands of the unit AI and the auto scout's area. |
| [`mod.rs`](mod.rs) | The views a unit's update and its unit AI read the world through (`UnitView`, `Step`, `Foreign`, `OwnerView`, the pickers). |
| [`side.rs`](side.rs) | What the unit AI asks of the world through its view: `crate::unit_ai::UnitSide` for `UnitView`. |
| [`tests.rs`](tests.rs) | Tests. |
| [`unit_view.rs`](unit_view.rs) | What the unit AI's view asks of its unit: its picker, a helper's tests and a master switch. |
| [`world_picker.rs`](world_picker.rs) | The world as `bestUnitToAttack` and the attack tests read it (`WorldPicker`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`construction`](../../construction/), [`garrison`](../../garrison/), [`gathering`](../../gathering/), [`groups`](../../groups/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`orders`](../../orders/), [`ownership`](../../ownership/), [`pathing`](../../pathing/), [`players`](../../players/), [`world`](../../world/).
<!-- /uses -->
