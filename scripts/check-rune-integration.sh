#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

# Limit parallelism by default; callers can choose a different limit.
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
cargo check --locked --bin snowbar

printf '\nSnowbar binary check passed; tests were not run. See docs/rune-integration.md.\n'
