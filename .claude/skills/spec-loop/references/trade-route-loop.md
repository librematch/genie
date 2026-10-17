# The trade-route loop (specialization for trade-route)

How the generic spec loop applies to the ¾-view medieval wagon death race at
`/home/otto/projects/trade-route` (Babylon.js 8.56 + Vite + TypeScript,
shipped as a Discord Activity and a plain web page).

## The structural gift: a deterministic sim already exists

- `src/sim/*` is **pure and deterministic**: no DOM, no engine, no
  `Math.random`, its own maths (`dmath.ts`), its own seeded RNG (`rng.ts`).
  Every gameplay constant lives in `src/sim/tuning.ts` (upgrades in
  `upgrades.ts`).
- The Node score host **replays heats** with the sim (`server/sim.bundle.mjs`),
  so replay determinism is load-bearing, not aspirational.
- The sim carries a **version hash** (e.g. `3cfbf557efdafbf8` after the
  2026-10-16 speed pass). A sim change moves the version and **invalidates
  online replay proofs** — every spec that touches the sim must state this.

This is the genie pattern at game scale: the sim is the oracle-executable
ground truth, and replays are its recordings.

## The oracles (verification half)

| Oracle | Command | Catches |
|---|---|---|
| Unit/behaviour tests | `npm test` (vitest; `pretest` rebuilds the server bundle) | sim logic, encounters, hazards, driving, audio cues |
| Types | `npm run typecheck` (+ `typecheck:babylon`) | view/sim drift |
| **Determinism/replay** | seeded heats through `src/sim`; state hashes; sim version key | accidental non-determinism, view-side mutation of sim state |
| **Balance proof** | `npm run probe` (scripts/winners.ts), `scripts/capture-balance.cjs`, held-out proofs | archetype/meta drift — the horse:oxen ratio (~1.8) is **load-bearing** |
| Visual captures | `scripts/capture-*.cjs` (Playwright): pixels, flow, hazards, gates, fx, pickups, military… | readability-feature regressions: player ring, rival pennants, tower flare + range ring, pikemen pikes, villagers diving, gate swing/break, dust and splash |
| Performance | `npm run perf` / `perf:all` | frame budget, draw calls |

Ranking for `verify`: determinism > tests > balance > captures > perf. A sim
spec's `verify` must include the replay/determinism gate; a view spec's must
include the relevant capture.

## Diagnosis (step 1 of the cycle)

- **Balance/meta issue** → run `npm run probe` across maps, read the winner
  distribution, find the archetype×map combination off target.
- **Sim bug** → failing vitest or a replay mismatch; bisect to the tuning
  constant or the `step.ts` logic; cite the constant name, not a vibe.
- **Readability/render** → run the matching `capture-*.cjs`; diff against the
  archived shots (`docs/notes/shots`).
- **Feel/design** → the target lives in `docs/GDD.md`; the current state in
  the latest `docs/handoff-*.md` / dated `docs/notes/*.md` and CHANGELOG.

## House rules (from AGENTS.md — do not drift)

- **Ownership seams**: `src/sim/*` stays pure; `src/view/*` only reads sim
  state and the event queue; shared meshes outlive heats (mesh records +
  `BbBatches`, never disposed between heats; every new mesh needs a correct
  AABB via `aabbOf`).
- Every gameplay constant in `src/sim/tuning.ts`/`upgrades.ts` — no magic
  numbers in views or game code.
- **Multi-session repo**: several Claude sessions work simultaneously, each in
  its own worktree under `.claude/worktrees/`; `main` can move by dozens of
  commits in an hour. Start work in your own worktree from current `main`,
  re-check `git log` before merging.
- Read `docs/GDD.md` before gameplay/UI/graphics work; read the latest
  handoff and CHANGELOG before continuing; they may be behind `git log`.
- Sim change ⇒ sim version bump ⇒ proof invalidation noted in the spec's
  `outcome`.
- Host-wide rules (GPUs, ports, the local model) live in
  `/home/otto/AGENTS.md`.

## Known frontier (2026-10-16)

- Speed pass landed (`1bcb0f0`): horses cruise 69–74 mph, oxen 41–42 mph,
  sim version `aff97e45280db8b9` → `3cfbf557efdafbf8`; online replay proofs
  no longer verify (owner-accepted).
- Open: re-establish the held-out balance proof on the new sim; the owner
  wants oxen ≥ 60 mph without handing every gold heat to the Hauler (the
  speed-pass note measured that pushing oxen toward 60 mph did exactly that).

## Open gaps (do not invent)

- The replay-determinism gate (seeded heat + state hash) is enforced by the
  score host in production but not wired into `npm test`/CI as a regression
  gate — candidate spec.
- Visual captures are manual scripts, not a regression gate — candidate spec.
- The local corpus of this project is its own maps/replays; there is no
  external ground truth (unlike genie) — the sim *is* the truth, so the
  loop's risk shifts from "matches the game?" to "matches the design intent?".
