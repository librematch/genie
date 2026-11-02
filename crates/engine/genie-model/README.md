# genie-model

The game data of Age of Empires II: Definitive Edition as the genie engine holds it: the players'
masters, the technologies and their effects, the tech tree, the unit lines, the drop sites and
the graphics, with the newtypes their fields hold and the operations that change them (an
effect's change to a master).

It holds plain data and the functions over it. The systems that run a game are genie-core's, and
genie-import fills the model from the game's files (FD-046). The modules mirror genie-core's
paths until the components regroup them.

## Licence

[AGPL-3.0-only](https://github.com/librematch/genie/blob/main/LICENSE-AGPL-3.0)

Age of Empires II © Microsoft Corporation. This project is not endorsed by or affiliated with
Microsoft.
