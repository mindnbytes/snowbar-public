# Project guidance

- Read `niri-rune-bar.md` before making project decisions; it defines the
  approved visual baseline, scope, development slices and review checkpoints.
- Start each new implementation task on a topic branch based on the latest
  `origin/master`; never implement directly on `master`. Continue follow-up
  work on its existing topic branch and preserve unrelated local changes.
  For delegated work, the coordinating agent sets up the branch before
  dispatch; workers sharing a checkout must not switch that checkout's branch.
- When authorized to run hosted CI, push the topic branch and open or update
  a draft pull request early so GitHub Actions can run the Linux build checks.
  Report the results, keep visual and click checks manual, and wait for
  approval before merging.
- When asked to run the Rune integration workflow, follow the
  [Rune integration runbook](docs/rune-integration.md#integration-workflow):
  delegate upstream review and implementation, then stop at the visual
  approval gate before publishing or promoting the pin.
- Use the project's pinned Nix development shell for development, builds,
  formatting, Clippy and tests. Run commands with `nix develop --command …`
  or enter `nix develop` first. Do not substitute the host's Rust toolchain or
  an unpinned environment; if the shell is unavailable, report the blocker.
- Use the existing niri VM for Linux/Wayland integration checks when needed.
  Keep the bar project separate from the VM configuration and do not modify
  a live session or invoke power actions without explicit approval. See
  `docs/rune-integration.md` for Rune-specific checks and resource limits.
  Never compile or run the full Rune test suite or heavy `--all-targets` test
  links on the local Mac or VM without explicit user approval, as neither
  machine has enough RAM for those large test binaries.
- Keep temporary Rune integration worktrees, clones, and builds under a
  dedicated scratch directory on each host. Track their paths and clean them
  up after promotion or abandonment once they contain no unique work; keep
  the permanent `rune-integration` checkout. See `docs/rune-integration.md`
  for the cleanup checklist.
- Work in small, verifiable slices and pause at meaningful review points. Keep
  the approved visuals and future palette configurability in mind without
  building speculative infrastructure.
