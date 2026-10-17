# specs

One Markdown file per open question/mechanism, in the shape described by
`.claude/skills/spec-loop/references/spec-template.md`: a JSON meta header
(first fenced block) plus free-form evidence and plan sections.

The files are canonical; `.knowledge.db` (gitignored) is a derived index
maintained by `.claude/skills/spec-loop/scripts/knowledge.py`:

```sh
python3 .claude/skills/spec-loop/scripts/knowledge.py init
python3 .claude/skills/spec-loop/scripts/knowledge.py index specs/*.md
python3 .claude/skills/spec-loop/scripts/knowledge.py next
```

Each spec carries a `verify` command written before implementation starts,
and closes with an `outcome` (what landed, and the wall it exposed) plus a
`next` (the follow-on question) — which becomes the next spec's `question`.
