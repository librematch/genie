# genie-core

The deterministic simulation of build 158041 of Age of Empires II: Definitive Edition: the
world, its load from a recording's or a game's save (`world::load`) and its step
(`World::tick_with`), system by system. It replays the build's recordings checkpoint for
checkpoint. `docs/ENGINE_SYSTEMS.md` maps its 19 systems, and each folder of `src/` has a
`README.md`.

genie-core is the engine's internals, open to the workspace's tools (genie-oracle, the
harness) and not a stable API. A frontend uses the facade crate `genie` (FD-001-D15, FD-003).

## Licence

[AGPL-3.0-only](https://github.com/librematch/genie/blob/main/LICENSE-AGPL-3.0)

Age of Empires II © Microsoft Corporation. This project is not endorsed by or affiliated with
Microsoft.
