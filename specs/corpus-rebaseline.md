---
name: corpus-rebaseline
---

# The oracle re-anchors on the local save-66.6/162286 corpus

```json
{
  "id": "corpus-rebaseline",
  "title": "The oracle re-anchors on the local save-66.6/162286 corpus",
  "category": "scope",
  "status": "done",
  "question": "The tier-1 corpus (save 66.6, build 158041, 137 recordings incl. MarvinBlue/426048731) is not on any reachable host, so the oracle's pinned lane tests (word9, tick_trace_corpus) skip everywhere. The desktop-main savegame has 304 MP recordings of save 66.6 at build 162286 (game v101.103.31214, Dec 2025–Feb 2026) — the same save version tier-1 used — and the reader plus oracle already run on them. Can the oracle's first-checkpoint verification re-anchor on this corpus: pin the lanes it already reproduces as a permanent test, so the loop's verify half runs again? Options: (a) pin the reproduced idle lanes of one recording (the sample's '@2026.01.08 163608 (6)' reproduces lanes p7 and p8 exactly at checkpoint 1) as the new permanent oracle test; (b) attempt to recover the original tier-1 recordings from community replay archives before re-anchoring. Recommended: (a), because verification must be runnable before any further mechanism work; (b) can follow later for the other 135 recordings.",
  "options": [
    "(a) pin the reproduced idle lanes of the local corpus as the new permanent oracle test",
    "(b) attempt to recover the original tier-1 recordings from community replay archives before re-anchoring"
  ],
  "recommended": "(a): verification must be runnable before any further mechanism work",
  "verify": "cargo test -p genie-oracle --test local_corpus (GENIE_REC_CORPUS/GENIE_DAT set): lanes p7 and p8 of the local corpus's save-66.6/162286 recording '@2026.01.08 163608 (6)' reproduce word-for-word at the first checkpoint, and the workspace suite stays green",
  "blocks": "every oracle test (word9, tick_trace_corpus) — they all skip without the tier-1 corpus",
  "unblocks": ["build-placement on the local corpus", "queue-payment", "order-movement", "the whole verification half of the loop"],
  "notes": "The engine slice overflows the default 2 MiB test-thread stack on a real corpus (RUST_MIN_STACK=268435456 in the probe); the pinned test must run its work in a thread with a large stack so it is self-contained. The corpus lives on desktop-main (C:\\Users\\Benjamin\\Games\\Age of Empires 2 DE\\76561198274534921\\savegame, 1185 recordings) and a copy on bigbox (~/projects/aoeguilds/devuploads/aoe2recordings, 863); ~92 MB of save-66.6/162286 recordings are staged at aibox ~/genie/corpus66 with the .dat at ~/genie/dat. SP recordings fail earlier: the reader does not read the world's AI scripts (AIExpert) at header byte 0x8ca.",
  "outcome": "Landed: genie-oracle/tests/local_corpus.rs pins lanes p7 and p8 of the local corpus's '@2026.01.08 163608 (6)' (save 66.6, build 162286) word-for-word at the first checkpoint, in a 256 MiB thread (the slice overflows the default 2 MiB test stack), and pins p1's frontier ('an order moves its units', tick 75, object 0). The verify half of the loop runs again without the tier-1 corpus; suite green, clippy clean, wasm32 check clean. New walls it exposed: 28 of 30 sampled lanes stop at movement within ~20 ticks; save 67.2+ recordings reject at GameOptionsRead.version (1122 -> 1123); SP recordings need AIExpert world reading; one 'QUEUE the player cannot pay' gap.",
  "next": "order-movement (dominant frontier; discovery needs the 162286 exe from the desktop-main Steam install), then game-options-1123 and queue-payment (implementable now), then build-placement (needs placement-search discovery)."
}
```

## Evidence

- The oracle probe on the 30-recording save-66.6/162286 sample (2026-10-16):
  `@2026.01.08 163608 (6)`: exact 2 (lanes p7, p8), off 0, unsupported 6; the
  other MP recordings stop at 'an order moves its units', 'BUILD tasks the
  builders' (ticks 124/225/451) or 'QUEUE the player cannot pay'.
- `word9.rs` and `tick_trace_corpus.rs` hardcode the maintainer's recordings
  (`MarvinBlue/426048731.aoe2record`, …) and exact totals (137 recordings,
  692,263 commands); with any other corpus they cannot pass as written.
- Save-version map of the local corpus (probe, 2026-10-16): 66.6/build 162286 =
  game v101.103.31214 (Dec 2025–Feb 2026), 67.2/build 169652–175278 = Feb–May
  2026 (game options version 1123), 68.0/build 178524–180059 = Jun–Sep 2026,
  68.9/build 185872 = Oct 2026. Tier-1's save 66.6/build 158041 is an earlier
  66.6 build than any local recording.
