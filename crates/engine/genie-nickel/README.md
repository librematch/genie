# genie-nickel

Nickel into the genie engine's model (FD-046). The build layer's facts and a mod's changes are
Nickel sources: `evaluate` runs one through the Nickel evaluator, and `from_expr` reads the
value into a model type through serde.

Nickel holds every number as an exact rational. `round_to_f32` rounds it once to the nearest
f32, ties to even, which is the f32 a compiler gives the same decimal; rounding through f64 first
would differ in rare halfway cases (FD-046-D16). An integer field takes the integer checked
against its type, and an error names the field's path.

`contracts` writes genie-model's types as Nickel contracts, from the JSON Schema schemars derives
for them (genie-model's `schema` feature): a struct a record contract, an enum a tag contract, a
bounded integer a predicate. The binary `contracts` writes them to `contracts/genie.ncl`
(`just contracts`); its `--check` runs in the pre-commit hook and CI, so the contracts cannot
drift from the engine's types (FD-046-D15). `schema_hash` is their SHA-256, a part of the model
cache's key (D18).

`layer::build_layer` evaluates a build's layer from its source `builds/<build>.ncl` (FD-046-D08),
which `just layer` generates from the build's symbol table. Its test holds the evaluated layer
equal to the Rust fixture genie-core's tests build, and the source within the contracts.

## Licence

[AGPL-3.0-only](https://github.com/librematch/genie/blob/main/LICENSE-AGPL-3.0)

Age of Empires II © Microsoft Corporation. This project is not endorsed by or affiliated with
Microsoft.
