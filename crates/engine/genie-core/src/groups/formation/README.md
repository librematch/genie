# `groups/formation`: a group's formation

A formation has slots in lines, and each slot holds one member of the group. This folder computes
where the slots are and which member goes to which slot.

- `column_line.rs` makes the slots of a column or a line formation.
- `desired_positions.rs` places the slots along the group's walked path, from the front back.
- `slots.rs` gives the slots their units: by unit category, by the closest unit and by the
  variance of the assignment.
- `passability.rs` checks that a unit can reach its slot over the terrain under the formation's
  box; `unreachable.rs` moves the slots no unit can reach.

The other layouts (a horde, a phalanx, a box, a user-defined formation) are in
`groups/layout.rs`.

## In the game

- `BFormation` (`formtn.cpp`), with `createColumnLine`, `UpdateDesiredPositions` and
  `FormationTransform`.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`column_line.rs`](column_line.rs) | createColumnLine (`FUN_141012280`): the sizes and the slots of its sub-formations. |
| [`desired_positions.rs`](desired_positions.rs) | `UpdateDesiredPositions` (`FUN_14103fd00`): the formation type, the walked path it measures along, a slot's place and the point back from the front. |
| [`mod.rs`](mod.rs) | A group's formation (FD-036): `BFormation`, its slots and lines, and the layouts that fill them (createColumnLine `FUN_141012280`, ...). |
| [`passability.rs`](passability.rs) | `FormationTransform` (`FUN_14103a1f0`) and its passability: the grid over the box, the slots' reach and the search for an unreachable slot's unit. |
| [`slots.rs`](slots.rs) | The slots' units: the unit categories, `ClosestUnit`, the assignment by variance, the population in order and a line after a column. |
| [`tests.rs`](tests.rs) | Tests. |
| [`unreachable.rs`](unreachable.rs) | The formation's slots their units cannot reach (`move_unreachable`): each one's place moves to the cell its unit can reach. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`world`](../../world/).
<!-- /uses -->
