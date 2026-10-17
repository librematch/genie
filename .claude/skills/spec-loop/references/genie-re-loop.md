# The genie RE loop (specialization for librematch/genie)

How the generic spec loop applies to the deterministic AoE2 DE engine
reimplementation (`github.com/librematch/genie`; the loop runs from the repo
root). Two halves of one loop:

- **Discovery half** — the game's own binary running headlessly under the
  private `aoe2de-exe-re` workspace (`load_oracle`). A spec's evidence comes
  from here: which function runs, which RNG stream it draws, which struct
  offsets it touches, tick by tick while a recording plays.
- **Verification half** — the Rust-side oracle in this repo: recordings store
  checkpoints (~every 500 syncs) holding 11 words per player ("lanes"); the
  reimplementation must reproduce them word-for-word.

## The discovery machine (aoe2de-exe-re / load_oracle) — private

Documented from the maintainer's own write-up ("How a load runs",
2026-10-16):

1. `image.py` maps the PE (cached as a pickle; pefile's mapping is slow).
2. `machine.py` is a lean Windows user-mode machine over unicorn: stack,
   TEB/TLS, a bump heap shared by Python and native code, import stubs. Only
   stubs, replaced functions and watched addresses are hooked. An unknown
   import fails loudly.
3. `crt.py` serves CRT/Win32 imports; hot ones (`malloc`, `memcpy`, ...) as
   native code. Time sources are constants so runs reproduce.
4. `startup.py` runs the CRT initializer tables (~7,500 C++ static ctors) and
   TLS callbacks, found from the entry code.
5. `environment.py` prepares game initialization: table-building init
   routines, registry placeholders filled from data files, zeroed
   singletons, and skips of routines that read no stream byte (telemetry,
   texture streaming, the CaptureAge spectator server, XS script
   compilation). Every entry names its evidence.
6. `datfile.py` makes a `.dat` the build accepts: the install's VER 8.9 file
   rewritten as VER 8.8 with genieutils-py (they differ in one count).
7. `header.py` constructs the World, loads the `.dat` into it (World vtable
   slot 29, as FUN_141530590 does), then mirrors the playback entry
   FUN_1415374f0: the save-game file header (FUN_140edcb10), the version
   global, the recording's GameOptions as the current ones, and World
   vtable slot 7 (`load_world`) over an RgeStream in memory mode.
8. `oracle.py` initializes once (~105 s) and loads each recording in a
   process forked from that state, in parallel.

Practical meaning for spec work: "decompile X" means **run X under
load_oracle on real inputs and watch what it does**, then read its body.
Traces it emits:

- `GRTICK1` (FD-015): per-tick ground truth of the game's simulation while it
  plays a recording — `u32` tick count, `u32` lane count, then zlib rows of
  world time + each lane's 11 checkpoint words. Format decoded in
  `genie-oracle/src/tick_trace.rs`. Also a mid-game snapshot: the game's own
  save after tick 999 of `MarvinBlue/426048731` (a 4,414,674-byte header,
  read to its last byte by the readers).
- Read traces (`GENIE_REC_TRACES`): a record of the game's own byte reads
  during header load; the Rust reader must never cross one (see
  verification techniques).

## Environment variables (tests skip silently when unset)

| Variable | Feeds | Notes |
|---|---|---|
| `GENIE_REC_CORPUS` | corpus tests | tree of `.aoe2record` files grouped in per-player dirs (`MarvinBlue/426048731.aoe2record`) |
| `GENIE_DAT` | `MasterTable` | an `empires2_x2_p1.dat` (VER 8.9 installed; oracle also handles the VER 8.8 rewrite) |
| `GENIE_GAME_DAT` | genie-json tests | the game install dir (typed JSON data tables) |
| `GENIE_REC_TRACES` | `tests/de_oracle.rs` | load_oracle's read traces |
| `GENIE_TICK_TRACE` + `GENIE_TICK_REC` | tick-oracle tests | a GRTICK1 file and its recording |

## Tier-1 (the corpus in code's terms)

- **Tier-1 = recordings with save version 66.6 (`hundredths()==6660`) from
  build 158041**: 137 recordings, 22 player lanes; scenario games
  (GameOptions `mGameMode == 3`) and recordings without a checkpoint are
  reported, not compared.
- The corpus is not public; tier-2 (other save versions/builds) is not yet
  defined in code.

## The 11 checkpoint words (complete; from `genie-core/src/checkpoint.rs`)

Derived by `WorldPlayerBase::create_checksum` (`0x1416607a0`); full word
table in the workspace's `docs/AoE2DE_s_exe_SIMULATION.md`. All sums wrap in
u32; every float→int step is x86 `CVTTSS2SI` (truncate toward zero, NaN/out
of range → `i32::MIN`).

| Word | Content |
|---|---|
| 0 | sleeping-object count |
| 1 | sum of the 4 resources, each truncated (CVTTSS2SI) |
| 2 | sum of `main_id` (sign-extended i16) over live objects |
| 3 | sum of object state bytes |
| 4 | sum of truncated `amount_held` |
| 5 | ObjectWaypoints — **always 0** (engine zeroes it, never writes) |
| 6 | live-object count |
| 7 | sum over objects of truncated (x\*100 + y\*100 + z\*100), scaled in f32 |
| 8 | player id |
| 9 | sum of (top-action type + action state byte + unit-AI retarget timer); objects without an action list contribute −1 |
| 10 | sum of object ids |

Word 9 is the only word the save alone cannot determine — it needs the
simulation (`SAVE_WORDS = [0..8, 10]`).

## Verification techniques used in this repo (reuse them)

1. **Lane pinning**: every newly reproduced lane/tick gets a permanent test in
   `genie-oracle/tests/` so regression is loud.
2. **Byte-for-byte round-trip**: header read→write must reproduce all 137
   tier-1 headers (1,272,883,998 bytes) exactly (`tests/de_roundtrip.rs`).
3. **Read-nesting oracle** (`tests/de_oracle.rs`): every read of the Rust
   reader must nest inside — or cover whole — one of the game's recorded
   reads; the first partial overlap is reported with both readers' bytes,
   crossings counted per game caller.
4. **Frontier honesty**: report the first tick where the slice stops and why
   (`Unsupported`/`Command::Unmodelled`); that is the next spec's `question`.
5. **`.dat` as data, never constants**: game values come through `genie-dat`
   / generated readers (hardcoding has needed steering before).

## Workspace doc references cited by commits (private, aoe2de-exe-re)

- `docs/AoE2DE_s_exe_SIMULATION.md` — checkpoint word table derivation
- FD-007 — per-frame order (SeedAll; op 2 then op-1 commands)
- FD-014 — layout mirror generator (save-write ↔ load-read naming)
- FD-015 — tick oracle trace/snapshot (GRTICK1)
- FD-016 — file census (workspace-wide, drives `genie-formats` probe)
- `scripts/dat_masters.py` — former TSV master table, replaced by `genie-dat`

## House rules (do not drift)

- Engine crates stay warning-free:
  `cargo clippy --locked --no-deps -p genie-core -p genie-oracle --all-targets -- -D warnings`
- Engine core compiles for wasm32.
- Comments follow the repo register: plain declarative prose naming the
  game's own routines and offsets (`FUN_1411cbc40`, `+0xbf8`); every claim
  carries its address.
- Commit style: `crate: behaviour claim; the evidence/frontier` — see `git
  log` for examples (QUEUE, artifact, transport-ship commits).
- Fast-iteration phase: no idiomatic-Rust refactoring; the later redesign
  keeps tests green as the contract.
- Scope may spill into format crates (`genie-dat`, `genie-json`, future
  `genie-sld` etc.) — that is in-spec, noted in `notes`.
- Reference implementations may be ported with licence care (genieutils,
  LGPL-3.0, Tapsa — the game itself ships it with Advanced Genie Editor).

## Known frontier (2026-10-16)

- **22/22 tier-1 lanes exact, 0 off, 0 unsupported** at word 9 checkpoint 1
  (`word9.rs` pins `(22, 0, 0)`); `MarvinBlue/426048731:p2` matches the tick
  oracle's whole block through its three villager QUEUEs to tick 678; tick
  679 is its own BUILD (see specs/build-placement).
- Modelled so far: WELL512a RNG + SeedAll (11 streams), idle unit-AI turn,
  retargeting (soldier/transport/livestock), Artifact action, orders'
  idle-timeout redraws, dead-object lifecycle, BUILD foundations (unit-AI
  ctor draws), QUEUE + production up to spawn.
- Known unsupported: builder tasking, placement search, movement ("an order
  moves its units"), computer-player look-around.

## Open gaps (do not invent; leave as gaps or ask)

1. **Corpus access** — recordings, `.dat` and GRTICK1 traces are not public;
   without them all corpus tests skip.
2. **load_oracle source** — private; behaviour documented above, code not
   available. Trace *generation* is therefore impossible locally for now.
3. **The canonical skill and .knowledge.db schema** — this reference is a
   *reconstruction* built from public evidence only: commit messages, the
   Discord-pasted spec question, and the "How a load runs" write-up. Where
   it guesses, it says so; corrections fold in here.
4. **Pathing/movement RE status** — nothing public; unknown whether the
   workspace has started it.
5. **Tier-2 definition** — other save versions/builds unknown.
6. **FD-007/014/016 + `docs/AoE2DE_s_exe_SIMULATION.md` contents** —
   referenced by name only; FD-015's formats are decoded (GRTICK1 in
   `tick_trace.rs`; read traces in `de_oracle.rs`: u32 count, u32 width, zlib
   rows of `1+width` words — sizes, then call sites, then callers).
7. **Corpus breadth** — only tier-1 (save 66.6 / build 158041) is defined in
   code; how many other versions/builds the corpus holds is unknown.

Former gaps, closed by reading the repo (2026-10-16):

- *Which 2 lanes were unsupported* — the water-start transport-ship lanes
  (unit-AI type 11, `TribeTransportShipUnitAIModule`); resolved by c294c43,
  current suite pins (22, 0, 0).
- *All 11 checkpoint word meanings* — complete table above.
- *Read-trace format* — decoded in `tests/de_oracle.rs` (see gap 6).
