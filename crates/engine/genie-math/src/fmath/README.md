# `math/fmath`: the game's `f32` math

The game links the math routines of musl, which come from FreeBSD's msun. This folder ports each
of them from the game's machine code: its constants, its order of operations, its branches and
its integer bit tests. The result is the game's bits on every host, WebAssembly included.

A NaN result needs care. SSE, ARM and WebAssembly make different NaN bits, so a routine whose
result is a NaN makes the game's bits itself.

`tests/crt_math.rs` compares the routines with the game's own on recorded inputs. `tests.rs` in
this folder holds the unit tests.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`fmod_and_log.rs`](fmod_and_log.rs) | fmodf, and logf with the double logarithm it rounds. |
| [`inverse_trig.rs`](inverse_trig.rs) | The inverse trigonometry: acosf, atanf and atan2f (msun's). |
| [`mod.rs`](mod.rs) | The game's f32 math, bit for bit (checked by `tests/crt_math.rs` against build 158041's own routines on recorded inputs). |
| [`pow.rs`](pow.rs) | powf (musl's) with its log2 and exp2 tables. |
| [`reduction.rs`](reduction.rs) | The argument reduction modulo π/2 (`__rem_pio2f`, `__kernel_rem_pio2`) and the floor and scalbn it uses. |
| [`sin_cos.rs`](sin_cos.rs) | The sine and the cosine (cosf, sinf) with their kernels. |
| [`tests.rs`](tests.rs) | Tests. |
| [`vectors.rs`](vectors.rs) | The game's 3-vectors (`BVector`): sums, products, length, angle, line distance and rotation. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`map`](../../map/).
<!-- /uses -->
