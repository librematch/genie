# `orders/town_bell`: the town bell's commands

A player rings a town centre's bell: the villagers near it go into the buildings that take them.
Ending the bell, or the back-to-work command, lets them out and sends each back to the work its
record holds (`town_bell.rs`).

- `commands.rs` holds `TOWN_BELL` (ring and end) and the back-to-work command.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`commands.rs`](commands.rs) | The town bell's commands: `TOWN_BELL` (0x7f) rings a town centre's bell or ends it, and the back-to-work command sends the owner's villagers back without the bell's flags. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`commands`](../../commands/), [`garrison`](../../garrison/), [`math`](../../math/), [`movement`](../../movement/), [`objects`](../../objects/), [`players`](../../players/), [`world`](../../world/).
<!-- /uses -->
