# `orders`: the orders to units

A command or a unit AI tells units what to do. This folder turns that order into the units'
actions.

INTERACT is the most common order: a right click on a target or a point. The order splitter
(`split.rs`) decides which units move alone and which form a unit group. Each unit then gets the
work command (`mod.rs`, `work.rs`). The unit takes the master and the task that the task finder
picks for the target. The task's action then replaces the unit's action list.

The files hold the orders by kind:

- Moves, follows and patrols: `moves.rs`, `follow.rs`, `patrol.rs`, `interact.rs`, `points.rs`.
- Work: `tasks.rs` (hunt, gather, build, repair), `gather.rs`, `attack.rs`, `drop_site.rs`.
- The unit's state and its stops: `commands.rs` (STOP, EXPLORE, the auto scout),
  `unit_order.rs`, `actions.rs`.
- `town_bell.rs`: what a villager worked on before an order took it away, for the way back.
- `start_work.rs`: the Empire Wars start, which puts every idle villager to work in the first tick.

## In the game

- `TRIBE_Command`'s order cases, the order splitter and the work command (object slot 37). The
  HD edition's debug symbols name no separate source file for them.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`actions.rs`](actions.rs) | Taking and releasing targets, the order record, actions cleared, transformations, gates and master switches. |
| [`attack.rs`](attack.rs) | An attack's start, a hunt's and an action's retarget. |
| [`commands.rs`](commands.rs) | The unit AI's state, gates, the auto scout and the STOP, EXPLORE and standing commands. |
| [`dispatch.rs`](dispatch.rs) | How an order reaches its units (`Dispatch`). |
| [`drop_site.rs`](drop_site.rs) | The drop-offs: an unasked one, the ordered drop site, the table's, one now and elsewhere, and the way back. |
| [`follow.rs`](follow.rs) | FOLLOW and GUARD, a move to an object and the head action's move. |
| [`form.rs`](form.rs) | `FORM_FORMATION`'s handler (`FUN_14161dba0`): the listed units are split into groups, which lock to the formation the command names. |
| [`gather.rs`](gather.rs) | A gather's retarget and take, the quarry, a gather's and a hunt's start. |
| [`interact.rs`](interact.rs) | An interact order's unit, an order on a target, MOVE and its groups. |
| [`mod.rs`](mod.rs) | The orders a command gives its units: INTERACT and its splitter into loose units and groups, the moves, patrols and follows, the work, gather, hunt and build orders, the drop-offs, the town bell and the Empire Wars start. |
| [`moves.rs`](moves.rs) | A unit's move, to a point or aside, a work order, its reach and a point's move order. |
| [`patrol.rs`](patrol.rs) | PATROL, the attack move and their sweep, and the group the splitter keeps. |
| [`points.rs`](points.rs) | Where the units of an order go: their centroid, offsets, spreads and grid points. |
| [`split.rs`](split.rs) | INTERACT and the order splitter: which units move loose and which form groups. |
| [`start_work.rs`](start_work.rs) | The Empire Wars start: in the game's first update every idle villager is put to work (`BaseWorld::update`, `0x14154fc70`, its per-player part before the player's objects update). |
| [`tasks.rs`](tasks.rs) | The hunt and gather commands, a build's and a repair's start, retake and target. |
| [`tests.rs`](tests.rs) | Tests. |
| [`town_bell.rs`](town_bell.rs) | A villager's town-bell record (`+0x9ac`..`+0x9dc`): what it worked on when an order to a target took it off its work (`FUN_141206a20`), which a later back-to-work order sends it back to, and the flag a garrison order sets (`FUN_1412067f0`). |
| [`unit_order.rs`](unit_order.rs) | An artifact's and a herd's take, an order's cancel and a unit's order. |
| [`work.rs`](work.rs) | A kept attack, the garrison task and the work command and its task. |
| [`gather/`](gather/) | The work command's Hunt. |
| [`work/`](work/) | The garrison finder's view. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`commands`](../commands/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`ownership`](../ownership/), [`pathing`](../pathing/), [`players`](../players/), [`production`](../production/), [`technology`](../technology/), [`testing`](../testing/), [`unit_ai`](../unit_ai/), [`world`](../world/).
- Used by: [`commands`](../commands/), [`construction`](../construction/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`groups`](../groups/), [`objects`](../objects/), [`ownership`](../ownership/), [`production`](../production/), [`unit_ai`](../unit_ai/), [`world`](../world/).
<!-- /uses -->
