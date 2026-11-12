#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

# Limit parallelism by default; callers can choose a different limit.
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
cargo check --locked --all-targets
cargo test --locked --all-targets

printf '\nAutomated checks passed. Complete the niri checklist in docs/rune-integration.md.\n'
