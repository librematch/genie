# genie workspace tasks — mirrors .github/workflows/ci.yml, plus local corpus
# runs. Local corpus paths may be set in tools/env.local (untracked); see
# tools/oracle.sh. Requires cargo on PATH (e.g. ~/.cargo/bin).

default:
    @just --list

# Format check (CI: job fmt)
fmt:
    cargo fmt --all --check

# Clippy; the engine crates must be warning-free (CI: job clippy)
clippy:
    cargo clippy --locked --no-deps -p genie-core -p genie-oracle --all-targets -- -D warnings
    cargo clippy --locked --workspace --all-targets

# Fast test suite; the corpus tests skip without their environment (CI: job test)
test:
    cargo test --locked --workspace

# Engine builds for wasm32 (CI: job wasm; needs `rustup target add wasm32-unknown-unknown`)
wasm:
    cargo build --locked -p genie-core -p genie-oracle --target wasm32-unknown-unknown

# Docs (CI: job docs)
doc:
    cargo doc --locked --workspace --no-deps

# Everything CI runs on a push/PR, one OS of the matrix
ci: fmt clippy test wasm doc

# The oracle suite against the local corpus. Note: the pinned expectations
# (de_corpus totals, word9's 22 lanes, marvinblue) describe the workspace's
# tier-1 corpus (save 66.6, build 158041); a corpus of another build fails
# them by design — run `just oracle --test corpus_probe` style probes instead.
oracle *args:
    tools/oracle.sh cargo test --locked -p genie-oracle -- {{args}}
