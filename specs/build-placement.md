---
name: build-placement
---

# BUILD placement, builder tasking and the spawn placement search

```json
{
  "id": "build-placement",
  "title": "BUILD placement, builder tasking and the spawn placement search",
  "category": "scope",
  "status": "open",
  "question": "The slice stops at tick 679 of MarvinBlue/426048731 player 2: its own BUILD (0x66) runs and 'BUILD tasks the builders' halts the lane; a QUEUE'd unit due to spawn likewise stops (placement not modelled). What is needed: (1) the map placement search that puts a foundation on the tile grid (first consumer of a real Map type in genie-core — positions are already part of the checkpoint ObjectState), (2) tasking the builders with the foundation (their orders; idle builders adjacent to the foundation may not need pathing yet — verify before assuming full pathfinding), (3) the spawn placement search for a unit due from a make action. Options: (a) model placement search first as the shared primitive, then BUILD tasking on top, then the spawn; (b) shortcut BUILD tasking for adjacent idle builders and defer the general placement search to the spawn. Recommended: (a), because the spawn needs the same search anyway and tick 679 is the current frontier of every QUEUE-following lane. Next step: decompile the placement search the game uses for a BUILD foundation and check whether the lane's builders move at all before tick 700.",
  "options": [
    "(a) model the placement search first as the shared primitive, then BUILD tasking on top, then the spawn",
    "(b) shortcut BUILD tasking for adjacent idle builders and defer the general placement search to the spawn"
  ],
  "recommended": "(a): the spawn needs the same search anyway and tick 679 is the current frontier",
  "verify": "cargo test -p genie-oracle --test tick_trace_corpus --test word9 (GENIE_REC_CORPUS/GENIE_DAT set): MarvinBlue/426048731:p2 holds past tick 679 with its foundation placed and word 1/9 intact",
  "blocks": "MarvinBlue/426048731:p2 at tick 679; every lane behind a QUEUE due to spawn",
  "unblocks": ["unit spawn (placement)", "builder tasking", "foundation lifecycle", "first Map type in genie-core"],
  "notes": "Likely touches genie-dat (foundation master data) and the generated readers (map load is already in genie-rec). If builders must move, this spec grows pathing — split that into its own spec and keep this one to placement.",
  "outcome": null,
  "next": null
}
```

## Evidence

- `4f14d02` commit message: "A unit due to spawn stops the player (placement
  not modelled)… tick 679 is its own BUILD."
- `Command::Build` in `genie-core/src/turn.rs` places the foundation object but
  inserts `Unsupported("BUILD tasks the builders")` immediately.
- Position is part of the checkpoint's per-object state (`ObjectState.position`),
  so placement must be exact, not approximate.
