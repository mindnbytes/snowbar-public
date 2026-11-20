# Rune integration checks

Run these checks for every candidate Rune update before updating the application
pin. The Rune dependency and every Rune-sourced Cargo patch must point at the same tested
fork revision; Rune's workspace patches are not inherited by applications.

## Integration workflow

“Run the Rune integration workflow” invokes this process. The primary agent
coordinates the reviewer and implementer, checks their evidence, and resolves
blockers rather than skipping a gate.

1. **Preflight.** Check Snowbar and the permanent fork checkout for local
   changes and unpublished work. Take the current fork revision from Snowbar's
   manifest, not the sibling checkout's `HEAD`. Resolve `anomalyco/rune`'s `dev`
   to an exact SHA and freeze it for this run. Use `gh` for GitHub reads and SSH
   for GitHub Git transport. If the VM is unavailable, report it; don't search
   unrelated local configuration for connection details.
2. **Review upstream and plan the merge.** Ask the read-only reviewer for the
   merge base, user-visible upstream changes, conflict risks, and changes to
   Rune's Cargo patches. Check whether upstream replaces or only overlaps the
   fork's reserved top layer/exclusive zone ([#138]), offset-safe keymap reads
   ([#319]), or layer-shell wheel forwarding ([#699]). Identify migration
   candidates, but don't drop a fork feature on the strength of an open issue
   or a partial implementation. Confirm the findings before handing off.
3. **Merge and review the resolution.** Have the implementer start from the
   pinned fork commit on a new dated branch in an isolated scratch checkout.
   Merge the frozen upstream SHA, preserving both sides' behavior and leaving
   unrelated work and parked prototypes alone. Review the resulting diff
   against both parents. Have the reviewer independently check meaningful
   conflict resolutions, and resolve any findings before accepting the build.
4. **Build and test a temporary Snowbar candidate.** Keep the real pin
   unchanged. Point the candidate's Rune dependency and every Rune-sourced
   patch at the same merge commit, adding or removing patches as needed.
   An isolated local Git URL lets Cargo test an unpublished merge. Use the
   pinned Nix shell to build the Linux binary and run focused Snowbar library
   and binary tests and Clippy; check the Rune probe when relevant. Follow the
   resource limits below. Rerun affected checks after fixing review findings.
   Report the exact candidate SHA, check results, ignores, and skips.
5. **Preview and get approval.** With permission to use the live VM session,
   launch only the temporary candidate and guide the user through the
   [niri checklist](#niri-vm-checklist), emphasizing any paths touched by the
   merge (e.g. workspace scroll or Escape on a real keyboard). Never execute a
   power action. Brief the user on upstream changes, fork features retained,
   migration prospects and remaining gaps. Stop the preview after review.
   Wait for explicit approval to publish and promote; a passing build by
   itself is not approval.
6. **Publish, promote, and clean up.** Publish the approved fork commit first
   and verify the remote ref. Update Snowbar's production anonymous HTTPS Cargo
   pin, lockfile, and `cargoHash` in `flake.nix`; check
   that the resolution matches the tested candidate aside from the source URL.
   Verify the locked dependency and record the upstream and merge SHAs, test
   and VM results, and known gaps in `reuse-decision.md`. Commit the dependency
   update with the evidence, push only when authorized, and follow the
   [artifact cleanup checklist](#temporary-artifact-cleanup).

[#138]: https://github.com/anomalyco/rune/issues/138
[#319]: https://github.com/anomalyco/rune/issues/319
[#699]: https://github.com/anomalyco/rune/issues/699

## Fork checkout and pin

Work from the sibling `rune-integration` checkout (the `rune-fork` project
reference if configured in your local `opencode.json`). Do not assume a
branch name or copy a revision from these notes: `Cargo.toml` is the source
of truth, and it should yield exactly one revision across the `rune`
dependency and every patch.

The production Cargo URLs in `Cargo.toml` and `Cargo.lock` use anonymous HTTPS
so Nix builds and CI do not need SSH credentials. Contributor Git transport,
including the permanent checkout and publishing the fork, remains SSH.

```sh
pc=$(grep -o 'rev = "[0-9a-f]\{40\}"' Cargo.toml | cut -d'"' -f2 | sort -u)
printf '%s\n' "$pc"   # exactly one line; more means the manifest disagrees
test "$(git -C ../rune-integration rev-parse HEAD)" = "$pc"
git -C ../rune-integration status --short
```

HEAD must equal the manifest revision; a detached HEAD is fine. The status
command reports local edits: they are acceptable for a source inspection if you
name which files you are reading around, but HEAD alone then does not prove the
files match the pin. Use a clean worktree at the pinned commit for reproducible
comparisons, builds or behavior claims. If HEAD differs, inspect that difference
rather than calling it the pin. Keep local absolute paths out of the public
fork's docs, issues and commits.

## Automated checks

On Linux, in the project's development shell, run:

```sh
nix develop --command sh scripts/check-rune-integration.sh
```

This checks the Snowbar binary using the committed lockfile; it does **not**
run tests or verify a live compositor. The script defaults to two build jobs;
set `CARGO_BUILD_JOBS` to change that. Record the missing test coverage
separately from the check result.

Do not compile or run the full Rune test suite or heavy `--all-targets` test
links on the local Mac or VM without explicit user approval. Rune's test binary
exceeded the VM's available memory and caused heavy swapping even with reduced
debug info. On a sufficiently provisioned machine, optional broader checks
include Snowbar's all-target tests and Rune's overlay tests:

```sh
nix develop --command env CARGO_BUILD_JOBS=2 cargo test --locked --all-targets
nix develop --command env CARGO_BUILD_JOBS=2 cargo test -p rune --lib overlay::
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

### Temporary VM checkout and Nix source copying

If a candidate is copied to the VM without its Git metadata, running
`nix develop` from that directory can copy the entire directory into the Nix
store, including an existing `target/` build tree. This can look like a stalled
launch and consume substantial disk space. For repeated commands, use a
separate small directory containing copies of the candidate's pinned
`flake.nix` and `flake.lock`, and run `nix develop /path/to/that-directory
--command ...` from the candidate directory. Verify the copies match the
candidate's files. This still uses the project's pinned shell while keeping
build output out of the flake source; do not substitute the host toolchain.

## Niri VM checklist

Run the candidate build in the existing graphical niri session; don't replace
the installed bar. Record the fork commit and results before updating the pin.

- [ ] The bar spans the expected output, renders correctly, and tiled windows
      start below its reserved area.
- [ ] Bar clicks work; the power menu opens at the correct position.
- [ ] Menu keyboard focus works and Escape dismisses it; the bar's power toggle
      also dismisses it.
- [ ] Power rows open action-specific confirmation without dispatching; Cancel
      and Escape clear an unconfirmed selection.
- [ ] The menu's blank area does not cover the bar's power toggle; only its
      buttons intercept the pointer, including at narrow/scaled output.
- [ ] A same-coordinate double click on each row, especially Restart, does not
      confirm.
- [ ] Fullscreen hides the bar and leaving fullscreen restores it. An already
      open power menu remaining above fullscreen is a known, deferred issue.
      If revisiting the Rune prototype, run the
      [fullscreen and placement checks](power-policy.md#deferred-fullscreen-layering)
      before updating the pin.
- [ ] Check scaling and output changes when practical; record any not tested.
- [ ] Stop the temporary candidate after testing; do not activate a power action.
      If the user chooses to test one, use a disposable session and see the
      user-only checklist in `power-policy.md`.

Do not infer a pass for untested items. Update `Cargo.toml` and `Cargo.lock`
only after reviewing the candidate's results and remaining risks.

## Temporary artifact cleanup

- Create one dedicated scratch root per integration on each host, rather than
  leaving separate clones and candidate directories beside the permanent
  checkouts. Record the paths of temporary worktrees, bare repositories,
  Snowbar candidates and VM flake copies as they are created.
- Once a candidate is promoted or abandoned, stop its preview and confirm no
  build is using those paths. Before removal, check for local edits, untracked
  user files, and commits or refs that are not preserved elsewhere. For a
  promoted merge, verify the published fork revision and Snowbar pin. If any
  work is unique or ownership is unclear, keep it and report why.
- Remove Git worktrees with `git worktree remove` without forcing. Then remove
  only the confirmed disposable paths, including candidate build output, bare
  mirrors and VM flake copies. Never remove the permanent `rune-integration`
  checkout, the Snowbar checkout, or shared caches as part of this cleanup.
- If review is paused or artifacts are still needed, list what remains and
  why. Revisit cleanup when that work finishes.
