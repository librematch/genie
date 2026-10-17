---
name: game-options-1123
---

# The reader reads the game-options version 1123 (save 67.2–68.9)

```json
{
  "id": "game-options-1123",
  "title": "The reader reads the game-options version 1123 (save 67.2–68.9)",
  "category": "scope",
  "status": "open",
  "question": "The generated reader asserts GameOptionsRead.version == 1122.0 (SaveGameFileHeader+0x18, FUN_1412ff850 of the build), so every recording from save 67.2 (build 169652, Feb 2026) through 68.9 (build 185872, Oct 2026) is rejected before parsing: about two-thirds of the local corpus. The newer builds write 1123.0 there. What changed in GameOptions between serializer versions 1122 and 1123, and can the reader read both? Options: (a) relax the assert to accept 1122.0 and 1123.0 and diff-parse one 1123 header against one 1122 header field by field until the whole world parses; (b) first decompile FUN_1412ff850 in the 162286 build to see how the version gates the layout before touching the generated reader. Recommended: (a), because the diff-parse is cheap on real data and the generated reader is the contract; decompile only where the diff shows a gap.",
  "options": [
    "(a) relax the assert to accept 1122.0 and 1123.0 and diff-parse one 1123 header against one 1122 header field by field",
    "(b) first decompile FUN_1412ff850 in the 162286 build to see how the version gates the layout"
  ],
  "recommended": "(a): the diff-parse is cheap on real data and the generated reader is the contract",
  "verify": "cargo test -p genie-rec (GENIE_REC_CORPUS set): one save-67.2 recording and one save-68.9 recording from the local corpus parse (header + ops round-trip); the 1122 fixtures keep passing",
  "blocks": "about two-thirds of the local corpus (save 67.2/68.0/68.9, Feb–Oct 2026)",
  "unblocks": ["the oracle on current-version recordings", "checkpoint words from the latest builds", "a reader for today's game version"],
  "notes": "Header probe (2026-10-16): 1123.0 first appears at save 67.2/build 169652 (game v101.103.38580) and stays through 68.9/build 185872; new_save_game_version goes 0x429999 (66.6) → 0x433333 (67.2) → 0x440000 (68.0) → 0x44e666 (68.9). The game_options_version is a separate field from the save version and gates the GameOptions layout inside the inflated header (~16 MB). Files for the diff are staged at aibox ~/genie/corpus (one per save version).",
  "outcome": null,
  "next": null
}
```

## Evidence

- `crates/formats/genie-rec/src/de/header/generated/world.rs:549`:
  `#[br(assert(version == 1122.0, "the build writes version 1122.0"))]`.
- The corpus probe (2026-10-16): every save-67.2+ recording fails with
  `header: 0: Error: the build writes version 1122.0 at 0x18`.
- The header-version dump (2026-10-16): the 31214-era recordings write
  game_options_version 1122; every 38580-and-later recording writes 1123.
