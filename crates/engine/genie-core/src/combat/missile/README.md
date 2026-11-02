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
| [`arc_ratio.rs`](arc_ratio.rs) | A missile master's arc. |
| [`hit_mode.rs`](hit_mode.rs) | A missile's hit mode. |
| [`miss_spread.rs`](miss_spread.rs) | How far a missed missile strays. |
| [`missile_modes.rs`](missile_modes.rs) | A missile master's modes (`SmartMode`, `VanishMode`). |
| [`missile_state.rs`](missile_state.rs) | A missile's state: launched, flying, ended. |
| [`missile_stats.rs`](missile_stats.rs) | What a missile master's flight reads of its master (FD-046: the model's part of the missile). |
| [`missile_target.rs`](missile_target.rs) | A missile's target as its own type: the object it was fired at. |
| [`mod.rs`](mod.rs) | The missile (`RGE_Missile_Object`, type 60) and its action (`RGE_Action_Missile`, type 8): a ranged attacker's swing makes one at its weapon offset (`FUN_141081020`, `spawn_point`), its action aims at the target (`FUN_140a9c600`, `aim`; `set_state` 3, `FUN_140a9de70`, `MissileAction::launch`) and flies it each update (`FUN_140a9e6e0`, `MissileAction::fly`) until it hits the target (the shooter's hit, slot 0x860) or the ground. |
| [`projectile_type.rs`](projectile_type.rs) | A missile master's projectile type. |
| [`shooter_id.rs`](shooter_id.rs) | A shooter's id as its own type: the object a missile was fired from. |
| [`strike_result.rs`](strike_result.rs) | What a missile's last hit did. |
| [`unit.rs`](unit.rs) | A missile's flight: where it ends, what it hits and where it lands. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`players`](../../players/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
