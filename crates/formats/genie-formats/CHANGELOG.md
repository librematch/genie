# genie-rs change log

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](http://semver.org/).

## 0.6.0
* **(breaking)** scx: `DiplomaticStance`, `DataSet`, `DlcPackage`, `StartingAge`, `VictoryState` and `AiErrorCode` keep a value they do not name (`Other`) and write it back, instead of failing the read; their `Parse…Error` types and `Error` variants go. `StartingAge::try_from(n, version)` is `StartingAge::from_i32`. The deprecated `Scenario::from`, `TribeScen::from` and `VictoryConditions::from` go (use `read_from`). genie-scx is v5.0.0.
* **(breaking)** dat: `TechTreeStatus`, `TechTreeKind` and `TechTreeDependency` keep a value they do not name (`Other`); their `Parse…Error` types go. The lookups drop their `get_` prefix (`tech`, `terrain`, `civilization`, `sound`, `sprite`, `gaia`, `Civilization::unit_kind`). genie-dat is v0.2.0.
* **(breaking)** rec: an unknown command keeps its id and bytes (`Command::Unknown`), an unknown game sub-command its fields (`GameCommand::Other`); `Error::UnsupportedCommand` and `UnsupportedGameCommand` go. `UngarrisonCommand::unit_kind_id` is an `Option<UnitKindId>`. The unread HD game settings (`HdGameOptions`, `Difficulty` and the placeholder enums) and the unused `element::ReadableElement` and `WritableElement` go. genie-rec is v0.2.0.
* **(breaking)** cpx: `Campaign::from` is `Campaign::read_from`; `get_name(i)`, `get_filename(i)` are `scenario_name(i)`, `scenario_filename(i)`. genie-cpx is v0.6.0.
* **(breaking)** hki: `HotkeyInfo::from(&mut R)` is `HotkeyInfo::read_from(impl Read)`; `get_string_from_lang` is `string_from_lang`. genie-hki is v0.3.0.
* **(breaking)** drs: `DrsWriter::add` takes a `ResourceKind`, and the `IntoResourceKind` trait goes: a caller with a name parses it first (`"slp".parse()?`).
* **(breaking)** support: `MapInto` is `MapIntoExt`, as an extension trait's name ends in `Ext` (RFC 445).
* **(breaking)** dat: `FileVersion` parses with `TryFrom<[u8; 8]>` and `TryFrom<&str>`, which return an error for a version of more than 8 bytes or not of the form `VER d.d`; the `From` implementations, which panicked there, go. `DatFile::read_from` fails on such a version.
* **(breaking)** The enums whose values come from the games' files (versions, kinds, ids, the hotkey catalogue, the language file kinds, the recording's ops and commands) are `#[non_exhaustive]`: a later game's value is not a breaking change. genie-lang is v0.3.0.
* Doc examples propagate errors with `?` instead of `unwrap`.

## 0.5.0
* **(breaking)** scx: fix Age of Empires 2: Definitive Edition tile data types. `MapTile.layered_terrain` now contains a u16 instead of a u8.
* **(breaking)** scx: read versioned map data from Age of Empires 2: Definitive Edition.
* **(breaking)** cpx: update genie-scx to v4.0.0.
* cpx: support reading and writing Age of Empires 2: Definitive Edition campaign files. (#22)
* rec: fix small action buffer optimisation.

## 0.4.0
* **(breaking)** scx: support Age of Empires 2: Definitive Edition scenario files. (#28)
* **(breaking)** scx: change `DataStruct::from(&mut Read)` methods to `DataStruct::read_from(impl Read)`. (#28)
* **(breaking)** cpx: update genie-scx to v3.0.0.
* cpx: support reading and writing AoE1: Definitive Edition campaign files. (#18)
* dat: Add a `.dat` file reader with support for The Conquerors and the HD Edition. It has some writing support but makes no guarantees yet.
* drs: make `ResourceKind` act more like a `&str`. (#15)
* lang: disable unused `pelite` features for leaner DLL reading.
* rec: Add a recorded game file reader with support for Age of Kings and The Conquerors. (#8)
* scx: support writing embedded AI information and triggers. (#17, #28)
* Use `thiserror` for custom error types. (#27)

## 0.3.0
* **(breaking)** genie: Raise minimum language version requirement to Rust 1.34, for the `TryFrom` trait.
* **(breaking)** scx: Add descriptive error types.
* **(breaking)** cpx: Add descriptive error types.
* **(breaking)** drs: Add descriptive error types.
* **(breaking)** hki: Add non-destructive update functions for binding hotkeys. (@twestura in #3)
* **(breaking)** lang: Overhaul APIs. (@twestura in #3)
* **(breaking)** pal: Replace `chariot_palette` with custom jascpal crate, adding support for writing palette files.
* drs: Add a DRS file writer.
* cpx: Detect and convert non-UTF8 encodings.
* drs: find resources faster using binary search. (#6)

## 0.2.0
* Add a cpx file writer.
* Import genie-drs, for reading .DRS files.
* Add read/write support for .ini and HD Edition key-value language files, and read support for .dll language files.

## 0.1.0
* Initial release.
