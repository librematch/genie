# Spec template

Copy to `specs/<kebab-id>.md`. The first fenced ```json block is the meta
header; `knowledge.py` parses it (unknown fields are preserved in `header`).
Everything after the header is for humans/agents: options analysis, evidence,
decompilation notes, implementation plan.

````markdown
---
name: <kebab-id>
---

# <Title>

```json
{
  "id": "<kebab-id>",
  "title": "<Title>",
  "category": "scope",
  "status": "open",
  "question": "What the verification loop exposed: where it stopped, which
    value differs from which, the minimal mechanism that closes the gap.
    Include concrete addresses/names (functions, struct offsets) already
    identified.",
  "options": [
    "(a) ...",
    "(b) ..."
  ],
  "recommended": "(a), because ...",
  "verify": "the exact command that proves this spec done, e.g. cargo test -p genie-oracle --test word9",
  "blocks": "<what cannot proceed while this is open (lane/test/component)>",
  "unblocks": ["<mechanisms enabled once this lands>"],
  "notes": "Implementation-scope notes: adjacent crates/modules to touch,
    fixtures needed, caveats.",
  "outcome": null,
  "next": null
}
```

## Options

(a) ...

(b) ...

## Evidence

- ... (decompilation digests, tick/word diffs, stream draws, screenshots)

## Plan

1. ...
````

Field semantics:

| Field | Meaning |
|---|---|
| `status` | `open` → `in-progress` → `done`, or `blocked` (with reason in `outcome`) |
| `blocks` | the concrete thing stalled; used for the ranked backlog |
| `unblocks` | what becomes possible; rank work by the count of lanes/items unblocked |
| `verify` | runnable command; written before implementation starts |
| `outcome` | written at close: what landed (commit), evidence, and the new wall |
| `next` | the follow-on question this spec's outcome generated |

Only change `status`/`outcome`/`next` via `knowledge.py update`; regenerate
the header's other fields by editing the spec and re-running
`knowledge.py index`.
