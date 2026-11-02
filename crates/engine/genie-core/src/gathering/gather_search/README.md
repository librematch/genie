# `gathering/gather_search`: the resource search's passes

The search for the closest resource floods the map tile by tile. Its passes start from a drop
site's footprint or from the unit's tile. Each pass orders its tiles and takes the first
resource a tile holds.

- `passes.rs` holds the passes, the flood's next tiles and their order.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`passes.rs`](passes.rs) | The resource search's passes: the tiles it starts from, the flood's next tiles and its order, and the first resource a tile holds. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../map/), [`math`](../../math/), [`objects`](../../objects/), [`players`](../../players/), [`vision`](../../vision/), [`world`](../../world/).
<!-- /uses -->
