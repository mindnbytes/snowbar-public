# Project guidance

- Read `niri-rune-bar.md` before making project decisions; it defines the
  approved visual baseline, scope, development slices and review checkpoints.
- Use the project's pinned Nix development shell for development, builds,
  formatting, Clippy and tests. Run commands with `nix develop --command …`
  or enter `nix develop` first. Do not substitute the host's Rust toolchain or
  an unpinned environment; if the shell is unavailable, report the blocker.
- Use the existing niri VM for Linux/Wayland integration checks when needed.
  Keep the bar project separate from the VM configuration and do not modify
  a live session or invoke power actions without explicit approval. See
  `docs/rune-integration.md` for Rune-specific checks and resource limits.
- Work in small, verifiable slices and pause at meaningful review points. Keep
  the approved visuals and future palette configurability in mind without
  building speculative infrastructure.
