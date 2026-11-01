# `combat/missile`: missiles and their flight

A ranged attacker's swing makes a missile at the weapon's offset. The missile is an object of its
own (type 60). Its Missile action aims at the target and flies the missile in every update, until
it hits the target or the ground.

- `mod.rs` holds the missile's state, its spawn point, its aim and its launch.
- `unit.rs` holds the flight in the world: where the missile ends, what it hits and where it
  lands.

The volleys that fire missiles are in `combat/volley.rs`.

## In the game

- `RGE_Missile_Object` (`misl_obj.cpp`) and `RGE_Action_Missile` (action type 8, `act_misl.cpp`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`missile_modes.rs`](missile_modes.rs) | A missile master's modes (`SmartMode`, `VanishMode`). |
| [`missile_state.rs`](missile_state.rs) | A missile's state: launched, flying, ended. |
| [`mod.rs`](mod.rs) | The missile (`RGE_Missile_Object`, type 60) and its action (`RGE_Action_Missile`, type 8): a ranged attacker's swing makes one at its weapon offset (`FUN_141081020`, `spawn_point`), its action aims at the target (`FUN_140a9c600`, `aim`; `set_state` 3, `FUN_140a9de70`, `MissileAction::launch`) and flies it each update (`FUN_140a9e6e0`, `MissileAction::fly`) until it hits the target (the shooter's hit, slot 0x860) or the ground. |
| [`unit.rs`](unit.rs) | A missile's flight: where it ends, what it hits and where it lands. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`players`](../../players/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
