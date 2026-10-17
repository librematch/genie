---
name: Spec Loop
description: >-
  Spec-driven development with a .knowledge.db backlog. Use when creating a
  feature spec, picking up the next backlog question, or running a
  diagnose-implement-verify loop against a test oracle (e.g. the genie
  reverse-engineering loop). Creates/updates spec files with a JSON meta header
  and indexes them into a .knowledge.db sqlite database.
---

# Spec Loop

A loop for work where the next step is *discovered* rather than planned:
reverse engineering, engine reimplementation, decoding unknown formats, or any
test-oracle-driven development. One cycle = diagnose → spec → implement →
verify → index → repeat.

## The artifacts

1. **Spec files** — one Markdown file per question/mechanism, living in the
   repo (default `specs/`), with a JSON meta header as the first fenced block.
2. **`.knowledge.db`** — a sqlite database at the repo root indexing every
   spec, its status, and outcomes. Managed only through `scripts/knowledge.py`.

## Cycle

### 1. Diagnose

- Run the project's verification loop first. In genie that is the oracle
  tests (`word9`, `tick_trace_corpus`) plus the RE harness inspection; in other
  projects it is the failing test, the bug report, or the profiler output.
- The diagnosis must be *concrete*: where the failure is, what value differs
  from what, and the minimal mechanism that would close the gap.
- If a prior spec is done, its `outcome` and `next` fields are the input here.

### 2. Spec

- Copy `references/spec-template.md` to `specs/<kebab-id>.md` and fill it in.
- The JSON header is the contract. `question`, `category`, and `options` with
  a `recommended` option are mandatory before implementation starts.
- Discovery may use subagents (e.g. `explore`) freely — this is the
  read-only, fan-out phase. Implementation, later, may use at most one agent.
- Decide the verification step *before* implementing and write it into
  `verify` as a command that either passes or does not.

### 3. Index

- Run `python3 <skill-dir>/scripts/knowledge.py index specs/<id>.md` so the
  spec lands in `.knowledge.db`.
- `--db` defaults to `.knowledge.db` in the current directory; pass the repo
  root if working elsewhere.

### 4. Implement

- Test-driven: write the failing test from `verify` first, make it pass, keep
  every other test green.
- Small, verifiable steps; commit per verified step with a message naming the
  crate/module and the behaviour (see the genie reference for the house
  style).
- **Do not refactor while implementing.** Fast-iteration phase: idiomatic
  redesign happens later, in a separate pass that must keep the tests green.
  The tests are the compatibility contract, not the code's shape.
- If the work requires touching adjacent crates/modules (in genie: the format
  crates), that is in scope for the same spec — note it in `notes`.
- At most one implementation agent at a time; discovery agents may run
  alongside it.

### 5. Verify

- Run the full verification suite, not just the new test.
- On success, pin the new behaviour with a permanent test so regressions are
  loud (in genie: a pinned lane/checkpoint test).

### 6. Index and continue

- `knowledge.py update <id> --status done --outcome "<what landed, evidence,
  and the new wall it exposed>"`
- The `outcome`/`next` you write *is* the diagnosis input of the next cycle.
- When the next question is not already clear, query the backlog:
  `knowledge.py next` (oldest open spec) or `knowledge.py list --status open`.
- Rank by *lanes unblocked*: prefer the mechanism whose completion unblocks
  the most blocked items.

## Guardrails

- Never implement without a spec whose `verify` is runnable.
- Never edit `.knowledge.db` with raw sqlite when `knowledge.py` covers it.
- Long unattended runs: commit after every verified step so a crash loses at
  most one step, and end the run when a cycle's `verify` cannot be satisfied
  — record that as `blocked` with the reason, not as "done".
- Honest status only. A partial pass is `in-progress` with what remains;
  claiming done without the suite passing is the one cardinal failure mode.

## Adapting to a new project

The loop is generic; only step 1 (diagnose) and step 5 (verify) are
project-specific. Read `references/genie-re-loop.md` for the genie
specialization, and write a sibling reference file for any other project that
adopts this loop (name it after the project, keep it beside this skill).
