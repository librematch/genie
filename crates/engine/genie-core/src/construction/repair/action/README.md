# `construction/repair/action`: the Repair's state entry

A Repair's `set_state` first leaves its old state, then enters the new one. The entry by the
state is here: the standing sprite and the notify of a done repair, and the walks to the point
or to the target.

- `enter.rs` holds the entry by the new state.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`enter.rs`](enter.rs) | A repair's entry into its new state: the part of its `set_state` by the state. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`objects`](../../../objects/), [`unit_ai`](../../../unit_ai/), [`world`](../../../world/).
<!-- /uses -->
