# `construction/housing`: the Housing action

`UNIT_ORDER`'s case 3 gives a building a new Housing (`../../orders/housing_order.rs`), and so
does the work command of a combat object or a building on a task of action type 0x7b, which no
master of build 158041's `.dat` holds; an action object's list makes none. Its start pays 50
wood straight from the owner's attributes; without the wood it fails (state 13). At work
(state 6) its timer starts at the owner's attribute 62 and runs down by the building's work rate
in every update, less while the building is damaged. Once the timer ran out, the building holds 5
more headroom and its owner gains 5. Its stop gives the cost back in every state, and its
destructor gives it back while it is at work.

- `mod.rs` holds the action's state, its start, its work and its stop.
- `unit.rs` binds it to the building and the world: the install, the update, the headroom and
  the destructor's refund.

## In the game

- `TRIBE_Action_Housing` (action type 0x7b, vtable `0x142ed7008`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`mod.rs`](mod.rs) | A building's Housing: `TRIBE_Action_Housing` (action type 0x7b, vtable `0x142ed7008`), which `UNIT_ORDER`'s case 3 installs on a building (`World::housing_order`), and the work command's factory on a task of its type (`FUN_140aca4b0` case 0x7b). |
| [`tests.rs`](tests.rs) | Tests: The Housing's tests on a house of player 1, and the fixtures `UNIT_ORDER`'s tests share. |
| [`unit.rs`](unit.rs) | A Housing bound to its building and the world: its install and start, its update with the building's work rate and headroom, and the cost its destructor gives back. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`commands`](../../commands/), [`math`](../../math/), [`objects`](../../objects/), [`players`](../../players/), [`technology`](../../technology/), [`world`](../../world/).
<!-- /uses -->
