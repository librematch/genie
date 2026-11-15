# `construction/pack`: the Pack action

A trebuchet (class 54) packs into its packed master (class 51) through a Pack on its action
list. At work (state 6) its timer runs up toward the master's pack time, the train time of its
first train location, by the building's work rate in every update, and its sprite's frame
follows the timer. Once the timer reached the pack time, its unit AI takes the attack order on
the Pack's target first, and the building turns into its master's transform: its action list
goes, this Pack with it. A stop ends the Pack and keeps its share done for the next Pack or
Unpack. The destructor tells the unit AI once the timer reached the pack time; in a played game
at work it puts the building's standing sprite and its annexes back.

A trebuchet's work order (slot 0x7e8) makes a Pack on a target out of its reach, or with no
target for a computer's trebuchet. The constructor gives the unit AI the order and the action on
a target, slot 0x808 puts the Pack alone on the list, and its start sets it to work: its timer
takes the share done that the building kept, the building shows its construction sprite and its
annexes that are buildings go. The Pack then takes the order's target.

- `mod.rs` holds the action's state, status, timer and queue id, its constructor's part, its
  work, its frame, its stop and its progress.
- `install.rs` makes and installs it: the constructor with the unit AI's order, slot 0x808, the
  start (slot 0x28) and `set_state` on the building, and the take of its target.
- `unit.rs` binds it to the building and the world: the update, the attack order, the transform
  and the destructor's part.

The load and the work order make the Pack here. Its slot 0x340 and `UNIT_ORDER`'s case 1 are
the tasks pack-install-move and pack-unit-order.

## In the game

- `TRIBE_Action_Pack` (action type 0x7c, vtable `0x142ed7978`).

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`install.rs`](install.rs) | A Pack's making and its start: the constructor with its unit AI's order, the install through slot 0x808, the start (slot 0x28) and `set_state` with its construction sprite and the annexes it destroys, and the target it takes. |
| [`mod.rs`](mod.rs) | A building's Pack: `TRIBE_Action_Pack` (action type 0x7c, vtable `0x142ed7978`), which a trebuchet (class 54) and `UNIT_ORDER`'s case 1 install on a building. |
| [`tests.rs`](tests.rs) | Tests: The Pack's tests on player 1's trebuchet 700 (master 42) and player 2's villager 800. |
| [`unit.rs`](unit.rs) | A Pack bound to its building and the world: its update with the building's work rate, its sprite's frame, its unit AI's attack and the transform, and its destructor's notify and the building it puts back. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`commands`](../../commands/), [`map`](../../map/), [`math`](../../math/), [`objects`](../../objects/), [`orders`](../../orders/), [`players`](../../players/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
