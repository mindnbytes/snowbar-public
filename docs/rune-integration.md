# Rune integration checks

Run these checks for every candidate Rune update before updating the application
pin. The application and all four Cargo patches must point at the same tested
fork revision; Rune's workspace patches are not inherited by applications.

## Automated checks

On Linux, in the project's development shell, run:

```sh
nix develop --command sh scripts/check-rune-integration.sh
```

This checks and tests all Snowbar targets using the committed lockfile. The
script defaults to two build jobs; set `CARGO_BUILD_JOBS` to change that. These
tests do not verify a live compositor or the whole Rune test suite. Rune's
overlay unit tests are optional locally because their test binary exceeded the
VM's available memory and caused heavy swapping, even with reduced debug info.
Run them on a sufficiently provisioned machine if available, after the
lightweight checks pass:

```sh
CARGO_BUILD_JOBS=2 cargo test -p rune --lib overlay::
```

Compiling Rune's test binary is very resource-intensive even when tests are
filtered. Limit build jobs and consider available memory before running builds
on multiple machines concurrently. Check upstream CI for the selected revision,
but do not treat upstream results as proof that fork-specific changes pass.
Record whether the fork's tests were actually run.

Linux checks and the niri checklist are required. macOS checks are only needed
when changing macOS-specific configuration or when developing Rune on a Mac;
they are not a prerequisite for every Linux-only integration. In those cases,
check the Rune checkout with `cargo check -p rune --example snowbar_probe`.
When changing the macOS linker, also consider building or running Rune's unit
tests to check large-binary linking. Upstream no longer forces LLD, so the fork
uses the normal platform linker; very large macOS test binaries may still
require a compatible LLD and SDK. Never disable compact unwind as a workaround.

## Niri VM checklist

Run the candidate build in the existing graphical niri session; don't replace
the installed bar. Record the fork commit and results before updating the pin.

- [ ] The bar spans the expected output, renders correctly, and tiled windows
      start below its reserved area.
- [ ] Bar clicks work; the power menu opens at the correct position.
- [ ] Menu keyboard focus works and Escape dismisses it; click dismissal works.
- [ ] Fullscreen hides the bar and leaving fullscreen restores it.
- [ ] Check scaling and output changes when practical; record any not tested.
- [ ] Stop the temporary candidate after testing; do not activate a power action.

Do not infer a pass for untested items. Update `Cargo.toml` and `Cargo.lock`
only after reviewing the candidate's results and remaining risks.
