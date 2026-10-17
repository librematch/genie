#!/usr/bin/env bash
# Runs a cargo command with the oracle environment set: the recording corpus
# and the DE data file.
#
# Paths may be overridden in tools/env.local (untracked) or via the
# environment. Test threads need a big stack — the generated readers parse
# large structs by value and overflow the default test-thread stack — so
# RUST_MIN_STACK is raised here.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -f tools/env.local ]]; then source tools/env.local; fi
export GENIE_REC_CORPUS="${GENIE_REC_CORPUS:-$HOME/genie/corpus66}"
export GENIE_DAT="${GENIE_DAT:-$HOME/genie/dat/empires2_x2_p1.dat}"
export RUST_MIN_STACK="${RUST_MIN_STACK:-67108864}"
exec "$@"
