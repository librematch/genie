---
name: queue-production
---

# QUEUE and production up to the spawn

```json
{
  "id": "queue-production",
  "title": "QUEUE and production up to the spawn",
  "category": "scope",
  "status": "done",
  "question": "The slice's next system: a lane's own commands. MarvinBlue/426048731 player 2 leaves the slice at tick 553 with a villager QUEUE (0x81: u16 buildings +4, building master +0xa, unit +0xc, amount +0xe, flag +0x10, building ids +0x14) at its Town Centre: word 1 (resources) and word 9 (the Town Centre's slot 86 reads its production-queue action list, +0xbf8, in state 6). What production needs, read so far: the player's queue (FUN_141614450) spreads the amount over the buildings (15 per queue) and pays the cost; the building's queue (FUN_1411cbc40) draws once from the Action stream per villager (raw*100 % 100 < 50 picks 83, else 293) and adds a 0x30-byte entry at +0xbe8; the spawn later needs the map's placement search and the villager unit AI. Options: (a) port QUEUE and production up to the spawn (cost, the Action-stream draw, the queue entry, the make action and its timer), then the spawn with placement; (b) port do_command's dispatch first and grow handlers in the order the lanes ask. Recommended: (a) inside (b), since QUEUE is the first command of nearly every lane. Next step: decompile the make action's update and the cost (word 1 drops by 46, not 50), then a test against tick 553.",
  "options": [
    "(a) port QUEUE and production up to the spawn (cost, the Action-stream draw, the queue entry, the make action and its timer), then the spawn with placement",
    "(b) port do_command's dispatch first and grow handlers in the order the lanes ask"
  ],
  "recommended": "(a) inside (b), since QUEUE is the first command of nearly every lane",
  "verify": "cargo test -p genie-oracle --test word9 --test tick_trace_corpus (GENIE_REC_CORPUS/GENIE_DAT set)",
  "blocks": "MarvinBlue/426048731:p2 beyond tick 553",
  "unblocks": ["unit spawn", "placement search", "BUILD handler reuse"],
  "notes": "From simonsan's spec-loop question, 2026-10-16. Original next step: decompile the make action's update and the cost (word 1 drops by 46, not 50), then a test against tick 553.",
  "outcome": "Landed as 4f14d02 (genie-core: QUEUE up to the spawn; the first lane holds to tick 678). Command::Queue decodes the 0x81 packet; cost paid per entry from the player master's build_inventory (read via genie-oracle from the save); per-villager male/female master draw from the Action stream (raw*100 wrapping % 100 < 50 -> 83 else 293, FUN_1411cbc40); 0x30-byte queue entry at +0xbe8; make action TRIBE_Action_Make_Obj (update 0x140ad0490) works rate*dt per tick (mWorkRatePowerup + master work_rate), building reports state 6. Lane matches the oracle's whole block through the three villager QUEUEs to tick 678. A unit due to spawn still stops the player (placement not modelled). Unmodelled commands become Command::Unmodelled, stopping the issuing player.",
  "next": "Tick 679 is player 2's own BUILD: placement plus builder tasking (see build-placement)."
}
```

## Evidence

- Commit `4f14d02` (2026-10-16): 469 insertions across `genie-core` (production.rs,
  rng.rs, turn.rs) and `genie-oracle`.
- Cost accounting: word 1 drops by 46 per villager, not the nominal 50 — resolved
  by reading the cost from the master's `build_inventory` rather than a constant.
