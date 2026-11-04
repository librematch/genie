# genie-error

The error every genie crate returns: `GenieError`, with its kind (a part of the game not modelled
yet, a state the game rules out, data the input lacks, input that breaks its format), its reason
(a fixed text, which groups the same failure across runs), where it was raised (the file and line
of the constructor's caller) and its context (key and value pairs the raising site attaches).

It sits below every other crate of the workspace and depends on none of them, so the format
crates (GPL-3.0) and the engine and tool crates (AGPL-3.0) can all use it.

`GenieResult<T>` is its one `Result` alias. The `serde` feature implements serde's `de::Error`
for it, so a deserializer's errors are `GenieError`s too (genie-nickel's).
