---
name: order-movement
---

# An order moves its units

```json
{
  "id": "order-movement",
  "title": "An order moves its units",
  "category": "scope",
  "status": "open",
  "question": "The slice stops with 'an order moves its units' within the first ~20 ticks on nearly every lane of the local corpus — the dominant frontier (28 of 30 sampled recordings; earliest at tick 7, most at tick 9). Movement is the next large system: an order sets targets and the units' update integrates velocity along a path every tick, and the checkpoint words count something it changes (word 9's unit-AI top action, and possibly position-bearing words). What is the first slice of movement that a lane can reproduce word-for-word: the order's target assignment, the path (or straight-line step), the velocity integration, or the unit-AI state transition it causes? Options: (a) decompile first — read the order handler and the unit movement update in the 162286 build (AoE2DE_s.exe in the desktop-main Steam install; openage's doc/reverse_engineering has the AoC-era siblings), then model the smallest complete step; (b) probe first — instrument genie-core to run the '@2026.01.08 163608 (6)' lane past tick 75 with a straight-line movement guess and see which words move. Recommended: (a), because movement is a per-tick continuous system where a wrong model shows up only as a silent word drift; the binary names the functions and the RNG streams, which the checkpoint compare alone cannot.",
  "options": [
    "(a) decompile the order handler and movement update in the 162286 build first, then model the smallest complete step",
    "(b) probe first: straight-line movement guess instrumented in genie-core, read which words move"
  ],
  "recommended": "(a): a wrong continuous model shows up only as silent word drift; the binary names the functions and streams",
  "verify": "cargo test -p genie-oracle --test local_corpus (GENIE_REC_CORPUS/GENIE_DAT set): one lane of a movement-frontier recording reproduces past its first order tick with word 9 intact",
  "blocks": "28 of 30 sampled local-corpus lanes (ticks 7–471); with build-placement, essentially every non-idle lane",
  "unblocks": ["the dominant corpus frontier", "gather/work behavior", "spawn placement search", "build tasking with real approach"],
  "notes": "Discovery needs the 162286 build's exe: C:\\Program Files (x86)\\Steam\\steamapps\\common\\AoE2DE\\ on desktop-main (copy to aibox for offline disassembly). The tier-1-era RE harness (aoe2de-exe-re/load_oracle, the GRTICK per-tick trace) is not on any reachable host; tick_trace tests stay skipped until then or until a per-tick trace is regenerated from a local build. openage's doc/reverse_engineering/game_mechanics has the AoC-era movement notes the repo already cites.",
  "outcome": null,
  "next": null
}
```

## Evidence

- The oracle probe on the 30-recording save-66.6/162286 sample (2026-10-16):
  first unsupported is `an order moves its units` on 28 of 30 MP recordings,
  from tick 7 onward (`@2026.01.08 163608 (6)` at tick 75 is the latest).
- `genie-core` `Unsupported("an order moves its units")` — the gate the loop is
  designed around; the two reproduced idle lanes (p7, p8 of
  `@2026.01.08 163608 (6)`) show the slice is sound up to the first order.
- The spec-loop reference names movement as the next large system (2026-10-16).
