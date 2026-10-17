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
  "question": "A lane's own BUILD (0x66) runs and 'BUILD tasks the builders' halts it; a QUEUE'd unit due to spawn likewise stops (placement not modelled). What is needed: (1) the map placement search that puts a foundation on the tile grid (first consumer of a real Map type in genie-core — positions are already part of the checkpoint ObjectState), (2) tasking the builders with the foundation (their orders; idle builders adjacent to the foundation may not need pathing yet — verify before assuming full pathfinding), (3) the spawn placement search for a unit due from a make action. Options: (a) model placement search first as the shared primitive, then BUILD tasking on top, then the spawn; (b) shortcut BUILD tasking for adjacent idle builders and defer the general placement search to the spawn. Recommended: (a), because the spawn needs the same search anyway. Next step: decompile the placement search the game uses for a BUILD foundation (the AoE2DE_s.exe of build 162286 is available in the desktop-main Steam install) and check whether the lane's builders move at all before the frontier tick.",
  "options": [
    "(a) model the placement search first as the shared primitive, then BUILD tasking on top, then the spawn",
    "(b) shortcut BUILD tasking for adjacent idle builders and defer the general placement search to the spawn"
  ],
  "recommended": "(a): the spawn needs the same search anyway",
  "verify": "cargo test -p genie-oracle --test local_corpus (GENIE_REC_CORPUS/GENIE_DAT set): a local-corpus lane that stops at 'BUILD tasks the builders' reproduces to its BUILD tick with words 1/9 intact — e.g. the 66.6/162286 recording '@2026.01.15 212255 (7)' at tick 451",
  "blocks": "the lane's own BUILD on the local corpus (e.g. 'MP Replay @2026.01.15 212255 (7)' at tick 451, '@2026.01.11 165434 (1)' at tick 225); every lane behind a QUEUE due to spawn",
  "unblocks": ["unit spawn (placement)", "builder tasking", "foundation lifecycle", "first Map type in genie-core"],
  "notes": "The tier-1 corpus (MarvinBlue/426048731, tick 679) is not on any reachable host (checked aibox, bigbox, desktop-main, 2026-10-16), so the verify is re-targeted to the local save-66.6/162286 corpus from the desktop-main savegame; the frontier is the same mechanism there ('BUILD tasks the builders'). Likely touches genie-dat (foundation master data) and the generated readers (map load is already in genie-rec). If builders must move, this spec grows pathing — split that into its own spec and keep this one to placement.",
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
- The local save-66.6/162286 corpus (desktop-main savegame, 304 recordings)
  shows the same frontier: `@2026.01.15 212255 (7)` at tick 451,
  `@2026.01.11 165434 (1)` at tick 225, `@2025.12.26 235003 (1)` at tick 124
  (oracle probe, 2026-10-16).
