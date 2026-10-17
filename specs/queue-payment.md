---
name: queue-payment
---

# QUEUE when the player cannot pay

```json
{
  "id": "queue-payment",
  "title": "QUEUE when the player cannot pay",
  "category": "scope",
  "status": "open",
  "question": "The slice stops with 'QUEUE the player cannot pay' at the first checkpoint of local-corpus recordings (e.g. '@2026.01.16 211610 (1)', tick 13, object 0). The landed QUEUE model (4f14d02) pays each entry's cost from the player master's build_inventory as it decodes the 0x81 packet; it does not say what happens when the player's resources do not cover the cost. Does the game still enqueue (going into debt), reject the entry, enqueue a shorter list, or pay from somewhere else (e.g. deduct from the player attributes as the queue drains)? Options: (a) instrument the decode in genie-core to log the player's resources, the entry cost and the exact fail point on the '@2026.01.16' recording, then model the observed rule; (b) decompile the QUEUE handler in the 162286 build first (the AoE2DE_s.exe is in the desktop-main Steam install) and read the rule off the binary. Recommended: (a), because the checkpoint words already decide which rule the game used on this recording — the log plus the checkpoint is the oracle.",
  "options": [
    "(a) instrument the decode on the failing recording and model the rule the checkpoint words imply",
    "(b) decompile the QUEUE handler in the 162286 build first and read the rule off the binary"
  ],
  "recommended": "(a): the checkpoint words already decide which rule the game used on this recording",
  "verify": "cargo test -p genie-oracle --test local_corpus (GENIE_REC_CORPUS/GENIE_DAT set): the lane(s) of '@2026.01.16 211610 (1)' reproduce past tick 13 with the payment rule modelled, words intact",
  "blocks": "the '@2026.01.16 211610 (1)' lane at tick 13; any lane whose first QUEUE overruns the player's stock",
  "unblocks": ["a QUEUE model that holds past the first payment", "part of the movement-frontier recordings"],
  "notes": "Seen only once in the 30-recording sample; may be rare. Do not model before seeing the rule: the checkpoint compare is the oracle for which of reject/debt/defer the game does.",
  "outcome": null,
  "next": null
}
```

## Evidence

- The oracle probe on the save-66.6/162286 sample (2026-10-16):
  `@2026.01.16 211610 (1)`: first unsupported `p1: tick 13, object 0: QUEUE the
  player cannot pay`.
- `genie-core/src/turn.rs` `Command::Queue`: cost paid per entry from the
  build_inventory; no branch for insufficient resources.
