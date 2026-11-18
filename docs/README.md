# Snowbar documentation index

Start with `../AGENTS.md` for project rules and `../niri-rune-bar.md` for the
brief and approved visual baseline. This index records what each document is
for, its status, and when to read it. It is descriptive; it does not relax the
safety constraints in `AGENTS.md`.

## Document authority

`Cargo.toml` is the source of truth for Snowbar's Rune fork revision; all of
its `rune` and `[patch.crates-io]` entries must name the same tested commit.
`Cargo.lock` pins the resolved Rust dependencies and `flake.lock` pins the
development shell; both are committed. Dated documents below describe the
revision they name and are not a substitute for checking the manifest.
Historical commit IDs and older conclusions are kept for provenance, not
presented as current state.

## Index

| Document | Status | Read when |
|---|---|---|
| [`rune-integration.md`](rune-integration.md) | Current checklist | Before updating the Rune pin, running Linux/Rune checks, or doing the niri VM checklist. Contains the resource limits for heavy tests. |
| [`reuse-decision.md`](reuse-decision.md) | Decision record, completed 2026-11-10; integration updates through 2026-11-17 | Before revisiting Hokum/Ashell/Rune reuse, the fork rationale, or earlier probe evidence. Historical pins; not the current dependency source. |
| [`workspace-policy.md`](workspace-policy.md) | Current behavior note | Before changing workspace display, output selection, focus actions, or reconnect behavior. Single-output scope and verified/unverified boundaries live here. |
| [`rune-layer-scroll-investigation.md`](rune-layer-scroll-investigation.md) | Investigation record for the pinned scroll candidate | Before changing layer-shell scroll input or removing Snowbar's fallback controls. Distinguishes tested conversion logic from the unverified full Wayland path. |
| [`rune-output-investigation.md`](rune-output-investigation.md) | Investigation/proposal, not implemented | Before designing multi-monitor or hotplug support. Explains fail-open risks and the single-monitor v1 choice. |
| [`output-diagnostic.md`](output-diagnostic.md) | Historical diagnostic | Before relying on display IDs, names, or placement. Records an older pin and explicitly does not prove live placement. |
| [`niri-blur-study.md`](niri-blur-study.md) | Completed study, negative result | Before proposing compositor blur. Records the approval needed for temporary layer rules and why per-capsule blur is deferred. |
| [`rune-visual-notes.md`](rune-visual-notes.md) | Current visual guidance, with a historical pin | Before visual, artwork, accessibility, or theming changes. Preserves the approved baseline and states the no-live-blur limitation. |
| [`configuration-candidates.md`](configuration-candidates.md) | Ideas, not implemented | Before designing a configuration format or palette support. Lists candidates without promising a schema. |
| [`cleanup-backlog.md`](cleanup-backlog.md) | Open backlog | Before cleanup, performance, IPC, or worker-lifecycle work. Separates investigation leads from confirmed defects. |

## Fork checkout

The Rune fork checkout is expected as a sibling `rune-integration` directory.
`opencode.json` exposes it as the `rune-fork` project reference for local
inspection. Do not put machine-specific absolute paths into the public fork.
