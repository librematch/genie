# genie-import

The game's files as the genie engine's model (FD-046): the `.dat`'s masters per civilization,
technologies, effects and terrains (`dat`), a saved game's masters (`save`) and the JSON tables
beside the `.dat` (`tables`).

Each importer reads the bytes or text it is given. `dat::MasterTable::read` also opens the
`.dat` and the tables beside it. genie-core never depends on this crate: the model it runs on
comes from here, through genie-model's types.

## Licence

[AGPL-3.0-only](https://github.com/librematch/genie/blob/main/LICENSE-AGPL-3.0)

Age of Empires II © Microsoft Corporation. This project is not endorsed by or affiliated with
Microsoft.
