# genie-command-codec

The command packets of an Age of Empires II: Definitive Edition recording (genie-rec's op 1) as
genie-core's commands, both ways: `decode` reads a packet into its command, `encode` writes a
command's packet. A packet whose command would not write back its bytes stays raw, so
`encode(decode(p)) == p` for every packet.

## Licence

[AGPL-3.0-only](https://github.com/librematch/genie/blob/main/LICENSE-AGPL-3.0)

Age of Empires II © Microsoft Corporation. This project is not endorsed by or affiliated with
Microsoft.
