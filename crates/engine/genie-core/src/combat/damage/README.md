# `combat/damage`: what a hit does

A hit reads its attacker and its target and changes the target. This folder holds that
computation.

The damage starts with the attacker's attacks against the target's armor, class by class
(`armor.rs`). A shield blocks part of it (`hit_points.rs`). The elevation, the facing and a
charge change it (`hit_factors.rs`). A blast spreads it to the objects around (`blasts.rs`).
Then the target takes it (`taking.rs`). Each kind of target takes damage its own way: a static
object, a combat unit, and a building, whose annex passes the hit to its main building.

`shown_values.rs` holds the bonus damage tasks, and the attack, armor and range that a combat
object shows.

When a melee hit lands is decided in `combat/attack/`. A missile's hit is decided in
`combat/missile/`.

## In the game

- The target's damage routine for each object type: the static object, the combat object of types
  50, 60 and 70, and the building.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`allied.rs`](allied.rs) | How a hit's two players stand to each other (`Allied`). |
| [`armor.rs`](armor.rs) | The armor against melee and against each weapon class, and the damage per weapon (`FUN_141082530`). |
| [`armor_pair.rs`](armor_pair.rs) | An object's two armor values (`ArmorPair`): melee and pierce. |
| [`blasts.rs`](blasts.rs) | The damage beyond what a blast has done (`FUN_141082e80`) and the blast itself (slot 0x848). |
| [`hit_factors.rs`](hit_factors.rs) | A hit's factors: the elevation, the facing offset and the charges (`FUN_1412623d0`). |
| [`hit_points.rs`](hit_points.rs) | The shield, the invulnerability floor, and the hit points' loss (slot 0xb0) and gain (slot 0xb8). |
| [`mod.rs`](mod.rs) | The damage side of a hit: what it reads of its target and its attacker and what it changes. |
| [`shown_values.rs`](shown_values.rs) | The bonus damage tasks (`FUN_14107ce60`) and the attack, armor and range a combat object shows. |
| [`taking.rs`](taking.rs) | A target's slot 0xe8: a building's, a static object's and a combat unit's, with the unit's facing, its retaliation and the hit's end. |
| [`tests.rs`](tests.rs) | Tests. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`construction`](../../construction/), [`map`](../../map/), [`math`](../../math/), [`objects`](../../objects/), [`players`](../../players/), [`world`](../../world/).
<!-- /uses -->
