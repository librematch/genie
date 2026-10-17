# Adopting the spec loop to a new project

The skill is a loop kernel: `SKILL.md` (the cycle), `scripts/knowledge.py`
(the index), `references/spec-template.md` (the spec shape) are
project-independent. Adopting a project means writing **one reference file**
and seeding the backlog. Recipe:

## 1. Find the ground truth that already exists

Read AGENTS.md / README / design docs / CI config / test dirs / scripts.
Look specifically for:

- **Byte-exact oracles**: golden files, round-trip tests, recorded corpora
  (genie: recordings + GRTICK1 traces).
- **Deterministic cores**: pure sim modules, seeded RNG, replay/host modes
  (trade-route: `src/sim/*` + `server/sim.bundle.mjs`).
- **Executable binaries to compare against**: the original game/app you are
  re-implementing, a headless harness, a reference implementation.
- **Golden-output oracles**: recorded API responses, snapshot tests.
- **Visual oracles**: deterministic capture + pixel diff (Playwright scripts,
  screenshot archives).
- **Performance oracles**: benchmark scripts with recorded baselines.

Rank the oracles by strength; the strongest runnable one is the default
`verify`.

## 2. Decide where diagnosis comes from

For each oracle: what tool produces "where it stopped and what differs"?
If the diagnosis needs a tool that does not exist yet, that tool is itself
the first spec (genie's bisect CLI; trade-route's probe scripts). Do not
invent evidence — an adopted loop with no diagnosis source is a TODO list.

## 3. Write `references/<project>-loop.md`

Required sections (copy the structure from `genie-re-loop.md` or
`trade-route-loop.md`):

1. Two halves: **diagnose** (where the evidence comes from) and **verify**
   (the oracles, ranked, with runnable commands).
2. **House rules** from the project's own AGENTS/CONTRIBUTING/ownership
   seams — the loop must not fight the repo's existing discipline.
3. **Known frontier** — dated, with the concrete stopping point.
4. **Open gaps** — ending with the instruction "do not invent; leave as
   gaps or ask".

## 4. Seed the backlog

- One **closed** spec (a thing that already landed, with its outcome and the
  wall it exposed) — this proves the loop can close.
- One or two **open** specs at the real frontier, with options and a
  recommended one.

## 5. Install and index

```sh
mkdir -p .claude/skills && cp -r <skill-dir>/spec-loop .claude/skills/
python3 .claude/skills/spec-loop/scripts/knowledge.py init
python3 .claude/skills/spec-loop/scripts/knowledge.py index specs/*.md
grep -qx .knowledge.db .gitignore || echo .knowledge.db >> .gitignore
```

Keep the global copy (`~/.config/opencode/skills/spec-loop`) in sync or let
the project copy be canonical for that project.

## 6. Make `verify` runnable on a clean checkout

Missing toolchain pieces (rust targets, playwright browsers, node version)
get fixed or scripted — see genie's `tools/oracle.sh` pattern (env file +
PATH self-healing + required env like `RUST_MIN_STACK`). An oracle that only
runs on one machine is a gap, not a verify.

## Fit check (before adopting)

The loop pays off when: a runnable oracle exists (or can be built as spec 1),
work is incremental and discovery-driven, and cycles are long enough that the
ceremony amortizes. It does *not* fit one-off scripts, pure creative work
with no binary success criterion, or codebases with no test surface.

## Worked examples

- `genie-re-loop.md` — oracle borrowed from the original binary (strongest
  possible ground truth; corpus not self-owned).
- `trade-route-loop.md` — oracle is the project's own deterministic sim
  (self-owned ground truth; risk shifts to "matches design intent").
